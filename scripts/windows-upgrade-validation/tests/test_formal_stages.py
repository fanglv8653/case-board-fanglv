import hashlib
import importlib.util
import json
import os
import shutil
import sqlite3
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[3]
SCRIPT = ROOT / "scripts" / "windows-upgrade-validation" / "Invoke-UpgradeValidation.ps1"
AUDIT_PATH = SCRIPT.parent / "db_audit.py"
SPEC = importlib.util.spec_from_file_location("db_audit_for_stages", AUDIT_PATH)
db_audit = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(db_audit)


def manifest_hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def artifact_path(manifest: dict, name: str) -> Path:
    matches = [item for item in manifest["artifacts"] if item["name"] == name]
    if len(matches) != 1:
        raise AssertionError(f"expected one artifact named {name}, got {matches}")
    return Path(matches[0]["path"])


class FormalStageTests(unittest.TestCase):
    def make_wal_db(self, path: Path):
        path.parent.mkdir(parents=True)
        connection = sqlite3.connect(path)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("PRAGMA wal_autocheckpoint=0")
        connection.executescript(
            "CREATE TABLE cases(id INTEGER PRIMARY KEY,name TEXT NOT NULL);"
            "CREATE TABLE _sqlx_migrations("
            "version INTEGER PRIMARY KEY,description TEXT NOT NULL,installed_on TEXT NOT NULL,"
            "success INTEGER NOT NULL,checksum BLOB NOT NULL,execution_time INTEGER NOT NULL);"
        )
        connection.execute("INSERT INTO cases VALUES(1,'synthetic')")
        connection.execute(
            "INSERT INTO _sqlx_migrations VALUES(1,'initial','2026-01-01',1,?,10)",
            (bytes(range(48)),),
        )
        connection.commit()
        return connection

    def invoke(self, *arguments: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        process_env = os.environ.copy()
        if env:
            process_env.update(env)
        return subprocess.run(
            [
                "powershell.exe",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                str(SCRIPT),
                *arguments,
            ],
            cwd=ROOT,
            env=process_env,
            text=True,
            capture_output=True,
            timeout=30,
        )

    def backup_and_audit(self, source: Path, output: Path):
        backup = self.invoke(
            "-Stage", "Backup",
            "-SourceDatabase", str(source),
            "-OutputDirectory", str(output),
            "-RunId", "synthetic",
        )
        self.assertEqual(backup.returncode, 0, backup.stdout + backup.stderr)
        run_root = output / "synthetic"
        backup_manifest = run_root / "manifest.backup.json"
        audit = self.invoke(
            "-Stage", "AuditCopy",
            "-ResumeManifest", str(backup_manifest),
            "-ExpectedResumeManifestSha256", manifest_hash(backup_manifest),
        )
        self.assertEqual(audit.returncode, 0, audit.stdout + audit.stderr)
        return run_root, backup_manifest, run_root / "manifest.audit.json"

    def test_external_db_postchecks_are_recorded_not_application_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source" / "caseboard.db"
            output = root / "evidence"
            writer = self.make_wal_db(source)
            try:
                before = db_audit.trio_facts(source)
                run_root, backup_manifest, audit_manifest = self.backup_and_audit(source, output)
                self.assertEqual(before, db_audit.trio_facts(source))
                backup_value = json.loads(backup_manifest.read_text(encoding="utf-8-sig"))
                main_only = artifact_path(backup_value, "main_only_database")
                for suffix in ("-wal", "-shm", "-journal"):
                    self.assertFalse(Path(f"{main_only}{suffix}").exists())

                proof_db = run_root / "05-isolated-proof" / "caseboard.db"
                proof_db.parent.mkdir()
                shutil.copy2(main_only, proof_db)
                forced = self.invoke(
                    "-Stage", "RecordExternalRunDbPostcheck",
                    "-ResumeManifest", str(audit_manifest),
                    "-ExpectedResumeManifestSha256", manifest_hash(audit_manifest),
                    "-ProofDatabase", str(proof_db),
                    "-ExitMode", "forced",
                )
                self.assertNotEqual(forced.returncode, 0)
                self.assertIn("UNVERIFIED_FORCED_EXIT_CLAIM_REJECTED", forced.stdout + forced.stderr)

                first = self.invoke(
                    "-Stage", "RecordExternalRunDbPostcheck",
                    "-ResumeManifest", str(audit_manifest),
                    "-ExpectedResumeManifestSha256", manifest_hash(audit_manifest),
                    "-ProofDatabase", str(proof_db),
                    "-ExitMode", "graceful",
                )
                self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
                self.assertNotIn("passed", first.stdout.lower())
                first_manifest = run_root / "manifest.isolated-db-postcheck.json"
                first_value = json.loads(first_manifest.read_text(encoding="utf-8-sig"))
                self.assertEqual(first_value["stage"], "RecordExternalRunDbPostcheck")
                self.assertEqual(first_value["status"], "isolated-db-postcheck-recorded")
                self.assertFalse(first_value["observed_application_execution"])
                self.assertEqual(
                    first_value["unverified_external_claim"],
                    {"exit_mode": "graceful", "asserted_by": "caller"},
                )
                second = self.invoke(
                    "-Stage", "RecordExternalRunDbPostcheck",
                    "-ResumeManifest", str(first_manifest),
                    "-ExpectedResumeManifestSha256", manifest_hash(first_manifest),
                    "-ProofDatabase", str(proof_db),
                    "-ExitMode", "graceful",
                    "-IdempotentPostcheck",
                )
                self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
                self.assertNotIn("passed", second.stdout.lower())
                second_manifest = run_root / "manifest.idempotent-db-postcheck.json"
                self.assertTrue(second_manifest.is_file())
                second_value = json.loads(second_manifest.read_text(encoding="utf-8-sig"))
                self.assertEqual(second_value["status"], "idempotent-db-postcheck-recorded")
                self.assertFalse(second_value["observed_application_execution"])
                self.assertTrue(
                    os.path.samefile(second_value["parent_manifest"], first_manifest),
                    "parent manifest must resolve to the exact prior evidence file",
                )
                self.assertEqual(
                    artifact_path(second_value, "proof_database"), proof_db
                )

                formal = self.invoke(
                    "-Stage", "FormalSwitch",
                    "-ResumeManifest", str(second_manifest),
                    "-ExpectedResumeManifestSha256", manifest_hash(second_manifest),
                    "-AllowFormalMutation",
                    "-ConfirmFormalMutation", "SWITCH-FORMAL-DATA-BY-ATOMIC-RENAME",
                )
                self.assertNotEqual(formal.returncode, 0)
                self.assertIn(
                    "RECORDED_POSTCHECK_NOT_FORMAL_SWITCH_EVIDENCE",
                    formal.stdout + formal.stderr,
                )

                install = self.invoke(
                    "-Stage", "Install",
                    "-ResumeManifest", str(second_manifest),
                    "-AllowFormalMutation",
                    "-ConfirmFormalMutation", "INSTALL-VERIFIED-FORMAL-PACKAGE",
                )
                self.assertNotEqual(install.returncode, 0)
                self.assertIn(
                    "RECORDED_POSTCHECK_NOT_INSTALL_EVIDENCE",
                    install.stdout + install.stderr,
                )
            finally:
                writer.close()

    def test_all_sidecars_are_preserved_and_rejected(self):
        for suffix in ("-wal", "-shm", "-journal"):
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / "source" / "caseboard.db"
                output = root / "evidence"
                writer = self.make_wal_db(source)
                try:
                    run_root, backup_manifest, audit_manifest = self.backup_and_audit(source, output)
                    main_only = artifact_path(
                        json.loads(backup_manifest.read_text(encoding="utf-8-sig")),
                        "main_only_database",
                    )
                    proof_db = run_root / "05-isolated-proof" / "caseboard.db"
                    proof_db.parent.mkdir()
                    shutil.copy2(main_only, proof_db)
                    sidecar = Path(f"{proof_db}{suffix}")
                    marker = f"synthetic-{suffix}-must-survive".encode()
                    sidecar.write_bytes(marker)
                    rejected = self.invoke(
                        "-Stage", "RecordExternalRunDbPostcheck",
                        "-ResumeManifest", str(audit_manifest),
                        "-ExpectedResumeManifestSha256", manifest_hash(audit_manifest),
                        "-ProofDatabase", str(proof_db),
                        "-ExitMode", "graceful",
                    )
                    self.assertNotEqual(rejected.returncode, 0)
                    self.assertIn("SIDECAR_INVALIDATES_PROOF", rejected.stdout + rejected.stderr)
                    self.assertEqual(sidecar.read_bytes(), marker)
                finally:
                    writer.close()

    def test_handwritten_manifest_with_self_hash_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            run_root = Path(directory) / "forged"
            run_root.mkdir()
            fake_db = run_root / "fake.db"
            fake_db.write_bytes(b"not sqlite")
            manifest = run_root / "manifest.backup.json"
            value = {
                "schema_version": 2,
                "stage": "Backup",
                "status": "backup-passed",
                "run_root": str(run_root),
                "artifacts": [{
                    "name": "main_only_database",
                    "path": str(fake_db),
                    "sha256": hashlib.sha256(fake_db.read_bytes()).hexdigest().upper(),
                }],
            }
            manifest.write_text(json.dumps(value), encoding="utf-8")
            result = self.invoke(
                "-Stage", "AuditCopy",
                "-ResumeManifest", str(manifest),
                "-ExpectedResumeManifestSha256", manifest_hash(manifest),
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("MANIFEST_HMAC_MISSING", result.stdout + result.stderr)

    def test_status_path_and_content_tampering_fail_closed(self):
        mutations = ("status", "artifact-path", "artifact-content")
        for mutation in mutations:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / "source" / "caseboard.db"
                output = root / "evidence"
                writer = self.make_wal_db(source)
                try:
                    backup = self.invoke(
                        "-Stage", "Backup", "-SourceDatabase", str(source),
                        "-OutputDirectory", str(output), "-RunId", "synthetic",
                    )
                    self.assertEqual(backup.returncode, 0, backup.stdout + backup.stderr)
                    manifest = output / "synthetic" / "manifest.backup.json"
                    value = json.loads(manifest.read_text(encoding="utf-8-sig"))
                    if mutation == "status":
                        value["status"] = "audit-passed"
                        manifest.write_text(json.dumps(value), encoding="utf-8")
                    elif mutation == "artifact-path":
                        replacement = output / "synthetic" / "replacement.db"
                        replacement.write_bytes(artifact_path(value, "main_only_database").read_bytes())
                        next(item for item in value["artifacts"] if item["name"] == "main_only_database")["path"] = str(replacement)
                        manifest.write_text(json.dumps(value), encoding="utf-8")
                    else:
                        artifact_path(value, "main_only_database").write_bytes(b"replaced-content")
                    result = self.invoke(
                        "-Stage", "AuditCopy", "-ResumeManifest", str(manifest),
                        "-ExpectedResumeManifestSha256", manifest_hash(manifest),
                    )
                    self.assertNotEqual(result.returncode, 0)
                    combined = result.stdout + result.stderr
                    if mutation == "artifact-content":
                        self.assertIn("ARTIFACT_HASH_MISMATCH", combined)
                    else:
                        self.assertIn("MANIFEST_HMAC_MISMATCH", combined)
                finally:
                    writer.close()

    def test_cross_run_root_and_parent_manifest_tampering_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source" / "caseboard.db"
            output = root / "evidence"
            writer = self.make_wal_db(source)
            try:
                run_root, backup_manifest, audit_manifest = self.backup_and_audit(source, output)
                copied_root = output / "other-run"
                shutil.copytree(run_root, copied_root)
                copied_manifest = copied_root / "manifest.backup.json"
                cross = self.invoke(
                    "-Stage", "AuditCopy", "-ResumeManifest", str(copied_manifest),
                    "-ExpectedResumeManifestSha256", manifest_hash(copied_manifest),
                )
                self.assertNotEqual(cross.returncode, 0)
                self.assertIn("RESUME_RUN_ROOT_MISMATCH", cross.stdout + cross.stderr)

                backup_manifest.write_bytes(backup_manifest.read_bytes() + b" ")
                audit_value = json.loads(audit_manifest.read_text(encoding="utf-8-sig"))
                proof_db = run_root / "proof" / "caseboard.db"
                proof_db.parent.mkdir()
                shutil.copy2(artifact_path(audit_value, "main_only_database"), proof_db)
                parent = self.invoke(
                    "-Stage", "RecordExternalRunDbPostcheck", "-ResumeManifest", str(audit_manifest),
                    "-ExpectedResumeManifestSha256", manifest_hash(audit_manifest),
                    "-ProofDatabase", str(proof_db), "-ExitMode", "graceful",
                )
                self.assertNotEqual(parent.returncode, 0)
                self.assertIn("PARENT_MANIFEST_HASH_MISMATCH", parent.stdout + parent.stderr)
            finally:
                writer.close()

    def test_process_enumeration_failure_stops_before_run_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source" / "caseboard.db"
            output = root / "evidence"
            writer = self.make_wal_db(source)
            try:
                result = self.invoke(
                    "-Stage", "Backup", "-SourceDatabase", str(source),
                    "-OutputDirectory", str(output), "-RunId", "synthetic",
                    env={"CASEBOARD_TEST_FORCE_PROCESS_ENUMERATION_FAILURE": "1"},
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("PROCESS_ENUMERATION_FAILED", result.stdout + result.stderr)
                self.assertFalse((output / "synthetic").exists())
            finally:
                writer.close()

    def test_path_escape_and_existing_target_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source" / "caseboard.db"
            writer = self.make_wal_db(source)
            try:
                escaped = self.invoke(
                    "-Stage", "Backup", "-SourceDatabase", str(source),
                    "-OutputDirectory", str(source.parent / "evidence"), "-RunId", "x",
                )
                self.assertNotEqual(escaped.returncode, 0)
                self.assertIn("PATH_ESCAPE_OR_SOURCE_CHILD", escaped.stdout + escaped.stderr)

                output = root / "evidence"
                (output / "fixed").mkdir(parents=True)
                existing = self.invoke(
                    "-Stage", "Backup", "-SourceDatabase", str(source),
                    "-OutputDirectory", str(output), "-RunId", "fixed",
                )
                self.assertNotEqual(existing.returncode, 0)
                self.assertIn("TARGET_ALREADY_EXISTS", existing.stdout + existing.stderr)
            finally:
                writer.close()


if __name__ == "__main__":
    unittest.main()
