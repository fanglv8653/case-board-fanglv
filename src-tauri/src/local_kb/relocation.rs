//! 本地知识库目录切换与迁移。
//!
//! 安全边界：
//! - `switch_existing` 只校验并切换绑定，不复制知识库内容；
//! - `migrate_current` 始终先复制到目标同级临时目录，逐文件哈希复核后再原子改名；
//! - 任何失败都不会修改旧绑定，也不会删除旧目录。

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::settings::Settings;

use super::cache::IndexEntry;

const REQUIRED_DIRS: &[&str] = &["raw", "wiki"];
const REQUIRED_FILES: &[&str] = &["wiki/index.md", "gap-log.md"];
const CACHE_INDEX: &str = "raw/yuandian-cache/index.json";
const CRITICAL_FILES: &[&str] = &["wiki/index.md", "gap-log.md", CACHE_INDEX];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KbRelocationError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_path: Option<PathBuf>,
}

impl KbRelocationError {
    fn new(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            retryable,
            recovery_path: None,
        }
    }

    fn with_recovery_path(mut self, path: PathBuf) -> Self {
        self.recovery_path = Some(path);
        self
    }

    fn io(code: &str, action: &str, error: std::io::Error) -> Self {
        let effective_code = if error.kind() == std::io::ErrorKind::PermissionDenied {
            "KB_PERMISSION_DENIED"
        } else {
            code
        };
        let retryable_kind = matches!(
            error.kind(),
            std::io::ErrorKind::NotFound
                | std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
        );
        // Windows 网络共享常见错误：53 网络路径不存在、64 网络名已删除、
        // 67 找不到网络名、121 信号灯超时、1231 网络位置不可达。
        let retryable_os = matches!(error.raw_os_error(), Some(53 | 64 | 67 | 121 | 1231));
        let retryable = retryable_kind || retryable_os;
        Self::new(effective_code, format!("{action}失败：{error}"), retryable)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KbManifestSummary {
    pub file_count: u64,
    pub total_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KbCriticalFile {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KbRelocationResult {
    pub operation: String,
    pub old_root: PathBuf,
    pub new_root: PathBuf,
    /// 旧目录即迁移后的人工回退副本；后端永不删除它。
    pub backup_path: PathBuf,
    pub backup_available: bool,
    pub copied: bool,
    pub manifest: KbManifestSummary,
    pub critical_manifest: Vec<KbCriticalFile>,
    /// 根目录变化后，语义索引必须按新根全量重建。
    pub index_rebuild_required: bool,
}

trait BindingStore {
    fn load(&mut self) -> Result<Settings, KbRelocationError>;
    fn save(&mut self, settings: &Settings) -> Result<(), KbRelocationError>;
}

struct ProductionBindingStore;

impl BindingStore for ProductionBindingStore {
    fn load(&mut self) -> Result<Settings, KbRelocationError> {
        crate::settings::read_settings()
            .map_err(|_| KbRelocationError::new("KB_SETTINGS_READ_FAILED", "读取设置失败", false))
    }

    fn save(&mut self, settings: &Settings) -> Result<(), KbRelocationError> {
        crate::settings::write_settings(settings).map_err(|_| {
            KbRelocationError::new(
                "KB_BINDING_ATOMIC_REPLACE_FAILED",
                "知识库目录绑定未能原子保存，旧绑定保持不变",
                false,
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MigrationFault {
    None,
    CopyAfterFirstFile,
    CorruptBeforeVerify,
    TargetCollisionBeforeRename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileManifestEntry {
    relative_path: String,
    size: u64,
    sha256: String,
}

#[tauri::command]
pub fn switch_existing_local_kb(
    target_path: String,
) -> Result<KbRelocationResult, KbRelocationError> {
    let mut store = ProductionBindingStore;
    switch_existing_with_store(&mut store, &target_path)
}

#[tauri::command]
pub fn migrate_current_local_kb(
    target_path: String,
) -> Result<KbRelocationResult, KbRelocationError> {
    let mut store = ProductionBindingStore;
    migrate_current_with_store(&mut store, &target_path, MigrationFault::None)
}

fn switch_existing_with_store(
    store: &mut dyn BindingStore,
    target_path: &str,
) -> Result<KbRelocationResult, KbRelocationError> {
    let mut settings = store.load()?;
    // 切换用于修复“旧盘符/NAS 已离线”等场景，旧根只作审计与回退路径记录，
    // 不能把“旧根仍可访问”设为切换新根的前置条件。
    let old_root = configured_root_for_switch(&settings)?;
    let target = existing_absolute_dir(target_path, "KB_TARGET_UNAVAILABLE")?;
    reject_same_or_child(&old_root, &target)?;
    validate_compatible_kb(&target)?;
    validate_writable(&target)?;

    let entries = build_manifest(&target)?;
    let result = result_from_manifest("switch_existing", old_root, target.clone(), false, &entries);
    settings.local_kb_root = Some(target.to_string_lossy().into_owned());
    settings.local_kb_enabled = Some(true);
    store.save(&settings)?;
    Ok(result)
}

fn migrate_current_with_store(
    store: &mut dyn BindingStore,
    target_path: &str,
    fault: MigrationFault,
) -> Result<KbRelocationResult, KbRelocationError> {
    let mut settings = store.load()?;
    let source = configured_root(&settings)?;
    validate_compatible_kb(&source)?;
    let target = resolve_new_target(target_path)?;
    reject_same_or_child(&source, &target)?;
    let parent = target.parent().ok_or_else(|| {
        KbRelocationError::new("KB_TARGET_PARENT_INVALID", "目标目录缺少有效父目录", false)
    })?;
    validate_writable(parent)?;

    let source_manifest = build_manifest(&source)?;
    let temp = parent.join(format!(
        ".caseboard-kb-migrate-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir(&temp)
        .map_err(|e| KbRelocationError::io("KB_TEMP_CREATE_FAILED", "创建迁移临时目录", e))?;

    let copy_result = copy_tree(&source, &temp, fault);
    if let Err(error) = copy_result {
        cleanup_temp(&temp, parent);
        return Err(error);
    }
    if fault == MigrationFault::CorruptBeforeVerify {
        let corrupt_target = first_regular_file(&temp)?.ok_or_else(|| {
            KbRelocationError::new(
                "KB_TEST_FAULT_FAILED",
                "没有可用于校验故障注入的文件",
                false,
            )
        })?;
        OpenOptions::new()
            .append(true)
            .open(&corrupt_target)
            .and_then(|mut f| f.write_all(b"fault"))
            .map_err(|e| KbRelocationError::io("KB_TEST_FAULT_FAILED", "注入校验故障", e))?;
    }

    let copied_manifest = build_manifest(&temp)?;
    if source_manifest != copied_manifest {
        cleanup_temp(&temp, parent);
        return Err(KbRelocationError::new(
            "KB_MIGRATION_VERIFY_FAILED",
            "迁移副本的文件数量、字节数或哈希与源目录不一致，旧绑定保持不变",
            false,
        ));
    }

    if fault == MigrationFault::TargetCollisionBeforeRename {
        fs::create_dir(&target)
            .map_err(|e| KbRelocationError::io("KB_TEST_FAULT_FAILED", "注入目标竞争故障", e))?;
        fs::write(target.join("collision"), b"fault")
            .map_err(|e| KbRelocationError::io("KB_TEST_FAULT_FAILED", "注入目标竞争故障", e))?;
    }
    if let Err(error) = fs::rename(&temp, &target) {
        cleanup_temp(&temp, parent);
        return Err(KbRelocationError::io(
            "KB_TARGET_ATOMIC_RENAME_FAILED",
            "提交迁移目录",
            error,
        ));
    }

    let result = result_from_manifest(
        "migrate_current",
        source,
        target.clone(),
        true,
        &source_manifest,
    );
    settings.local_kb_root = Some(target.to_string_lossy().into_owned());
    settings.local_kb_enabled = Some(true);
    if let Err(error) = store.save(&settings) {
        return Err(error.with_recovery_path(target));
    }
    Ok(result)
}

fn configured_root(settings: &Settings) -> Result<PathBuf, KbRelocationError> {
    let raw = configured_root_raw(settings)?;
    existing_absolute_dir(raw, "KB_SOURCE_UNAVAILABLE")
}

fn configured_root_for_switch(settings: &Settings) -> Result<PathBuf, KbRelocationError> {
    let raw = configured_root_raw(settings)?;
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() {
        return Err(KbRelocationError::new(
            "KB_SOURCE_PATH_NOT_ABSOLUTE",
            "当前知识库绑定不是绝对路径，无法安全切换",
            false,
        ));
    }
    Ok(path.canonicalize().unwrap_or(path))
}

fn configured_root_raw(settings: &Settings) -> Result<&str, KbRelocationError> {
    let raw = settings
        .local_kb_root
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            KbRelocationError::new("KB_SOURCE_NOT_CONFIGURED", "尚未绑定本地知识库", false)
        })?;
    Ok(raw)
}

fn existing_absolute_dir(raw: &str, code: &str) -> Result<PathBuf, KbRelocationError> {
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() {
        return Err(KbRelocationError::new(
            "KB_PATH_NOT_ABSOLUTE",
            "知识库目录必须是绝对路径",
            false,
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| KbRelocationError::io(code, "访问知识库目录", e))?;
    if !canonical.is_dir() {
        return Err(KbRelocationError::new(
            "KB_PATH_NOT_DIRECTORY",
            "所选路径不是目录",
            false,
        ));
    }
    fs::read_dir(&canonical).map_err(|e| KbRelocationError::io(code, "读取知识库目录", e))?;
    Ok(canonical)
}

fn resolve_new_target(raw: &str) -> Result<PathBuf, KbRelocationError> {
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() {
        return Err(KbRelocationError::new(
            "KB_PATH_NOT_ABSOLUTE",
            "迁移目标必须是绝对路径",
            false,
        ));
    }
    if path.exists() {
        return Err(KbRelocationError::new(
            "KB_TARGET_ALREADY_EXISTS",
            "迁移目标已存在，请选择一个尚未创建的新目录",
            false,
        ));
    }
    let name = path.file_name().ok_or_else(|| {
        KbRelocationError::new("KB_TARGET_NAME_INVALID", "迁移目标目录名无效", false)
    })?;
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(KbRelocationError::new(
            "KB_TARGET_NAME_INVALID",
            "迁移目标不能包含 . 或 .. 路径段",
            false,
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        KbRelocationError::new("KB_TARGET_PARENT_INVALID", "迁移目标缺少父目录", false)
    })?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| KbRelocationError::io("KB_TARGET_PARENT_UNAVAILABLE", "访问目标父目录", e))?;
    if !canonical_parent.is_dir() {
        return Err(KbRelocationError::new(
            "KB_TARGET_PARENT_INVALID",
            "迁移目标的父路径不是目录",
            false,
        ));
    }
    Ok(canonical_parent.join(name))
}

fn reject_same_or_child(source: &Path, target: &Path) -> Result<(), KbRelocationError> {
    if target == source {
        return Err(KbRelocationError::new(
            "KB_TARGET_SAME_AS_SOURCE",
            "目标目录不能与当前知识库相同",
            false,
        ));
    }
    if target.starts_with(source) {
        return Err(KbRelocationError::new(
            "KB_TARGET_INSIDE_SOURCE",
            "目标目录不能位于当前知识库内部",
            false,
        ));
    }
    Ok(())
}

fn validate_compatible_kb(root: &Path) -> Result<(), KbRelocationError> {
    for relative in REQUIRED_DIRS {
        if !root.join(relative).is_dir() {
            return Err(KbRelocationError::new(
                "KB_STRUCTURE_INCOMPATIBLE",
                format!("所选目录缺少必要目录：{relative}"),
                false,
            ));
        }
    }
    for relative in REQUIRED_FILES {
        if !root.join(relative).is_file() {
            return Err(KbRelocationError::new(
                "KB_STRUCTURE_INCOMPATIBLE",
                format!("所选目录缺少必要文件：{relative}"),
                false,
            ));
        }
    }
    let index_path = root.join(CACHE_INDEX);
    if index_path.exists() {
        let raw = fs::read_to_string(&index_path)
            .map_err(|e| KbRelocationError::io("KB_INDEX_UNREADABLE", "读取知识库索引", e))?;
        let index = serde_json::from_str::<HashMap<String, IndexEntry>>(&raw).map_err(|_| {
            KbRelocationError::new(
                "KB_INDEX_INCOMPATIBLE",
                "元典缓存索引格式与当前版本不兼容",
                false,
            )
        })?;
        if index.values().any(|entry| {
            let path = Path::new(&entry.path);
            entry.path.trim().is_empty()
                || path.is_absolute()
                || path
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
        }) {
            return Err(KbRelocationError::new(
                "KB_INDEX_PATH_UNSAFE",
                "元典缓存索引包含越界路径，不能绑定该知识库",
                false,
            ));
        }
    }
    Ok(())
}

fn validate_writable(dir: &Path) -> Result<(), KbRelocationError> {
    let probe = dir.join(format!(
        ".caseboard-write-probe-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|e| KbRelocationError::io("KB_PATH_NOT_WRITABLE", "验证目录写权限", e))?;
    if let Err(error) = file.write_all(b"caseboard") {
        let _ = fs::remove_file(&probe);
        return Err(KbRelocationError::io(
            "KB_PATH_NOT_WRITABLE",
            "验证目录写权限",
            error,
        ));
    }
    drop(file);
    fs::remove_file(&probe)
        .map_err(|e| KbRelocationError::io("KB_WRITE_PROBE_CLEANUP_FAILED", "清理写权限探针", e))
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    fault: MigrationFault,
) -> Result<(), KbRelocationError> {
    let mut copied_files = 0usize;
    copy_dir_recursive(source, source, destination, fault, &mut copied_files)
}

fn copy_dir_recursive(
    root: &Path,
    current: &Path,
    destination: &Path,
    fault: MigrationFault,
    copied_files: &mut usize,
) -> Result<(), KbRelocationError> {
    for entry in fs::read_dir(current)
        .map_err(|e| KbRelocationError::io("KB_COPY_READ_FAILED", "读取迁移源目录", e))?
    {
        let entry =
            entry.map_err(|e| KbRelocationError::io("KB_COPY_READ_FAILED", "读取目录项", e))?;
        let source_path = entry.path();
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|e| KbRelocationError::io("KB_COPY_METADATA_FAILED", "读取目录项元数据", e))?;
        if metadata.file_type().is_symlink() {
            return Err(KbRelocationError::new(
                "KB_SYMLINK_UNSUPPORTED",
                "知识库包含符号链接，为防止越界复制已停止迁移",
                false,
            ));
        }
        let relative = source_path.strip_prefix(root).map_err(|_| {
            KbRelocationError::new("KB_COPY_PATH_ESCAPE", "迁移路径越过知识库根目录", false)
        })?;
        let destination_path = destination.join(relative);
        if metadata.is_dir() {
            fs::create_dir(&destination_path).map_err(|e| {
                KbRelocationError::io("KB_COPY_CREATE_DIR_FAILED", "创建迁移子目录", e)
            })?;
            copy_dir_recursive(root, &source_path, destination, fault, copied_files)?;
        } else if metadata.is_file() {
            if fault == MigrationFault::CopyAfterFirstFile && *copied_files >= 1 {
                return Err(KbRelocationError::new(
                    "KB_COPY_INJECTED_FAILURE",
                    "测试注入：复制中断",
                    false,
                ));
            }
            fs::copy(&source_path, &destination_path)
                .map_err(|e| KbRelocationError::io("KB_COPY_FILE_FAILED", "复制知识库文件", e))?;
            *copied_files += 1;
        }
    }
    Ok(())
}

fn build_manifest(root: &Path) -> Result<Vec<FileManifestEntry>, KbRelocationError> {
    let mut files = Vec::new();
    collect_manifest(root, root, &mut files)?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

fn collect_manifest(
    root: &Path,
    current: &Path,
    files: &mut Vec<FileManifestEntry>,
) -> Result<(), KbRelocationError> {
    for entry in fs::read_dir(current)
        .map_err(|e| KbRelocationError::io("KB_MANIFEST_READ_FAILED", "读取知识库清单", e))?
    {
        let entry =
            entry.map_err(|e| KbRelocationError::io("KB_MANIFEST_READ_FAILED", "读取目录项", e))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| {
            KbRelocationError::io("KB_MANIFEST_METADATA_FAILED", "读取文件元数据", e)
        })?;
        if metadata.file_type().is_symlink() {
            return Err(KbRelocationError::new(
                "KB_SYMLINK_UNSUPPORTED",
                "知识库包含符号链接，无法生成可信迁移清单",
                false,
            ));
        }
        if metadata.is_dir() {
            collect_manifest(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = normalized_relative(root, &path)?;
            files.push(FileManifestEntry {
                relative_path: relative,
                size: metadata.len(),
                sha256: hash_file(&path)?,
            });
        }
    }
    Ok(())
}

fn normalized_relative(root: &Path, path: &Path) -> Result<String, KbRelocationError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        KbRelocationError::new("KB_MANIFEST_PATH_ESCAPE", "清单路径越过知识库根目录", false)
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn hash_file(path: &Path) -> Result<String, KbRelocationError> {
    let mut file = File::open(path)
        .map_err(|e| KbRelocationError::io("KB_MANIFEST_FILE_UNREADABLE", "读取清单文件", e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| KbRelocationError::io("KB_MANIFEST_FILE_UNREADABLE", "读取清单文件", e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn result_from_manifest(
    operation: &str,
    old_root: PathBuf,
    new_root: PathBuf,
    copied: bool,
    files: &[FileManifestEntry],
) -> KbRelocationResult {
    let file_count = files.len() as u64;
    let total_bytes = files.iter().map(|entry| entry.size).sum();
    let mut aggregate = Sha256::new();
    for entry in files {
        aggregate.update(entry.relative_path.as_bytes());
        aggregate.update([0]);
        aggregate.update(entry.size.to_le_bytes());
        aggregate.update(entry.sha256.as_bytes());
    }
    let critical_manifest = CRITICAL_FILES
        .iter()
        .filter_map(|critical| {
            files
                .iter()
                .find(|entry| entry.relative_path == *critical)
                .map(|entry| KbCriticalFile {
                    relative_path: entry.relative_path.clone(),
                    size: entry.size,
                    sha256: entry.sha256.clone(),
                })
        })
        .collect();
    KbRelocationResult {
        operation: operation.to_string(),
        backup_available: old_root.is_dir(),
        backup_path: old_root.clone(),
        old_root,
        new_root,
        copied,
        manifest: KbManifestSummary {
            file_count,
            total_bytes,
            sha256: format!("{:x}", aggregate.finalize()),
        },
        critical_manifest,
        index_rebuild_required: true,
    }
}

fn first_regular_file(root: &Path) -> Result<Option<PathBuf>, KbRelocationError> {
    for entry in fs::read_dir(root)
        .map_err(|e| KbRelocationError::io("KB_TEST_FAULT_FAILED", "读取故障注入目录", e))?
    {
        let path = entry
            .map_err(|e| KbRelocationError::io("KB_TEST_FAULT_FAILED", "读取故障注入目录项", e))?
            .path();
        if path.is_dir() {
            if let Some(found) = first_regular_file(&path)? {
                return Ok(Some(found));
            }
        } else if path.is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn cleanup_temp(temp: &Path, expected_parent: &Path) {
    if temp.parent() == Some(expected_parent)
        && temp
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".caseboard-kb-migrate-"))
    {
        let _ = fs::remove_dir_all(temp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemoryStore {
        settings: Settings,
        fail_save: bool,
        save_count: usize,
    }

    impl BindingStore for MemoryStore {
        fn load(&mut self) -> Result<Settings, KbRelocationError> {
            Ok(self.settings.clone())
        }

        fn save(&mut self, settings: &Settings) -> Result<(), KbRelocationError> {
            self.save_count += 1;
            if self.fail_save {
                return Err(KbRelocationError::new(
                    "KB_BINDING_ATOMIC_REPLACE_FAILED",
                    "测试注入：设置写入失败",
                    false,
                ));
            }
            self.settings = settings.clone();
            Ok(())
        }
    }

    fn seed_kb(root: &Path) {
        fs::create_dir_all(root.join("raw/yuandian-cache")).expect("raw cache");
        fs::create_dir_all(root.join("wiki")).expect("wiki");
        fs::write(root.join("wiki/index.md"), "# index").expect("wiki index");
        fs::write(root.join("gap-log.md"), "# gaps").expect("gap log");
        fs::write(root.join("raw/note.md"), "material").expect("material");
        fs::write(root.join(CACHE_INDEX), "{}").expect("cache index");
    }

    fn store_for(source: &Path) -> MemoryStore {
        MemoryStore {
            settings: Settings {
                local_kb_root: Some(source.to_string_lossy().into_owned()),
                local_kb_enabled: Some(true),
                ..Settings::default()
            },
            fail_save: false,
            save_count: 0,
        }
    }

    #[test]
    fn switch_existing_changes_only_binding_and_never_copies() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        seed_kb(&target);
        fs::write(target.join("target-only.md"), "keep").expect("target content");
        let mut store = store_for(&source);

        let result = switch_existing_with_store(&mut store, target.to_str().expect("utf8 target"))
            .expect("switch");

        assert!(!result.copied);
        assert!(source.exists());
        assert_eq!(
            store.settings.local_kb_root.as_deref(),
            Some(result.new_root.to_string_lossy().as_ref())
        );
        assert!(!fs::read_to_string(target.join("raw/note.md"))
            .unwrap()
            .is_empty());
        assert!(target.join("target-only.md").exists());
    }

    #[test]
    fn migrate_verifies_copy_keeps_source_and_returns_backup() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        let mut store = store_for(&source);

        let result = migrate_current_with_store(
            &mut store,
            target.to_str().expect("utf8 target"),
            MigrationFault::None,
        )
        .expect("migrate");

        assert!(result.copied);
        assert_eq!(result.backup_path, source.canonicalize().unwrap());
        assert!(source.exists());
        assert!(target.exists());
        assert_eq!(
            result.manifest,
            result_from_manifest(
                "test",
                result.old_root.clone(),
                result.new_root.clone(),
                true,
                &build_manifest(&target).unwrap(),
            )
            .manifest
        );
        assert_eq!(store.save_count, 1);
    }

    #[test]
    fn copy_failure_leaves_binding_and_source_untouched() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        let mut store = store_for(&source);
        let before = store.settings.local_kb_root.clone();

        let error = migrate_current_with_store(
            &mut store,
            target.to_str().expect("utf8 target"),
            MigrationFault::CopyAfterFirstFile,
        )
        .expect_err("copy must fail");

        assert_eq!(error.code, "KB_COPY_INJECTED_FAILURE");
        assert_eq!(store.settings.local_kb_root, before);
        assert_eq!(store.save_count, 0);
        assert!(source.exists());
        assert!(!target.exists());
    }

    #[test]
    fn verification_failure_leaves_binding_and_source_untouched() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        let mut store = store_for(&source);
        let before = store.settings.local_kb_root.clone();

        let error = migrate_current_with_store(
            &mut store,
            target.to_str().expect("utf8 target"),
            MigrationFault::CorruptBeforeVerify,
        )
        .expect_err("verification must fail");

        assert_eq!(error.code, "KB_MIGRATION_VERIFY_FAILED");
        assert_eq!(store.settings.local_kb_root, before);
        assert_eq!(store.save_count, 0);
        assert!(source.exists());
        assert!(!target.exists());
    }

    #[test]
    fn settings_failure_keeps_old_binding_and_recovery_copy() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        let mut store = store_for(&source);
        store.fail_save = true;
        let before = store.settings.local_kb_root.clone();

        let error = migrate_current_with_store(
            &mut store,
            target.to_str().expect("utf8 target"),
            MigrationFault::None,
        )
        .expect_err("save must fail");

        assert_eq!(error.code, "KB_BINDING_ATOMIC_REPLACE_FAILED");
        assert_eq!(store.settings.local_kb_root, before);
        assert_eq!(
            error.recovery_path,
            Some(target.canonicalize().expect("canonical recovery target"))
        );
        assert!(source.exists());
        assert!(target.exists());
    }

    #[test]
    fn target_collision_before_atomic_rename_keeps_old_binding() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        let mut store = store_for(&source);
        let before = store.settings.local_kb_root.clone();

        let error = migrate_current_with_store(
            &mut store,
            target.to_str().expect("utf8 target"),
            MigrationFault::TargetCollisionBeforeRename,
        )
        .expect_err("rename collision");

        assert_eq!(error.code, "KB_TARGET_ATOMIC_RENAME_FAILED");
        assert_eq!(store.settings.local_kb_root, before);
        assert_eq!(store.save_count, 0);
        assert!(source.exists());
    }

    #[test]
    fn target_cannot_be_source_or_child_of_source() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        seed_kb(&source);
        let mut store = store_for(&source);

        let same = switch_existing_with_store(&mut store, source.to_str().expect("utf8 source"))
            .expect_err("same target");
        assert_eq!(same.code, "KB_TARGET_SAME_AS_SOURCE");

        let child = source.join("new-location");
        let child_error = migrate_current_with_store(
            &mut store,
            child.to_str().expect("utf8 child"),
            MigrationFault::None,
        )
        .expect_err("child target");
        assert_eq!(child_error.code, "KB_TARGET_INSIDE_SOURCE");
    }

    #[test]
    fn incompatible_index_is_rejected_without_saving_binding() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        seed_kb(&target);
        fs::write(target.join(CACHE_INDEX), "{not-json").expect("corrupt index");
        let mut store = store_for(&source);

        let error = switch_existing_with_store(&mut store, target.to_str().expect("utf8 target"))
            .expect_err("incompatible index");

        assert_eq!(error.code, "KB_INDEX_INCOMPATIBLE");
        assert_eq!(store.save_count, 0);
    }

    #[test]
    fn index_path_escape_is_rejected_without_saving_binding() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        seed_kb(&source);
        seed_kb(&target);
        let unsafe_index = serde_json::json!({
            "key": {
                "path": "../outside.md",
                "query_type": "law",
                "summary": "unsafe",
                "cached_at": "2026-07-28 00:00:00"
            }
        });
        fs::write(
            target.join(CACHE_INDEX),
            serde_json::to_vec(&unsafe_index).unwrap(),
        )
        .expect("unsafe index");
        let mut store = store_for(&source);

        let error = switch_existing_with_store(&mut store, target.to_str().expect("utf8 target"))
            .expect_err("unsafe index path");

        assert_eq!(error.code, "KB_INDEX_PATH_UNSAFE");
        assert_eq!(store.save_count, 0);
    }

    #[test]
    fn unavailable_target_is_reported_as_retryable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        seed_kb(&source);
        let missing = temp.path().join("offline-share");
        let mut store = store_for(&source);

        let error = switch_existing_with_store(&mut store, missing.to_str().expect("utf8 target"))
            .expect_err("missing target");

        assert_eq!(error.code, "KB_TARGET_UNAVAILABLE");
        assert!(error.retryable);
        assert_eq!(store.save_count, 0);
    }

    #[test]
    fn missing_old_root_does_not_block_switch_to_valid_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let missing_source = temp.path().join("removed-old-kb");
        let target = temp.path().join("target");
        seed_kb(&target);
        let mut store = store_for(&missing_source);

        let result = switch_existing_with_store(&mut store, target.to_str().expect("utf8 target"))
            .expect("switch away from missing old root");

        assert_eq!(result.old_root, missing_source);
        assert_eq!(result.backup_path, result.old_root);
        assert!(!result.backup_available);
        assert_eq!(store.save_count, 1);
        assert_eq!(
            store.settings.local_kb_root.as_deref(),
            Some(result.new_root.to_string_lossy().as_ref())
        );
    }
}
