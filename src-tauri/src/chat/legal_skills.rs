//! 方律全局法律 Skills（方法包）的受控注册表。
//!
//! 方法包是低于 Constitution、场景工作流和工具白名单的纯文本方法上下文：
//! - 本模块不创建、注册或授权任何工具；
//! - 导入面只接收已经读取为 UTF-8 文本的相对路径与内容，不读取任意本地路径；
//! - 同 slug/version 异哈希会隔离，不允许静默覆盖；
//! - 每次运行最多选择一个主方法包，并记录 slug/version/hash。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

const MAX_PACKAGE_BYTES: usize = 512 * 1024;
const MAX_ARCHIVE_BYTES: usize = 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 2 + MAX_REFERENCE_FILES;
const MAX_SKILL_BYTES: usize = 128 * 1024;
const MAX_REFERENCE_FILES: usize = 20;
const MAX_IMPORTED_PACKAGES: i64 = 32;
const MAX_METHOD_CHARS: usize = 4_000;

const ALLOWED_DOMAINS: &[&str] = &[
    "criminal",
    "civil",
    "enforcement",
    "non_litigation",
    "legal_research",
];

const BUILTIN_SLUGS: &[&str] = &[
    "fanglv-criminal-defense-cn",
    "fanglv-civil-litigation-cn",
    "fanglv-enforcement-recovery-cn",
    "fanglv-contract-nonlitigation-cn",
    "fanglv-legal-research-cn",
];

const ALLOWED_TASK_TYPES: &[&str] = &[
    "free_chat",
    "compile_legal_basis",
    "find_similar_cases",
    "verify_my_draft",
    "simulate_opposition",
    "deep_analysis",
    "criminal_deep_analysis",
];

/// 仅用于校验 manifest 中的兼容性声明。此清单绝不产生工具授权。
const KNOWN_TOOLS: &[&str] = &[
    "list_case_docs",
    "read_case_doc",
    "find_in_document",
    "semantic_search_case_docs",
    "search_laws",
    "get_law_article",
    "search_regulations",
    "get_regulation_detail",
    "law_vector_search",
    "search_cases_normal",
    "search_cases_authority",
    "get_case_detail",
    "case_vector_search",
    "search_local_kb",
    "semantic_search_local_kb",
    "read_kb_file",
    "verify_legal_citations",
    "ask_user",
    "save_artifact",
    "edit_artifact",
];

const BUILTIN_CRIMINAL_MANIFEST: &str =
    include_str!("../../resources/legal-skills/fanglv-criminal-defense-cn/manifest.json");
const BUILTIN_CRIMINAL_SKILL: &str =
    include_str!("../../resources/legal-skills/fanglv-criminal-defense-cn/SKILL.md");
const BUILTIN_CRIMINAL_GUARDRAILS: &str = include_str!(
    "../../resources/legal-skills/fanglv-criminal-defense-cn/references/guardrails.md"
);

const BUILTIN_CIVIL_MANIFEST: &str =
    include_str!("../../resources/legal-skills/fanglv-civil-litigation-cn/manifest.json");
const BUILTIN_CIVIL_SKILL: &str =
    include_str!("../../resources/legal-skills/fanglv-civil-litigation-cn/SKILL.md");
const BUILTIN_CIVIL_GUARDRAILS: &str = include_str!(
    "../../resources/legal-skills/fanglv-civil-litigation-cn/references/guardrails.md"
);

const BUILTIN_ENFORCEMENT_MANIFEST: &str =
    include_str!("../../resources/legal-skills/fanglv-enforcement-recovery-cn/manifest.json");
const BUILTIN_ENFORCEMENT_SKILL: &str =
    include_str!("../../resources/legal-skills/fanglv-enforcement-recovery-cn/SKILL.md");
const BUILTIN_ENFORCEMENT_GUARDRAILS: &str = include_str!(
    "../../resources/legal-skills/fanglv-enforcement-recovery-cn/references/guardrails.md"
);

const BUILTIN_CONTRACT_MANIFEST: &str =
    include_str!("../../resources/legal-skills/fanglv-contract-nonlitigation-cn/manifest.json");
const BUILTIN_CONTRACT_SKILL: &str =
    include_str!("../../resources/legal-skills/fanglv-contract-nonlitigation-cn/SKILL.md");
const BUILTIN_CONTRACT_GUARDRAILS: &str = include_str!(
    "../../resources/legal-skills/fanglv-contract-nonlitigation-cn/references/guardrails.md"
);

const BUILTIN_RESEARCH_MANIFEST: &str =
    include_str!("../../resources/legal-skills/fanglv-legal-research-cn/manifest.json");
const BUILTIN_RESEARCH_SKILL: &str =
    include_str!("../../resources/legal-skills/fanglv-legal-research-cn/SKILL.md");
