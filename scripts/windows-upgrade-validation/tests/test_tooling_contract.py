import re
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[3]


class ToolingContractTests(unittest.TestCase):
    def test_ci_and_toolchain_are_pinned_consistently(self):
        toolchain = (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")
        version = re.search(r'channel = "([^"]+)"', toolchain).group(1)
        self.assertEqual(version, "1.96.0")
        for name in ("ci.yml", "build-windows.yml"):
            workflow = (ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8")
            self.assertNotIn("dtolnay/rust-toolchain@stable", workflow)
            self.assertIn(f"dtolnay/rust-toolchain@{version}", workflow)

    def test_r3_orchestrator_has_no_launch_kill_delete_or_install_implementation(self):
        script = (
            ROOT / "scripts" / "windows-upgrade-validation" / "Invoke-UpgradeValidation.ps1"
        ).read_text(encoding="utf-8")
        for forbidden in (
            "Start-Process",
            "Stop-Process",
            "Remove-Item",
            "CredRead",
            "CredWrite",
            "CredDelete",
            "_sqlx_migrations SET",
            "release/latest.json",
        ):
            self.assertNotIn(forbidden, script)
        for required in (
            "ExpectedResumeManifestSha256",
            "RESUME_MANIFEST_HASH_MISMATCH",
            "UNVERIFIED_FORCED_EXIT_CLAIM_REJECTED",
            "SIDECAR_INVALIDATES_PROOF",
            "FORMAL_SWITCH_DISABLED_IN_R3",
            "FORMAL_INSTALL_DISABLED_IN_R3",
            "SWITCH-FORMAL-DATA-BY-ATOMIC-RENAME",
            "INSTALL-VERIFIED-FORMAL-PACKAGE",
            "PATH_ESCAPE_OR_SOURCE_CHILD",
            "TARGET_ALREADY_EXISTS",
            "MANIFEST_HMAC_MISMATCH",
            "PARENT_MANIFEST_HASH_MISMATCH",
            "ARTIFACT_HASH_MISMATCH",
            "RESUME_RUN_ROOT_MISMATCH",
            "PROCESS_ENUMERATION_FAILED",
            "'-wal','-shm','-journal'",
            "RecordExternalRunDbPostcheck",
            "isolated-db-postcheck-recorded",
            "idempotent-db-postcheck-recorded",
            "unverified_external_claim",
            "observed_application_execution = $false",
            "RECORDED_POSTCHECK_NOT_FORMAL_SWITCH_EVIDENCE",
            "RECORDED_POSTCHECK_NOT_INSTALL_EVIDENCE",
        ):
            self.assertIn(required, script)

        for forbidden_claim in (
            "ValidateIsolatedExit",
            "isolated-start-passed",
            "isolated-second-start-passed",
            "first-start-passed",
            "second-start-passed",
            "graceful-exit-passed",
            "observed_application_execution = $true",
        ):
            self.assertNotIn(forbidden_claim, script)

        process_gate = re.search(
            r"function Assert-NoCaseboardProcess \{(.*?)\n\}", script, re.DOTALL
        ).group(1)
        self.assertIn("Get-CimInstance Win32_Process -ErrorAction Stop", process_gate)
        self.assertNotIn("SilentlyContinue", process_gate)

    def test_migration_history_keeps_raw_success_and_installed_on(self):
        audit = (
            ROOT / "scripts" / "windows-upgrade-validation" / "db_audit.py"
        ).read_text(encoding="utf-8")
        self.assertIn('"installed_on": str(installed_on)', audit)
        self.assertIn('"success": int(success)', audit)
        self.assertIn('row["success"] != 1', audit)

    def test_local_cargo_wrapper_enforces_pinned_version(self):
        wrapper = (
            ROOT / "scripts" / "windows-upgrade-validation" / "Invoke-PinnedCargo.ps1"
        ).read_text(encoding="utf-8")
        self.assertIn("stable-x86_64-pc-windows-msvc", wrapper)
        self.assertIn("rustc 1\\.96\\.0", wrapper)


if __name__ == "__main__":
    unittest.main()
