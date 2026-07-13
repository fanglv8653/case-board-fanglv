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

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

pub mod bookmarks;
pub mod calendar_events;
pub mod case_instances;
pub mod case_work_items;
pub mod cases;
pub mod chat;
pub mod chat_tasks;
pub mod contract_drafts;
pub mod contract_preferences;
pub mod court_filing;
pub mod credits;
pub mod criminal_cases;
pub mod document_tags;
pub mod documents;
pub mod income_records;
pub mod lawyer_profiles;
pub mod metrics;
pub mod payments;
pub mod seed;
pub mod todos;

/// `directories` 用的标识——macOS 上这会拼成 `~/Library/Application Support/FanglvCaseBoard/`
const APP_QUALIFIER: &str = "";
const APP_ORG: &str = "";
const APP_NAME: &str = "FanglvCaseBoard";
const LEGACY_APP_NAME: &str = "CaseBoard";

/// 显式指定应用数据根目录，用于自动化验证、便携或隔离运行。
///
/// 该值必须是绝对目录；设置后不会读取或迁移默认/旧版数据目录。
pub const CASEBOARD_DATA_DIR_ENV: &str = "CASEBOARD_DATA_DIR";

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

    copy_dir_missing_only(&legacy, current)?;
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

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .map_err(|e| DbError::Connect(e.to_string()))?;

    // 2026-06-15:跑迁移前先对齐 _sqlx_migrations 校验值,根治「migration N ... has been modified」
    // 启动崩溃。病根 = 双轨发布(私人仓 vs 开源仓)对**同一批已发布迁移**做了去身份化注释改动
    // (`lawtools.top`→`lawtools.top`、本地路径→泛化),SQL 一字未改但 SHA-384 变了 → 老用户 DB 里
    // 存的旧校验值对不上新二进制内嵌值 → sqlx 启动中止(release 是 panic=abort,直接闪退)。
    // 详见 docs/反馈问题排查-2026-06-15.md。
    reconcile_migration_checksums(&pool).await?;

    // 2026-06-18(整合外部 PR #13 @zzf516988659-del):容忍「DB 里已 applied 但本二进制 resolved
    // 里没有」的迁移行(sqlx 0.8 默认遇此 panic)。病根 = 跨 fork/跨仓发布节奏漂移:用户先装了某
    // fork binary(内嵌更多迁移、apply 过)、再装主仓 binary(内嵌较少)→ 启动报「migration N
    // previously applied but missing」直接闪退。已 applied 的不会重跑,schema 不受影响。
    // 配合上面的 reconcile_migration_checksums,是跨仓发布漂移的最后一道兜底。
    sqlx::migrate!("./migrations")
        .set_ignore_missing(true)
        .run(&pool)
        .await
        .map_err(|e| DbError::Migrate(e.to_string()))?;

    Ok(pool)
}

/// 把已存在的 `_sqlx_migrations.checksum` 对齐到本二进制内嵌的迁移校验值。
///
/// 仅当该表已存在(= 非全新库,跑过至少一次迁移)时才动;逐条只在校验值**不同**时更新并 dlog。
/// SQL 一字未改(只是注释/项目名漂移),已应用的迁移 sqlx 本就不会重跑 —— 对齐校验值不改变任何
/// 已执行的 SQL、不动数据,只是消掉「文件被动过」这道与双轨发布天然冲突的 tripwire。
async fn reconcile_migration_checksums(pool: &SqlitePool) -> Result<(), DbError> {
    // 全新库还没这张表 → 无需对齐(后续 migrate 会正常建表并全量应用)。
    let table_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| DbError::Migrate(e.to_string()))?;
    if table_exists.is_none() {
        return Ok(());
    }

    for m in sqlx::migrate!("./migrations").iter() {
        let embedded: &[u8] = &m.checksum;
        let stored: Option<(Vec<u8>,)> =
            sqlx::query_as("SELECT checksum FROM _sqlx_migrations WHERE version = ?1")
                .bind(m.version)
                .fetch_optional(pool)
                .await
                .map_err(|e| DbError::Migrate(e.to_string()))?;
        if let Some((stored,)) = stored {
            if stored.as_slice() != embedded {
                sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2")
                    .bind(embedded)
                    .bind(m.version)
                    .execute(pool)
                    .await
                    .map_err(|e| DbError::Migrate(e.to_string()))?;
                crate::dlog!(
                    "[db] 迁移 {} 校验值与内嵌不一致,已对齐(注释漂移,SQL 未变)",
                    m.version
                );
            }
        }
    }
    Ok(())
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
}

impl serde::Serialize for DbError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

// ============================================================================
// 测试
// ============================================================================

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
}
