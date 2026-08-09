"""Fail-closed SQLite backup and copy-only upgrade audit helper.

The only command that may open a source database with WAL sidecars is
``backup``.  It records byte-level facts for the source DB/WAL/SHM before and
after SQLite's online backup API runs, and fails unless those facts are
identical.  Structural and business-data queries are then performed only on
the new main-only destination.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


EXCLUDED_BUSINESS_TABLE_PATTERNS = ("device_sync_", "cases_fts")
SQLITE_SIDECAR_SUFFIXES = ("-wal", "-shm", "-journal")
M63_COLUMNS = {
    "device_sync_groups": [
        "last_attempt_at",
        "last_success_at",
        "auto_paused",
        "pause_reason_code",
    ],
    "device_sync_outbox": ["capture_sequence"],
    "device_sync_quarantine": [
        "source_path",
        "source_device_id",
        "source_sequence",
        "status",
        "first_seen_at",
        "last_seen_at",
        "retry_count",
        "resolved_at",
        "last_error_code",
    ],
    "device_sync_export_drafts": [
        "group_id",
        "local_device_id",
        "sequence",
        "key_epoch",
        "previous_manifest_hash",
        "event_envelope_bytes",
        "manifest_envelope_bytes",
        "event_ciphertext_sha256",
        "manifest_ciphertext_sha256",
        "operation_ids_json",
        "operation_fingerprint",
        "state",
        "created_at",
        "updated_at",
        "finalized_at",
    ],
}
M63_INDEXES = [
    "idx_device_sync_quarantine_active_key",
    "idx_device_sync_quarantine_group_status",
    "idx_device_sync_outbox_capture_sequence",
    "idx_device_sync_outbox_pending_capture",
    "idx_device_sync_export_drafts_state",
    "idx_device_sync_export_drafts_one_prepared",
]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def sha256_rows(rows: Iterable[Any]) -> str:
    digest = hashlib.sha256()
    for row in rows:
        encoded = json.dumps(row, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
    return digest.hexdigest().upper()


def file_fact(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"path": str(path), "exists": False}
    stat = path.stat()
    return {
        "path": str(path),
        "exists": True,
        "bytes": stat.st_size,
        "last_write_time_ns": stat.st_mtime_ns,
        "sha256": sha256(path),
    }


def trio_facts(db_path: Path) -> dict[str, dict[str, Any]]:
    db_path = db_path.resolve(strict=True)
    if not db_path.is_file():
        raise FileNotFoundError(db_path)
    return {
        "main": file_fact(db_path),
        "wal": file_fact(Path(f"{db_path}-wal")),
        "shm": file_fact(Path(f"{db_path}-shm")),
        "journal": file_fact(Path(f"{db_path}-journal")),
    }


def comparable_fact(fact: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in fact.items() if key != "path"}


def trio_contents_equal(
    left: dict[str, dict[str, Any]], right: dict[str, dict[str, Any]]
) -> bool:
    return all(comparable_fact(left[name]) == comparable_fact(right[name]) for name in left)


def copy_source_trio(source: Path, raw_copy_dir: Path) -> tuple[Path, dict[str, dict[str, Any]]]:
    if raw_copy_dir.exists():
        raise FileExistsError(f"refusing to reuse raw trio directory: {raw_copy_dir}")
    raw_copy_dir.mkdir(parents=True)
    copied_main = raw_copy_dir / source.name
    for suffix in ("", *SQLITE_SIDECAR_SUFFIXES):
        origin = Path(f"{source}{suffix}")
        if origin.exists():
            shutil.copy2(origin, Path(f"{copied_main}{suffix}"))
    copied = trio_facts(copied_main)
    return copied_main, copied


def require_main_only(db_path: Path) -> None:
    sidecars = [Path(f"{db_path}{suffix}") for suffix in SQLITE_SIDECAR_SUFFIXES]
    present = [str(path) for path in sidecars if path.exists()]
    if present:
        raise RuntimeError(f"MAIN_ONLY_SIDECAR_PRESENT: {', '.join(present)}")


def quote_identifier(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def update_value(digest: Any, value: Any) -> None:
    if value is None:
        digest.update(b"N")
    elif isinstance(value, bytes):
        digest.update(b"B" + len(value).to_bytes(8, "big") + value)
    elif isinstance(value, int):
        data = str(value).encode("ascii")
        digest.update(b"I" + len(data).to_bytes(8, "big") + data)
    elif isinstance(value, float):
        data = value.hex().encode("ascii")
        digest.update(b"F" + len(data).to_bytes(8, "big") + data)
    else:
        data = str(value).encode("utf-8")
        digest.update(b"S" + len(data).to_bytes(8, "big") + data)


def table_fingerprint(connection: sqlite3.Connection, table: str) -> dict[str, Any]:
    info = connection.execute(f"PRAGMA table_info({quote_identifier(table)})").fetchall()
    columns = [row[1] for row in info]
    primary_key = [row[1] for row in sorted((row for row in info if row[5]), key=lambda row: row[5])]
    order_columns = primary_key or ["rowid"]
    select_columns = ",".join(quote_identifier(column) for column in columns)
    order_by = ",".join(
        quote_identifier(column) if column != "rowid" else "rowid" for column in order_columns
    )
    digest = hashlib.sha256()
    digest.update(table.encode("utf-8"))
    for column in columns:
        update_value(digest, column)
    rows = 0
    query = f"SELECT {select_columns} FROM {quote_identifier(table)} ORDER BY {order_by}"
    for row in connection.execute(query):
        digest.update(b"R")
        for value in row:
            update_value(digest, value)
        rows += 1
    return {"rows": rows, "primary_key": primary_key, "sha256": digest.hexdigest().upper()}


def object_exists(connection: sqlite3.Connection, object_type: str, name: str) -> bool:
    return (
        connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type=? AND name=?", (object_type, name)
        ).fetchone()
        is not None
    )


def column_names(connection: sqlite3.Connection, table: str) -> set[str]:
    return {
        row[1]
        for row in connection.execute(f"PRAGMA table_info({quote_identifier(table)})")
    }


def migration_sources(migrations_dir: Path | None) -> dict[int, dict[str, str]]:
    if migrations_dir is None:
        return {}
    root = migrations_dir.resolve(strict=True)
    result: dict[int, dict[str, str]] = {}
    for path in sorted(root.glob("*.sql")):
        match = re.match(r"^(\d+)_", path.name)
        if not match:
            continue
        raw = path.read_bytes()
        result[int(match.group(1))] = {
            "file": path.name,
            "sha384": hashlib.sha384(raw).hexdigest().upper(),
            "sha256": hashlib.sha256(raw).hexdigest().upper(),
        }
    return result


def sync_safety_metrics(connection: sqlite3.Connection, tables: set[str]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    if "device_sync_groups" in tables:
        columns = column_names(connection, "device_sync_groups")
        fields = ["COUNT(*)"]
        labels = ["group_count"]
        if "paused" in columns:
            fields.append("COALESCE(SUM(CASE WHEN paused=1 THEN 1 ELSE 0 END),0)")
            labels.append("paused_count")
        if "auto_paused" in columns:
            fields.append("COALESCE(SUM(CASE WHEN auto_paused=1 THEN 1 ELSE 0 END),0)")
            labels.append("auto_paused_count")
        row = connection.execute(f"SELECT {','.join(fields)} FROM device_sync_groups").fetchone()
        result["groups"] = dict(zip(labels, row))
    if "device_sync_outbox" in tables:
        result["outbox_states"] = {
            str(state): count
            for state, count in connection.execute(
                "SELECT state,COUNT(*) FROM device_sync_outbox GROUP BY state ORDER BY state"
            )
        }
        columns = column_names(connection, "device_sync_outbox")
        if "capture_sequence" in columns:
            row = connection.execute(
                "SELECT COUNT(*),"
                "SUM(CASE WHEN capture_sequence IS NULL OR capture_sequence=0 THEN 1 ELSE 0 END),"
                "COUNT(capture_sequence)-COUNT(DISTINCT capture_sequence),"
                "MIN(capture_sequence),MAX(capture_sequence) FROM device_sync_outbox"
            ).fetchone()
            result["outbox_capture_sequence"] = {
                "count": row[0],
                "null_or_zero": row[1] or 0,
                "duplicate_count": row[2] or 0,
                "min": row[3],
                "max": row[4],
            }
    if "device_sync_quarantine" in tables:
        columns = column_names(connection, "device_sync_quarantine")
        if "status" in columns:
            result["quarantine_states"] = {
                str(state): count
                for state, count in connection.execute(
                    "SELECT status,COUNT(*) FROM device_sync_quarantine GROUP BY status ORDER BY status"
                )
            }
        else:
            result["quarantine_legacy_count"] = connection.execute(
                "SELECT COUNT(*) FROM device_sync_quarantine"
            ).fetchone()[0]
    if "device_sync_export_drafts" in tables:
        result["export_drafts_count"] = connection.execute(
            "SELECT COUNT(*) FROM device_sync_export_drafts"
        ).fetchone()[0]
    return result


def snapshot(db_path: Path, migrations_dir: Path | None = None) -> dict[str, Any]:
    db_path = db_path.resolve(strict=True)
    require_main_only(db_path)
    connection = sqlite3.connect(db_path.as_uri() + "?mode=ro", uri=True)
    connection.execute("PRAGMA query_only=ON")
    connection.execute("PRAGMA busy_timeout=5000")
    try:
        quick_check = [row[0] for row in connection.execute("PRAGMA quick_check")]
        foreign_key_rows = connection.execute("PRAGMA foreign_key_check").fetchall()
        table_names = [
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_master "
                "WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            )
        ]
        tables = set(table_names)
        counts = {
            table: connection.execute(
                f"SELECT COUNT(*) FROM {quote_identifier(table)}"
            ).fetchone()[0]
            for table in table_names
            if not table.startswith("cases_fts")
        }
        sources = migration_sources(migrations_dir)
        migrations: list[dict[str, Any]] = []
        if "_sqlx_migrations" in tables:
            for version, description, installed_on, success, checksum, execution_time in connection.execute(
                "SELECT version,description,installed_on,success,checksum,execution_time "
                "FROM _sqlx_migrations ORDER BY version"
            ):
                stored = bytes(checksum).hex().upper()
                current = sources.get(version)
                migrations.append(
                    {
                        "version": version,
                        "description": description,
                        "installed_on": str(installed_on),
                        "success": int(success),
                        "stored_checksum_sha384": stored,
                        "execution_time": execution_time,
                        "current_file": current["file"] if current else None,
                        "current_checksum_sha384": current["sha384"] if current else None,
                        "checksum_matches_current": bool(current and stored == current["sha384"]),
                    }
                )
        schema_rows = [
            list(row)
            for row in connection.execute(
                "SELECT type,name,tbl_name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name"
            )
        ]
        business_tables = [
            table
            for table in table_names
            if table != "_sqlx_migrations"
            and not any(table.startswith(prefix) for prefix in EXCLUDED_BUSINESS_TABLE_PATTERNS)
        ]
        business_fingerprints = {
            table: table_fingerprint(connection, table) for table in business_tables
        }
        m63_columns: dict[str, dict[str, bool]] = {}
        for table, expected in M63_COLUMNS.items():
            actual = column_names(connection, table) if table in tables else set()
            m63_columns[table] = {column: column in actual for column in expected}
        m63_indexes = {
            index: object_exists(connection, "index", index) for index in M63_INDEXES
        }
        migration_history_rows = [
            [
                row["version"],
                row["description"],
                row["installed_on"],
                row["success"],
                row["stored_checksum_sha384"],
                row["execution_time"],
            ]
            for row in migrations
        ]
        result = {
            "path": str(db_path),
            "file": file_fact(db_path),
            "main_only": True,
            "sqlite": {
                "sqlite_version": sqlite3.sqlite_version,
                "journal_mode": connection.execute("PRAGMA journal_mode").fetchone()[0],
                "page_size": connection.execute("PRAGMA page_size").fetchone()[0],
                "page_count": connection.execute("PRAGMA page_count").fetchone()[0],
                "freelist_count": connection.execute("PRAGMA freelist_count").fetchone()[0],
                "quick_check": quick_check,
                "foreign_key_violation_count": len(foreign_key_rows),
            },
            "table_counts": counts,
            "table_counts_sha256": sha256_rows([[name, value] for name, value in counts.items()]),
            "migrations": migrations,
            "migration_summary": {
                "count": len(migrations),
                "max_version": max((row["version"] for row in migrations), default=None),
                "failed_count": sum(row["success"] != 1 for row in migrations),
                "checksum_mismatch_versions": [
                    row["version"] for row in migrations if row["current_file"] and not row["checksum_matches_current"]
                ],
                "unknown_applied_versions": [
                    row["version"] for row in migrations if sources and row["current_file"] is None
                ],
                "history_sha256": sha256_rows(migration_history_rows),
            },
            "schema": {"object_count": len(schema_rows), "sha256": sha256_rows(schema_rows)},
            "business_projection": {
                "excluded": ["_sqlx_migrations", "device_sync_*", "cases_fts*"],
                "table_count": len(business_fingerprints),
                "tables": business_fingerprints,
                "sha256": sha256_rows(
                    [[name, value["rows"], value["sha256"]] for name, value in business_fingerprints.items()]
                ),
            },
            "m63": {
                "applied": any(row["version"] == 63 and row["success"] == 1 for row in migrations),
                "columns": m63_columns,
                "indexes": m63_indexes,
                "quarantine_group_fk": [
                    list(row)
                    for row in connection.execute("PRAGMA foreign_key_list(device_sync_quarantine)")
                ]
                if "device_sync_quarantine" in tables
                else [],
                "export_drafts_group_fk": [
                    list(row)
                    for row in connection.execute("PRAGMA foreign_key_list(device_sync_export_drafts)")
                ]
                if "device_sync_export_drafts" in tables
                else [],
            },
            "sync_safety": sync_safety_metrics(connection, tables),
        }
    finally:
        connection.close()
    require_main_only(db_path)
    return result


def online_backup(
    source: Path,
    destination: Path,
    migrations_dir: Path | None = None,
    raw_copy_dir: Path | None = None,
) -> dict[str, Any]:
    source = source.resolve(strict=True)
    destination = destination.resolve()
    if destination.exists() or any(
        Path(f"{destination}{suffix}").exists() for suffix in SQLITE_SIDECAR_SUFFIXES
    ):
        raise FileExistsError(f"refusing to overwrite backup or sidecar: {destination}")
    if destination == source:
        raise ValueError("backup destination must differ from source")
    destination.parent.mkdir(parents=True, exist_ok=True)
    before = trio_facts(source)
    raw_copy_dir = (
        raw_copy_dir.resolve()
        if raw_copy_dir is not None
        else destination.parent / f"{destination.name}.source-trio"
    )
    copied_main, copied_before = copy_source_trio(source, raw_copy_dir)
    after_copy = trio_facts(source)
    if before != after_copy:
        raise RuntimeError("SOURCE_TRIO_CHANGED_DURING_RAW_COPY")
    if not trio_contents_equal(before, copied_before):
        raise RuntimeError("RAW_COPY_TRIO_MISMATCH")
    # SQLite is intentionally opened only on the retained raw copy.  Opening a
    # WAL database may update SHM lock bytes even in read-only mode; the formal
    # source must never be exposed to that side effect.
    source_connection = sqlite3.connect(copied_main.as_uri() + "?mode=ro", uri=True)
    source_connection.execute("PRAGMA query_only=ON")
    destination_connection = sqlite3.connect(destination)
    try:
        source_connection.backup(destination_connection)
        # The backup copies the source database header, including persistent
        # WAL mode.  Normalize only the new destination to DELETE mode so the
        # accepted artifact is a self-contained main file.  Source and raw
        # trio evidence remain untouched.
        mode = destination_connection.execute("PRAGMA journal_mode=DELETE").fetchone()[0]
        if str(mode).lower() != "delete":
            raise RuntimeError("BACKUP_MAIN_ONLY_NORMALIZATION_FAILED")
    finally:
        destination_connection.close()
        source_connection.close()
    after = trio_facts(source)
    if before != after:
        raise RuntimeError("SOURCE_TRIO_CHANGED_DURING_BACKUP")
    require_main_only(destination)
    backup_snapshot = snapshot(destination, migrations_dir)
    checks = {
        "source_trio_unchanged": True,
        "destination_main_only": True,
        "quick_check_ok": backup_snapshot["sqlite"]["quick_check"] == ["ok"],
        "foreign_key_check_ok": backup_snapshot["sqlite"]["foreign_key_violation_count"] == 0,
    }
    if not all(checks.values()):
        raise RuntimeError("BACKUP_VALIDATION_FAILED")
    return {
        "status": "backup-passed",
        "source_trio_before": before,
        "source_trio_after": after,
        "raw_source_copy_before_sqlite": copied_before,
        "raw_source_copy_directory": str(raw_copy_dir),
        "checks": checks,
        "backup": backup_snapshot,
    }


def compare(before: dict[str, Any], after: dict[str, Any], idempotent: bool = False) -> dict[str, Any]:
    before_counts = before["table_counts"]
    after_counts = after["table_counts"]
    before_tables = before["business_projection"]["tables"]
    after_tables = after["business_projection"]["tables"]
    shared = sorted(set(before_tables) & set(after_tables))
    content_changes = {
        table: {"before": before_tables[table], "after": after_tables[table]}
        for table in shared
        if before_tables[table]["sha256"] != after_tables[table]["sha256"]
    }
    count_changes = {
        table: {"before": before_counts[table], "after": after_counts[table]}
        for table in sorted(set(before_counts) & set(after_counts))
        if before_counts[table] != after_counts[table]
    }
    removed_business_tables = sorted(set(before_tables) - set(after_tables))
    added_business_tables = sorted(set(after_tables) - set(before_tables))
    checks = {
        "quick_check_ok": after["sqlite"]["quick_check"] == ["ok"],
        "foreign_key_check_ok": after["sqlite"]["foreign_key_violation_count"] == 0,
        "business_content_unchanged": not content_changes,
        "no_removed_business_tables": not removed_business_tables,
        "no_failed_migrations": after["migration_summary"]["failed_count"] == 0,
    }
    if idempotent:
        checks.update(
            {
                "schema_unchanged": before["schema"]["sha256"] == after["schema"]["sha256"],
                "migration_history_unchanged": before["migration_summary"]["history_sha256"]
                == after["migration_summary"]["history_sha256"],
                "sync_safety_unchanged": before["sync_safety"] == after["sync_safety"],
                "no_added_business_tables": not added_business_tables,
            }
        )
    return {
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "business_content_changes": content_changes,
        "table_count_changes": count_changes,
        "added_business_tables": added_business_tables,
        "removed_business_tables": removed_business_tables,
        "business_projection_before": before["business_projection"]["sha256"],
        "business_projection_after": after["business_projection"]["sha256"],
        "migration_versions_before": [row["version"] for row in before["migrations"]],
        "migration_versions_after": [row["version"] for row in after["migrations"]],
        "schema_before": before["schema"]["sha256"],
        "schema_after": after["schema"]["sha256"],
        "sync_safety_before": before["sync_safety"],
        "sync_safety_after": after["sync_safety"],
    }


def write_json(path: Path, value: dict[str, Any]) -> None:
    path = path.resolve()
    if path.exists():
        raise FileExistsError(f"refusing to overwrite evidence: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")


def load_snapshot(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8-sig"))
    return value.get("snapshot", value.get("backup", value))


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    facts_parser = subparsers.add_parser("facts")
    facts_parser.add_argument("--db", required=True)
    facts_parser.add_argument("--output", required=True)
    snapshot_parser = subparsers.add_parser("snapshot")
    snapshot_parser.add_argument("--db", required=True)
    snapshot_parser.add_argument("--migrations-dir")
    snapshot_parser.add_argument("--output", required=True)
    backup_parser = subparsers.add_parser("backup")
    backup_parser.add_argument("--source", required=True)
    backup_parser.add_argument("--destination", required=True)
    backup_parser.add_argument("--raw-copy-dir")
    backup_parser.add_argument("--migrations-dir")
    backup_parser.add_argument("--output", required=True)
    compare_parser = subparsers.add_parser("compare")
    compare_parser.add_argument("--before", required=True)
    compare_parser.add_argument("--after", required=True)
    compare_parser.add_argument("--idempotent", action="store_true")
    compare_parser.add_argument("--output", required=True)
    args = parser.parse_args()

    if args.command == "facts":
        result = {
            "captured_at_utc": datetime.now(timezone.utc).isoformat(),
            "status": "captured",
            "source_trio": trio_facts(Path(args.db)),
        }
    elif args.command == "snapshot":
        migrations = Path(args.migrations_dir) if args.migrations_dir else None
        result = {
            "captured_at_utc": datetime.now(timezone.utc).isoformat(),
            "status": "captured",
            "snapshot": snapshot(Path(args.db), migrations),
        }
    elif args.command == "backup":
        migrations = Path(args.migrations_dir) if args.migrations_dir else None
        result = {
            "captured_at_utc": datetime.now(timezone.utc).isoformat(),
            **online_backup(
                Path(args.source),
                Path(args.destination),
                migrations,
                Path(args.raw_copy_dir) if args.raw_copy_dir else None,
            ),
        }
    else:
        result = compare(
            load_snapshot(Path(args.before)),
            load_snapshot(Path(args.after)),
            idempotent=args.idempotent,
        )
    write_json(Path(args.output), result)
    print(json.dumps({"status": result.get("status", "captured")}, ensure_ascii=False))
    return 2 if result.get("status") == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
