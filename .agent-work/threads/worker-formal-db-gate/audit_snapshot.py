from __future__ import annotations

import hashlib
import json
import re
import sqlite3
import sys
from pathlib import Path


def sha256_rows(rows) -> str:
    digest = hashlib.sha256()
    for row in rows:
        encoded = json.dumps(row, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
    return digest.hexdigest().upper()


def update_value(digest: "hashlib._Hash", value) -> None:
    if value is None:
        digest.update(b"N")
    elif isinstance(value, bytes):
        digest.update(b"B")
        digest.update(len(value).to_bytes(8, "big"))
        digest.update(value)
    elif isinstance(value, int):
        data = str(value).encode("ascii")
        digest.update(b"I" + len(data).to_bytes(8, "big") + data)
    elif isinstance(value, float):
        data = value.hex().encode("ascii")
        digest.update(b"F" + len(data).to_bytes(8, "big") + data)
    else:
        data = str(value).encode("utf-8")
        digest.update(b"S" + len(data).to_bytes(8, "big") + data)


def quote_identifier(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def table_fingerprint(conn: sqlite3.Connection, table: str) -> dict:
    info = conn.execute(f"PRAGMA table_info({quote_identifier(table)})").fetchall()
    columns = [row[1] for row in info]
    pk_columns = [row[1] for row in sorted((row for row in info if row[5]), key=lambda row: row[5])]
    order_columns = pk_columns or ["rowid"]
    select_columns = ",".join(quote_identifier(column) for column in columns)
    order_by = ",".join(quote_identifier(column) if column != "rowid" else "rowid" for column in order_columns)
    digest = hashlib.sha256()
    digest.update(table.encode("utf-8"))
    for column in columns:
        update_value(digest, column)
    count = 0
    for row in conn.execute(
        f"SELECT {select_columns} FROM {quote_identifier(table)} ORDER BY {order_by}"
    ):
        digest.update(b"R")
        for value in row:
            update_value(digest, value)
        count += 1
    return {
        "rows": count,
        "primary_key": pk_columns,
        "sha256": digest.hexdigest().upper(),
    }


def column_names(conn: sqlite3.Connection, table: str) -> set[str]:
    return {row[1] for row in conn.execute(f"PRAGMA table_info({quote_identifier(table)})")}


def object_exists(conn: sqlite3.Connection, object_type: str, name: str) -> bool:
    return (
        conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type=? AND name=?", (object_type, name)
        ).fetchone()
        is not None
    )


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit("usage: audit_snapshot.py <snapshot-db> <migrations-dir> <output-json>")

    database = Path(sys.argv[1]).resolve()
    migrations_dir = Path(sys.argv[2]).resolve()
    output = Path(sys.argv[3]).resolve()

    conn = sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)
    conn.execute("PRAGMA query_only=ON")
    conn.execute("PRAGMA busy_timeout=5000")

    quick_check = [row[0] for row in conn.execute("PRAGMA quick_check")]
    foreign_key_rows = conn.execute("PRAGMA foreign_key_check").fetchall()

    migration_rows = conn.execute(
        "SELECT version, description, success, checksum, execution_time "
        "FROM _sqlx_migrations ORDER BY version"
    ).fetchall()

    migration_sources: dict[int, dict] = {}
    for path in sorted(migrations_dir.glob("*.sql")):
        match = re.match(r"^(\d+)_", path.name)
        if not match:
            continue
        raw = path.read_bytes()
        migration_sources[int(match.group(1))] = {
            "file": path.name,
            "sha384": hashlib.sha384(raw).hexdigest().upper(),
            "sha256": hashlib.sha256(raw).hexdigest().upper(),
        }

    migrations = []
    for version, description, success, checksum, execution_time in migration_rows:
        stored = bytes(checksum).hex().upper()
        current = migration_sources.get(version)
        migrations.append(
            {
                "version": version,
                "description": description,
                "success": bool(success),
                "stored_checksum_sha384": stored,
                "current_file": current["file"] if current else None,
                "current_checksum_sha384": current["sha384"] if current else None,
                "checksum_matches_current": bool(current and stored == current["sha384"]),
                "execution_time": execution_time,
            }
        )

    table_names = [
        row[0]
        for row in conn.execute(
            "SELECT name FROM sqlite_master "
            "WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        )
    ]
    table_counts = {
        table: conn.execute(f"SELECT COUNT(*) FROM {quote_identifier(table)}").fetchone()[0]
        for table in table_names
        if not table.startswith("cases_fts")
    }

    schema_rows = [
        list(row)
        for row in conn.execute(
            "SELECT type, name, tbl_name, COALESCE(sql,'') FROM sqlite_master "
            "ORDER BY type, name"
        )
    ]

    business_tables = [
        table
        for table in table_names
        if table != "_sqlx_migrations"
        and not table.startswith("device_sync_")
        and not table.startswith("cases_fts")
    ]
    business_table_fingerprints = {
        table: table_fingerprint(conn, table) for table in business_tables
    }
    business_projection_sha256 = sha256_rows(
        [[table, values["rows"], values["sha256"]] for table, values in business_table_fingerprints.items()]
    )

    m63_columns = {
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
    m63_column_presence = {}
    for table, expected_columns in m63_columns.items():
        actual = column_names(conn, table) if object_exists(conn, "table", table) else set()
        m63_column_presence[table] = {column: column in actual for column in expected_columns}

    m63_indexes = [
        "idx_device_sync_quarantine_active_key",
        "idx_device_sync_quarantine_group_status",
        "idx_device_sync_outbox_capture_sequence",
        "idx_device_sync_outbox_pending_capture",
        "idx_device_sync_export_drafts_state",
        "idx_device_sync_export_drafts_one_prepared",
    ]
    m63_index_presence = {index: object_exists(conn, "index", index) for index in m63_indexes}

    quarantine_fks = conn.execute("PRAGMA foreign_key_list(device_sync_quarantine)").fetchall()
    export_draft_fks = (
        conn.execute("PRAGMA foreign_key_list(device_sync_export_drafts)").fetchall()
        if object_exists(conn, "table", "device_sync_export_drafts")
        else []
    )

    result = {
        "snapshot_database": str(database),
        "sqlite": {
            "sqlite_version": sqlite3.sqlite_version,
            "journal_mode": conn.execute("PRAGMA journal_mode").fetchone()[0],
            "page_size": conn.execute("PRAGMA page_size").fetchone()[0],
            "page_count": conn.execute("PRAGMA page_count").fetchone()[0],
            "freelist_count": conn.execute("PRAGMA freelist_count").fetchone()[0],
            "quick_check": quick_check,
            "foreign_key_violation_count": len(foreign_key_rows),
        },
        "migration_summary": {
            "count": len(migrations),
            "max_version": max((row["version"] for row in migrations), default=None),
            "failed_count": sum(not row["success"] for row in migrations),
            "checksum_mismatch_versions": [
                row["version"] for row in migrations if not row["checksum_matches_current"]
            ],
            "unknown_applied_versions": [
                row["version"] for row in migrations if row["current_file"] is None
            ],
            "migration_history_sha256": sha256_rows(
                [
                    [
                        row["version"],
                        row["description"],
                        row["success"],
                        row["stored_checksum_sha384"],
                        row["execution_time"],
                    ]
                    for row in migrations
                ]
            ),
        },
        "migrations": migrations,
        "schema": {
            "object_count": len(schema_rows),
            "sha256": sha256_rows(schema_rows),
        },
        "m63": {
            "applied": any(row["version"] == 63 and row["success"] for row in migrations),
            "export_drafts_table": object_exists(conn, "table", "device_sync_export_drafts"),
            "columns": m63_column_presence,
            "indexes": m63_index_presence,
            "quarantine_group_fk": [list(row) for row in quarantine_fks],
            "export_drafts_group_fk": [list(row) for row in export_draft_fks],
        },
        "table_counts": table_counts,
        "table_counts_sha256": sha256_rows([[table, count] for table, count in table_counts.items()]),
        "business_projection": {
            "excluded": ["_sqlx_migrations", "device_sync_*", "cases_fts*"],
            "table_count": len(business_table_fingerprints),
            "sha256": business_projection_sha256,
            "tables": business_table_fingerprints,
        },
    }
    conn.close()
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
