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

    def test_formal_install_has_hard_gates(self):
        script = (ROOT / "scripts/windows-upgrade-validation/Invoke-UpgradeValidation.ps1").read_text(encoding="utf-8")
        for required in (
            "INSTALL-AND-MIGRATE-FORMAL-DATABASE",
            "ExpectedInstallerSha256",
            "Formal database is forbidden",
            "-Install requires a successful isolated upgrade",
            "WEBVIEW2_USER_DATA_FOLDER",
            "ExecutionPolicy Bypass",
            "ExpectedInstalledVersion",
            "registry_display_version",
            "formal-webview-data",
            "Resolve-Python",
            "-AsciiTempRoot is required",
            "MinimumScreenshotBytes",
        ):
            self.assertIn(required, script)

    def test_local_cargo_wrapper_enforces_pinned_version(self):
        wrapper = (ROOT / "scripts/windows-upgrade-validation/Invoke-PinnedCargo.ps1").read_text(encoding="utf-8")
        self.assertIn("stable-x86_64-pc-windows-msvc", wrapper)
        self.assertIn("rustc 1\\.96\\.0", wrapper)


if __name__ == "__main__":
    unittest.main()
