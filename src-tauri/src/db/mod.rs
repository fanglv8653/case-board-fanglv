//! 数据库连接池与 schema migrations。
//!
//! V0.1 用 SQLite + sqlx。数据库文件落在 macOS 标准 app data 目录:
//!   `~/Library/Application Support/CaseBoard/caseboard.db`
//!
//! 启动流程:
//!   1. 拿到 app data dir(`directories` crate 跨平台)
//!   2. 确保目录存在(首次启动)
//!   3. 创建 SqlitePool(`?mode=rwc` 不存在自动建)
//!   4. 跑 migrations(`sqlx::migrate!`)
//!
//! 测试模式可以传 `sqlite::memory:` 跑内存库,不污染本机文件系统。

use std::borrow::Cow;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

mod migration_safety;

#[cfg(test)]
type InitPoolAfterPreflightAction = Box<dyn FnOnce(&Path) + Send + 'static>;

#[cfg(test)]
struct InitPoolAfterPreflightHook {
    database_path: PathBuf,
    action: InitPoolAfterPreflightAction,
}

#[cfg(test)]
static INIT_POOL_AFTER_PREFLIGHT_HOOK: std::sync::Mutex<Option<InitPoolAfterPreflightHook>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn install_init_pool_after_preflight_hook(
    database_path: PathBuf,
    action: impl FnOnce(&Path) + Send + 'static,
) {
    let mut slot = INIT_POOL_AFTER_PREFLIGHT_HOOK
        .lock()
        .expect("init_pool after-preflight hook mutex poisoned");
    assert!(
        slot.is_none(),
        "an init_pool after-preflight hook is already installed"
    );
    *slot = Some(InitPoolAfterPreflightHook {
        database_path,
        action: Box::new(action),
    });
}