const BUILTIN_RESEARCH_GUARDRAILS: &str =
    include_str!("../../resources/legal-skills/fanglv-legal-research-cn/references/guardrails.md");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LegalSkillManifest {
    pub schema_version: u32,
    pub slug: String,
    pub title: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub legal_domains: Vec<String>,
    pub task_types: Vec<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub requested_tools: Vec<String>,
    #[serde(default)]
    pub default_enabled: bool,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegalSkillFile {
    pub relative_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidatedLegalSkillPackage {
    pub manifest: LegalSkillManifest,
    pub files: BTreeMap<String, String>,
    pub content_hash: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LegalSkillPackageRecord {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub version: String,
    pub description: String,
    pub origin: String,
    pub status: String,
    pub manifest_json: String,
    pub package_content_json: String,
    pub content_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LegalSkillRegistration {
    pub package: LegalSkillPackageRecord,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LegalSkillRevisionRecord {
    pub id: String,
    pub skill_id: String,
    pub slug: String,
    pub version: String,
    pub content_hash: String,
    pub manifest_json: String,
    pub package_content_json: String,
    pub revision_action: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LegalSkillVersionHistory {
    pub packages: Vec<LegalSkillPackageRecord>,
    pub revisions: Vec<LegalSkillRevisionRecord>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LegalSkillFileDiff {
    pub path: String,
    pub change: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LegalSkillDiffPreview {
    pub slug: String,
    pub from_skill_id: String,
    pub from_version: String,
    pub from_hash: String,
    pub to_skill_id: String,
    pub to_version: String,
    pub to_hash: String,
    pub files: Vec<LegalSkillFileDiff>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LegalSkillArchiveExport {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectedLegalSkill {
    pub skill_id: String,
    pub slug: String,
    pub title: String,
    pub version: String,
    pub content_hash: String,
    pub selection_source: String,
    pub method_context: String,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LegalSkillError {
    #[error("{code}: {message}")]
    Validation { code: &'static str, message: String },
    #[error("SKILL_DATABASE_ERROR: {0}")]
    Database(String),
}

impl From<sqlx::Error> for LegalSkillError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value.to_string())
    }
}

fn validation(code: &'static str, message: impl Into<String>) -> LegalSkillError {
    LegalSkillError::Validation {
        code,
        message: message.into(),
    }
}

pub fn validate_package_files(
    files: Vec<LegalSkillFile>,
) -> Result<ValidatedLegalSkillPackage, LegalSkillError> {
    if files.is_empty() {
        return Err(validation("SKILL_PACKAGE_EMPTY", "方法包不能为空"));
    }

    let mut normalized = BTreeMap::new();
    let mut total_bytes = 0usize;
    let mut reference_count = 0usize;
    for file in files {
        let path = validate_relative_path(&file.relative_path)?;
        if normalized.contains_key(&path) {
            return Err(validation(
                "SKILL_DUPLICATE_PATH",
                format!("方法包含重复路径: {path}"),
            ));
        }
        let content = normalize_text(&file.content)?;
        let size = content.len();
        total_bytes = total_bytes.saturating_add(size);
        if total_bytes > MAX_PACKAGE_BYTES {
            return Err(validation(
                "SKILL_PACKAGE_TOO_LARGE",
                format!("方法包解包后不能超过 {MAX_PACKAGE_BYTES} 字节"),
            ));
        }
        if path == "SKILL.md" && size > MAX_SKILL_BYTES {
            return Err(validation(
                "SKILL_BODY_TOO_LARGE",
                format!("SKILL.md 不能超过 {MAX_SKILL_BYTES} 字节"),
            ));
        }
        if path.starts_with("references/") {
            reference_count += 1;
            if reference_count > MAX_REFERENCE_FILES {
                return Err(validation(
                    "SKILL_TOO_MANY_REFERENCES",
                    format!("references 最多 {MAX_REFERENCE_FILES} 个文件"),
                ));
            }
        }
        normalized.insert(path, content);
    }

    let raw_manifest = normalized
        .get("manifest.json")
        .ok_or_else(|| validation("SKILL_MANIFEST_MISSING", "方法包根目录缺少 manifest.json"))?;
    let skill_body = normalized
        .get("SKILL.md")
        .ok_or_else(|| validation("SKILL_BODY_MISSING", "方法包根目录缺少 SKILL.md"))?;
    if skill_body.trim().is_empty() {
        return Err(validation("SKILL_BODY_EMPTY", "SKILL.md 不能为空"));
    }

    let manifest: LegalSkillManifest = serde_json::from_str(raw_manifest).map_err(|err| {
        validation(
            "SKILL_MANIFEST_INVALID",
            format!("manifest.json 解析失败: {err}"),
        )
    })?;
    validate_manifest(&manifest)?;

    let content_hash = canonical_hash(&normalized);
    let warnings = security_warnings(&normalized);
    Ok(ValidatedLegalSkillPackage {
        manifest,
        files: normalized,
        content_hash,
        warnings,
    })
}

pub fn validate_package_archive(
    file_name: &str,
    archive_bytes: &[u8],
) -> Result<ValidatedLegalSkillPackage, LegalSkillError> {
    if !file_name
        .to_ascii_lowercase()
        .ends_with(".fanglv-skill.zip")
    {
        return Err(validation(
            "SKILL_ARCHIVE_EXTENSION_INVALID",
            "方法包压缩文件必须使用 .fanglv-skill.zip 扩展名",
        ));
    }
    if archive_bytes.is_empty() || archive_bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(validation(
            "SKILL_ARCHIVE_SIZE_INVALID",
            format!("压缩包本身必须为 1-{MAX_ARCHIVE_BYTES} 字节"),
        ));
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes)).map_err(|err| {
        validation(
            "SKILL_ARCHIVE_INVALID",
            format!("无法解析方法包 ZIP: {err}"),
        )
    })?;
    if archive.len() > MAX_ARCHIVE_FILES + 1 {
        return Err(validation(
            "SKILL_ARCHIVE_TOO_MANY_ENTRIES",
            format!("ZIP 条目最多 {} 个", MAX_ARCHIVE_FILES + 1),
        ));
    }

    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            validation(
                "SKILL_ARCHIVE_ENTRY_INVALID",
                format!("无法读取 ZIP 条目 {index}: {err}"),
            )
        })?;
        let raw_name = entry.name().to_string();
        if entry.is_dir() {
            if raw_name.replace('\\', "/") != "references/" {
                return Err(validation(
                    "SKILL_ARCHIVE_DIRECTORY_INVALID",
                    format!("仅允许可选目录项 references/: {raw_name}"),
                ));
            }
            continue;
        }
        if entry.is_symlink() {
            return Err(validation(
                "SKILL_ARCHIVE_SYMLINK_BLOCKED",
                format!("ZIP 不允许符号链接: {raw_name}"),
            ));
        }
        if entry.enclosed_name().is_none() {
            return Err(validation(
                "SKILL_ARCHIVE_ZIP_SLIP",
                format!("ZIP 条目越出包根目录: {raw_name}"),
            ));
        }
        if !matches!(
            entry.compression(),
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            return Err(validation(
                "SKILL_ARCHIVE_COMPRESSION_BLOCKED",
                format!("ZIP 条目使用了不支持的压缩算法: {raw_name}"),
            ));
        }
        let lower_path = raw_name.to_ascii_lowercase();
        if [".zip", ".gz", ".tgz", ".tar", ".7z", ".rar", ".bz2", ".xz"]
            .iter()
            .any(|suffix| lower_path.ends_with(suffix))
        {
            return Err(validation(
                "SKILL_ARCHIVE_NESTED_BLOCKED",
                format!("方法包不允许嵌套压缩文件: {raw_name}"),
            ));
        }
        let normalized_path = validate_relative_path(&raw_name)?;
        if files.len() >= MAX_ARCHIVE_FILES {
            return Err(validation(
                "SKILL_ARCHIVE_TOO_MANY_FILES",
                format!("解包后文件最多 {MAX_ARCHIVE_FILES} 个"),
            ));
        }
        let reported_size = usize::try_from(entry.size()).unwrap_or(usize::MAX);
        if reported_size > MAX_PACKAGE_BYTES.saturating_sub(total_bytes) {
            return Err(validation(
                "SKILL_PACKAGE_TOO_LARGE",
                format!("方法包解包后不能超过 {MAX_PACKAGE_BYTES} 字节"),
            ));
        }
        let remaining = MAX_PACKAGE_BYTES.saturating_sub(total_bytes);
        let mut bytes = Vec::with_capacity(reported_size);
        entry
            .by_ref()
            .take((remaining + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|err| {
                validation(
                    "SKILL_ARCHIVE_ENTRY_READ_FAILED",
                    format!("读取 ZIP 条目失败 {normalized_path}: {err}"),
                )
            })?;
        if bytes.len() > remaining {
            return Err(validation(
                "SKILL_PACKAGE_TOO_LARGE",
                format!("方法包解包后不能超过 {MAX_PACKAGE_BYTES} 字节"),
            ));
        }
        total_bytes += bytes.len();
        let content = String::from_utf8(bytes).map_err(|_| {
            validation(
                "SKILL_ARCHIVE_BINARY_BLOCKED",
                format!("方法包只能包含 UTF-8 文本: {normalized_path}"),
            )
        })?;
        files.push(LegalSkillFile {
            relative_path: normalized_path,
            content,
        });
    }
    validate_package_files(files)
}

pub fn build_package_archive(
    package: &LegalSkillPackageRecord,
) -> Result<LegalSkillArchiveExport, LegalSkillError> {
    if package.status == "deleted" {
        return Err(validation("SKILL_DELETED", "已删除的方法包不能导出"));
    }
    let files: BTreeMap<String, String> = serde_json::from_str(&package.package_content_json)
        .map_err(|err| LegalSkillError::Database(format!("已保存方法包内容无法解析: {err}")))?;
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (path, content) in files {
        validate_relative_path(&path)?;
        writer.start_file(path.as_str(), options).map_err(|err| {
            validation(
                "SKILL_ARCHIVE_EXPORT_FAILED",
                format!("创建 ZIP 条目失败 {path}: {err}"),
            )
        })?;
        writer.write_all(content.as_bytes()).map_err(|err| {
            validation(
                "SKILL_ARCHIVE_EXPORT_FAILED",
                format!("写入 ZIP 条目失败 {path}: {err}"),
            )
        })?;
    }
    let cursor = writer.finish().map_err(|err| {
        validation(
            "SKILL_ARCHIVE_EXPORT_FAILED",
            format!("完成 ZIP 导出失败: {err}"),
        )
    })?;
    Ok(LegalSkillArchiveExport {
        file_name: format!("{}-{}.fanglv-skill.zip", package.slug, package.version),
        bytes: cursor.into_inner(),
    })
}

fn validate_relative_path(raw: &str) -> Result<String, LegalSkillError> {
    let path = raw.trim().replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(':')
        || path.contains('\0')
        || path.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.starts_with('.')
        })
    {
        return Err(validation(
            "SKILL_PATH_UNSAFE",
            format!("不安全的方法包路径: {raw}"),
        ));
    }
    if path != "manifest.json" && path != "SKILL.md" && !path.starts_with("references/") {
        return Err(validation(
            "SKILL_PATH_OUTSIDE_LAYOUT",
            format!("仅允许根目录 manifest.json、SKILL.md 和 references/: {path}"),
        ));
    }
    let extension = path.rsplit('.').next().unwrap_or_default();
    if !matches!(extension, "json" | "md" | "txt") {
        return Err(validation(
            "SKILL_FILE_TYPE_BLOCKED",
            format!("只允许 .json/.md/.txt: {path}"),
        ));
    }
    Ok(path)
}

fn normalize_text(raw: &str) -> Result<String, LegalSkillError> {
    if raw.contains('\0') {
        return Err(validation(
            "SKILL_TEXT_INVALID",
            "文本包含 NUL，疑似二进制内容",
        ));
    }
    Ok(raw
        .strip_prefix('\u{feff}')
        .unwrap_or(raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

fn validate_manifest(manifest: &LegalSkillManifest) -> Result<(), LegalSkillError> {
    if manifest.schema_version != 1 {
        return Err(validation(
            "SKILL_SCHEMA_UNSUPPORTED",
            "仅支持 schema_version=1",
        ));
    }
    let slug_regex = Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").expect("static regex");
    if manifest.slug.len() > 64 || !slug_regex.is_match(&manifest.slug) {
        return Err(validation(
            "SKILL_SLUG_INVALID",
            "slug 只能使用小写字母、数字和连字符，且最长 64 字符",
        ));
    }
    if manifest.title.trim().is_empty() || manifest.title.chars().count() > 100 {
        return Err(validation(
            "SKILL_TITLE_INVALID",
            "title 必须为 1-100 个字符",
        ));
    }
    if manifest.description.chars().count() > 1_024 {
        return Err(validation(
            "SKILL_DESCRIPTION_TOO_LONG",
            "description 最长 1024 字符",
        ));
    }
    validate_semver(&manifest.version)?;
    validate_unique_members(
        "legal_domains",
        &manifest.legal_domains,
        ALLOWED_DOMAINS,
        true,
    )?;
    validate_unique_members("task_types", &manifest.task_types, ALLOWED_TASK_TYPES, true)?;
    validate_unique_members(
        "requested_tools",
        &manifest.requested_tools,
        KNOWN_TOOLS,
        false,
    )?;
    if manifest
        .required_capabilities
        .iter()
        .any(|item| item.trim().is_empty() || item.len() > 64)
    {
        return Err(validation(
            "SKILL_CAPABILITY_INVALID",
            "required_capabilities 含空值或超长值",
        ));
    }
    Ok(())
}

fn validate_semver(version: &str) -> Result<(), LegalSkillError> {
    let semver = Regex::new(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$")
        .expect("static regex");
    if version.len() > 64 || !semver.is_match(version) {
        Err(validation(
            "SKILL_VERSION_INVALID",
            "version 必须是语义版本，例如 1.0.0",
        ))
    } else {
        Ok(())
    }
}

fn validate_unique_members(
    field: &str,
    values: &[String],
    allowed: &[&str],
    required: bool,
) -> Result<(), LegalSkillError> {
    if required && values.is_empty() {
        return Err(validation(
            "SKILL_MANIFEST_SCOPE_EMPTY",
            format!("{field} 不能为空"),
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        let normalized = value.trim();
        if !allowed.contains(&normalized) {
            return Err(validation(
                "SKILL_MANIFEST_SCOPE_UNSUPPORTED",
                format!("{field} 含不支持的值: {value}"),
            ));
        }
        if !seen.insert(normalized) {
            return Err(validation(
                "SKILL_MANIFEST_DUPLICATE_VALUE",
                format!("{field} 含重复值: {value}"),
            ));
        }
    }
    Ok(())
}

fn canonical_hash(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (path, content) in files {
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
        hasher.update((content.len() as u64).to_be_bytes());
        hasher.update(content.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn security_warnings(files: &BTreeMap<String, String>) -> Vec<String> {
    const SUSPICIOUS: &[(&str, &str)] = &[
        ("忽略系统", "包含试图忽略系统规则的表述"),
        ("绕过律师复核", "包含试图绕过律师复核的表述"),
        ("扩大工具权限", "包含试图扩大工具权限的表述"),
        ("读取任意文件", "包含请求读取任意文件的表述"),
        ("自动写入案件字段", "包含请求自动写入案件字段的表述"),
    ];
    let joined = files.values().cloned().collect::<Vec<_>>().join("\n");
    SUSPICIOUS
        .iter()
        .filter(|(needle, _)| joined.contains(needle))
        .map(|(_, warning)| (*warning).to_string())
        .collect()
}

pub fn builtin_packages() -> Result<Vec<ValidatedLegalSkillPackage>, LegalSkillError> {
    [
        (
            BUILTIN_CRIMINAL_MANIFEST,
            BUILTIN_CRIMINAL_SKILL,
            BUILTIN_CRIMINAL_GUARDRAILS,
        ),
        (
            BUILTIN_CIVIL_MANIFEST,
            BUILTIN_CIVIL_SKILL,
            BUILTIN_CIVIL_GUARDRAILS,
        ),
        (
            BUILTIN_ENFORCEMENT_MANIFEST,
            BUILTIN_ENFORCEMENT_SKILL,
            BUILTIN_ENFORCEMENT_GUARDRAILS,
        ),
        (
            BUILTIN_CONTRACT_MANIFEST,
            BUILTIN_CONTRACT_SKILL,
            BUILTIN_CONTRACT_GUARDRAILS,
        ),
        (
            BUILTIN_RESEARCH_MANIFEST,
            BUILTIN_RESEARCH_SKILL,
            BUILTIN_RESEARCH_GUARDRAILS,
        ),
    ]
    .into_iter()
    .map(|(manifest, skill, guardrails)| {
        validate_package_files(vec![
            LegalSkillFile {
                relative_path: "manifest.json".into(),
                content: manifest.into(),
            },
            LegalSkillFile {
                relative_path: "SKILL.md".into(),
                content: skill.into(),
            },
            LegalSkillFile {
                relative_path: "references/guardrails.md".into(),
                content: guardrails.into(),
            },
        ])
    })
    .collect()
}

pub async fn seed_builtin_packages(pool: &SqlitePool) -> Result<(), LegalSkillError> {
    for package in builtin_packages()? {
        let enabled = package.manifest.default_enabled;
        register_package(pool, package, "builtin", enabled).await?;
    }
    Ok(())
}

pub async fn register_package(
    pool: &SqlitePool,
    package: ValidatedLegalSkillPackage,
    origin: &str,
    enabled: bool,
) -> Result<LegalSkillRegistration, LegalSkillError> {
    if !matches!(origin, "builtin" | "imported") {
        return Err(validation(
            "SKILL_ORIGIN_INVALID",
            format!("不支持的方法包来源: {origin}"),
        ));
    }
    if origin == "imported" && BUILTIN_SLUGS.contains(&package.manifest.slug.as_str()) {
        return Err(validation(
            "SKILL_BUILTIN_SLUG_RESERVED",
            "导入方法包不能使用方律内置方法包的保留 slug",
        ));
    }
    let mut tx = pool.begin().await?;
    let existing = sqlx::query_as::<_, LegalSkillPackageRecord>(
        "SELECT * FROM legal_skill_packages WHERE slug=? AND version=?",
    )
    .bind(&package.manifest.slug)
    .bind(&package.manifest.version)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(existing) = existing {
        if existing.content_hash == package.content_hash {
            if existing.status == "deleted" && origin == "imported" {
                sqlx::query(
                    "UPDATE legal_skill_packages
                     SET status='disabled', updated_at=datetime('now') WHERE id=?",
                )
                .bind(&existing.id)
                .execute(&mut *tx)
                .await?;
                insert_import_audit(
                    &mut tx,
                    Some(&existing.id),
                    Some(&existing.slug),
                    Some(&existing.version),
                    Some(&existing.content_hash),
                    "register",
                    "succeeded",
                    None,
                )
                .await?;
                tx.commit().await?;
                let restored = get_package(pool, &existing.id)
                    .await?
                    .ok_or_else(|| LegalSkillError::Database("方法包恢复后读取失败".into()))?;
                return Ok(LegalSkillRegistration {
                    package: restored,
                    created: false,
                });
            }
            tx.commit().await?;
            return Ok(LegalSkillRegistration {
                package: existing,
                created: false,
            });
        }
        sqlx::query(
            "UPDATE legal_skill_packages SET status='quarantined', updated_at=datetime('now') WHERE id=?",
        )
        .bind(&existing.id)
        .execute(&mut *tx)
        .await?;
        insert_import_audit(
            &mut tx,
            Some(&existing.id),
            Some(&package.manifest.slug),
            Some(&package.manifest.version),
            Some(&package.content_hash),
            "quarantine",
            "rejected",
            Some("SKILL_VERSION_HASH_CONFLICT"),
        )
        .await?;
        tx.commit().await?;
        return Err(validation(
            "SKILL_VERSION_HASH_CONFLICT",
            format!(
                "{} {} 已存在不同哈希，双方均已隔离，不能静默覆盖",
                package.manifest.slug, package.manifest.version
            ),
        ));
    }
    if origin == "imported" {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM legal_skill_packages
                 WHERE origin='imported' AND status!='deleted'",
        )
        .fetch_one(&mut *tx)
        .await?;
        if count >= MAX_IMPORTED_PACKAGES {
            return Err(validation(
                "SKILL_IMPORT_LIMIT",
                format!("导入方法包最多 {MAX_IMPORTED_PACKAGES} 个"),
            ));
        }
    }

    let id = Uuid::new_v4().to_string();
    let manifest_json = serde_json::to_string(&package.manifest).map_err(|err| {
        validation(
            "SKILL_MANIFEST_INVALID",
            format!("manifest 规范化失败: {err}"),
        )
    })?;
    let package_content_json = serde_json::to_string(&package.files).map_err(|err| {
        validation(
            "SKILL_CONTENT_INVALID",
            format!("方法包内容规范化失败: {err}"),
        )
    })?;
    let status = if enabled { "enabled" } else { "disabled" };
    sqlx::query(
        "INSERT INTO legal_skill_packages
         (id,slug,title,version,description,origin,status,manifest_json,package_content_json,content_hash)
         VALUES(?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&package.manifest.slug)
    .bind(&package.manifest.title)
    .bind(&package.manifest.version)
    .bind(&package.manifest.description)
    .bind(origin)
    .bind(status)
    .bind(&manifest_json)
    .bind(&package_content_json)
    .bind(&package.content_hash)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO legal_skill_revisions
         (id,skill_id,slug,version,content_hash,manifest_json,package_content_json,revision_action)
         VALUES(?,?,?,?,?,?,?,'registered')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&id)
    .bind(&package.manifest.slug)
    .bind(&package.manifest.version)
    .bind(&package.content_hash)
    .bind(&manifest_json)
    .bind(&package_content_json)
    .execute(&mut *tx)
    .await?;
    insert_import_audit(
        &mut tx,
        Some(&id),
        Some(&package.manifest.slug),
        Some(&package.manifest.version),
        Some(&package.content_hash),
        "register",
        "succeeded",
        None,
    )
    .await?;
    tx.commit().await?;

    let saved = get_package(pool, &id)
        .await?
        .ok_or_else(|| LegalSkillError::Database("方法包注册后读取失败".into()))?;
    Ok(LegalSkillRegistration {
        package: saved,
        created: true,
    })
}

#[allow(clippy::too_many_arguments)]
async fn insert_import_audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    skill_id: Option<&str>,
    slug: Option<&str>,
    version: Option<&str>,
    content_hash: Option<&str>,
    action: &str,
    outcome: &str,
    error_code: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO legal_skill_import_audits
         (id,skill_id,slug,version,content_hash,action,outcome,error_code)
         VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(skill_id)
    .bind(slug)
    .bind(version)
    .bind(content_hash)
    .bind(action)
    .bind(outcome)
    .bind(error_code)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn list_packages(
    pool: &SqlitePool,
) -> Result<Vec<LegalSkillPackageRecord>, LegalSkillError> {
    Ok(sqlx::query_as::<_, LegalSkillPackageRecord>(
        "SELECT * FROM legal_skill_packages
         WHERE status!='deleted'
         ORDER BY origin ASC, slug ASC, version DESC",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_package(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<LegalSkillPackageRecord>, LegalSkillError> {
    Ok(sqlx::query_as::<_, LegalSkillPackageRecord>(
        "SELECT * FROM legal_skill_packages WHERE id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_package_versions(
    pool: &SqlitePool,
    slug: &str,
) -> Result<LegalSkillVersionHistory, LegalSkillError> {
    let packages = sqlx::query_as::<_, LegalSkillPackageRecord>(
        "SELECT * FROM legal_skill_packages
         WHERE slug=?
         ORDER BY created_at DESC, version DESC",
    )
    .bind(slug)
    .fetch_all(pool)
    .await?;
    let revisions = sqlx::query_as::<_, LegalSkillRevisionRecord>(
        "SELECT * FROM legal_skill_revisions
         WHERE slug=?
         ORDER BY created_at DESC, id DESC",
    )
    .bind(slug)
    .fetch_all(pool)
    .await?;
    Ok(LegalSkillVersionHistory {
        packages,
        revisions,
    })
}

pub async fn preview_package_diff(
    pool: &SqlitePool,
    from_skill_id: &str,
    to_skill_id: &str,
) -> Result<LegalSkillDiffPreview, LegalSkillError> {
    let from = get_package(pool, from_skill_id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "当前方法包不存在"))?;
    let to = get_package(pool, to_skill_id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "目标方法包不存在"))?;
    ensure_same_imported_slug(&from, &to)?;
    let before = package_files(&from)?;
    let after = package_files(&to)?;
    let paths = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut files = Vec::new();
    for path in paths {
        let old = before.get(&path);
        let new = after.get(&path);
        if old == new {
            continue;
        }
        let change = match (old, new) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            _ => "modified",
        };
        files.push(LegalSkillFileDiff {
            path,
            change: change.to_string(),
            before: old.cloned(),
            after: new.cloned(),
        });
    }
    Ok(LegalSkillDiffPreview {
        slug: from.slug,
        from_skill_id: from.id,
        from_version: from.version,
        from_hash: from.content_hash,
        to_skill_id: to.id,
        to_version: to.version,
        to_hash: to.content_hash,
        files,
    })
}

pub async fn switch_package_version(
    pool: &SqlitePool,
    current_skill_id: &str,
    target_skill_id: &str,
    action: &str,
) -> Result<LegalSkillPackageRecord, LegalSkillError> {
    let current = get_package(pool, current_skill_id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "当前方法包不存在"))?;
    let target = get_package(pool, target_skill_id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "目标方法包不存在"))?;
    ensure_same_imported_slug(&current, &target)?;
    if current.status != "enabled" {
        return Err(validation(
            "SKILL_CURRENT_NOT_ENABLED",
            "只有当前已启用版本可以升级或回滚",
        ));
    }
    if matches!(target.status.as_str(), "quarantined" | "deleted") {
        return Err(validation(
            "SKILL_TARGET_UNAVAILABLE",
            "隔离或已删除版本不能作为切换目标",
        ));
    }
    let order = compare_versions(&target.version, &current.version)?;
    let revision_action = match action {
        "upgrade" if order.is_gt() => "upgraded",
        "rollback" if order.is_lt() => "rolled_back",
        "upgrade" => {
            return Err(validation(
                "SKILL_UPGRADE_DIRECTION_INVALID",
                "升级目标版本必须高于当前版本",
            ))
        }
        "rollback" => {
            return Err(validation(
                "SKILL_ROLLBACK_DIRECTION_INVALID",
                "回滚目标版本必须低于当前版本",
            ))
        }
        _ => {
            return Err(validation(
                "SKILL_VERSION_ACTION_INVALID",
                "版本切换动作只能是 upgrade 或 rollback",
            ))
        }
    };
    let target_manifest: LegalSkillManifest = serde_json::from_str(&target.manifest_json)
        .map_err(|err| LegalSkillError::Database(format!("目标版本 manifest 无法解析: {err}")))?;
    let defaults: Vec<(String, String)> = sqlx::query_as(
        "SELECT legal_domain, task_type FROM legal_skill_bindings
         WHERE skill_id=? AND is_default=1",
    )
    .bind(&current.id)
    .fetch_all(pool)
    .await?;
    for (domain, task_type) in &defaults {
        if !target_manifest.legal_domains.contains(domain)
            || !target_manifest.task_types.contains(task_type)
        {
            return Err(validation(
                "SKILL_VERSION_BINDING_INCOMPATIBLE",
                format!("目标版本不再支持默认绑定 {domain}/{task_type}"),
            ));
        }
    }

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE legal_skill_packages SET status='disabled', updated_at=datetime('now') WHERE id=?",
    )
    .bind(&current.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE legal_skill_packages SET status='enabled', updated_at=datetime('now') WHERE id=?",
    )
    .bind(&target.id)
    .execute(&mut *tx)
    .await?;
    for (domain, task_type) in &defaults {
        sqlx::query(
            "DELETE FROM legal_skill_bindings
             WHERE skill_id=? AND legal_domain=? AND task_type=?",
        )
        .bind(&target.id)
        .bind(domain)
        .bind(task_type)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE legal_skill_bindings
             SET skill_id=?, updated_at=datetime('now')
             WHERE skill_id=? AND legal_domain=? AND task_type=? AND is_default=1",
        )
        .bind(&target.id)
        .bind(&current.id)
        .bind(domain)
        .bind(task_type)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM legal_skill_binding_suppressions
             WHERE legal_domain=? AND task_type=?",
        )
        .bind(domain)
        .bind(task_type)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT OR IGNORE INTO legal_skill_revisions
         (id,skill_id,slug,version,content_hash,manifest_json,package_content_json,revision_action)
         VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&target.id)
    .bind(&target.slug)
    .bind(&target.version)
    .bind(&target.content_hash)
    .bind(&target.manifest_json)
    .bind(&target.package_content_json)
    .bind(revision_action)
    .execute(&mut *tx)
    .await?;
    insert_import_audit(
        &mut tx,
        Some(&target.id),
        Some(&target.slug),
        Some(&target.version),
        Some(&target.content_hash),
        action,
        "succeeded",
        None,
    )
    .await?;
    tx.commit().await?;
    get_package(pool, &target.id)
        .await?
        .ok_or_else(|| LegalSkillError::Database("版本切换后读取失败".into()))
}

pub fn require_explicit_confirmation(confirmed: bool, action: &str) -> Result<(), LegalSkillError> {
    if confirmed {
        Ok(())
    } else {
        Err(validation(
            "SKILL_CONFIRMATION_REQUIRED",
            format!("{action}前必须明确确认"),
        ))
    }
}

pub async fn delete_imported_package(
    pool: &SqlitePool,
    skill_id: &str,
) -> Result<(), LegalSkillError> {
    let package = get_package(pool, skill_id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "方法包不存在"))?;
    if package.origin == "builtin" {
        return Err(validation(
            "SKILL_BUILTIN_DELETE_BLOCKED",
            "方律内置方法包不可删除",
        ));
    }
    if package.status == "deleted" {
        return Ok(());
    }
    let defaults: Vec<(String, String)> = sqlx::query_as(
        "SELECT legal_domain, task_type FROM legal_skill_bindings
         WHERE skill_id=? AND is_default=1",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await?;
    let mut tx = pool.begin().await?;
    for (domain, task_type) in &defaults {
        sqlx::query(
            "INSERT INTO legal_skill_binding_suppressions
             (id,legal_domain,task_type,reason)
             VALUES(lower(hex(?1 || char(31) || ?2)),?1,?2,'deleted_default_package')
             ON CONFLICT(legal_domain,task_type) DO UPDATE SET
               reason='deleted_default_package', updated_at=datetime('now')",
        )
        .bind(domain)
        .bind(task_type)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM legal_skill_bindings WHERE skill_id=?")
        .bind(skill_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE legal_skill_packages SET status='deleted', updated_at=datetime('now') WHERE id=?",
    )
    .bind(skill_id)
    .execute(&mut *tx)
    .await?;
    insert_import_audit(
        &mut tx,
        Some(skill_id),
        Some(&package.slug),
        Some(&package.version),
        Some(&package.content_hash),
        "delete",
        "succeeded",
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn export_package_archive(
    pool: &SqlitePool,
    skill_id: &str,
) -> Result<LegalSkillArchiveExport, LegalSkillError> {
    let package = get_package(pool, skill_id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "方法包不存在"))?;
    build_package_archive(&package)
}

fn package_files(
    package: &LegalSkillPackageRecord,
) -> Result<BTreeMap<String, String>, LegalSkillError> {
    serde_json::from_str(&package.package_content_json)
        .map_err(|err| LegalSkillError::Database(format!("已保存方法包内容无法解析: {err}")))
}

fn ensure_same_imported_slug(
    from: &LegalSkillPackageRecord,
    to: &LegalSkillPackageRecord,
) -> Result<(), LegalSkillError> {
    if from.id == to.id || from.slug != to.slug {
        return Err(validation(
            "SKILL_VERSION_TARGET_INVALID",
            "版本切换必须选择同一 slug 的另一个版本",
        ));
    }
    if from.origin != "imported" || to.origin != "imported" {
        return Err(validation(
            "SKILL_BUILTIN_VERSION_SWITCH_BLOCKED",
            "内置方法包不通过导入包升级或回滚",
        ));
    }
    Ok(())
}

fn compare_versions(left: &str, right: &str) -> Result<std::cmp::Ordering, LegalSkillError> {
    fn parse(value: &str) -> Result<((u64, u64, u64), bool), LegalSkillError> {
        validate_semver(value)?;
        let (core, prerelease) = value
            .split_once('-')
            .map(|(core, suffix)| (core, !suffix.is_empty()))
            .unwrap_or((value, false));
        let mut parts = core.split('.');
        let major = parts
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .map_err(|_| validation("SKILL_VERSION_INVALID", "版本号数值超出支持范围"))?;
        let minor = parts
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .map_err(|_| validation("SKILL_VERSION_INVALID", "版本号数值超出支持范围"))?;
        let patch = parts
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .map_err(|_| validation("SKILL_VERSION_INVALID", "版本号数值超出支持范围"))?;
        Ok(((major, minor, patch), prerelease))
    }
    let (left_core, left_pre) = parse(left)?;
    let (right_core, right_pre) = parse(right)?;
    Ok(left_core
        .cmp(&right_core)
        .then_with(|| right_pre.cmp(&left_pre)))
}

pub async fn set_package_status(
    pool: &SqlitePool,
    id: &str,
    enabled: bool,
) -> Result<LegalSkillPackageRecord, LegalSkillError> {
    let current = get_package(pool, id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "方法包不存在"))?;
    if matches!(current.status.as_str(), "quarantined" | "deleted") {
        return Err(validation(
            "SKILL_QUARANTINED",
            "隔离中的方法包不能直接启用",
        ));
    }
    let status = if enabled { "enabled" } else { "disabled" };
    let action = if enabled { "enable" } else { "disable" };
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE legal_skill_packages SET status=?, updated_at=datetime('now') WHERE id=?")
        .bind(status)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    insert_import_audit(
        &mut tx,
        Some(id),
        Some(&current.slug),
        Some(&current.version),
        Some(&current.content_hash),
        action,
        "succeeded",
        None,
    )
    .await?;
    tx.commit().await?;
    get_package(pool, id)
        .await?
        .ok_or_else(|| LegalSkillError::Database("方法包状态更新后读取失败".into()))
}

pub async fn bind_default(
    pool: &SqlitePool,
    skill_id: &str,
    legal_domain: &str,
    task_type: &str,
) -> Result<(), LegalSkillError> {
    let package = get_package(pool, skill_id)
        .await?
        .ok_or_else(|| validation("SKILL_NOT_FOUND", "方法包不存在"))?;
    if package.status != "enabled" {
        return Err(validation(
            "SKILL_NOT_ENABLED",
            "只有已启用的方法包可以设为默认",
        ));
    }
    let manifest: LegalSkillManifest = serde_json::from_str(&package.manifest_json)
        .map_err(|err| LegalSkillError::Database(format!("已保存 manifest 无法解析: {err}")))?;
    if !manifest.legal_domains.iter().any(|v| v == legal_domain)
        || !manifest.task_types.iter().any(|v| v == task_type)
    {
        return Err(validation(
            "SKILL_BINDING_INCOMPATIBLE",
            "该方法包不适用于所选领域或任务",
        ));
    }

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE legal_skill_bindings SET is_default=0, updated_at=datetime('now')
         WHERE legal_domain=? AND task_type=? AND is_default=1",
    )
    .bind(legal_domain)
    .bind(task_type)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM legal_skill_binding_suppressions
         WHERE legal_domain=? AND task_type=?",
    )
    .bind(legal_domain)
    .bind(task_type)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO legal_skill_bindings
         (id,skill_id,legal_domain,task_type,is_default)
         VALUES(?,?,?,?,1)
         ON CONFLICT(skill_id,legal_domain,task_type) DO UPDATE SET
           is_default=1, updated_at=datetime('now')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(skill_id)
    .bind(legal_domain)
    .bind(task_type)
    .execute(&mut *tx)
    .await?;
    insert_import_audit(
        &mut tx,
        Some(skill_id),
        Some(&package.slug),
        Some(&package.version),
        Some(&package.content_hash),
        "bind",
        "succeeded",
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn select_method(
    pool: &SqlitePool,
    legal_domain: &str,
    task_type: &str,
    preferred_slug: Option<&str>,
) -> Result<Option<SelectedLegalSkill>, LegalSkillError> {
    if !ALLOWED_DOMAINS.contains(&legal_domain) || !ALLOWED_TASK_TYPES.contains(&task_type) {
        return Ok(None);
    }
    if preferred_slug.is_none() {
        let suppressed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM legal_skill_binding_suppressions
             WHERE legal_domain=? AND task_type=?",
        )
        .bind(legal_domain)
        .bind(task_type)
        .fetch_one(pool)
        .await?;
        if suppressed > 0 {
            return Ok(None);
        }
    }

    let mut selected = if let Some(slug) = preferred_slug {
        sqlx::query_as::<_, LegalSkillPackageRecord>(
            "SELECT * FROM legal_skill_packages
             WHERE slug=? AND status='enabled'
             ORDER BY CASE origin WHEN 'imported' THEN 0 ELSE 1 END, version DESC LIMIT 1",
        )
        .bind(slug)
        .fetch_optional(pool)
        .await?
        .map(|record| (record, "user".to_string()))
    } else {
        sqlx::query_as::<_, LegalSkillPackageRecord>(
            "SELECT p.* FROM legal_skill_bindings b
             JOIN legal_skill_packages p ON p.id=b.skill_id
             WHERE b.legal_domain=? AND b.task_type=? AND b.is_default=1 AND p.status='enabled'
             ORDER BY p.slug ASC LIMIT 1",
        )
        .bind(legal_domain)
        .bind(task_type)
        .fetch_optional(pool)
        .await?
        .map(|record| (record, "automatic".to_string()))
    };

    // 未设置默认绑定时，按 origin=builtin、slug、version 的稳定顺序选择第一个兼容包。
    // 这里只选方法上下文，不读取 requested_tools 来增减工具权限。
    if selected.is_none() && preferred_slug.is_none() {
        let candidates = sqlx::query_as::<_, LegalSkillPackageRecord>(
            "SELECT * FROM legal_skill_packages
             WHERE status='enabled' AND origin='builtin'
             ORDER BY slug ASC, version DESC",
        )
        .fetch_all(pool)
        .await?;
        for candidate in candidates {
            let manifest: LegalSkillManifest = serde_json::from_str(&candidate.manifest_json)
                .map_err(|err| {
                    LegalSkillError::Database(format!("已保存 manifest 无法解析: {err}"))
                })?;
            if manifest.legal_domains.iter().any(|v| v == legal_domain)
                && manifest.task_types.iter().any(|v| v == task_type)
            {
                selected = Some((candidate, "automatic".to_string()));
                break;
            }
        }
    }

    let Some((record, source)) = selected else {
        return Ok(None);
    };
    let manifest: LegalSkillManifest = serde_json::from_str(&record.manifest_json)
        .map_err(|err| LegalSkillError::Database(format!("已保存 manifest 无法解析: {err}")))?;
    if !manifest.legal_domains.iter().any(|v| v == legal_domain)
        || !manifest.task_types.iter().any(|v| v == task_type)
    {
        return Ok(None);
    }
    let files: BTreeMap<String, String> = serde_json::from_str(&record.package_content_json)
        .map_err(|err| LegalSkillError::Database(format!("已保存方法包内容无法解析: {err}")))?;
    let body = files
        .get("SKILL.md")
        .ok_or_else(|| LegalSkillError::Database("已保存方法包缺少 SKILL.md".into()))?;
    let (method_context, truncated) = truncate_method(body);
    Ok(Some(SelectedLegalSkill {
        skill_id: record.id,
        slug: record.slug,
        title: record.title,
        version: record.version,
        content_hash: record.content_hash,
        selection_source: source,
        method_context,
        truncated,
    }))
}

fn truncate_method(body: &str) -> (String, bool) {
    if body.chars().count() <= MAX_METHOD_CHARS {
        return (body.to_string(), false);
    }
    let mut truncated: String = body.chars().take(MAX_METHOD_CHARS).collect();
    truncated.push_str("\n\n[方法包正文因上下文预算已截断]");
    (truncated, true)
}

pub async fn audit_run(
    pool: &SqlitePool,
    run_id: &str,
    selected: Option<&SelectedLegalSkill>,
) -> Result<(), LegalSkillError> {
    let (skill_id, slug, version, content_hash, selection_source, truncated) =
        if let Some(selected) = selected {
            (
                Some(selected.skill_id.as_str()),
                Some(selected.slug.as_str()),
                Some(selected.version.as_str()),
                Some(selected.content_hash.as_str()),
                selected.selection_source.as_str(),
                selected.truncated as i64,
            )
        } else {
            (None, None, None, None, "none", 0)
        };
    sqlx::query(
        "INSERT INTO legal_skill_run_audits
         (id,run_id,skill_id,slug,version,content_hash,selection_source,truncated)
         VALUES(?,?,?,?,?,?,?,?)
         ON CONFLICT(run_id) DO NOTHING",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(run_id)
    .bind(skill_id)
    .bind(slug)
    .bind(version)
    .bind(content_hash)
    .bind(selection_source)
    .bind(truncated)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(slug: &str, version: &str, body: &str) -> Vec<LegalSkillFile> {
        vec![
            LegalSkillFile {
                relative_path: "manifest.json".into(),
                content: serde_json::json!({
                    "schema_version": 1,
                    "slug": slug,
                    "title": "测试方法",
                    "version": version,
                    "description": "测试",
                    "legal_domains": ["civil"],
                    "task_types": ["deep_analysis"],
                    "required_capabilities": ["case_context"],
                    "requested_tools": ["search_laws"],
                    "default_enabled": false,
                    "author": "测试",
                    "license": "internal"
                })
                .to_string(),
            },
            LegalSkillFile {
                relative_path: "SKILL.md".into(),
                content: body.into(),
            },
        ]
    }

    fn archive(entries: Vec<(String, Vec<u8>, Option<u32>)>) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content, mode) in entries {
            let mut options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            if let Some(mode) = mode {
                options = options.unix_permissions(mode);
            }
            writer.start_file(name, options).unwrap();
            writer.write_all(&content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn package_archive(slug: &str, version: &str, body: &str) -> Vec<u8> {
        let files = package(slug, version, body);
        archive(
            files
                .into_iter()
                .map(|file| (file.relative_path, file.content.into_bytes(), None))
                .collect(),
        )
    }

    fn symlink_archive() -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .add_symlink(
                "SKILL.md",
                "target",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn builtins_are_valid_and_distinct() {
        let packages = builtin_packages().expect("validate builtins");
        assert_eq!(packages.len(), 5);
        let slugs = packages
            .iter()
            .map(|package| package.manifest.slug.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(slugs.len(), 5);
        assert!(packages.iter().all(|package| package.warnings.is_empty()));
    }

    #[test]
    fn rejects_scripts_path_traversal_and_unknown_tools() {
        let mut script = package("safe-name", "1.0.0", "正文");
        script.push(LegalSkillFile {
            relative_path: "references/run.ps1".into(),
            content: "Write-Host unsafe".into(),
        });
        assert!(matches!(
            validate_package_files(script),
            Err(LegalSkillError::Validation {
                code: "SKILL_FILE_TYPE_BLOCKED",
                ..
            })
        ));

        let mut traversal = package("safe-name", "1.0.0", "正文");
        traversal.push(LegalSkillFile {
            relative_path: "../escape.md".into(),
            content: "unsafe".into(),
        });
        assert!(matches!(
            validate_package_files(traversal),
            Err(LegalSkillError::Validation {
                code: "SKILL_PATH_UNSAFE",
                ..
            })
        ));

        let mut unknown_tool = package("safe-name", "1.0.0", "正文");
        unknown_tool[0].content = unknown_tool[0]
            .content
            .replace("search_laws", "execute_arbitrary_command");
        assert!(matches!(
            validate_package_files(unknown_tool),
            Err(LegalSkillError::Validation {
                code: "SKILL_MANIFEST_SCOPE_UNSUPPORTED",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn registration_is_idempotent_and_hash_conflict_is_quarantined() {
        let pool = crate::db::init_pool(":memory:").await.expect("migrate");
        let reserved =
            validate_package_files(package("fanglv-criminal-defense-cn", "9.9.9", "伪装内置包"))
                .unwrap();
        assert!(matches!(
            register_package(&pool, reserved, "imported", false).await,
            Err(LegalSkillError::Validation {
                code: "SKILL_BUILTIN_SLUG_RESERVED",
                ..
            })
        ));

        let validated = validate_package_files(package("civil-test", "1.0.0", "版本一")).unwrap();
        let first = register_package(&pool, validated.clone(), "imported", false)
            .await
            .unwrap();
        assert!(first.created);
        let second = register_package(&pool, validated, "imported", false)
            .await
            .unwrap();
        assert!(!second.created);
        assert_eq!(first.package.id, second.package.id);

        let conflicting = validate_package_files(package("civil-test", "1.0.0", "版本二")).unwrap();
        let error = register_package(&pool, conflicting, "imported", false)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            LegalSkillError::Validation {
                code: "SKILL_VERSION_HASH_CONFLICT",
                ..
            }
        ));
        let saved = get_package(&pool, &first.package.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, "quarantined");
    }

    #[tokio::test]
    async fn only_enabled_compatible_default_is_selected_and_audited() {
        let pool = crate::db::init_pool(":memory:").await.expect("migrate");
        let validated =
            validate_package_files(package("civil-select", "1.0.0", "民事方法正文")).unwrap();
        let saved = register_package(&pool, validated, "imported", true)
            .await
            .unwrap()
            .package;
        bind_default(&pool, &saved.id, "civil", "deep_analysis")
            .await
            .unwrap();

        let selected = select_method(&pool, "civil", "deep_analysis", None)
            .await
            .unwrap()
            .expect("selected");
        assert_eq!(selected.slug, "civil-select");
        assert_eq!(selected.selection_source, "automatic");
        assert!(
            select_method(&pool, "criminal", "deep_analysis", Some("civil-select"))
                .await
                .unwrap()
                .is_none()
        );

        audit_run(&pool, "run-1", Some(&selected)).await.unwrap();
        audit_run(&pool, "run-1", Some(&selected)).await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM legal_skill_run_audits WHERE run_id='run-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn builtin_selection_has_deterministic_compatible_fallback() {
        let pool = crate::db::init_pool(":memory:").await.expect("migrate");
        seed_builtin_packages(&pool).await.expect("seed builtins");

        let criminal = select_method(&pool, "criminal", "criminal_deep_analysis", None)
            .await
            .unwrap()
            .expect("criminal builtin");
        assert_eq!(criminal.slug, "fanglv-criminal-defense-cn");

        let contract = select_method(&pool, "non_litigation", "verify_my_draft", None)
            .await
            .unwrap()
            .expect("contract builtin");
        assert_eq!(contract.slug, "fanglv-contract-nonlitigation-cn");
    }

    #[test]
    fn canonical_hash_normalizes_line_endings() {
        let lf = validate_package_files(package("hash-test", "1.0.0", "一\n二\n")).unwrap();
        let crlf = validate_package_files(package("hash-test", "1.0.0", "一\r\n二\r\n")).unwrap();
        assert_eq!(lf.content_hash, crlf.content_hash);
    }

    #[test]
    fn archive_round_trip_and_security_limits_are_fail_closed() {
        let bytes = package_archive("archive-test", "1.0.0", "安全正文");
        let validated = validate_package_archive("archive-test.fanglv-skill.zip", &bytes).unwrap();
        let record = LegalSkillPackageRecord {
            id: "skill-1".into(),
            slug: validated.manifest.slug.clone(),
            title: validated.manifest.title.clone(),
            version: validated.manifest.version.clone(),
            description: validated.manifest.description.clone(),
            origin: "imported".into(),
            status: "disabled".into(),
            manifest_json: serde_json::to_string(&validated.manifest).unwrap(),
            package_content_json: serde_json::to_string(&validated.files).unwrap(),
            content_hash: validated.content_hash.clone(),
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        let exported = build_package_archive(&record).unwrap();
        assert!(exported.file_name.ends_with(".fanglv-skill.zip"));
        let revalidated = validate_package_archive(&exported.file_name, &exported.bytes).unwrap();
        assert_eq!(revalidated.content_hash, validated.content_hash);

        let slip = archive(vec![("../escape.md".into(), b"x".to_vec(), None)]);
        assert!(matches!(
            validate_package_archive("bad.fanglv-skill.zip", &slip),
            Err(LegalSkillError::Validation {
                code: "SKILL_ARCHIVE_ZIP_SLIP",
                ..
            })
        ));

        let symlink = symlink_archive();
        assert!(matches!(
            validate_package_archive("bad.fanglv-skill.zip", &symlink),
            Err(LegalSkillError::Validation {
                code: "SKILL_ARCHIVE_SYMLINK_BLOCKED",
                ..
            })
        ));

        let nested = archive(vec![("references/nested.zip".into(), b"PK".to_vec(), None)]);
        assert!(matches!(
            validate_package_archive("bad.fanglv-skill.zip", &nested),
            Err(LegalSkillError::Validation {
                code: "SKILL_ARCHIVE_NESTED_BLOCKED",
                ..
            })
        ));

        let mut too_many = package("many-files", "1.0.0", "正文")
            .into_iter()
            .map(|file| (file.relative_path, file.content.into_bytes()))
            .collect::<Vec<_>>();
        for index in 0..=MAX_REFERENCE_FILES {
            too_many.push((
                format!("references/{index}.md"),
                format!("参考 {index}").into_bytes(),
            ));
        }
        let too_many = archive(
            too_many
                .into_iter()
                .map(|(path, bytes)| (path, bytes, None))
                .collect(),
        );
        assert!(matches!(
            validate_package_archive("bad.fanglv-skill.zip", &too_many),
            Err(LegalSkillError::Validation {
                code: "SKILL_ARCHIVE_TOO_MANY_FILES",
                ..
            })
        ));

        let executable = archive(vec![("references/run.exe".into(), b"MZ".to_vec(), None)]);
        assert!(matches!(
            validate_package_archive("bad.fanglv-skill.zip", &executable),
            Err(LegalSkillError::Validation {
                code: "SKILL_FILE_TYPE_BLOCKED",
                ..
            })
        ));

        let oversized = package_archive("oversized", "1.0.0", &"x".repeat(MAX_PACKAGE_BYTES));
        assert!(matches!(
            validate_package_archive("bad.fanglv-skill.zip", &oversized),
            Err(LegalSkillError::Validation {
                code: "SKILL_PACKAGE_TOO_LARGE",
                ..
            })
        ));
        assert!(matches!(
            validate_package_archive("bad.zip", &bytes),
            Err(LegalSkillError::Validation {
                code: "SKILL_ARCHIVE_EXTENSION_INVALID",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn version_diff_upgrade_rollback_and_delete_keep_audit_snapshot() {
        let pool = crate::db::init_pool(":memory:").await.expect("migrate");
        seed_builtin_packages(&pool).await.expect("seed builtins");
        let version_one = register_package(
            &pool,
            validate_package_files(package("versioned-test", "1.0.0", "旧方法")).unwrap(),
            "imported",
            true,
        )
        .await
        .unwrap()
        .package;
        let version_two = register_package(
            &pool,
            validate_package_files(package("versioned-test", "2.0.0", "新方法")).unwrap(),
            "imported",
            false,
        )
        .await
        .unwrap()
        .package;
        bind_default(&pool, &version_one.id, "civil", "deep_analysis")
            .await
            .unwrap();

        let preview = preview_package_diff(&pool, &version_one.id, &version_two.id)
            .await
            .unwrap();
        assert!(preview
            .files
            .iter()
            .any(|file| file.path == "SKILL.md" && file.change == "modified"));
        assert!(preview
            .files
            .iter()
            .any(|file| file.path == "manifest.json" && file.change == "modified"));

        let upgraded = switch_package_version(&pool, &version_one.id, &version_two.id, "upgrade")
            .await
            .unwrap();
        assert_eq!(upgraded.status, "enabled");
        assert_eq!(
            select_method(&pool, "civil", "deep_analysis", None)
                .await
                .unwrap()
                .unwrap()
                .version,
            "2.0.0"
        );
        let rolled_back =
            switch_package_version(&pool, &version_two.id, &version_one.id, "rollback")
                .await
                .unwrap();
        assert_eq!(rolled_back.status, "enabled");

        let selected = select_method(&pool, "civil", "deep_analysis", None)
            .await
            .unwrap()
            .unwrap();
        audit_run(&pool, "historical-run", Some(&selected))
            .await
            .unwrap();
        delete_imported_package(&pool, &version_one.id)
            .await
            .unwrap();
        assert!(select_method(&pool, "civil", "deep_analysis", None)
            .await
            .unwrap()
            .is_none());
        let snapshot: (String, String, String) = sqlx::query_as(
            "SELECT slug,version,content_hash FROM legal_skill_run_audits
             WHERE run_id='historical-run'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(snapshot.0, "versioned-test");
        assert_eq!(snapshot.1, "1.0.0");
        assert_eq!(snapshot.2, version_one.content_hash);

        let history = list_package_versions(&pool, "versioned-test")
            .await
            .unwrap();
        assert_eq!(history.packages.len(), 2);
        assert!(history
            .revisions
            .iter()
            .any(|revision| revision.revision_action == "upgraded"));
        assert!(history
            .revisions
            .iter()
            .any(|revision| revision.revision_action == "rolled_back"));

        let builtin = list_packages(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|package| package.origin == "builtin")
            .unwrap();
        assert!(matches!(
            delete_imported_package(&pool, &builtin.id).await,
            Err(LegalSkillError::Validation {
                code: "SKILL_BUILTIN_DELETE_BLOCKED",
                ..
            })
        ));
        assert!(matches!(
            require_explicit_confirmation(false, "升级"),
            Err(LegalSkillError::Validation {
                code: "SKILL_CONFIRMATION_REQUIRED",
                ..
            })
        ));
        require_explicit_confirmation(true, "升级").unwrap();
    }
}
