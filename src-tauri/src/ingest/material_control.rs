//! 材料预检与持久队列的业务接线。
//!
//! 预检只读取文件元数据；确认后才写案件、文档决策和队列。原文件始终只读。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::db::cases::Case;
use crate::db::documents::{Document, SyncStats};
use crate::db::material_queue::{
    MaterialBatchDetail, MaterialDecisionInput, MaterialQueueItemInput,
};
use crate::ingest::scanner::{scan_folder_for_domain, ScannedDoc};

const CRIMINAL_LARGE_BATCH_THRESHOLD: usize = 20;
const INDEX_ONLY_REASON: &str = "仅建立索引，未调用 OCR/LLM";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialPreflightItem {
    pub source_path: String,
    pub relative_path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub stage: Option<String>,
    pub category: Option<String>,
    pub is_existing: bool,
    pub default_disposition: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialPreflight {
    pub mode: String,
    pub case_id: Option<String>,
    pub root_path: String,
    pub legal_domain: String,
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub large_criminal_batch: bool,
    pub items: Vec<MaterialPreflightItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitMaterialPreflightInput {
    pub mode: String,
    pub case_id: Option<String>,
    pub root_path: String,
    pub legal_domain: String,
    pub decisions: Vec<MaterialDecisionInput>,
    #[serde(default)]
    pub start_processing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitMaterialPreflightResult {
    pub case: Case,
    pub documents: Vec<Document>,
    pub sync: SyncStats,
    pub batch: Option<MaterialBatchDetail>,
    pub is_existing: bool,
}

fn validate_domain(value: &str) -> Result<&str, String> {
    match value {
        "criminal" | "civil" => Ok(value),
        _ => Err("材料预检必须明确选择刑事或民事案件域".to_string()),
    }
}

fn relative_path(root: &Path, source: &Path) -> String {
    source
        .strip_prefix(root)
        .unwrap_or(source)
        .to_string_lossy()
        .replace('\\', "/")
}

fn build_preflight(
    mode: &str,
    case_id: Option<String>,
    root: &Path,
    legal_domain: &str,
    scanned: Vec<ScannedDoc>,
    existing_paths: &HashSet<String>,
    saved_decisions: &HashMap<String, String>,
) -> MaterialPreflight {
    let large_criminal_batch =
        legal_domain == "criminal" && scanned.len() >= CRIMINAL_LARGE_BATCH_THRESHOLD;
    let total_size_bytes = scanned.iter().map(|doc| doc.size_bytes).sum();
    let items: Vec<MaterialPreflightItem> = scanned
        .into_iter()
        .map(|doc| {
            let is_existing = existing_paths.contains(&doc.source_path);
            let default_disposition = saved_decisions
                .get(&doc.source_path)
                .cloned()
                .unwrap_or_else(|| {
                    if (mode == "refresh" && !is_existing) || large_criminal_batch {
                        "index_only".to_string()
                    } else {
                        "recognize".to_string()
                    }
                });
            MaterialPreflightItem {
                relative_path: relative_path(root, Path::new(&doc.source_path)),
                source_path: doc.source_path,
                filename: doc.filename,
                size_bytes: doc.size_bytes,
                stage: doc.stage,
                category: doc.category,
                is_existing,
                default_disposition,
            }
        })
        .collect();
    MaterialPreflight {
        mode: mode.to_string(),
        case_id,
        root_path: root.to_string_lossy().to_string(),
        legal_domain: legal_domain.to_string(),
        total_files: items.len(),
        total_size_bytes,
        large_criminal_batch,
        items,
    }
}

/// 纯文件元数据扫描：不读数据库、不写数据库、不调用 OCR/LLM/网络。
#[tauri::command]
pub fn preview_material_import(
    path: String,
    legal_domain: String,
) -> Result<MaterialPreflight, String> {
    let legal_domain = validate_domain(&legal_domain)?;
    let root = Path::new(&path);
    if !root.is_dir() {
        return Err(format!("不是文件夹: {path}"));
    }
    let scanned = scan_folder_for_domain(root, legal_domain);
    Ok(build_preflight(
        "import",
        None,
        root,
        legal_domain,
        scanned,
        &HashSet::new(),
        &HashMap::new(),
    ))
}

/// 刷新预检只读取案件/既有决策与文件元数据，不产生任何持久化变更或网络调用。
#[tauri::command]
pub async fn preview_material_refresh(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<MaterialPreflight, String> {
    let case = crate::db::cases::get_case(pool.inner(), &case_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "案件不存在".to_string())?;
    let legal_domain = validate_domain(&case.legal_domain)?;
    let root = Path::new(&case.source_folder);
    if !root.is_dir() {
        return Err(format!("案件源文件夹不可用: {}", case.source_folder));
    }
    let documents = crate::db::documents::list_documents_by_case(pool.inner(), &case_id)
        .await
        .map_err(|error| error.to_string())?;
    let saved_decisions = crate::db::material_queue::list_decisions(pool.inner(), &case_id)
        .await?
        .into_iter()
        .map(|decision| (decision.source_path, decision.disposition))
        .collect::<HashMap<_, _>>();
    // 只有已经经过三态确认的源文件才算“既有”。法院短信、case bundle 等旁路
    // 写入的 pending 文档没有决策，刷新预检时仍显示为“新增待确认”。
    let existing_paths = documents
        .iter()
        .filter(|doc| !doc.is_ai_artifact && saved_decisions.contains_key(&doc.source_path))
        .map(|doc| doc.source_path.clone())
        .collect::<HashSet<_>>();
    let scanned = scan_folder_for_domain(root, legal_domain);
    Ok(build_preflight(
        "refresh",
        Some(case_id),
        root,
        legal_domain,
        scanned,
        &existing_paths,
        &saved_decisions,
    ))
}

#[tauri::command]
pub async fn commit_material_preflight(
    app: AppHandle,
    pool: tauri::State<'_, SqlitePool>,
    input: CommitMaterialPreflightInput,
) -> Result<CommitMaterialPreflightResult, String> {
    let legal_domain = validate_domain(&input.legal_domain)?;
    if !matches!(input.mode.as_str(), "import" | "refresh") {
        return Err("不支持的材料预检提交模式".to_string());
    }
    let root = Path::new(&input.root_path);
    if !root.is_dir() {
        return Err(format!("不是文件夹: {}", input.root_path));
    }

    // 提交时重扫一次，预览后新增/删除文件会使提交失败，必须重新预检。
    let scanned = scan_folder_for_domain(root, legal_domain);
    let current_paths = scanned
        .iter()
        .map(|doc| doc.source_path.clone())
        .collect::<HashSet<_>>();
    let decision_paths = input
        .decisions
        .iter()
        .map(|decision| decision.source_path.clone())
        .collect::<HashSet<_>>();
    if current_paths != decision_paths || input.decisions.len() != decision_paths.len() {
        return Err("目录内容在预检后发生变化，请重新预检并确认".to_string());
    }

    let existing_case = crate::db::cases::find_case_by_folder(pool.inner(), &input.root_path)
        .await
        .map_err(|error| error.to_string())?;
    let is_existing = existing_case.is_some();
    let case = if input.mode == "refresh" {
        let case_id = input
            .case_id
            .as_deref()
            .ok_or_else(|| "刷新预检缺少案件 ID".to_string())?;
        let case = crate::db::cases::get_case(pool.inner(), case_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "案件不存在".to_string())?;
        if case.source_folder != input.root_path || case.legal_domain != legal_domain {
            return Err("预检案件与提交案件不一致".to_string());
        }
        case
    } else {
        let default_name = root
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "未命名案件".to_string());
        let case_type = if legal_domain == "criminal" {
            "刑事诉讼"
        } else {
            "民事诉讼"
        };
        crate::db::cases::upsert_case_for_folder(
            pool.inner(),
            &input.root_path,
            &default_name,
            case_type,
        )
        .await
        .map_err(|error| error.to_string())?
    };
    if case.legal_domain != legal_domain {
        return Err(format!(
            "该文件夹已绑定为 {} 案件，不能按 {} 材料提交",
            case.legal_domain, legal_domain
        ));
    }

    let disposition_by_path = input
        .decisions
        .iter()
        .map(|decision| (decision.source_path.as_str(), decision.disposition.as_str()))
        .collect::<HashMap<_, _>>();
    let included = scanned
        .iter()
        .filter(|doc| disposition_by_path.get(doc.source_path.as_str()) != Some(&"excluded"))
        .cloned()
        .collect::<Vec<_>>();
    let sync = crate::db::documents::sync_documents_for_case(pool.inner(), &case.id, &included)
        .await
        .map_err(|error| error.to_string())?;
    let documents = crate::db::documents::list_documents_by_case(pool.inner(), &case.id)
        .await
        .map_err(|error| error.to_string())?;
    let document_by_path = documents
        .iter()
        .map(|doc| (doc.source_path.as_str(), doc.id.as_str()))
        .collect::<HashMap<_, _>>();

    let decisions = input
        .decisions
        .iter()
        .map(|decision| MaterialDecisionInput {
            source_path: decision.source_path.clone(),
            disposition: decision.disposition.clone(),
            document_id: (decision.disposition != "excluded")
                .then(|| document_by_path.get(decision.source_path.as_str()).copied())
                .flatten()
                .map(str::to_string),
        })
        .collect::<Vec<_>>();
    crate::db::material_queue::save_decisions(pool.inner(), &case.id, &decisions).await?;

    for decision in &decisions {
        if let Some(document_id) = decision.document_id.as_deref() {
            if decision.disposition == "index_only" {
                sqlx::query(
                    "UPDATE documents SET extraction_status='skipped',last_error=?1 WHERE id=?2",
                )
                .bind(INDEX_ONLY_REASON)
                .bind(document_id)
                .execute(pool.inner())
                .await
                .map_err(|error| error.to_string())?;
            } else if decision.disposition == "recognize" {
                sqlx::query(
                    "UPDATE documents SET extraction_status='pending',last_error=NULL \
                     WHERE id=?1 AND extraction_status='skipped' AND last_error=?2",
                )
                .bind(document_id)
                .bind(INDEX_ONLY_REASON)
                .execute(pool.inner())
                .await
                .map_err(|error| error.to_string())?;
            }
        }
    }

    let documents = crate::db::documents::list_documents_by_case(pool.inner(), &case.id)
        .await
        .map_err(|error| error.to_string())?;
    let recognize_paths = decisions
        .iter()
        .filter(|decision| decision.disposition == "recognize")
        .map(|decision| decision.source_path.as_str())
        .collect::<HashSet<_>>();
    let queue_items = documents
        .iter()
        .filter(|doc| {
            doc.extraction_status == "pending" && recognize_paths.contains(doc.source_path.as_str())
        })
        .map(|doc| MaterialQueueItemInput {
            source_path: doc.source_path.clone(),
            document_id: Some(doc.id.clone()),
        })
        .collect::<Vec<_>>();
    let batch = if queue_items.is_empty() {
        None
    } else {
        let detail =
            crate::db::material_queue::create_batch(pool.inner(), &case.id, &queue_items).await?;
        if input.start_processing {
            let started =
                crate::db::material_queue::start_batch(pool.inner(), &detail.batch.id).await?;
            crate::ingest::pipeline::spawn_material_processing_batch(
                app,
                pool.inner().clone(),
                detail.batch.id.clone(),
            );
            Some(started)
        } else {
            Some(detail)
        }
    };
    Ok(CommitMaterialPreflightResult {
        case,
        documents,
        sync,
        batch,
        is_existing,
    })
}

#[tauri::command]
pub async fn start_material_batch_execution(
    app: AppHandle,
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
) -> Result<MaterialBatchDetail, String> {
    let detail = crate::db::material_queue::start_batch(pool.inner(), &batch_id).await?;
    crate::ingest::pipeline::spawn_material_processing_batch(app, pool.inner().clone(), batch_id);
    Ok(detail)
}

#[tauri::command]
pub async fn resume_material_batch_execution(
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
) -> Result<MaterialBatchDetail, String> {
    // 恢复只把 paused/recovery_required 明确放回 queued；仍需用户再次点击“开始”。
    crate::db::material_queue::resume_batch(pool.inner(), &batch_id).await
}

#[tauri::command]
pub async fn ignore_failed_material_items(
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
    error_category: Option<String>,
) -> Result<MaterialBatchDetail, String> {
    crate::db::material_queue::ignore_failed_items(
        pool.inner(),
        &batch_id,
        error_category.as_deref(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_criminal_preflight_defaults_to_index_only_without_side_effects() {
        let root = Path::new("C:/cases/example");
        let scanned = (0..CRIMINAL_LARGE_BATCH_THRESHOLD)
            .map(|index| ScannedDoc {
                source_path: format!("C:/cases/example/侦查/{index}.pdf"),
                filename: format!("{index}.pdf"),
                stage: Some("侦查".to_string()),
                category: None,
                is_ai_artifact: false,
                size_bytes: 10,
                modified_at: None,
            })
            .collect();
        let preview = build_preflight(
            "import",
            None,
            root,
            "criminal",
            scanned,
            &HashSet::new(),
            &HashMap::new(),
        );
        assert!(preview.large_criminal_batch);
        assert!(preview
            .items
            .iter()
            .all(|item| item.default_disposition == "index_only"));
    }

    #[test]
    fn existing_decision_wins_over_default_and_paths_are_relative() {
        let root = Path::new("C:/cases/example");
        let path = "C:/cases/example/审查起诉/材料.pdf".to_string();
        let preview = build_preflight(
            "refresh",
            Some("case-1".to_string()),
            root,
            "criminal",
            vec![ScannedDoc {
                source_path: path.clone(),
                filename: "材料.pdf".to_string(),
                stage: Some("审查起诉".to_string()),
                category: None,
                is_ai_artifact: false,
                size_bytes: 10,
                modified_at: None,
            }],
            &HashSet::from([path.clone()]),
            &HashMap::from([(path, "excluded".to_string())]),
        );
        assert_eq!(preview.items[0].relative_path, "审查起诉/材料.pdf");
        assert!(preview.items[0].is_existing);
        assert_eq!(preview.items[0].default_disposition, "excluded");
    }
}