#[cfg(test)]
fn run_init_pool_after_preflight_hook(database_path: &Path) {
    let hook = {
        let mut slot = INIT_POOL_AFTER_PREFLIGHT_HOOK
            .lock()
            .expect("init_pool after-preflight hook mutex poisoned");
        if slot
            .as_ref()
            .is_some_and(|hook| hook.database_path.as_path() == database_path)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(hook) = hook {
        (hook.action)(database_path);
    }
}

pub mod bookmarks;
pub mod calendar_events;
pub mod case_instances;
pub mod case_memory;
pub mod case_work_items;
pub mod cases;
pub mod chat;
pub mod chat_tasks;
pub mod contract_drafts;
pub mod contract_preferences;
pub mod court_filing;
pub mod credits;
pub mod criminal_cases;
pub mod criminal_extraction_candidates;
pub mod criminal_sentencing_estimates;
pub mod criminal_workflows;
pub mod criminal_workspace;
pub mod document_tags;
pub mod documents;
pub mod feishu_entities;
pub mod feishu_sync;
pub mod income_records;
pub mod lawyer_profiles;
pub mod material_queue;
pub mod metrics;
pub mod payments;
pub mod seed;
pub mod todos;
pub mod usage_dashboard;

#[cfg(test)]
mod feishu_f1_tests;

/// `directories` 用的标识——macOS 上这会拼成 `~/Library/Application Support/FanglvCaseBoard/`
const APP_QUALIFIER: &str = "";
const APP_ORG: &str = "";
const APP_NAME: &str = "FanglvCaseBoard";
const LEGACY_APP_NAME: &str = "CaseBoard";

/// 显式指定应用数据根目录，用于自动化验证、便携或隔离运行。
///
/// 该值必须是绝对目录；设置后不会读取或迁移默认/旧版数据目录。
pub const CASEBOARD_DATA_DIR_ENV: &str = "CASEBOARD_DATA_DIR";

// 由 build.rs 根据全部 SQL 迁移内容生成。显式引用可确保迁移集合变化时本模块
// 被重新编译，避免 sqlx::migrate! 在本机增量 Release 中沿用旧宏展开。
const _MIGRATION_BUILD_FINGERPRINT: &str = env!("CASEBOARD_MIGRATION_BUILD_FINGERPRINT");

/// 拿到当前操作系统下方律案件看板的数据目录路径。
///
/// macOS: `~/Library/Application Support/FanglvCaseBoard/`
/// Linux: `~/.local/share/FanglvCaseBoard/`
/// Windows: `%APPDATA%\FanglvCaseBoard\data\`
pub fn app_data_dir() -> Result<PathBuf, DbError> {
    let override_value = std::env::var_os(CASEBOARD_DATA_DIR_ENV);
    if override_value.is_some() {
        // 覆盖模式不得触碰 ProjectDirs 或旧版数据目录，避免自动化运行访问正式库。
        return app_data_dir_from_paths(override_value, None, None);
    }

    let current = project_data_dir(APP_NAME)?;
    let legacy = project_data_dir(LEGACY_APP_NAME)?;
    app_data_dir_from_paths(None, Some(current), Some(legacy))
}

/// 默认数据库文件路径(`<app_data_dir>/caseboard.db`)。
pub fn default_db_path() -> Result<PathBuf, DbError> {
    Ok(app_data_dir()?.join("caseboard.db"))
}

fn project_data_dir(app_name: &str) -> Result<PathBuf, DbError> {
    let proj =
        ProjectDirs::from(APP_QUALIFIER, APP_ORG, app_name).ok_or(DbError::HomeDirNotFound)?;
    Ok(proj.data_dir().to_path_buf())
}

/// Returns the default current/legacy data directories only when no explicit
/// override is active. Callers must treat `None` as a hard boundary and never
/// inspect either default directory.
pub(crate) fn default_data_dirs_if_unoverridden() -> Result<Option<(PathBuf, PathBuf)>, DbError> {
    let override_value = std::env::var_os(CASEBOARD_DATA_DIR_ENV);
    if override_value.is_some() {
        return default_data_dirs_from_paths(override_value, None, None);
    }
    default_data_dirs_from_paths(
        None,
        Some(project_data_dir(APP_NAME)?),
        Some(project_data_dir(LEGACY_APP_NAME)?),
    )
}

fn default_data_dirs_from_paths(
    override_value: Option<OsString>,
    current: Option<PathBuf>,
    legacy: Option<PathBuf>,
) -> Result<Option<(PathBuf, PathBuf)>, DbError> {
    if override_value.is_some() {
        return Ok(None);
    }
    Ok(Some((
        current.ok_or(DbError::HomeDirNotFound)?,
        legacy.ok_or(DbError::HomeDirNotFound)?,
    )))
}

fn app_data_dir_from_paths(
    override_value: Option<OsString>,
    current: Option<PathBuf>,
    legacy: Option<PathBuf>,
) -> Result<PathBuf, DbError> {
    if let Some(override_dir) = data_dir_override_from_value(override_value)? {
        return Ok(override_dir);
    }

    let current = current.ok_or(DbError::HomeDirNotFound)?;
    let legacy = legacy.ok_or(DbError::HomeDirNotFound)?;
    migrate_legacy_data_dir_if_needed(&current, &legacy)?;
    Ok(current)
}

fn data_dir_override_from_value(value: Option<OsString>) -> Result<Option<PathBuf>, DbError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Err(DbError::DataDirOverrideInvalid("不能为空".to_string()));
    }

    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(DbError::DataDirOverrideInvalid(format!(
            "必须是绝对路径: {}",
            path.display()
        )));
    }
    if path.exists() && !path.is_dir() {
        return Err(DbError::DataDirOverrideInvalid(format!(
            "必须指向目录: {}",
            path.display()
        )));
    }
    Ok(Some(path))
}

fn migrate_legacy_data_dir_if_needed(current: &Path, legacy: &Path) -> Result<(), DbError> {
    let current_db = current.join("caseboard.db");
    if current_db.exists() {
        return Ok(());
    }

    let legacy_db = legacy.join("caseboard.db");
    if !legacy_db.exists() || legacy == current {
        return Ok(());
    }

    copy_dir_missing_only(legacy, current)?;
    crate::dlog!(
        "[db] 已从旧数据目录 {} 复制到新数据目录 {}",
        legacy.display(),
        current.display()
    );
    Ok(())
}

fn copy_dir_missing_only(src: &Path, dst: &Path) -> Result<(), DbError> {
    fs::create_dir_all(dst).map_err(|e| DbError::Io(e.to_string()))?;
    for entry in fs::read_dir(src).map_err(|e| DbError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| DbError::Io(e.to_string()))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if dst_path.exists() {
            continue;
        }
        if src_path.is_dir() {
            copy_dir_missing_only(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| DbError::Io(e.to_string()))?;
        }
    }
    Ok(())
}

