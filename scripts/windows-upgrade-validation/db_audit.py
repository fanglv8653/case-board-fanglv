"""Fail-closed SQLite backup and upgrade comparison helper."""

from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path


RUNTIME_ONLY_TABLES = {
    "deepseek_balance_snapshots",
    "usage_events",
    # The read-only Feishu preview refresh may append runtime cache/audit rows
    # when the application starts. These tables are not local case business
    # records and must remain separate from case/work/stage/contact/link data.
    "feishu_sync_runs",
    "feishu_sync_inbox",
    "feishu_sync_snapshots",
    "feishu_sync_conflicts",
    "feishu_sync_field_previews",
    # 0.7.6 records every idempotent Feishu entity decision here. It is an
    # append-only synchronization audit, not a case/work/stage/contact record.
    "feishu_sync_entity_audits",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def quote_identifier(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def snapshot(db_path: Path) -> dict:
    connection = sqlite3.connect(f"file:{db_path.as_posix()}?mode=ro", uri=True)
    try:
        quick_check = [row[0] for row in connection.execute("PRAGMA quick_check")]
        tables = [
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_master "
                "WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            )
        ]
        counts = {
            table: connection.execute(
                f"SELECT COUNT(*) FROM {quote_identifier(table)}"
            ).fetchone()[0]
            for table in tables
        }
        migrations = []
        if "_sqlx_migrations" in tables:
            migrations = [
                {"version": row[0], "description": row[1], "success": row[2]}
                for row in connection.execute(
                    "SELECT version, description, success "
                    "FROM _sqlx_migrations ORDER BY version"
                )
            ]
        return {
            "path": str(db_path),
            "bytes": db_path.stat().st_size,
            "sha256": sha256(db_path),
            "wal": file_fact(Path(f"{db_path}-wal")),
            "shm": file_fact(Path(f"{db_path}-shm")),
            "quick_check": quick_check,
            "table_counts": counts,
            "migrations": migrations,
        }
    finally:
        connection.close()


def file_fact(path: Path) -> dict | None:
    if not path.exists():
        return None
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha256(path)}


def online_backup(source: Path, destination: Path) -> None:
    if destination.exists():
        raise FileExistsError(f"refusing to overwrite backup: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    source_connection = sqlite3.connect(f"file:{source.as_posix()}?mode=ro", uri=True)
    destination_connection = sqlite3.connect(destination)
    try:
        source_connection.backup(destination_connection)
    finally:
        destination_connection.close()
        source_connection.close()
    if snapshot(destination)["quick_check"] != ["ok"]:
        destination.unlink(missing_ok=True)
        raise RuntimeError("backup quick_check failed")


def compare(before: dict, after: dict) -> dict:
    before_counts = before["table_counts"]
    after_counts = after["table_counts"]
    shared = sorted(set(before_counts) & set(after_counts) - {"_sqlx_migrations"})
    changes = {
        table: {"before": before_counts[table], "after": after_counts[table]}
        for table in shared
        if before_counts[table] != after_counts[table]
    }
    runtime_changes = {k: v for k, v in changes.items() if k in RUNTIME_ONLY_TABLES}
    business_changes = {k: v for k, v in changes.items() if k not in RUNTIME_ONLY_TABLES}
    failed_migrations = [m for m in after["migrations"] if m["success"] != 1]
    checks = {
        "quick_check_ok": after["quick_check"] == ["ok"],
        "no_existing_business_table_count_changes": not business_changes,
        "no_failed_migrations": not failed_migrations,
        "no_removed_tables": not (set(before_counts) - set(after_counts)),
    }
    return {
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "business_table_changes": business_changes,
        "runtime_snapshot_changes": runtime_changes,
        "new_tables": sorted(set(after_counts) - set(before_counts)),
        "removed_tables": sorted(set(before_counts) - set(after_counts)),
        "migration_versions_before": [m["version"] for m in before["migrations"]],
        "migration_versions_after": [m["version"] for m in after["migrations"]],
        "wal_state_after": after["wal"],
    }


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    snapshot_parser = subparsers.add_parser("snapshot")
    snapshot_parser.add_argument("--db", required=True)
    snapshot_parser.add_argument("--output", required=True)
    backup_parser = subparsers.add_parser("backup")
    backup_parser.add_argument("--source", required=True)
    backup_parser.add_argument("--destination", required=True)
    backup_parser.add_argument("--output", required=True)
    compare_parser = subparsers.add_parser("compare")
    compare_parser.add_argument("--before", required=True)
    compare_parser.add_argument("--after", required=True)
    compare_parser.add_argument("--output", required=True)
    args = parser.parse_args()

    if args.command == "snapshot":
        db = Path(args.db).resolve(strict=True)
        result = {"captured_at_utc": datetime.now(timezone.utc).isoformat(), "snapshot": snapshot(db)}
    elif args.command == "backup":
        source = Path(args.source).resolve(strict=True)
        destination = Path(args.destination).resolve()
        online_backup(source, destination)
        result = {
            "captured_at_utc": datetime.now(timezone.utc).isoformat(),
            "source": snapshot(source),
            "backup": snapshot(destination),
        }
    else:
        before = json.loads(Path(args.before).read_text(encoding="utf-8-sig"))["snapshot"]
        after = json.loads(Path(args.after).read_text(encoding="utf-8-sig"))["snapshot"]
        result = compare(before, after)
    write_json(Path(args.output), result)
    print(json.dumps({"status": result.get("status", "captured")}, ensure_ascii=False))
    return 2 if result.get("status") == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
