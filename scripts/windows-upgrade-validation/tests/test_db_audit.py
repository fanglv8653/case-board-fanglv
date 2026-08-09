import importlib.util
import sqlite3
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).parents[1] / "db_audit.py"
SPEC = importlib.util.spec_from_file_location("db_audit", MODULE_PATH)
db_audit = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(db_audit)


class DbAuditTests(unittest.TestCase):
    def make_db(self, path: Path, cases: tuple[tuple[int, str], ...] = ((1, "alpha"),)):
        connection = sqlite3.connect(path)
        connection.execute("PRAGMA foreign_keys=ON")
        connection.executescript(
            "CREATE TABLE cases(id INTEGER PRIMARY KEY,name TEXT NOT NULL);"
            "CREATE TABLE case_items(id INTEGER PRIMARY KEY,case_id INTEGER NOT NULL "
            "REFERENCES cases(id));"
            "CREATE TABLE device_sync_groups(id TEXT PRIMARY KEY,paused INTEGER NOT NULL);"
            "CREATE TABLE device_sync_outbox(id TEXT PRIMARY KEY,state TEXT NOT NULL);"
            "CREATE TABLE device_sync_quarantine(id TEXT PRIMARY KEY);"
            "CREATE TABLE _sqlx_migrations("
            "version INTEGER PRIMARY KEY,description TEXT NOT NULL,installed_on TEXT NOT NULL,"
            "success INTEGER NOT NULL,checksum BLOB NOT NULL,execution_time INTEGER NOT NULL);"
        )
        connection.executemany("INSERT INTO cases(id,name) VALUES(?,?)", cases)
        connection.execute("INSERT INTO case_items VALUES(1,1)")
        connection.execute("INSERT INTO device_sync_groups VALUES('g1',1)")
        connection.execute("INSERT INTO device_sync_outbox VALUES('o1','exported')")
        connection.execute("INSERT INTO device_sync_quarantine VALUES('q1')")
        connection.execute(
            "INSERT INTO _sqlx_migrations VALUES(1,'initial','2026-01-01',1,?,10)",
            (bytes(range(48)),),
        )
        connection.commit()
        return connection

    def test_online_backup_merges_nonzero_wal_and_preserves_source_trio(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.db"
            backup = Path(directory) / "backup.db"
            writer = self.make_db(source)
            self.assertEqual(writer.execute("PRAGMA journal_mode=WAL").fetchone()[0], "wal")
            writer.execute("PRAGMA wal_autocheckpoint=0")
            writer.execute("INSERT INTO cases VALUES(2,'wal-row')")
            writer.commit()
            try:
                self.assertGreater(Path(f"{source}-wal").stat().st_size, 0)
                result = db_audit.online_backup(source, backup)
                self.assertEqual(result["status"], "backup-passed")
                self.assertEqual(result["source_trio_before"], result["source_trio_after"])
                self.assertTrue(result["checks"]["destination_main_only"])
                self.assertFalse(Path(f"{backup}-wal").exists())
                self.assertFalse(Path(f"{backup}-shm").exists())
                self.assertEqual(result["backup"]["sqlite"]["quick_check"], ["ok"])
                self.assertEqual(result["backup"]["sqlite"]["foreign_key_violation_count"], 0)
                self.assertEqual(result["backup"]["table_counts"]["cases"], 2)
            finally:
                writer.close()

    def test_same_count_different_content_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            connection = self.make_db(path)
            connection.close()
            before = db_audit.snapshot(path)
            connection = sqlite3.connect(path)
            connection.execute("UPDATE cases SET name='changed' WHERE id=1")
            connection.commit()
            connection.close()
            after = db_audit.snapshot(path)
            self.assertEqual(before["table_counts"]["cases"], after["table_counts"]["cases"])
            result = db_audit.compare(before, after)
            self.assertEqual(result["status"], "failed")
            self.assertIn("cases", result["business_content_changes"])

    def test_device_sync_content_is_separate_from_business_projection(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            connection = self.make_db(path)
            connection.close()
            before = db_audit.snapshot(path)
            connection = sqlite3.connect(path)
            connection.execute("UPDATE device_sync_outbox SET state='pending' WHERE id='o1'")
            connection.commit()
            connection.close()
            after = db_audit.snapshot(path)
            result = db_audit.compare(before, after)
            self.assertEqual(result["status"], "passed")
            self.assertEqual(
                before["business_projection"]["sha256"], after["business_projection"]["sha256"]
            )
            self.assertNotEqual(before["sync_safety"], after["sync_safety"])

    def test_idempotent_comparison_rejects_sync_or_schema_change(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            connection = self.make_db(path)
            connection.close()
            before = db_audit.snapshot(path)
            connection = sqlite3.connect(path)
            connection.execute("UPDATE device_sync_outbox SET state='pending' WHERE id='o1'")
            connection.commit()
            connection.close()
            result = db_audit.compare(before, db_audit.snapshot(path), idempotent=True)
            self.assertEqual(result["status"], "failed")
            self.assertFalse(result["checks"]["sync_safety_unchanged"])

    def test_snapshot_rejects_sidecar_instead_of_cleaning_it(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            connection = self.make_db(path)
            connection.execute("PRAGMA journal_mode=WAL")
            connection.execute("INSERT INTO cases VALUES(2,'pending')")
            connection.commit()
            self.assertTrue(Path(f"{path}-wal").exists())
            with self.assertRaisesRegex(RuntimeError, "MAIN_ONLY_SIDECAR_PRESENT"):
                db_audit.snapshot(path)
            self.assertTrue(Path(f"{path}-wal").exists())
            connection.close()

    def test_backup_refuses_existing_target_or_sidecar(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.db"
            connection = self.make_db(source)
            connection.close()
            backup = Path(directory) / "backup.db"
            backup.touch()
            with self.assertRaises(FileExistsError):
                db_audit.online_backup(source, backup)
            backup.unlink()
            Path(f"{backup}-wal").touch()
            with self.assertRaises(FileExistsError):
                db_audit.online_backup(source, backup)
            Path(f"{backup}-wal").unlink()
            journal = Path(f"{backup}-journal")
            marker = b"preexisting-journal-must-survive"
            journal.write_bytes(marker)
            with self.assertRaises(FileExistsError):
                db_audit.online_backup(source, backup)
            self.assertEqual(journal.read_bytes(), marker)

    def test_migration_tuple_and_hashes_are_captured(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            connection = self.make_db(path)
            connection.close()
            result = db_audit.snapshot(path)
            migration = result["migrations"][0]
            self.assertEqual(migration["version"], 1)
            self.assertEqual(migration["description"], "initial")
            self.assertEqual(migration["execution_time"], 10)
            self.assertEqual(len(migration["stored_checksum_sha384"]), 96)
            self.assertEqual(len(result["schema"]["sha256"]), 64)
            self.assertEqual(len(result["migration_summary"]["history_sha256"]), 64)
            self.assertEqual(len(result["business_projection"]["sha256"]), 64)

    def test_migration_success_and_installed_on_keep_raw_history_semantics(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            connection = self.make_db(path)
            connection.close()
            before = db_audit.snapshot(path)
            self.assertEqual(before["migrations"][0]["success"], 1)
            connection = sqlite3.connect(path)
            connection.execute(
                "UPDATE _sqlx_migrations SET success=2,installed_on='2026-02-02' WHERE version=1"
            )
            connection.commit()
            connection.close()
            after = db_audit.snapshot(path)
            self.assertEqual(after["migrations"][0]["success"], 2)
            self.assertNotEqual(
                before["migration_summary"]["history_sha256"],
                after["migration_summary"]["history_sha256"],
            )
            result = db_audit.compare(before, after, idempotent=True)
            self.assertEqual(result["status"], "failed")
            self.assertFalse(result["checks"]["migration_history_unchanged"])

    def test_snapshot_rejects_rollback_journal_without_deleting_it(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            connection = self.make_db(path)
            connection.close()
            journal = Path(f"{path}-journal")
            marker = b"rollback-journal-evidence"
            journal.write_bytes(marker)
            with self.assertRaisesRegex(RuntimeError, "MAIN_ONLY_SIDECAR_PRESENT"):
                db_audit.snapshot(path)
            self.assertEqual(journal.read_bytes(), marker)


if __name__ == "__main__":
    unittest.main()