/// 初始化连接池:确保目录存在、连接、跑 migrations。
///
/// `db_path` 可以是真实路径(`PathBuf::from("...caseboard.db")`)或者特殊串:
///   - `:memory:` —— 内存库,测试用
pub async fn init_pool(db_path: &str) -> Result<SqlitePool, DbError> {
    // 如果不是内存库,先确保父目录存在
    if db_path != ":memory:" {
        let path = PathBuf::from(db_path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| DbError::Io(e.to_string()))?;
        }
    }

    let is_memory = db_path == ":memory:";
    if !is_memory {
        // Refuse orphaned as well as paired WAL/SHM files before deciding
        // whether the main database itself is new or existing.
        migration_safety::ensure_no_wal_sidecars(Path::new(db_path))?;
    }
    let is_existing_file = !is_memory && Path::new(db_path).is_file();

    // Existing databases are inspected through a separate read-only connection
    // before any read-write/WAL connection is created. This ordering is the
    // fail-closed boundary: incompatible lineage must not reach a connection
    // option or migration step that can write the database header, WAL, schema,
    // migration history or business tables.
    let migration_preflight = if is_existing_file {
        migration_safety::preflight_existing_database(Path::new(db_path)).await?
    } else {
        migration_safety::MigrationPreflight::default()
    };

    #[cfg(test)]
    if is_existing_file {
        run_init_pool_after_preflight_hook(Path::new(db_path));
    }

    let mut options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);

    // 文件库走 WAL(并发友好),内存库不能用 WAL
    if !is_memory {
        options = options.journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    }

    // 内存库每个连接是独立的 SQLite 实例 → 必须只用 1 个连接,否则
    // migration 跑完表只在那一个连接里,其他连接看不到
    let max_connections = if is_memory { 1 } else { 5 };

    if !is_memory {
        // Close the preflight-to-write sidecar window as far as this process
        // can without claiming an OS-wide SQLite lock. A sidecar created after
        // immutable preflight must be refused before the read-write pool opens.
        migration_safety::ensure_no_wal_sidecars(Path::new(db_path))?;
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .map_err(|e| DbError::Connect(e.to_string()))?;

    let embedded_migrator = sqlx::migrate!("./migrations");
    if migration_preflight.allow_missing_legacy_migration_36 {
        // Add only the fixed compatibility metadata. `ignore_missing` remains
        // false, so SQLx still rejects every unknown applied version other than
        // the explicitly represented version 36. Its placeholder SQL is
        // an unconditional SQLite syntax error if a different file without
        // applied v36 is substituted after immutable preflight.
        let mut migrations: Vec<_> = embedded_migrator.iter().cloned().collect();
        let legacy_migration = migration_safety::legacy_migration_36_metadata();
        let insertion_index =
            migrations.partition_point(|migration| migration.version < legacy_migration.version);
        migrations.insert(insertion_index, legacy_migration);
        let compatible_migrator = Migrator {
            migrations: Cow::Owned(migrations),
            ignore_missing: false,
            locking: embedded_migrator.locking,
            no_tx: embedded_migrator.no_tx,
        };
        compatible_migrator
            .run(&pool)
            .await
            .map_err(|e| DbError::Migrate(e.to_string()))?;
    } else {
        // Unknown applied versions have already been rejected by the read-only
        // preflight. Keep sqlx's default ignore_missing=false as a second guard.
        embedded_migrator
            .run(&pool)
            .await
            .map_err(|e| DbError::Migrate(e.to_string()))?;
    }

    // A process cannot safely prove that previously-running external work is
    // still alive. Move it to an explicit user-reviewed recovery state in one
    // transaction; never auto-resume or consume provider quota on startup.
    material_queue::recover_interrupted_material_processing(&pool)
        .await
        .map_err(|e| DbError::Migrate(format!("恢复材料处理队列失败: {e}")))?;

    Ok(pool)
}

pub const DB_MIGRATION_CHECKSUM_UNKNOWN: &str = "DB_MIGRATION_CHECKSUM_UNKNOWN";
pub const DB_MIGRATION_APPLIED_VERSION_UNKNOWN: &str = "DB_MIGRATION_APPLIED_VERSION_UNKNOWN";
pub const DB_MIGRATION_SCHEMA_SENTINEL_MISSING: &str = "DB_MIGRATION_SCHEMA_SENTINEL_MISSING";
pub const DB_MIGRATION_LINEAGE_INCOMPATIBLE: &str = "DB_MIGRATION_LINEAGE_INCOMPATIBLE";

/// Safe, structured metadata for migration-lineage failures. Values are
/// limited to migration/schema metadata and never contain SQL parameters or
/// business-table contents.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DbMigrationCompatibilityError {
    pub code: &'static str,
    pub version: Option<i64>,
    pub reason: &'static str,
    pub stored_checksum: Option<String>,
    pub current_checksum: Option<String>,
    pub missing_sentinels: Vec<String>,
}

impl std::fmt::Display for DbMigrationCompatibilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({})", self.code, self.reason)
    }
}

