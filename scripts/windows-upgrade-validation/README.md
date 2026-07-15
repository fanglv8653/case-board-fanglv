# Windows upgrade validation

`Invoke-UpgradeValidation.ps1` has three explicit modes:

- **dry-run** (default): read-only source snapshot and path/tool preflight; no app or installer launch.
- **isolated** (`-RunIsolatedUpgrade`): SQLite online backup to a new run directory, isolated app launch, screenshot, cleanup and before/after comparison.
- **formal** (`-RunIsolatedUpgrade -Install`): isolated gate first, then another verified online backup, installer hash check, silent install, executable/registry version checks, formal startup screenshot with a separate WebView2 profile, database comparison and process cleanup.

It rejects the canonical formal database unless all formal-install gates are explicitly supplied.

Dry-run example:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows-upgrade-validation/Invoke-UpgradeValidation.ps1 `
  -SourceDatabase D:\isolated\caseboard.db `
  -OutputDirectory D:\evidence `
  -PythonPath D:\tools\python.exe
```

Add `-RunIsolatedUpgrade -AppExecutable <absolute exe> -AsciiTempRoot
D:\CodexWorkspace\tmp\caseboard-capture` to create an SQLite online
backup, launch only against isolated `CASEBOARD_DATA_DIR` and
`WEBVIEW2_USER_DATA_FOLDER`, capture the window, stop the process and compare the
database. Formal installation additionally requires `-Install`, the exact canonical
database path, installer SHA-256, installed executable and the literal confirmation
`INSTALL-AND-MIGRATE-FORMAL-DATABASE`. Formal mode also requires the expected installed
version and records both executable and uninstall-registry versions. Never put signing
secrets on this command line.

The JSON/Markdown evidence deliberately separates existing **business-table row-count
changes** (fail closed) from known **runtime snapshot changes** such as DeepSeek balance
snapshots (reported but not treated as case-data changes). Migration success, removed/new
tables and the post-run WAL file state are recorded separately. A WAL file hash is not a
claim that the database main-file bytes are unchanged.

`capture-window.ps1` accepts `-AsciiTempRoot` (or
`CASEBOARD_CAPTURE_TEMP_ROOT`). The orchestrator always points it inside the D-drive run
directory and invokes Windows PowerShell with `-ExecutionPolicy Bypass`. If no explicit
root is supplied and the ambient TEMP contains non-ASCII characters, capture fails closed.

Rust is pinned to 1.96.0 in both `rust-toolchain.toml` and CI. On this Windows host,
`Invoke-PinnedCargo.ps1` selects the already-installed stable MSVC toolchain, verifies
that its actual `rustc` version is exactly 1.96.0, and only then forwards Cargo arguments.
This avoids rustup's network synchronization check while still failing if local stable
has drifted away from the pinned version.
