//! Read-only migration-lineage preflight. Existing databases must pass this
//! module before a read-write/WAL pool is created.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use std::fs::{self, File, OpenOptions};
#[cfg(target_os = "windows")]
use std::io::{self, BufReader, Read, Write};
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
use sha2::{Digest, Sha256};
use sqlx::migrate::{Migration, MigrationType};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use super::{
    DbError, DbMigrationCompatibilityError, DB_MIGRATION_APPLIED_VERSION_UNKNOWN,
    DB_MIGRATION_CHECKSUM_UNKNOWN, DB_MIGRATION_LINEAGE_INCOMPATIBLE,
    DB_MIGRATION_SCHEMA_SENTINEL_MISSING,
};

#[cfg(target_os = "windows")]
unsafe extern "C" {
    #[link_name = "sqlite3_db_config"]
    fn caseboard_sqlite3_db_config(
        database: *mut std::ffi::c_void,
        operation: std::ffi::c_int,
        ...
    ) -> std::ffi::c_int;
}

#[cfg(target_os = "windows")]
const SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE: std::ffi::c_int = 1006;

#[derive(Debug)]
struct MissingSentinel {
    migration_version: i64,
    code: &'static str,
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct TableColumnDefinition {
    cid: i64,
    name: String,
    data_type: String,
    not_null: i64,
    default_value: Option<String>,
    primary_key_order: i64,
    hidden: i64,
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct TableListDefinition {
    object_type: String,
    column_count: i64,
    without_rowid: i64,
    strict: i64,
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct IndexListDefinition {
    sequence: i64,
    name: String,
    unique: i64,
    origin: String,
    partial: i64,
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct IndexColumnDefinition {
    sequence: i64,
    column_id: i64,
    name: Option<String>,
    descending: i64,
    collation: Option<String>,
    is_key: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MigrationPreflight {
    pub(crate) allow_missing_legacy_migration_36: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WalRecoveryBackup {
    pub(crate) directory: PathBuf,
    pub(crate) database_sha256: String,
    pub(crate) wal_sha256: String,
    pub(crate) shm_sha256: String,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnapshot {
    len: u64,
    sha256: String,
}

const LEGACY_MIGRATION_36_VERSION: i64 = 36;
const LEGACY_MIGRATION_36_DESCRIPTION: &str = "feishu reminder runs";
static LEGACY_MIGRATION_36_CHECKSUM: [u8; 48] = [
    0x84, 0xf8, 0x59, 0x10, 0x24, 0x47, 0xac, 0xb5, 0xdb, 0xee, 0x9e, 0x17, 0x9a, 0x0a, 0xe3, 0x49,
    0x3d, 0x7e, 0xd2, 0x48, 0x3b, 0x28, 0xa4, 0x47, 0xbd, 0xf0, 0xf4, 0xf9, 0x36, 0x0c, 0xc2, 0x39,
    0x9f, 0xc1, 0xf7, 0xaa, 0x08, 0xcb, 0xa2, 0xf0, 0xbe, 0x50, 0xf4, 0x44, 0xf4, 0x84, 0x14, 0x80,
];
const LEGACY_MIGRATION_36_SCHEMA_SENTINEL: &str = "M36.table.feishu_reminder_runs.exact_definition";
const LEGACY_MIGRATION_36_PRIMARY_KEY_INDEX: &str = "sqlite_autoindex_feishu_reminder_runs_1";
const LEGACY_MIGRATION_36_TABLE_SQL: &str = r#"
CREATE TABLE feishu_reminder_runs (
    sent_date TEXT PRIMARY KEY NOT NULL,
    sent_at TEXT NOT NULL DEFAULT (datetime('now')),
    item_count INTEGER NOT NULL DEFAULT 0
)
"#;
const LEGACY_MIGRATION_36_NEVER_EXECUTE_SQL: &str = r#"
SELECT FROM;
"#;

const M63_QUARANTINE_TABLE_SQL: &str = r#"
CREATE TABLE device_sync_quarantine (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT,
    source_path TEXT,
    source_device_id TEXT NOT NULL,
    source_sequence INTEGER NOT NULL,
    reason_code TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active','resolved','manual_review')),
    first_seen_at TEXT NOT NULL DEFAULT(datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT(datetime('now')),
    retry_count INTEGER NOT NULL DEFAULT 1 CHECK(retry_count >= 1),
    resolved_at TEXT,
    last_error_code TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE SET NULL
)
"#;

const M63_ACTIVE_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX idx_device_sync_quarantine_active_key
ON device_sync_quarantine(
    COALESCE(group_id,''), source_device_id, source_sequence, reason_code
)
WHERE status='active'
"#;

const M63_GROUP_STATUS_INDEX_SQL: &str = r#"
CREATE INDEX idx_device_sync_quarantine_group_status
ON device_sync_quarantine(group_id, status, last_seen_at DESC)
"#;

const M63_OUTBOX_CAPTURE_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX idx_device_sync_outbox_capture_sequence
ON device_sync_outbox(group_id, capture_sequence)
"#;

const M63_OUTBOX_PENDING_CAPTURE_INDEX_SQL: &str = r#"
CREATE INDEX idx_device_sync_outbox_pending_capture
ON device_sync_outbox(group_id, state, capture_sequence)
"#;

const M63_EXPORT_DRAFTS_TABLE_SQL: &str = r#"
CREATE TABLE device_sync_export_drafts (
    group_id TEXT NOT NULL,
    local_device_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence >= 1),
    key_epoch INTEGER NOT NULL CHECK(key_epoch >= 1),
    previous_manifest_hash TEXT,
    event_envelope_bytes BLOB NOT NULL,
    manifest_envelope_bytes BLOB NOT NULL,
    event_ciphertext_sha256 TEXT NOT NULL,
    manifest_ciphertext_sha256 TEXT NOT NULL,
    operation_ids_json TEXT NOT NULL,
    operation_fingerprint TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'prepared'
        CHECK(state IN ('prepared','finalized')),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    finalized_at TEXT,
    PRIMARY KEY(group_id, local_device_id, sequence),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
)
"#;

const M63_EXPORT_DRAFTS_STATE_INDEX_SQL: &str = r#"
CREATE INDEX idx_device_sync_export_drafts_state
ON device_sync_export_drafts(group_id, local_device_id, state, sequence)
"#;

const M63_EXPORT_DRAFTS_ONE_PREPARED_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX idx_device_sync_export_drafts_one_prepared
ON device_sync_export_drafts(group_id)
WHERE state='prepared'
"#;

fn compatibility_error(
    code: &'static str,
    version: Option<i64>,
    reason: &'static str,
    stored_checksum: Option<&[u8]>,
    current_checksum: Option<&[u8]>,
    missing_sentinels: Vec<String>,
) -> DbError {
    let error = DbMigrationCompatibilityError {
        code,
        version,
        reason,
        stored_checksum: stored_checksum.map(checksum_hex),
        current_checksum: current_checksum.map(checksum_hex),
        missing_sentinels,
    };
    crate::dlog!(
        "[db] migration preflight blocked code={} version={:?} reason={} missing_sentinels={:?}",
        error.code,
        error.version,
        error.reason,
        error.missing_sentinels
    );
    DbError::MigrationCompatibility(error)
}

fn checksum_hex(checksum: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(checksum.len() * 2);
    for byte in checksum {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(crate) fn legacy_migration_36_metadata() -> Migration {
    Migration {
        version: LEGACY_MIGRATION_36_VERSION,
        description: Cow::Borrowed(LEGACY_MIGRATION_36_DESCRIPTION),
        migration_type: MigrationType::Simple,
        sql: Cow::Borrowed(LEGACY_MIGRATION_36_NEVER_EXECUTE_SQL),
        checksum: Cow::Borrowed(&LEGACY_MIGRATION_36_CHECKSUM),
        no_tx: false,
    }
}

fn schema_metadata_unreadable(_error: sqlx::Error) -> DbError {
    compatibility_error(
        DB_MIGRATION_LINEAGE_INCOMPATIBLE,
        None,
        "schema_metadata_unreadable",
        None,
        None,
        Vec::new(),
    )
}

pub(crate) async fn preflight_existing_database(
    database_path: &Path,
) -> Result<MigrationPreflight, DbError> {
    // SQLite's immutable mode does not reliably expose committed content that
    // exists only in a WAL. Refuse every sidecar shape before the first SQLite
    // connection; recovery/checkpointing must happen on an isolated copy.
    ensure_no_wal_sidecars(database_path)?;

    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(false)
        .read_only(true)
        .immutable(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| DbError::Connect(format!("数据库只读预检连接失败: {error}")))?;

    let result = preflight_pool(&pool).await;
    pool.close().await;

    // Detect a sidecar that appeared during preflight. Sidecar recovery has
    // higher priority than a classification made from the immutable main DB.
    ensure_no_wal_sidecars(database_path)?;
    result
}

fn sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut value = database_path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(target_os = "windows")]
fn sidecar_recovery_error(reason: &'static str) -> DbError {
    compatibility_error(
        DB_MIGRATION_LINEAGE_INCOMPATIBLE,
        None,
        reason,
        None,
        None,
        Vec::new(),
    )
}

#[cfg(target_os = "windows")]
fn file_snapshot(path: &Path) -> Result<FileSnapshot, DbError> {
    let file = File::open(path).map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    let len = file
        .metadata()
        .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?
        .len();
    let mut reader = BufReader::new(file);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(FileSnapshot {
        len,
        sha256: format!("{:x}", digest.finalize()),
    })
}

#[cfg(target_os = "windows")]
fn backup_file(source: &Path, destination: &Path) -> Result<(), DbError> {
    let mut input =
        File::open(source).map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    io::copy(&mut input, &mut output)
        .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    output
        .sync_all()
        .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))
}

#[cfg(target_os = "windows")]
fn read_u32(bytes: &[u8], big_endian: bool) -> u32 {
    let value: [u8; 4] = bytes.try_into().expect("four-byte WAL word");
    if big_endian {
        u32::from_be_bytes(value)
    } else {
        u32::from_le_bytes(value)
    }
}

#[cfg(target_os = "windows")]
fn wal_checksum(bytes: &[u8], big_endian: bool, mut state: (u32, u32)) -> (u32, u32) {
    for words in bytes.chunks_exact(8) {
        let first = read_u32(&words[..4], big_endian);
        let second = read_u32(&words[4..], big_endian);
        state.0 = state.0.wrapping_add(first).wrapping_add(state.1);
        state.1 = state.1.wrapping_add(second).wrapping_add(state.0);
    }
    state
}

#[cfg(target_os = "windows")]
fn validate_wal_pair(database_path: &Path) -> Result<(), DbError> {
    let wal = fs::read(sidecar_path(database_path, "-wal"))
        .map_err(|_| sidecar_recovery_error("wal_sidecar_physical_validation_failed"))?;
    let shm = fs::read(sidecar_path(database_path, "-shm"))
        .map_err(|_| sidecar_recovery_error("wal_sidecar_physical_validation_failed"))?;
    if wal.len() < 32 || shm.len() < 136 || shm.len() % 32_768 != 0 {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let magic = u32::from_be_bytes(wal[0..4].try_into().unwrap());
    let checksum_big_endian = match magic {
        0x377f_0682 => false,
        0x377f_0683 => true,
        _ => {
            return Err(sidecar_recovery_error(
                "wal_sidecar_physical_validation_failed",
            ))
        }
    };
    if u32::from_be_bytes(wal[4..8].try_into().unwrap()) != 3_007_000 {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let encoded_page_size = u32::from_be_bytes(wal[8..12].try_into().unwrap());
    let page_size = if encoded_page_size == 1 {
        65_536_usize
    } else {
        encoded_page_size as usize
    };
    if !(512..=65_536).contains(&page_size) || !page_size.is_power_of_two() {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let mut database_header = [0_u8; 100];
    File::open(database_path)
        .and_then(|mut database| database.read_exact(&mut database_header))
        .map_err(|_| sidecar_recovery_error("wal_sidecar_physical_validation_failed"))?;
    if &database_header[..16] != b"SQLite format 3\0" {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let database_page_size = u16::from_be_bytes(database_header[16..18].try_into().unwrap());
    let database_page_size = if database_page_size == 1 {
        65_536_usize
    } else {
        database_page_size as usize
    };
    if database_page_size != page_size {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let frame_size = 24_usize + page_size;
    if (wal.len() - 32) % frame_size != 0 || wal.len() == 32 {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    // On Windows the wal-index header is native little-endian. SQLite keeps
    // two copies; requiring agreement avoids trusting a torn SHM header. Use
    // mxFrame rather than WAL EOF because a legal WAL reset may retain stale
    // old-salt frames beyond the active prefix.
    if shm[..48] != shm[48..96]
        || u32::from_le_bytes(shm[0..4].try_into().unwrap()) != 3_007_000
        || shm[12] != 1
    {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let max_frame = u32::from_le_bytes(shm[16..20].try_into().unwrap()) as usize;
    if max_frame == 0 {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let active_end = 32_usize
        .checked_add(
            max_frame
                .checked_mul(frame_size)
                .ok_or_else(|| sidecar_recovery_error("wal_sidecar_physical_validation_failed"))?,
        )
        .ok_or_else(|| sidecar_recovery_error("wal_sidecar_physical_validation_failed"))?;
    if active_end > wal.len() {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let expected_header_checksum = (
        u32::from_be_bytes(wal[24..28].try_into().unwrap()),
        u32::from_be_bytes(wal[28..32].try_into().unwrap()),
    );
    let mut checksum = wal_checksum(&wal[..24], checksum_big_endian, (0, 0));
    if checksum != expected_header_checksum {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    let mut has_commit = false;
    for frame in wal[32..active_end].chunks_exact(frame_size) {
        if frame[..4] == [0, 0, 0, 0] || frame[8..16] != wal[16..24] {
            return Err(sidecar_recovery_error(
                "wal_sidecar_physical_validation_failed",
            ));
        }
        checksum = wal_checksum(&frame[..8], checksum_big_endian, checksum);
        checksum = wal_checksum(&frame[24..], checksum_big_endian, checksum);
        let stored = (
            u32::from_be_bytes(frame[16..20].try_into().unwrap()),
            u32::from_be_bytes(frame[20..24].try_into().unwrap()),
        );
        if checksum != stored {
            return Err(sidecar_recovery_error(
                "wal_sidecar_physical_validation_failed",
            ));
        }
        has_commit |= frame[4..8] != [0, 0, 0, 0];
    }
    if !has_commit {
        return Err(sidecar_recovery_error(
            "wal_sidecar_physical_validation_failed",
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn atomic_replace_file(temporary: &Path, target: &Path) -> Result<(), DbError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        REPLACEFILE_WRITE_THROUGH,
    };

    let temporary_wide = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = if target.exists() {
        unsafe {
            ReplaceFileW(
                PCWSTR(target_wide.as_ptr()),
                PCWSTR(temporary_wide.as_ptr()),
                PCWSTR::null(),
                REPLACEFILE_WRITE_THROUGH,
                None,
                None,
            )
        }
    } else {
        unsafe {
            MoveFileExW(
                PCWSTR(temporary_wide.as_ptr()),
                PCWSTR(target_wide.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    result.map_err(|_| sidecar_recovery_error("wal_sidecar_backup_restore_failed"))
}

#[cfg(target_os = "windows")]
fn write_atomic_bytes(target: &Path, bytes: &[u8]) -> Result<(), DbError> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?
        .as_nanos();
    let mut temporary_name = target.as_os_str().to_os_string();
    temporary_name.push(format!(".tmp-{}-{unique}", std::process::id()));
    let temporary = PathBuf::from(temporary_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    atomic_replace_file(&temporary, target)
}

/// Recover a legacy, complete WAL/SHM trio before immutable lineage preflight.
///
/// After a physical WAL check, one SQLite connection acquires exclusive writer
/// ownership with checkpoint-on-close disabled. The raw trio at that locked
/// point is copied to a content-addressed sibling directory and verified by
/// length and SHA-256. Sidecars are never removed by application filesystem
/// code: only an explicitly successful SQLite checkpoint and journal-mode
/// transition may retire them.
pub(crate) async fn recover_complete_wal_pair(
    database_path: &Path,
) -> Result<Option<WalRecoveryBackup>, DbError> {
    #[cfg(target_os = "windows")]
    {
        return recover_complete_wal_pair_windows(database_path).await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        ensure_no_wal_sidecars(database_path)?;
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
async fn recover_complete_wal_pair_windows(
    database_path: &Path,
) -> Result<Option<WalRecoveryBackup>, DbError> {
    let wal_path = sidecar_path(database_path, "-wal");
    let shm_path = sidecar_path(database_path, "-shm");
    let journal_path = sidecar_path(database_path, "-journal");
    let wal_exists = wal_path.try_exists().unwrap_or(true);
    let shm_exists = shm_path.try_exists().unwrap_or(true);

    // SHM is a reconstructible WAL index and contains no committed database
    // pages.  With no WAL and no rollback journal it is safe to ignore an
    // orphaned SHM left by SQLite after a proven checkpoint.
    if !wal_exists && !journal_path.try_exists().unwrap_or(true) {
        return Ok(None);
    }
    if !wal_exists || !shm_exists || journal_path.try_exists().unwrap_or(true) {
        return Err(sidecar_recovery_error(
            "wal_sidecar_present_requires_recovery",
        ));
    }

    // SQLite can leave an empty WAL together with its reconstructible SHM
    // after a clean shutdown.  There are no frames (and therefore no durable
    // pages) to recover in that state.  Still let SQLite acquire exclusive
    // ownership, audit the authoritative main database and retire its own
    // sidecars; never remove them directly from application filesystem code.
    let wal_length = fs::metadata(&wal_path)
        .map_err(|_| sidecar_recovery_error("wal_sidecar_physical_validation_failed"))?
        .len();
    if wal_length == 0 {
        retire_empty_wal_pair_windows(database_path).await?;
        return Ok(None);
    }

    // Reject malformed/truncated/checksum-invalid WAL before SQLite can choose
    // to ignore it as if no recoverable sidecar existed.
    validate_wal_pair(database_path)?;
    let exclusive_options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let exclusive_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                let mut no_checkpoint_on_close = 0_i32;
                {
                    let mut handle = connection.lock_handle().await?;
                    let result = unsafe {
                        caseboard_sqlite3_db_config(
                            handle.as_raw_handle().as_ptr().cast(),
                            SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                            1_i32,
                            &mut no_checkpoint_on_close as *mut i32,
                        )
                    };
                    if result != 0 || no_checkpoint_on_close != 1 {
                        return Err(sqlx::Error::Protocol(
                            "SQLite no-checkpoint-on-close unavailable".to_string(),
                        ));
                    }
                }
                let mode: String = sqlx::query_scalar("PRAGMA locking_mode=EXCLUSIVE")
                    .fetch_one(&mut *connection)
                    .await?;
                if !mode.eq_ignore_ascii_case("exclusive") {
                    return Err(sqlx::Error::Protocol(
                        "exclusive SQLite locking mode unavailable".to_string(),
                    ));
                }
                sqlx::query("BEGIN EXCLUSIVE")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(exclusive_options)
        .await
        .map_err(|_| sidecar_recovery_error("wal_sidecar_exclusive_lock_failed"))?;
    validate_wal_pair(database_path)?;

    let source_paths = [database_path.to_path_buf(), wal_path, shm_path];
    let before = source_paths
        .iter()
        .map(|path| file_snapshot(path))
        .collect::<Result<Vec<_>, _>>()?;
    let identity = {
        let mut digest = Sha256::new();
        // SHM read marks may change merely by acquiring SQLite's exclusive
        // connection. The durable database+WAL pair identifies one recovery
        // set; the exact SHM bytes are still preserved and verified inside it.
        for snapshot in before.iter().take(2) {
            digest.update(snapshot.len.to_le_bytes());
            digest.update(snapshot.sha256.as_bytes());
        }
        format!("{:x}", digest.finalize())
    };
    let parent = database_path
        .parent()
        .ok_or_else(|| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    let backup_directory = parent.join(format!(".caseboard-v083-wal-recovery-{}", &identity[..20]));
    match fs::create_dir(&backup_directory) {
        Ok(()) => {
            for source in &source_paths {
                let file_name = source
                    .file_name()
                    .ok_or_else(|| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
                backup_file(source, &backup_directory.join(file_name))?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(sidecar_recovery_error("wal_sidecar_backup_failed")),
    }
    for source in &source_paths {
        let file_name = source
            .file_name()
            .ok_or_else(|| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
        let destination = backup_directory.join(file_name);
        if !destination.try_exists().unwrap_or(true) {
            backup_file(source, &destination)?;
        }
    }

    let after = source_paths
        .iter()
        .map(|path| file_snapshot(path))
        .collect::<Result<Vec<_>, _>>()?;
    if after != before {
        return Err(sidecar_recovery_error(
            "wal_sidecar_source_changed_during_backup",
        ));
    }
    for (index, source) in source_paths.iter().enumerate() {
        let backup = backup_directory.join(
            source
                .file_name()
                .ok_or_else(|| sidecar_recovery_error("wal_sidecar_backup_failed"))?,
        );
        if file_snapshot(&backup)? != before[index] {
            return Err(sidecar_recovery_error(
                "wal_sidecar_backup_verification_failed",
            ));
        }
    }

    let manifest_bytes = serde_json::to_vec_pretty(&serde_json::json!({
        "format": "caseboard-v083-wal-recovery-v1",
        "files": [
            {"role": "database", "length": before[0].len, "sha256": &before[0].sha256},
            {"role": "wal", "length": before[1].len, "sha256": &before[1].sha256},
            {"role": "shm", "length": before[2].len, "sha256": &before[2].sha256}
        ]
    }))
    .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?;
    let manifest_path = backup_directory.join("manifest.json");
    if manifest_path.try_exists().unwrap_or(false) {
        if fs::read(&manifest_path)
            .map_err(|_| sidecar_recovery_error("wal_sidecar_backup_failed"))?
            != manifest_bytes
        {
            return Err(sidecar_recovery_error(
                "wal_sidecar_backup_verification_failed",
            ));
        }
    } else {
        write_atomic_bytes(&manifest_path, &manifest_bytes)?;
    }

    // Every audit query reuses the one connection whose raw EXCLUSIVE
    // transaction was opened by `after_connect`; no second writer can enter
    // between the backed-up bytes, lineage classification and checkpoint.
    let audit_result = preflight_pool(&exclusive_pool).await;
    let max_version = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1",
    )
    .fetch_one(&exclusive_pool)
    .await
    .map_err(|_| sidecar_recovery_error("wal_sidecar_combined_audit_failed"));
    let integrity: Result<String, DbError> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&exclusive_pool)
        .await
        .map_err(|_| sidecar_recovery_error("wal_sidecar_combined_audit_failed"));
    let audit_failure = audit_result
        .and_then(|preflight| {
            if max_version?.unwrap_or(0) > 62 {
                return Err(sidecar_recovery_error("wal_sidecar_version_not_legacy_082"));
            }
            if !integrity?.eq_ignore_ascii_case("ok") {
                return Err(sidecar_recovery_error("wal_sidecar_integrity_check_failed"));
            }
            Ok(preflight)
        })
        .err();
    if let Some(error) = audit_failure {
        sqlx::query("ROLLBACK").execute(&exclusive_pool).await.ok();
        exclusive_pool.close().await;
        return Err(error);
    }
    sqlx::query("COMMIT")
        .execute(&exclusive_pool)
        .await
        .map_err(|_| sidecar_recovery_error("wal_sidecar_checkpoint_failed"))?;

    // `locking_mode=EXCLUSIVE` keeps this sole connection's ownership after
    // COMMIT, which SQLite requires because wal_checkpoint cannot run inside a
    // transaction.
    let checkpoint: (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_one(&exclusive_pool)
        .await
        .map_err(|_| sidecar_recovery_error("wal_sidecar_checkpoint_failed"))?;
    // TRUNCATE success is represented by (busy=0, log=0, checkpointed=0).
    // Accepting busy=0 with residual frames would let startup proceed without
    // proving that every committed frame reached the main database.
    if checkpoint != (0, 0, 0) {
        exclusive_pool.close().await;
        return Err(sidecar_recovery_error("wal_sidecar_checkpoint_busy"));
    }
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode = DELETE")
        .fetch_one(&exclusive_pool)
        .await
        .map_err(|_| sidecar_recovery_error("wal_sidecar_checkpoint_failed"))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        exclusive_pool.close().await;
        return Err(sidecar_recovery_error(
            "wal_sidecar_journal_mode_delete_failed",
        ));
    }
    // NO_CKPT_ON_CLOSE and exclusive locking deliberately keep handles and
    // empty sidecars alive on failure paths. After explicit checkpoint+DELETE
    // have both been proven successful, return this same connection to NORMAL
    // so SQLite—not application filesystem code—can retire the empty files.
    let locking_mode: String = sqlx::query_scalar("PRAGMA locking_mode=NORMAL")
        .fetch_one(&exclusive_pool)
        .await
        .map_err(|_| sidecar_recovery_error("wal_sidecar_checkpoint_failed"))?;
    if !locking_mode.eq_ignore_ascii_case("normal") {
        exclusive_pool.close().await;
        return Err(sidecar_recovery_error(
            "wal_sidecar_locking_mode_normal_failed",
        ));
    }
    // Failure paths keep this enabled so merely closing the connection cannot
    // checkpoint an unaudited WAL.  Once the explicit checkpoint and both mode
    // transitions have succeeded, restore SQLite's normal close behaviour so
    // SQLite itself can retire the now-empty WAL/SHM files.
    let mut checkpoint_on_close_restored = 1_i32;
    {
        let mut connection = exclusive_pool
            .acquire()
            .await
            .map_err(|_| sidecar_recovery_error("wal_sidecar_checkpoint_failed"))?;
        let mut handle = connection
            .lock_handle()
            .await
            .map_err(|_| sidecar_recovery_error("wal_sidecar_checkpoint_failed"))?;
        let result = unsafe {
            caseboard_sqlite3_db_config(
                handle.as_raw_handle().as_ptr().cast(),
                SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                0_i32,
                &mut checkpoint_on_close_restored as *mut i32,
            )
        };
        if result != 0 || checkpoint_on_close_restored != 0 {
            return Err(sidecar_recovery_error(
                "wal_sidecar_checkpoint_on_close_restore_failed",
            ));
        }
    }
    exclusive_pool.close().await;
    for (suffix, reason) in [
        ("-wal", "wal_sidecar_wal_remained_after_checkpoint"),
        ("-journal", "wal_sidecar_journal_remained_after_checkpoint"),
    ] {
        if sidecar_path(database_path, suffix)
            .try_exists()
            .unwrap_or(true)
        {
            return Err(sidecar_recovery_error(reason));
        }
    }
    ensure_no_wal_sidecars(database_path)?;

    Ok(Some(WalRecoveryBackup {
        directory: backup_directory,
        database_sha256: before[0].sha256.clone(),
        wal_sha256: before[1].sha256.clone(),
        shm_sha256: before[2].sha256.clone(),
    }))
}

#[cfg(target_os = "windows")]
async fn retire_empty_wal_pair_windows(database_path: &Path) -> Result<(), DbError> {
    let wal_path = sidecar_path(database_path, "-wal");
    let journal_path = sidecar_path(database_path, "-journal");
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                let mode: String = sqlx::query_scalar("PRAGMA locking_mode=EXCLUSIVE")
                    .fetch_one(&mut *connection)
                    .await?;
                if !mode.eq_ignore_ascii_case("exclusive") {
                    return Err(sqlx::Error::Protocol(
                        "exclusive SQLite locking mode unavailable".to_string(),
                    ));
                }
                sqlx::query("BEGIN EXCLUSIVE")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .map_err(|_| sidecar_recovery_error("empty_wal_exclusive_lock_failed"))?;

    // Opening the exclusive SQLite connection may already retire the empty
    // WAL.  If it remains, it must still be empty while the lock is held.
    if wal_path
        .try_exists()
        .map_err(|_| sidecar_recovery_error("empty_wal_source_changed"))?
        && fs::metadata(&wal_path)
            .map_err(|_| sidecar_recovery_error("empty_wal_source_changed"))?
            .len()
            != 0
    {
        sqlx::query("ROLLBACK").execute(&pool).await.ok();
        pool.close().await;
        return Err(sidecar_recovery_error("empty_wal_source_changed"));
    }
    if journal_path.try_exists().unwrap_or(true) {
        sqlx::query("ROLLBACK").execute(&pool).await.ok();
        pool.close().await;
        return Err(sidecar_recovery_error("empty_wal_source_changed"));
    }

    let audit_result = preflight_pool(&pool).await;
    let integrity: Result<String, DbError> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .map_err(|_| sidecar_recovery_error("empty_wal_integrity_check_failed"));
    if let Err(error) = audit_result.and_then(|preflight| {
        if !integrity?.eq_ignore_ascii_case("ok") {
            return Err(sidecar_recovery_error("empty_wal_integrity_check_failed"));
        }
        Ok(preflight)
    }) {
        sqlx::query("ROLLBACK").execute(&pool).await.ok();
        pool.close().await;
        return Err(error);
    }
    sqlx::query("COMMIT")
        .execute(&pool)
        .await
        .map_err(|_| sidecar_recovery_error("empty_wal_retirement_failed"))?;

    let checkpoint: (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_one(&pool)
        .await
        .map_err(|_| sidecar_recovery_error("empty_wal_retirement_failed"))?;
    // SQLite reports (0,-1,-1) when opening the empty sidecar has already
    // established that no WAL transaction exists; (0,0,0) is the equivalent
    // result when an empty WAL connection is still active.
    if checkpoint != (0, 0, 0) && checkpoint != (0, -1, -1) {
        pool.close().await;
        return Err(sidecar_recovery_error("empty_wal_checkpoint_busy"));
    }
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode = DELETE")
        .fetch_one(&pool)
        .await
        .map_err(|_| sidecar_recovery_error("empty_wal_retirement_failed"))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        pool.close().await;
        return Err(sidecar_recovery_error(
            "empty_wal_journal_mode_delete_failed",
        ));
    }
    let locking_mode: String = sqlx::query_scalar("PRAGMA locking_mode=NORMAL")
        .fetch_one(&pool)
        .await
        .map_err(|_| sidecar_recovery_error("empty_wal_retirement_failed"))?;
    if !locking_mode.eq_ignore_ascii_case("normal") {
        pool.close().await;
        return Err(sidecar_recovery_error(
            "empty_wal_locking_mode_normal_failed",
        ));
    }
    pool.close().await;
    ensure_no_wal_sidecars(database_path)
}

pub(crate) fn ensure_no_wal_sidecars(database_path: &Path) -> Result<(), DbError> {
    // An orphaned SHM is only a reconstructible index. A zero-byte WAL also
    // contains no header or frames, so it cannot carry durable pages. SQLite
    // may legitimately retain that empty filename after a clean shutdown.
    // Any non-empty or unreadable WAL, and every rollback journal, still fail
    // closed and require the recovery path.
    let wal_path = sidecar_path(database_path, "-wal");
    let wal_requires_recovery = match std::fs::metadata(&wal_path) {
        Ok(metadata) => metadata.len() != 0,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    };
    let journal_requires_recovery = sidecar_path(database_path, "-journal")
        .try_exists()
        .unwrap_or(true);
    let sidecar_present_or_unreadable = wal_requires_recovery || journal_requires_recovery;
    if sidecar_present_or_unreadable {
        return Err(compatibility_error(
            DB_MIGRATION_LINEAGE_INCOMPATIBLE,
            None,
            "wal_sidecar_present_requires_recovery",
            None,
            None,
            Vec::new(),
        ));
    }
    Ok(())
}

async fn preflight_pool(pool: &SqlitePool) -> Result<MigrationPreflight, DbError> {
    if !object_exists(pool, "table", "_sqlx_migrations").await? {
        if has_user_schema_objects_other_than_migration_table(pool).await? {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                None,
                "migration_history_missing_for_existing_schema",
                None,
                None,
                Vec::new(),
            ));
        }
        return Ok(MigrationPreflight::default());
    }

    let history: Vec<(i64, String, i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version, description, success, checksum \
         FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| {
        compatibility_error(
            DB_MIGRATION_LINEAGE_INCOMPATIBLE,
            None,
            "migration_history_unreadable",
            None,
            None,
            Vec::new(),
        )
    })?;

    if history.is_empty() {
        if has_user_schema_objects_other_than_migration_table(pool).await? {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                None,
                "migration_history_empty_for_existing_schema",
                None,
                None,
                Vec::new(),
            ));
        }
        return Ok(MigrationPreflight::default());
    }

    let embedded_migrator = sqlx::migrate!("./migrations");
    let embedded_by_version: HashMap<i64, _> = embedded_migrator
        .iter()
        .map(|migration| (migration.version, migration))
        .collect();

    let mut allow_missing_legacy_migration_36 = false;
    for (version, description, success, checksum) in &history {
        if *success != 1 {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                Some(*version),
                "failed_history_row",
                Some(checksum),
                embedded_by_version
                    .get(version)
                    .map(|migration| migration.checksum.as_ref()),
                Vec::new(),
            ));
        }
        if !embedded_by_version.contains_key(version) {
            if *version == LEGACY_MIGRATION_36_VERSION
                && description == LEGACY_MIGRATION_36_DESCRIPTION
            {
                allow_missing_legacy_migration_36 = true;
                continue;
            }
            return Err(compatibility_error(
                DB_MIGRATION_APPLIED_VERSION_UNKNOWN,
                Some(*version),
                "applied_version_not_embedded",
                Some(checksum),
                None,
                Vec::new(),
            ));
        }
    }

    let applied_versions: HashSet<i64> = history.iter().map(|row| row.0).collect();
    if let Some(max_applied_version) = applied_versions.iter().max().copied() {
        if let Some(missing) = embedded_migrator.iter().find(|migration| {
            migration.version <= max_applied_version
                && !applied_versions.contains(&migration.version)
        }) {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                Some(missing.version),
                "applied_history_gap",
                None,
                Some(missing.checksum.as_ref()),
                Vec::new(),
            ));
        }
    }
    let missing_sentinels = collect_missing_sentinels(pool, &applied_versions).await?;

    // Freeze combination priority: a proven schema defect is more actionable
    // than a checksum mismatch and must not be hidden by it.
    if !missing_sentinels.is_empty() {
        let version = missing_sentinels
            .iter()
            .map(|missing| missing.migration_version)
            .min();
        return Err(compatibility_error(
            DB_MIGRATION_SCHEMA_SENTINEL_MISSING,
            version,
            "applied_migration_schema_missing",
            None,
            None,
            missing_sentinels
                .into_iter()
                .map(|missing| missing.code.to_string())
                .collect(),
        ));
    }

    for (version, _description, _success, stored_checksum) in &history {
        if *version == LEGACY_MIGRATION_36_VERSION {
            if stored_checksum.as_slice() == LEGACY_MIGRATION_36_CHECKSUM.as_slice() {
                continue;
            }
            return Err(compatibility_error(
                DB_MIGRATION_CHECKSUM_UNKNOWN,
                Some(*version),
                "checksum_not_allowlisted",
                Some(stored_checksum),
                Some(&LEGACY_MIGRATION_36_CHECKSUM),
                Vec::new(),
            ));
        }

        let embedded = embedded_by_version
            .get(version)
            .expect("unknown versions were rejected above");
        let current_checksum = embedded.checksum.as_ref();
        if stored_checksum.as_slice() == current_checksum {
            continue;
        }

        return Err(compatibility_error(
            DB_MIGRATION_CHECKSUM_UNKNOWN,
            Some(*version),
            "checksum_not_allowlisted",
            Some(stored_checksum),
            Some(current_checksum),
            Vec::new(),
        ));
    }

    Ok(MigrationPreflight {
        allow_missing_legacy_migration_36,
    })
}

async fn collect_missing_sentinels(
    pool: &SqlitePool,
    applied_versions: &HashSet<i64>,
) -> Result<Vec<MissingSentinel>, DbError> {
    let mut missing = Vec::new();

    if applied_versions.contains(&LEGACY_MIGRATION_36_VERSION)
        && !legacy_migration_36_schema_matches(pool).await?
    {
        missing.push(MissingSentinel {
            migration_version: LEGACY_MIGRATION_36_VERSION,
            code: LEGACY_MIGRATION_36_SCHEMA_SENTINEL,
        });
    }

    for (version, code, table) in [
        (49, "M49.table.feishu_sync_links", "feishu_sync_links"),
        (49, "M49.table.feishu_sync_inbox", "feishu_sync_inbox"),
        (
            51,
            "M51.table.feishu_sync_binding_audits",
            "feishu_sync_binding_audits",
        ),
        (58, "M58.table.device_sync_groups", "device_sync_groups"),
        (58, "M58.table.device_sync_members", "device_sync_members"),
        (58, "M58.table.device_sync_outbox", "device_sync_outbox"),
        (
            58,
            "M58.table.device_sync_dirty_entities",
            "device_sync_dirty_entities",
        ),
        (
            58,
            "M58.table.device_sync_applied_operations",
            "device_sync_applied_operations",
        ),
        (
            58,
            "M58.table.device_sync_entity_revisions",
            "device_sync_entity_revisions",
        ),
        (
            58,
            "M58.table.device_sync_conflicts",
            "device_sync_conflicts",
        ),
        (58, "M58.table.device_sync_receipts", "device_sync_receipts"),
        (
            58,
            "M58.table.device_sync_snapshots",
            "device_sync_snapshots",
        ),
        (
            58,
            "M58.table.device_sync_quarantine",
            "device_sync_quarantine",
        ),
        (58, "M58.table.device_sync_audits", "device_sync_audits"),
        (
            59,
            "M59.table.legal_skill_binding_suppressions",
            "legal_skill_binding_suppressions",
        ),
        (
            60,
            "M60.table.case_domain_status_migration_audits",
            "case_domain_status_migration_audits",
        ),
        (
            61,
            "M61.table.feishu_sync_operation_audits",
            "feishu_sync_operation_audits",
        ),
        (
            62,
            "M62.table.feishu_sync_entity_previews",
            "feishu_sync_entity_previews",
        ),
        (
            63,
            "M63.table.device_sync_export_drafts",
            "device_sync_export_drafts",
        ),
    ] {
        if applied_versions.contains(&version) && !object_exists(pool, "table", table).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    for (version, code, table, column) in [
        (
            49,
            "M49.column.links.entity_type",
            "feishu_sync_links",
            "entity_type",
        ),
        (
            49,
            "M49.column.links.local_entity_id",
            "feishu_sync_links",
            "local_entity_id",
        ),
        (49, "M49.column.links.status", "feishu_sync_links", "status"),
        (49, "M49.column.inbox.status", "feishu_sync_inbox", "status"),
        (
            49,
            "M49.column.inbox.bound_case_id",
            "feishu_sync_inbox",
            "bound_case_id",
        ),
        (
            51,
            "M51.column.inbox.auto_bind_suppressed",
            "feishu_sync_inbox",
            "auto_bind_suppressed",
        ),
        (
            59,
            "M59.column.suppression.id",
            "legal_skill_binding_suppressions",
            "id",
        ),
        (
            59,
            "M59.column.suppression.legal_domain",
            "legal_skill_binding_suppressions",
            "legal_domain",
        ),
        (
            59,
            "M59.column.suppression.task_type",
            "legal_skill_binding_suppressions",
            "task_type",
        ),
        (
            61,
            "M61.column.field_preview.review_status",
            "feishu_sync_field_previews",
            "review_status",
        ),
        (
            61,
            "M61.column.field_preview.resolution_value_json",
            "feishu_sync_field_previews",
            "resolution_value_json",
        ),
        (
            61,
            "M61.column.field_preview.resolved_at",
            "feishu_sync_field_previews",
            "resolved_at",
        ),
        (
            62,
            "M62.column.entity_preview.review_status",
            "feishu_sync_entity_previews",
            "review_status",
        ),
        (
            63,
            "M63.column.groups.last_attempt_at",
            "device_sync_groups",
            "last_attempt_at",
        ),
        (
            63,
            "M63.column.groups.last_success_at",
            "device_sync_groups",
            "last_success_at",
        ),
        (
            63,
            "M63.column.groups.auto_paused",
            "device_sync_groups",
            "auto_paused",
        ),
        (
            63,
            "M63.column.groups.pause_reason_code",
            "device_sync_groups",
            "pause_reason_code",
        ),
        (
            63,
            "M63.column.outbox.capture_sequence",
            "device_sync_outbox",
            "capture_sequence",
        ),
        (
            63,
            "M63.column.quarantine.source_device_id",
            "device_sync_quarantine",
            "source_device_id",
        ),
        (
            63,
            "M63.column.quarantine.source_sequence",
            "device_sync_quarantine",
            "source_sequence",
        ),
        (
            63,
            "M63.column.quarantine.status",
            "device_sync_quarantine",
            "status",
        ),
        (
            63,
            "M63.column.quarantine.first_seen_at",
            "device_sync_quarantine",
            "first_seen_at",
        ),
        (
            63,
            "M63.column.quarantine.last_seen_at",
            "device_sync_quarantine",
            "last_seen_at",
        ),
        (
            63,
            "M63.column.quarantine.retry_count",
            "device_sync_quarantine",
            "retry_count",
        ),
        (
            63,
            "M63.column.quarantine.resolved_at",
            "device_sync_quarantine",
            "resolved_at",
        ),
        (
            63,
            "M63.column.quarantine.last_error_code",
            "device_sync_quarantine",
            "last_error_code",
        ),
        (
            63,
            "M63.column.export_drafts.group_id",
            "device_sync_export_drafts",
            "group_id",
        ),
        (
            63,
            "M63.column.export_drafts.local_device_id",
            "device_sync_export_drafts",
            "local_device_id",
        ),
        (
            63,
            "M63.column.export_drafts.sequence",
            "device_sync_export_drafts",
            "sequence",
        ),
        (
            63,
            "M63.column.export_drafts.key_epoch",
            "device_sync_export_drafts",
            "key_epoch",
        ),
        (
            63,
            "M63.column.export_drafts.previous_manifest_hash",
            "device_sync_export_drafts",
            "previous_manifest_hash",
        ),
        (
            63,
            "M63.column.export_drafts.event_envelope_bytes",
            "device_sync_export_drafts",
            "event_envelope_bytes",
        ),
        (
            63,
            "M63.column.export_drafts.manifest_envelope_bytes",
            "device_sync_export_drafts",
            "manifest_envelope_bytes",
        ),
        (
            63,
            "M63.column.export_drafts.event_ciphertext_sha256",
            "device_sync_export_drafts",
            "event_ciphertext_sha256",
        ),
        (
            63,
            "M63.column.export_drafts.manifest_ciphertext_sha256",
            "device_sync_export_drafts",
            "manifest_ciphertext_sha256",
        ),
        (
            63,
            "M63.column.export_drafts.operation_ids_json",
            "device_sync_export_drafts",
            "operation_ids_json",
        ),
        (
            63,
            "M63.column.export_drafts.operation_fingerprint",
            "device_sync_export_drafts",
            "operation_fingerprint",
        ),
        (
            63,
            "M63.column.export_drafts.state",
            "device_sync_export_drafts",
            "state",
        ),
        (
            63,
            "M63.column.export_drafts.created_at",
            "device_sync_export_drafts",
            "created_at",
        ),
        (
            63,
            "M63.column.export_drafts.updated_at",
            "device_sync_export_drafts",
            "updated_at",
        ),
        (
            63,
            "M63.column.export_drafts.finalized_at",
            "device_sync_export_drafts",
            "finalized_at",
        ),
    ] {
        if applied_versions.contains(&version) && !column_exists(pool, table, column).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    if applied_versions.contains(&63) {
        for (code, table, column, expected_type, not_null, default_value) in [
            (
                "M63.column.groups.auto_paused.definition",
                "device_sync_groups",
                "auto_paused",
                "INTEGER",
                true,
                Some("0"),
            ),
            (
                "M63.column.outbox.capture_sequence.definition",
                "device_sync_outbox",
                "capture_sequence",
                "INTEGER",
                true,
                Some("0"),
            ),
            (
                "M63.column.quarantine.source_device_id.definition",
                "device_sync_quarantine",
                "source_device_id",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.quarantine.source_sequence.definition",
                "device_sync_quarantine",
                "source_sequence",
                "INTEGER",
                true,
                None,
            ),
            (
                "M63.column.quarantine.status.definition",
                "device_sync_quarantine",
                "status",
                "TEXT",
                true,
                Some("'active'"),
            ),
            (
                "M63.column.quarantine.retry_count.definition",
                "device_sync_quarantine",
                "retry_count",
                "INTEGER",
                true,
                Some("1"),
            ),
            (
                "M63.column.quarantine.last_error_code.definition",
                "device_sync_quarantine",
                "last_error_code",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.group_id.definition",
                "device_sync_export_drafts",
                "group_id",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.local_device_id.definition",
                "device_sync_export_drafts",
                "local_device_id",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.sequence.definition",
                "device_sync_export_drafts",
                "sequence",
                "INTEGER",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.key_epoch.definition",
                "device_sync_export_drafts",
                "key_epoch",
                "INTEGER",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.previous_manifest_hash.definition",
                "device_sync_export_drafts",
                "previous_manifest_hash",
                "TEXT",
                false,
                None,
            ),
            (
                "M63.column.export_drafts.event_envelope_bytes.definition",
                "device_sync_export_drafts",
                "event_envelope_bytes",
                "BLOB",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.manifest_envelope_bytes.definition",
                "device_sync_export_drafts",
                "manifest_envelope_bytes",
                "BLOB",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.event_ciphertext_sha256.definition",
                "device_sync_export_drafts",
                "event_ciphertext_sha256",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.manifest_ciphertext_sha256.definition",
                "device_sync_export_drafts",
                "manifest_ciphertext_sha256",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.operation_ids_json.definition",
                "device_sync_export_drafts",
                "operation_ids_json",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.operation_fingerprint.definition",
                "device_sync_export_drafts",
                "operation_fingerprint",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.state.definition",
                "device_sync_export_drafts",
                "state",
                "TEXT",
                true,
                Some("'prepared'"),
            ),
            (
                "M63.column.export_drafts.created_at.definition",
                "device_sync_export_drafts",
                "created_at",
                "TEXT",
                true,
                Some("datetime('now')"),
            ),
            (
                "M63.column.export_drafts.updated_at.definition",
                "device_sync_export_drafts",
                "updated_at",
                "TEXT",
                true,
                Some("datetime('now')"),
            ),
            (
                "M63.column.export_drafts.finalized_at.definition",
                "device_sync_export_drafts",
                "finalized_at",
                "TEXT",
                false,
                None,
            ),
        ] {
            if !column_definition_matches(
                pool,
                table,
                column,
                expected_type,
                not_null,
                default_value,
            )
            .await?
            {
                missing.push(MissingSentinel {
                    migration_version: 63,
                    code,
                });
            }
        }

        if !schema_object_sql_matches(
            pool,
            "table",
            "device_sync_quarantine",
            "device_sync_quarantine",
            M63_QUARANTINE_TABLE_SQL,
        )
        .await?
        {
            missing.push(MissingSentinel {
                migration_version: 63,
                code: "M63.table.device_sync_quarantine.definition",
            });
        }

        if !schema_object_sql_matches(
            pool,
            "table",
            "device_sync_export_drafts",
            "device_sync_export_drafts",
            M63_EXPORT_DRAFTS_TABLE_SQL,
        )
        .await?
        {
            missing.push(MissingSentinel {
                migration_version: 63,
                code: "M63.table.device_sync_export_drafts.definition",
            });
        }
    }

    for (version, code, index) in [
        (
            49,
            "M49.index.idx_feishu_sync_inbox_status",
            "idx_feishu_sync_inbox_status",
        ),
        (
            58,
            "M58.index.idx_device_sync_outbox_pending",
            "idx_device_sync_outbox_pending",
        ),
        (
            60,
            "M60.index.idx_case_domain_status_migration_audits_case",
            "idx_case_domain_status_migration_audits_case",
        ),
        (
            61,
            "M61.index.idx_feishu_sync_operation_audits_preview",
            "idx_feishu_sync_operation_audits_preview",
        ),
        (
            62,
            "M62.index.idx_feishu_sync_entity_previews_pending",
            "idx_feishu_sync_entity_previews_pending",
        ),
    ] {
        if applied_versions.contains(&version) && !object_exists(pool, "index", index).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    if applied_versions.contains(&63) {
        for (code, table, index, unique, partial, expected_sql) in [
            (
                "M63.index.idx_device_sync_quarantine_active_key",
                "device_sync_quarantine",
                "idx_device_sync_quarantine_active_key",
                true,
                true,
                M63_ACTIVE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_quarantine_group_status",
                "device_sync_quarantine",
                "idx_device_sync_quarantine_group_status",
                false,
                false,
                M63_GROUP_STATUS_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_outbox_capture_sequence",
                "device_sync_outbox",
                "idx_device_sync_outbox_capture_sequence",
                true,
                false,
                M63_OUTBOX_CAPTURE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_outbox_pending_capture",
                "device_sync_outbox",
                "idx_device_sync_outbox_pending_capture",
                false,
                false,
                M63_OUTBOX_PENDING_CAPTURE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_export_drafts_state",
                "device_sync_export_drafts",
                "idx_device_sync_export_drafts_state",
                false,
                false,
                M63_EXPORT_DRAFTS_STATE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_export_drafts_one_prepared",
                "device_sync_export_drafts",
                "idx_device_sync_export_drafts_one_prepared",
                true,
                true,
                M63_EXPORT_DRAFTS_ONE_PREPARED_INDEX_SQL,
            ),
        ] {
            if !index_definition_matches(pool, table, index, unique, partial, expected_sql).await? {
                missing.push(MissingSentinel {
                    migration_version: 63,
                    code,
                });
            }
        }
    }

    for (version, code, trigger) in [
        (
            58,
            "M58.trigger.device_sync_cases_insert",
            "device_sync_cases_insert",
        ),
        (
            58,
            "M58.trigger.device_sync_contacts_insert",
            "device_sync_contacts_insert",
        ),
        (
            59,
            "M59.trigger.device_sync_skill_binding_suppressions_insert",
            "device_sync_skill_binding_suppressions_insert",
        ),
        (
            59,
            "M59.trigger.device_sync_skill_binding_suppressions_update",
            "device_sync_skill_binding_suppressions_update",
        ),
        (
            59,
            "M59.trigger.device_sync_skill_binding_suppressions_delete",
            "device_sync_skill_binding_suppressions_delete",
        ),
        (
            60,
            "M60.trigger.case_stage_items_domain_guard_insert",
            "case_stage_items_domain_guard_insert",
        ),
        (
            60,
            "M60.trigger.case_stage_items_domain_guard_update",
            "case_stage_items_domain_guard_update",
        ),
    ] {
        if applied_versions.contains(&version) && !object_exists(pool, "trigger", trigger).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    for (version, code, table, from, target_table, target_column, on_delete) in [
        (
            49,
            "M49.fk.inbox.bound_case_id",
            "feishu_sync_inbox",
            "bound_case_id",
            "cases",
            "id",
            "SET NULL",
        ),
        (
            51,
            "M51.fk.binding_audit.inbox_id",
            "feishu_sync_binding_audits",
            "inbox_id",
            "feishu_sync_inbox",
            "id",
            "CASCADE",
        ),
        (
            51,
            "M51.fk.binding_audit.previous_case_id",
            "feishu_sync_binding_audits",
            "previous_case_id",
            "cases",
            "id",
            "SET NULL",
        ),
        (
            58,
            "M58.fk.member.group_id",
            "device_sync_members",
            "group_id",
            "device_sync_groups",
            "id",
            "CASCADE",
        ),
        (
            61,
            "M61.fk.operation_audit.preview_id",
            "feishu_sync_operation_audits",
            "preview_id",
            "feishu_sync_field_previews",
            "id",
            "SET NULL",
        ),
        (
            62,
            "M62.fk.entity_preview.case_id",
            "feishu_sync_entity_previews",
            "case_id",
            "cases",
            "id",
            "CASCADE",
        ),
        (
            63,
            "M63.fk.export_drafts.group_id",
            "device_sync_export_drafts",
            "group_id",
            "device_sync_groups",
            "id",
            "CASCADE",
        ),
    ] {
        if applied_versions.contains(&version)
            && !foreign_key_exists(pool, table, from, target_table, target_column, on_delete)
                .await?
        {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    if applied_versions.contains(&58) {
        let (version, code) = if applied_versions.contains(&63) {
            (63, "M63.fk.quarantine.group_id")
        } else {
            (58, "M58.fk.quarantine.group_id")
        };
        if !foreign_key_exists(
            pool,
            "device_sync_quarantine",
            "group_id",
            "device_sync_groups",
            "id",
            "SET NULL",
        )
        .await?
        {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    Ok(missing)
}

async fn legacy_migration_36_schema_matches(pool: &SqlitePool) -> Result<bool, DbError> {
    if !object_exists(pool, "table", "feishu_reminder_runs").await? {
        return Ok(false);
    }

    let columns: Vec<TableColumnDefinition> = sqlx::query_as(
        "SELECT cid, name, type AS data_type, \"notnull\" AS not_null, \
                dflt_value AS default_value, pk AS primary_key_order, hidden \
         FROM pragma_table_xinfo('feishu_reminder_runs') ORDER BY cid",
    )
    .fetch_all(pool)
    .await
    .map_err(schema_metadata_unreadable)?;
    let expected_columns = vec![
        TableColumnDefinition {
            cid: 0,
            name: "sent_date".to_string(),
            data_type: "TEXT".to_string(),
            not_null: 1,
            default_value: None,
            primary_key_order: 1,
            hidden: 0,
        },
        TableColumnDefinition {
            cid: 1,
            name: "sent_at".to_string(),
            data_type: "TEXT".to_string(),
            not_null: 1,
            default_value: Some("datetime('now')".to_string()),
            primary_key_order: 0,
            hidden: 0,
        },
        TableColumnDefinition {
            cid: 2,
            name: "item_count".to_string(),
            data_type: "INTEGER".to_string(),
            not_null: 1,
            default_value: Some("0".to_string()),
            primary_key_order: 0,
            hidden: 0,
        },
    ];
    if columns != expected_columns {
        return Ok(false);
    }

    let table_list: Option<TableListDefinition> = sqlx::query_as(
        "SELECT type AS object_type, ncol AS column_count, wr AS without_rowid, strict \
         FROM pragma_table_list WHERE schema='main' AND name='feishu_reminder_runs'",
    )
    .fetch_optional(pool)
    .await
    .map_err(schema_metadata_unreadable)?;
    if table_list
        != Some(TableListDefinition {
            object_type: "table".to_string(),
            column_count: 3,
            without_rowid: 0,
            strict: 0,
        })
    {
        return Ok(false);
    }

    let ddl: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master \
         WHERE type='table' AND name='feishu_reminder_runs' \
           AND tbl_name='feishu_reminder_runs'",
    )
    .fetch_optional(pool)
    .await
    .map_err(schema_metadata_unreadable)?;
    let Some(ddl) = ddl else {
        return Ok(false);
    };
    let normalized_ddl = normalize_schema_sql(&ddl);
    if normalized_ddl != normalize_schema_sql(LEGACY_MIGRATION_36_TABLE_SQL) {
        return Ok(false);
    }

    let indexes: Vec<IndexListDefinition> = sqlx::query_as(
        "SELECT seq AS sequence, name, \"unique\", origin, partial \
         FROM pragma_index_list('feishu_reminder_runs') ORDER BY seq",
    )
    .fetch_all(pool)
    .await
    .map_err(schema_metadata_unreadable)?;
    if indexes
        != vec![IndexListDefinition {
            sequence: 0,
            name: LEGACY_MIGRATION_36_PRIMARY_KEY_INDEX.to_string(),
            unique: 1,
            origin: "pk".to_string(),
            partial: 0,
        }]
    {
        return Ok(false);
    }

    let index_columns: Vec<IndexColumnDefinition> = sqlx::query_as(
        "SELECT seqno AS sequence, cid AS column_id, name, \"desc\" AS descending, \
                coll AS collation, \"key\" AS is_key \
         FROM pragma_index_xinfo('sqlite_autoindex_feishu_reminder_runs_1') \
         ORDER BY seqno",
    )
    .fetch_all(pool)
    .await
    .map_err(schema_metadata_unreadable)?;
    Ok(index_columns
        == vec![
            IndexColumnDefinition {
                sequence: 0,
                column_id: 0,
                name: Some("sent_date".to_string()),
                descending: 0,
                collation: Some("BINARY".to_string()),
                is_key: 1,
            },
            IndexColumnDefinition {
                sequence: 1,
                column_id: -1,
                name: None,
                descending: 0,
                collation: Some("BINARY".to_string()),
                is_key: 0,
            },
        ])
}

async fn object_exists(pool: &SqlitePool, object_type: &str, name: &str) -> Result<bool, DbError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2)",
    )
    .bind(object_type)
    .bind(name)
    .fetch_one(pool)
    .await
    .map(|exists| exists == 1)
    .map_err(schema_metadata_unreadable)
}

async fn has_user_schema_objects_other_than_migration_table(
    pool: &SqlitePool,
) -> Result<bool, DbError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(\
            SELECT 1 FROM sqlite_master \
            WHERE type IN ('table', 'view', 'trigger', 'index') \
              AND name NOT GLOB 'sqlite_*' \
              AND name <> '_sqlx_migrations'\
        )",
    )
    .fetch_one(pool)
    .await
    .map(|exists| exists == 1)
    .map_err(schema_metadata_unreadable)
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> Result<bool, DbError> {
    let query = format!("PRAGMA table_info(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    for row in rows {
        let name: String = row.try_get("name").map_err(schema_metadata_unreadable)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn column_definition_matches(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    expected_type: &str,
    expected_not_null: bool,
    expected_default: Option<&str>,
) -> Result<bool, DbError> {
    let query = format!("PRAGMA table_info(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    for row in rows {
        let name: String = row.try_get("name").map_err(schema_metadata_unreadable)?;
        if name != column {
            continue;
        }
        let data_type: String = row.try_get("type").map_err(schema_metadata_unreadable)?;
        let not_null: i64 = row.try_get("notnull").map_err(schema_metadata_unreadable)?;
        let default_value: Option<String> = row
            .try_get("dflt_value")
            .map_err(schema_metadata_unreadable)?;
        return Ok(data_type.eq_ignore_ascii_case(expected_type)
            && (not_null == 1) == expected_not_null
            && default_value.as_deref().map(normalize_schema_sql)
                == expected_default.map(normalize_schema_sql));
    }
    Ok(false)
}

async fn index_definition_matches(
    pool: &SqlitePool,
    table: &str,
    index: &str,
    expected_unique: bool,
    expected_partial: bool,
    expected_sql: &str,
) -> Result<bool, DbError> {
    let query = format!("PRAGMA index_list(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    let mut flags_match = false;
    for row in rows {
        let row_name: String = row.try_get("name").map_err(schema_metadata_unreadable)?;
        if row_name != index {
            continue;
        }
        let unique: i64 = row.try_get("unique").map_err(schema_metadata_unreadable)?;
        let partial: i64 = row.try_get("partial").map_err(schema_metadata_unreadable)?;
        flags_match = (unique == 1) == expected_unique && (partial == 1) == expected_partial;
        break;
    }
    if !flags_match {
        return Ok(false);
    }

    schema_object_sql_matches(pool, "index", index, table, expected_sql).await
}

async fn schema_object_sql_matches(
    pool: &SqlitePool,
    object_type: &str,
    name: &str,
    table: &str,
    expected_sql: &str,
) -> Result<bool, DbError> {
    let sql: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(sql, '') FROM sqlite_master \
         WHERE type = ?1 AND name = ?2 AND tbl_name = ?3",
    )
    .bind(object_type)
    .bind(name)
    .bind(table)
    .fetch_optional(pool)
    .await
    .map_err(schema_metadata_unreadable)?;
    let Some(sql) = sql else {
        return Ok(false);
    };
    Ok(normalize_schema_sql(&sql) == normalize_schema_sql(expected_sql))
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

async fn foreign_key_exists(
    pool: &SqlitePool,
    table: &str,
    from: &str,
    target_table: &str,
    target_column: &str,
    on_delete: &str,
) -> Result<bool, DbError> {
    let query = format!("PRAGMA foreign_key_list(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    for row in rows {
        let row_from: String = row.try_get("from").map_err(schema_metadata_unreadable)?;
        let row_table: String = row.try_get("table").map_err(schema_metadata_unreadable)?;
        let row_to: String = row.try_get("to").map_err(schema_metadata_unreadable)?;
        let row_on_delete: String = row
            .try_get("on_delete")
            .map_err(schema_metadata_unreadable)?;
        if row_from == from
            && row_table == target_table
            && row_to == target_column
            && row_on_delete == on_delete
        {
            return Ok(true);
        }
    }
    Ok(false)
}
