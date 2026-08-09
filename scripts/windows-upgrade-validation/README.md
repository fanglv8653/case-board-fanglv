# Windows upgrade validation (R3)

R3 is a fail-closed, staged backup/audit tool. It does **not** launch, observe,
or kill
CaseBoard, run an installer, switch a formal data directory, delete SQLite
sidecars, edit `_sqlx_migrations`, access credentials/NAS, or publish a release.

## Safety model

`db_audit.py backup` never opens the supplied source database through SQLite.
It:

1. records DB/WAL/SHM/rollback-journal size, mtime and SHA-256;
2. copies every present SQLite file byte-for-byte into a retained staging folder;
3. proves the source files did not change and the copied files match them;
4. opens only the staged copy and uses SQLite online backup;
5. normalizes only the new destination to DELETE journal mode;
6. proves the destination is main-only, `quick_check=ok` and FK-clean; and
7. audits only that main-only copy.

This copy-first design is intentional: merely opening a WAL database in
read-only mode can update SHM lock bytes.

Snapshots include the raw SQLx migration success integer plus version,
description, installed time, checksum and execution time,
migration-history and schema hashes, row-content fingerprints for every
non-device-sync table, a combined non-device-sync projection, migration 0063
sentinels and device-sync safety aggregates. A comparison therefore fails when
row contents change even if every row count remains the same. Idempotent
comparison also fails when an installed time changes or a success value changes
from `1` to another integer such as `2`.

## PowerShell stages

`Invoke-UpgradeValidation.ps1` requires an explicit `-Stage`:

- `Backup`: fail-closed process gate, copy-first SQLite-file backup, main-only
  validation and an integrity-associated `manifest.backup.json`.
- `AuditCopy`: requires the exact SHA-256 of an accepted Backup manifest and
  produces the full copy-only audit plus `manifest.audit.json`.
- `Compare`: compares two snapshot JSON files; add `-Idempotent` to also require
  identical schema, migration history and device-sync safety metrics.
- `RecordExternalRunDbPostcheck`: records only a database postcheck after the
  caller says an external run exited gracefully. `-ExitMode` is stored under
  `unverified_external_claim`; it is not an observed execution fact. The tool
  emits only `isolated-db-postcheck-recorded` or
  `idempotent-db-postcheck-recorded`, never an application start/exit pass.
  A caller-reported forced exit or any WAL/SHM/rollback-journal hard-fails; the
  tool never deletes a sidecar. The first proof must be a separate file under
  the same run root; `-IdempotentPostcheck` binds the next record to that exact
  proof path, prior snapshot and comparison.
- `FormalSwitch` and `Install`: deliberately disabled in R3. Recorded database
  postchecks are explicitly rejected as parent evidence. They retain the
  explicit mutation switch and literal confirmation boundary. Implementation belongs to a
  separate reviewed task.

Every resume consumes both `-ResumeManifest` and
`-ExpectedResumeManifestSha256`. That caller-provided hash is necessary but not
sufficient: each manifest also has a per-run, current-user protected HMAC. The
validator enforces fixed stage/status/filename pairs, the same `run_root`, the
parent-manifest SHA chain, and absolute in-root artifact paths with SHA-256.
Hand-written manifests with a self-computed SHA, cross-run copies, changed
status, and replaced artifacts all fail closed. Evidence files and run
directories are never overwritten.

The DPAPI/HMAC mechanism protects against accidental edits and edits made
without access to the creating Windows user. It is **not** an adversarial trust
root against the same Windows user, because that user can decrypt the run key.
It also does not prove that CaseBoard was launched or observed.

## Example: synthetic or authorized backup source

Do not point these commands at the formal database until the R3 report has
passed independent review and the user has authorized the maintenance window.

```powershell
$script = 'D:\CodexWorkspace\008案件看板应用\case-board-v0.8.3-dev\scripts\windows-upgrade-validation\Invoke-UpgradeValidation.ps1'
$source = 'D:\isolated-input\caseboard.db'
$evidence = 'D:\isolated-evidence'

powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script `
  -Stage Backup `
  -SourceDatabase $source `
  -OutputDirectory $evidence `
  -RunId synthetic-001
```

The command returns the backup manifest path and SHA-256. Supply both to the
next stage:

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script `
  -Stage AuditCopy `
  -ResumeManifest 'D:\isolated-evidence\synthetic-001\manifest.backup.json' `
  -ExpectedResumeManifestSha256 '<64-hex-sha256>' `
  -MigrationsDirectory 'D:\CodexWorkspace\008案件看板应用\case-board-v0.8.3-dev\src-tauri\migrations'
```

Application execution is deliberately outside R3. It must occur in a
disposable VM or new Windows user without formal credentials. Afterwards,
`RecordExternalRunDbPostcheck` can record only a sidecar-free database
postcheck plus the caller's unverified exit-mode claim. It cannot conclude that
the application started, exited gracefully, or passed a run test.
Copy the accepted `main_only_database` artifact to a new proof path under the
same run root before the external first start; the retained backup itself is
never used as the mutable proof database.

## Tests

```powershell
python -m unittest discover -s scripts\windows-upgrade-validation\tests -p 'test_*.py' -v
```

Tests use only temporary synthetic SQLite databases. They cover non-zero WAL
merge with unchanged source files, main-only/FK/quick assertions, raw migration
history semantics, same-count content changes, idempotent sync changes, all
three sidecar types and byte preservation, integrity-associated chained manifests,
manifest/artifact/cross-root tampering, process-enumeration failure, forced-exit
claim rejection, recorded-only postcheck semantics, rejection of recorded
postchecks as formal parent evidence, path escape and existing-target
fail-closed behavior, and static absence of launch/kill/delete/install code.

Rust remains pinned to 1.96.0 in `rust-toolchain.toml` and CI.
`Invoke-PinnedCargo.ps1` is unchanged; R3 does not run Cargo.
