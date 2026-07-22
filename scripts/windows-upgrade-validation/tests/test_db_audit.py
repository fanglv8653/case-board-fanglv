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
    def make_db(self, path: Path, cases: int = 1, snapshots: int = 0):
        connection = sqlite3.connect(path)
        connection.executescript(
            "CREATE TABLE cases(id INTEGER PRIMARY KEY);"
            "CREATE TABLE deepseek_balance_snapshots(id INTEGER PRIMARY KEY);"
            "CREATE TABLE _sqlx_migrations(version INTEGER, description TEXT, success INTEGER);"
        )
        connection.executemany("INSERT INTO cases DEFAULT VALUES", [()] * cases)
        connection.executemany("INSERT INTO deepseek_balance_snapshots DEFAULT VALUES", [()] * snapshots)
        connection.execute("INSERT INTO _sqlx_migrations VALUES(1, 'initial', 1)")
        connection.commit()
        connection.close()

    def test_online_backup_and_runtime_change_classification(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.db"
            backup = Path(directory) / "backup.db"
            self.make_db(source)
            db_audit.online_backup(source, backup)
            before = db_audit.snapshot(backup)
            connection = sqlite3.connect(backup)
            connection.execute("INSERT INTO deepseek_balance_snapshots DEFAULT VALUES")
            connection.commit()
            connection.close()
            result = db_audit.compare(before, db_audit.snapshot(backup))
            self.assertEqual(result["status"], "passed")
            self.assertIn("deepseek_balance_snapshots", result["runtime_snapshot_changes"])

    def test_business_change_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            self.make_db(path)
            before = db_audit.snapshot(path)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO cases DEFAULT VALUES")
            connection.commit()
            connection.close()
            result = db_audit.compare(before, db_audit.snapshot(path))
            self.assertEqual(result["status"], "failed")
            self.assertIn("cases", result["business_table_changes"])

    def test_feishu_entity_audit_growth_is_runtime_only(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "caseboard.db"
            self.make_db(path)
            connection = sqlite3.connect(path)
            connection.execute("CREATE TABLE feishu_sync_entity_audits(id INTEGER PRIMARY KEY)")
            connection.commit()
            connection.close()
            before = db_audit.snapshot(path)
            connection = sqlite3.connect(path)
            connection.execute("INSERT INTO feishu_sync_entity_audits DEFAULT VALUES")
            connection.commit()
            connection.close()
            result = db_audit.compare(before, db_audit.snapshot(path))
            self.assertEqual(result["status"], "passed")
            self.assertIn("feishu_sync_entity_audits", result["runtime_snapshot_changes"])

    def test_backup_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.db"
            backup = Path(directory) / "backup.db"
            self.make_db(source)
            backup.touch()
            with self.assertRaises(FileExistsError):
                db_audit.online_backup(source, backup)


if __name__ == "__main__":
    unittest.main()