/// 数据库相关错误。映射到前端友好的字符串。
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("找不到用户主目录")]
    HomeDirNotFound,
    #[error("CASEBOARD_DATA_DIR 无效: {0}")]
    DataDirOverrideInvalid(String),
    #[error("IO 错误: {0}")]
    Io(String),
    #[error("数据库连接失败: {0}")]
    Connect(String),
    #[error("数据库迁移失败: {0}")]
    Migrate(String),
    #[error("数据库迁移兼容性检查失败: {0}")]
    MigrationCompatibility(DbMigrationCompatibilityError),
}

impl DbError {
    pub fn migration_compatibility(&self) -> Option<&DbMigrationCompatibilityError> {
        match self {
            Self::MigrationCompatibility(error) => Some(error),
            _ => None,
        }
    }

    pub fn startup_recovery_message(&self, db_path: &str) -> Option<String> {
        let error = self.migration_compatibility()?;
        let summary = match error.code {
            DB_MIGRATION_CHECKSUM_UNKNOWN => "检测到无法验证的数据库迁移校验值。",
            DB_MIGRATION_APPLIED_VERSION_UNKNOWN => "数据库包含当前版本无法识别的已应用迁移。",
            DB_MIGRATION_SCHEMA_SENTINEL_MISSING => "数据库结构与已记录的迁移历史不一致。",
            DB_MIGRATION_LINEAGE_INCOMPATIBLE => "数据库迁移谱系不兼容。",
            _ => "数据库迁移兼容性检查未通过。",
        };
        Some(format!(
            "{summary}\n\n错误码：{}\n数据库位置：{db_path}\n\n为保护现有数据，本次已在写入前停止，未继续迁移、重建、覆盖或删除数据库。\n不要删除、重命名、分离或单独处理数据库旁的 WAL/SHM 文件。请先将数据库及 WAL/SHM 原样完整备份，再由支持人员在隔离副本上进行受控 checkpoint、恢复与只读谱系审计。\n\n关闭本提示后，方律案件看板将退出。",
            error.code
        ))
    }
}

impl serde::Serialize for DbError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::MigrationCompatibility(error) => serde::Serialize::serialize(error, s),
            _ => s.serialize_str(&self.to_string()),
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod migration_lineage_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_path(label: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "caseboard-db-test-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn no_override_keeps_default_path_and_legacy_migration_behavior() {
        let root = temp_path("default");
        let current = root.join("current");
        let legacy = root.join("legacy");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("caseboard.db"), b"legacy-db").unwrap();

        let actual = app_data_dir_from_paths(None, Some(current.clone()), Some(legacy)).unwrap();

        assert_eq!(actual, current);
        assert_eq!(
            fs::read(current.join("caseboard.db")).unwrap(),
            b"legacy-db"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn absolute_override_is_used_without_mutating_process_environment() {
        let root = temp_path("override");
        let override_dir = root.join("isolated");
        let actual =
            app_data_dir_from_paths(Some(override_dir.clone().into_os_string()), None, None)
                .unwrap();

        assert_eq!(actual, override_dir);
        assert!(!actual.exists());
    }

    #[test]
    fn empty_relative_or_file_override_is_rejected() {
        let empty = data_dir_override_from_value(Some(OsString::new())).unwrap_err();
        assert!(matches!(empty, DbError::DataDirOverrideInvalid(_)));

        let relative = data_dir_override_from_value(Some(OsString::from("isolated"))).unwrap_err();
        assert!(matches!(relative, DbError::DataDirOverrideInvalid(_)));

        let file_path = temp_path("file");
        fs::write(&file_path, b"not-a-directory").unwrap();
        let file =
            data_dir_override_from_value(Some(file_path.clone().into_os_string())).unwrap_err();
        assert!(matches!(file, DbError::DataDirOverrideInvalid(_)));
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn override_does_not_trigger_legacy_data_migration() {
        let root = temp_path("no-legacy-copy");
        let override_dir = root.join("isolated");
        let default_dir = root.join("default");
        let legacy_dir = root.join("legacy");
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join("caseboard.db"), b"legacy-db").unwrap();

        let actual = app_data_dir_from_paths(
            Some(override_dir.clone().into_os_string()),
            Some(default_dir),
            Some(legacy_dir),
        )
        .unwrap();

        assert_eq!(actual, override_dir);
        assert!(!actual.join("caseboard.db").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn override_hides_default_settings_directories() {
        let hidden =
            default_data_dirs_from_paths(Some(OsString::from("override-is-active")), None, None)
                .expect("override boundary");

        assert!(hidden.is_none());
    }
}
