//! 案件级批处理管线:扫描完后台跑字段抽取,通过 Tauri Event 推送进度。
//!
//! 设计:
//!   - 输入: case_id + 该案件所有 documents
//!   - 流程: 对每个 doc 跑 extractor → 写入 documents.extracted_fields + extraction_status
//!   - 每个文档处理前后都 emit 一次 Event 给前端
//!   - 不阻塞调用方:在 tokio task 里跑
//!
//! 前端订阅 `extraction_progress` 事件即可看到实时进度。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

use crate::db::documents::Document;
use crate::ingest::extractor::{extract_one, ExtractResult, ExtractionExecutionGuard};
use crate::ingest::ocr::OcrContext;
use crate::llm;
use crate::settings;

/// 三轮动态降级:8 路 → 4 路 → 1 路
///
/// 2026-05-25 V0.1.8 加(替代原来固定 8 路):MinerU 精准 API 偶发限流时,
/// 第 1 轮失败的进第 2 轮(并发减半),第 2 轮还失败进第 3 轮(单线程串行),
/// 第 3 轮失败才算真失败,落 last_error。"要把它提取完毕"——作者原话。
const ROUND_CONCURRENCY: [usize; 3] = [8, 4, 1];

/// 每轮之间的缓冲 sleep(秒),给服务端限流计数器恢复
const INTER_ROUND_SLEEP_SEC: u64 = 3;

/// 全应用唯一材料供应商调用闸门。批次可以并发创建/排队，但 OCR/LLM 执行器只能有一个。
static MATERIAL_EXECUTION_GATE: OnceLock<Semaphore> = OnceLock::new();

fn material_execution_gate() -> &'static Semaphore {
    MATERIAL_EXECUTION_GATE.get_or_init(|| Semaphore::new(1))
}

/// MinerU "提交任务"接口最小间隔(毫秒)
///
/// 官网限流:**50 文件/分钟**(提交任务接口共用频控,详见 docs/MinerU精准解析API使用整理.md 第 12 节)。
/// 1400ms 间隔 = ~43 次/分钟,留 7 次 buffer 避免撞顶。
/// 节流只对**云端 OCR**生效(本机 vision / pdftotext / textutil 不消耗配额)。
const SUBMIT_MIN_INTERVAL_MS: u64 = 1400;

/// 全局节流闸门 —— 控制 MinerU API 提交频率,避开 50 文件/分钟限流。
///
/// 跨所有 buffer_unordered task 共享(Arc 包裹),跨三轮重试也共享。
/// 实现:Mutex 保护 last_submit 时间戳,acquire 时计算需要等多久,
/// 释放锁后 sleep,再回去更新时间戳(避免持锁 sleep 串行化所有 task)。
pub struct SubmitThrottle {
    last_submit: tokio::sync::Mutex<std::time::Instant>,
    min_interval: Duration,
}

impl Default for SubmitThrottle {
    fn default() -> Self {
        Self::new()
    }
}

impl SubmitThrottle {
    pub fn new() -> Self {
        Self {
            // 初始化"60 秒前",首次 acquire 不等
            last_submit: tokio::sync::Mutex::new(
                std::time::Instant::now() - Duration::from_secs(60),
            ),
            min_interval: Duration::from_millis(SUBMIT_MIN_INTERVAL_MS),
        }
    }

    pub async fn acquire(&self) {
        loop {
            let mut last = self.last_submit.lock().await;
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(*last);
            if elapsed >= self.min_interval {
                *last = now;
                return;
            }
            let wait = self.min_interval - elapsed;
            drop(last); // 关键:释放锁再 sleep,允许别的 task 排队
            tokio::time::sleep(wait).await;
        }
    }
}

/// 判断这个文件名是否会触发**云端 OCR / 文档解析提交**(走 MinerU API)。
///
/// PDF / 图片 / office 文档(doc/rtf/odt/ppt/xls,2026-06-16 起统一走 MinerU 云端解析)
/// **且** cloud_enabled 时才占 MinerU 配额。docx / txt / md / html 走原生解析 / 直接读,
/// 不消耗 MinerU 配额,不必节流。
///
/// 注意:PDF 可能 pdf-inspector 直抽成功无需 OCR fallback,这种情况节流是"误打"——
/// 多 sleep 1.4s 而已,可接受(简化判断,避免在调度层重复 PDF 文本探测)。
fn might_hit_mineru(filename: &str) -> bool {
    let f = filename.to_lowercase();
    f.ends_with(".pdf")
        || super::extractor::is_ocr_image_ext(&f)
        || super::extractor::is_office_cloud_ext(&f)
}

/// 进度事件 payload,emit 给前端的 "extraction_progress" 事件。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// 整批开始(2026-05-23 加 backend 字段,前端显示用什么后端)
    Started {
        case_id: String,
        total: usize,
        ocr_provider: String, // "local" | "cloud"
        llm_provider: String,
        llm_model: String, // 用具体模型名,前端展示更细
    },
    /// 单个文档开始处理
    DocStarted {
        case_id: String,
        doc_id: String,
        filename: String,
        index: usize,
        total: usize,
        ocr_provider: String,
        llm_provider: String,
    },
    /// 2026-06-14:单个文档**云端 OCR 轮询中**的实时状态(治大图扫描件"看着卡死")。
    /// 不进 DocStarted/DocFinished 那条主进度线,前端作为附加子状态显示(不动百分比),
    /// 每 ~3 秒来一拍。`phase`:queued(排队)/ processing(识别中)/ converting(转换中)。
    DocOcrStatus {
        case_id: String,
        doc_id: String,
        filename: String,
        index: usize,
        total: usize,
        phase: String,
        elapsed_secs: u64,
        pages_done: Option<i64>,
        pages_total: Option<i64>,
    },
    /// 单个文档处理完成(成功/跳过/失败任意一种)。
    ///
    /// 2026-05-24 i:`index` 是 doc 在原列表里的固定序号(并发顺序不保证);
    /// `completed_count` 是**单调递增的完成计数**(用 AtomicUsize 算),
    /// 前端进度条 percent 应该用 `completed_count / total` 而不是 `index / total`,
    /// 否则并发完成顺序乱会让进度回退。
    DocFinished {
        case_id: String,
        doc_id: String,
        filename: String,
        index: usize,
        total: usize,
        completed_count: usize,
        outcome: DocOutcome,
    },
    /// 2026-06-11:逐文档 OCR 完成后、全案 LLM 分析开始(这步几十秒到几分钟,
    /// 没这事件前端浮层会停在"已完成 N/N 100%"转圈,被用户当成卡死)
    Analyzing { case_id: String },
    /// 整批完成
    Completed {
        case_id: String,
        total: usize,
        extracted: usize,
        skipped: usize,
        failed: usize,
        elapsed_ms: u128,
        /// 2026-06-11:全案 LLM 分析是否成功(失败时 agg_*/详情页不会更新,
        /// 此前静默吞掉、浮层照样显示"全部完成",用户以为成功)
        analysis_ok: bool,
        analysis_error: Option<String>,
    },
    /// 本机服务 / 云端 token 没就绪,整批没法开跑 — 2026-05-23 加
    Error { case_id: String, error: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DocOutcome {
    Extracted,
    Skipped { reason: String },
    Failed { error: String },
}

/// 在 tokio task 里跑批处理。调用立即返回,前端通过事件订阅进度。
///
/// `app`: AppHandle 用于 emit 事件;`pool`: sqlx 连接池;`case_id`: 案件 ID;
/// `documents`: 该案件下所有要处理的文档(调用方先 list_documents_by_case)。
/// `run_analysis`:OCR 抽完后是否自动跑全案 LLM 分析(run_global_extract,烧 DeepSeek)。
/// 导入 / 刷新源文件 = true(一批文档完一次性分析);**单文档重识别 = false**
/// (否则连续重识别 N 个失败文档 = 触发 N 次全案分析,白白烧钱 —— 胡彬律师反馈)。
/// run_analysis=false 时,用户识别完一批后手动点「重新分析」一次即可。
pub fn spawn_extraction(
    app: AppHandle,
    pool: SqlitePool,
    case_id: String,
    documents: Vec<Document>,
    _run_analysis: bool,
) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = enqueue_decided_documents_and_run(&app, &pool, &case_id, &documents).await {
            crate::dlog!("[material_queue] case {} 入队/执行失败: {}", case_id, e);
        }
    });
}

async fn enqueue_decided_documents_and_run(
    app: &AppHandle,
    pool: &SqlitePool,
    case_id: &str,
    documents: &[Document],
) -> Result<Option<String>, String> {
    let decisions = crate::db::material_queue::list_decisions(pool, case_id).await?;
    let recognized = decisions
        .into_iter()
        .filter(|decision| decision.disposition == "recognize")
        .map(|decision| decision.source_path)
        .collect::<std::collections::HashSet<_>>();
    let items = documents
        .iter()
        .filter(|doc| {
            doc.extraction_status == "pending"
                && doc.deleted_at.is_none()
                && recognized.contains(&doc.source_path)
        })
        .map(|doc| crate::db::material_queue::MaterialQueueItemInput {
            source_path: doc.source_path.clone(),
            document_id: Some(doc.id.clone()),
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Ok(None);
    }
    let detail = crate::db::material_queue::create_batch(pool, case_id, &items).await?;
    crate::db::material_queue::start_batch(pool, &detail.batch.id).await?;
    run_material_processing_batch(app, pool, &detail.batch.id).await?;
    Ok(Some(detail.batch.id))
}

/// 启动已经由用户确认并创建的持久批次。领取令牌始终只存在 Rust 内部。
pub fn spawn_material_processing_batch(app: AppHandle, pool: SqlitePool, batch_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_material_processing_batch(&app, &pool, &batch_id).await {
            crate::dlog!("[material_queue] batch {} 执行失败: {}", batch_id, error);
        }
    });
}

async fn run_material_processing_batch(
    app: &AppHandle,
    pool: &SqlitePool,
    batch_id: &str,
) -> Result<(), String> {
    let _execution_permit = material_execution_gate()
        .acquire()
        .await
        .map_err(|_| "材料执行闸门已关闭".to_string())?;
    // 等待全局闸门期间，用户可能已暂停/取消该批次；获闸后必须重新确认。
    let initial = crate::db::material_queue::get_batch_detail(pool, batch_id).await?;
    if initial.batch.status != "running" {
        return Ok(());
    }
    let case_id = initial.batch.case_id.clone();
    let total = initial.items.len();
    let user_settings = settings::read_settings().unwrap_or_default();
    let llm_config = llm::LlmConfig::from_settings(&user_settings);
    let cloud_ocr = user_settings.effective_ocr_provider() == "cloud";
    let ocr_ctx = OcrContext {
        cloud_enabled: cloud_ocr,
        mineru_token: cloud_ocr
            .then(|| {
                crate::credentials::resolve_static_string(
                    crate::credentials::StaticCredential::Mineru,
                )
                .ok()
                .flatten()
            })
            .flatten(),
        paddle_vl_token: cloud_ocr
            .then(|| {
                crate::credentials::resolve_static_string(
                    crate::credentials::StaticCredential::PaddleVl,
                )
                .ok()
                .flatten()
            })
            .flatten(),
        cloud_primary: user_settings.effective_ocr_cloud_primary().to_string(),
        force_backend: None,
        poll_tx: None,
    };
    let ocr_provider = user_settings.effective_ocr_provider().to_string();
    let llm_provider = user_settings.effective_llm_provider().to_string();
    let _ = app.emit(
        "extraction_progress",
        ProgressEvent::Started {
            case_id: case_id.clone(),
            total,
            ocr_provider: ocr_provider.clone(),
            llm_provider: llm_provider.clone(),
            llm_model: llm_config.model.clone(),
        },
    );
    if user_settings.needs_local_server() {
        crate::lifecycle::ensure_local_ready(user_settings.local_model_dir.as_deref())
            .await
            .map_err(|error| format!("本机模型未就绪: {error}"))?;
    }

    let throttle = SubmitThrottle::new();
    let mut completed_count = 0usize;
    loop {
        let Some(item) = crate::db::material_queue::claim_next(pool, batch_id).await? else {
            break;
        };
        let Some(claim_token) = item.claim_token.clone() else {
            return Err("队列条目领取后缺少内部令牌".to_string());
        };
        let Some(document_id) = item.document_id.as_deref() else {
            crate::db::material_queue::fail_item(
                pool,
                item.id.clone(),
                claim_token,
                Some("missing_document".to_string()),
                Some("识别条目未关联文档索引".to_string()),
            )
            .await?;
            continue;
        };
        let Some(doc) = crate::db::documents::get_document_by_id(pool, document_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            crate::db::material_queue::fail_item(
                pool,
                item.id.clone(),
                claim_token,
                Some("missing_document".to_string()),
                Some("文档索引不存在或已删除".to_string()),
            )
            .await?;
            continue;
        };
        if settle_without_network_if_document_terminal(
            pool,
            &item.id,
            &claim_token,
            &doc.extraction_status,
        )
        .await?
        {
            completed_count += 1;
            let outcome = DocOutcome::Skipped {
                reason: format!("文档状态已为 {}，零网络结算队列条目", doc.extraction_status),
            };
            let _ = app.emit(
                "extraction_progress",
                ProgressEvent::DocFinished {
                    case_id: case_id.clone(),
                    doc_id: doc.id,
                    filename: doc.filename,
                    index: item.ordinal as usize,
                    total,
                    completed_count,
                    outcome,
                },
            );
            continue;
        }
        let guard =
            ExtractionExecutionGuard::new(pool.clone(), item.id.clone(), claim_token.clone());
        let mut outcome = DocOutcome::Failed {
            error: "识别未开始".to_string(),
        };
        for round_num in 1..=ROUND_CONCURRENCY.len() {
            if !crate::db::material_queue::execution_allowed(pool, &item.id, &claim_token).await? {
                break;
            }
            outcome = process_one_doc(
                app,
                pool,
                &case_id,
                &llm_config,
                &ocr_ctx,
                &ocr_provider,
                &llm_provider,
                item.ordinal as usize,
                total,
                doc.clone(),
                round_num,
                round_num == ROUND_CONCURRENCY.len(),
                &throttle,
                Some(&guard),
            )
            .await;
            if !matches!(outcome, DocOutcome::Failed { .. }) {
                break;
            }
            if round_num < ROUND_CONCURRENCY.len() {
                tokio::time::sleep(Duration::from_secs(INTER_ROUND_SLEEP_SEC)).await;
            }
        }

        if !crate::db::material_queue::execution_allowed(pool, &item.id, &claim_token).await? {
            continue;
        }
        match &outcome {
            DocOutcome::Extracted | DocOutcome::Skipped { .. } => {
                crate::db::material_queue::finish_item(pool, &item.id, &claim_token).await?;
            }
            DocOutcome::Failed { error } => {
                let category = classify_queue_error(error);
                crate::db::material_queue::fail_item(
                    pool,
                    item.id.clone(),
                    claim_token.clone(),
                    Some(category.to_string()),
                    Some(error.clone()),
                )
                .await?;
            }
        }
        completed_count += 1;
        let _ = app.emit(
            "extraction_progress",
            ProgressEvent::DocFinished {
                case_id: case_id.clone(),
                doc_id: doc.id,
                filename: doc.filename,
                index: item.ordinal as usize,
                total,
                completed_count,
                outcome,
            },
        );
    }
    Ok(())
}

async fn settle_without_network_if_document_terminal(
    pool: &SqlitePool,
    item_id: &str,
    claim_token: &str,
    extraction_status: &str,
) -> Result<bool, String> {
    if !matches!(extraction_status, "done" | "skipped") {
        return Ok(false);
    }
    crate::db::material_queue::finish_item(pool, item_id, claim_token).await?;
    Ok(true)
}

fn classify_queue_error(error: &str) -> &'static str {
    let lower = error.to_lowercase();
    if lower.contains("429") || lower.contains("限额") || lower.contains("quota") {
        "rate_limit"
    } else if lower.contains("timeout") || lower.contains("超时") {
        "timeout"
    } else if lower.contains("ocr") {
        "ocr"
    } else if lower.contains("llm") || lower.contains("json") {
        "llm"
    } else {
        "processing"
    }
}

/// 强制重抽单个文档的共享入口:重置 `extraction_status='pending'` + 清 `last_error`
/// → 取回文档 → `spawn_extraction` 后台异步抽取(走现有 `extraction_progress` 事件通道,
/// 前端订阅看进度 + 完成自动刷新)。返回被重抽文档的 `filename`(给调用方做提示)。
///
/// 由两个调用方复用,防逻辑漂移:① 源文件列表「重新抽取」按钮的 `reextract_document` 命令;
/// ② 案件 AI 助手的 `reextract_document` chat 工具。
/// ⚠️ 会重跑 OCR/LLM(PDF 走云端 OCR 会再烧 MinerU 积分,须用户主动选择)。
/// `ocr_backend_override`:`Some("ppocrv6")` = 去水印重识别(强制 PP-OCRv6+去水印);
/// `None` = 普通重识别,**清除**该文档之前可能设过的覆盖(回到常规 OCR 策略)。
pub async fn trigger_reextract(
    app: AppHandle,
    pool: &SqlitePool,
    doc_id: &str,
    ocr_backend_override: Option<&str>,
) -> Result<String, String> {
    crate::db::documents::reset_for_reextract(pool, doc_id)
        .await
        .map_err(|e| e.to_string())?;
    // 写/清文档级 OCR 覆盖(必须在 get_document_by_id 之前,这样取回的 doc 带上覆盖,
    // 随 spawn_extraction → process_one_doc 生效)。
    crate::db::documents::set_ocr_backend_override(pool, doc_id, ocr_backend_override)
        .await
        .map_err(|e| e.to_string())?;
    let doc = crate::db::documents::get_document_by_id(pool, doc_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "文档不存在或已删除".to_string())?;
    let case_id = doc.case_id.clone();
    let filename = doc.filename.clone();
    crate::db::material_queue::save_decisions(
        pool,
        &case_id,
        &[crate::db::material_queue::MaterialDecisionInput {
            source_path: doc.source_path.clone(),
            disposition: "recognize".to_string(),
            document_id: Some(doc.id.clone()),
        }],
    )
    .await?;
    // run_analysis=false:重识别单文档不自动跑全案分析(省钱),用户识别完一批后手动点「重新分析」。
    spawn_extraction(app, pool.clone(), case_id, vec![doc], false);
    Ok(filename)
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedRetryReport {
    pub used_cached_text: bool,
    pub status: String,
    pub error: Option<String>,
}

/// 兼容旧的缓存重试入口，但实际执行统一进入持久队列。
///
/// 缓存文本的 LLM 重试此前绕过 claim/token，无法可靠暂停或取消。为保证所有外部调用
/// 都受执行令牌控制，此入口不再直接调用 LLM；由队列工作器按统一策略处理。
pub async fn trigger_reextract_cached(
    app: AppHandle,
    pool: &SqlitePool,
    doc_id: &str,
) -> Result<CachedRetryReport, String> {
    trigger_reextract(app, pool, doc_id, None).await?;
    Ok(CachedRetryReport {
        used_cached_text: false,
        status: "pending".into(),
        error: None,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct CriminalCaseReextractReport {
    pub cached_count: usize,
    pub scheduled_ocr_count: usize,
    pub failed_count: usize,
    pub errors: Vec<String>,
}

fn require_criminal_material_domain(legal_domain: &str) -> Result<(), String> {
    if legal_domain == "criminal" {
        Ok(())
    } else {
        Err(format!(
            "DOMAIN_MISMATCH: 只有刑事案件可以使用刑事材料重新识别（当前领域: {legal_domain}）"
        ))
    }
}

/// 案件级刑事材料重新识别：逐份优先复用持久化正文；缺失缓存的材料合并回退到
/// 既有 OCR/LLM 队列。不会调用民事全案聚合，也不会直接写刑事画像。
pub async fn reextract_criminal_case_materials(
    app: AppHandle,
    pool: &SqlitePool,
    case_id: &str,
) -> Result<CriminalCaseReextractReport, String> {
    let case = crate::db::cases::get_case(pool, case_id)
        .await
        .map_err(|e| format!("DATABASE_READ_FAILED: 读取案件失败: {e}"))?
        .ok_or_else(|| "CASE_NOT_FOUND: 案件不存在".to_string())?;
    require_criminal_material_domain(&case.legal_domain)?;
    let docs = crate::db::documents::list_documents_by_case(pool, case_id)
        .await
        .map_err(|e| format!("MATERIAL_UNREADABLE: 读取案件材料清单失败: {e}"))?;
    let mut cached_count = 0;
    let mut missing = Vec::new();
    let mut errors = Vec::new();
    for doc in docs
        .into_iter()
        .filter(|d| d.deleted_at.is_none() && !d.is_ai_artifact)
    {
        if doc
            .extracted_text_path
            .as_deref()
            .is_some_and(|path| std::fs::read_to_string(path).is_ok())
        {
            match trigger_reextract_cached(app.clone(), pool, &doc.id).await {
                Ok(_) => cached_count += 1,
                Err(e) => errors.push(format!(
                    "RECOGNITION_ENGINE_FAILED: {}: {}",
                    doc.filename, e
                )),
            }
        } else {
            crate::db::documents::reset_for_reextract(pool, &doc.id)
                .await
                .map_err(|e| format!("RECOGNITION_ENGINE_FAILED: 重置材料状态失败: {e}"))?;
            crate::db::documents::set_ocr_backend_override(pool, &doc.id, None)
                .await
                .map_err(|e| format!("RECOGNITION_ENGINE_FAILED: 重置 OCR 路由失败: {e}"))?;
            let mut refreshed = crate::db::documents::get_document_by_id(pool, &doc.id)
                .await
                .map_err(|e| format!("MATERIAL_UNREADABLE: 重读材料失败: {e}"))?
                .ok_or_else(|| format!("MATERIAL_UNREADABLE: 文档已不存在: {}", doc.filename))?;
            refreshed.extraction_status = "pending".into();
            missing.push(refreshed);
        }
    }
    if cached_count == 0 && missing.is_empty() && errors.is_empty() {
        return Err("MATERIAL_UNREADABLE: 案件中没有可识别的材料".into());
    }
    let scheduled_ocr_count = missing.len();
    if !missing.is_empty() {
        let decisions = missing
            .iter()
            .map(|doc| crate::db::material_queue::MaterialDecisionInput {
                source_path: doc.source_path.clone(),
                disposition: "recognize".to_string(),
                document_id: Some(doc.id.clone()),
            })
            .collect::<Vec<_>>();
        crate::db::material_queue::save_decisions(pool, case_id, &decisions).await?;
        spawn_extraction(app, pool.clone(), case_id.to_string(), missing, false);
    }
    Ok(CriminalCaseReextractReport {
        cached_count,
        scheduled_ocr_count,
        failed_count: errors.len(),
        errors,
    })
}

/// 单个 doc 的完整处理(emit 进度 + 调 extractor + 写 DB)。
///
/// 设计成 owned-by-task:所有参数 borrow,但传给它的实际值都是 task 自己 clone 的副本,
/// 这样 buffer_unordered 的多个 task 可以独立运行不互相阻塞。
///
/// 2026-05-25 V0.1.8 加 `round_num` / `is_final_round`:
///   - DocStarted 事件带轮次(前端可显示"重试中 N/3")
///   - 失败时只有 is_final_round=true 才写 status='failed' + last_error;
///     中间轮失败回退到 status='pending'(下一轮会重新 UPDATE 成 processing)
#[allow(clippy::too_many_arguments)]
async fn process_one_doc(
    app: &AppHandle,
    pool: &SqlitePool,
    case_id: &str,
    llm_config: &llm::LlmConfig,
    ocr_ctx: &OcrContext,
    ocr_provider: &str,
    llm_provider: &str,
    index: usize,
    total: usize,
    doc: Document,
    round_num: usize,
    is_final_round: bool,
    throttle: &SubmitThrottle,
    execution_guard: Option<&crate::ingest::extractor::ExtractionExecutionGuard>,
) -> DocOutcome {
    // 文件名加轮次后缀(第 2/3 轮),前端能感知"在重试"
    let display_name = if round_num > 1 {
        format!(
            "{} (重试 {}/{})",
            doc.filename,
            round_num,
            ROUND_CONCURRENCY.len()
        )
    } else {
        doc.filename.clone()
    };
    let _ = app.emit(
        "extraction_progress",
        ProgressEvent::DocStarted {
            case_id: case_id.to_string(),
            doc_id: doc.id.clone(),
            filename: display_name.clone(),
            index,
            total,
            ocr_provider: ocr_provider.to_string(),
            llm_provider: llm_provider.to_string(),
        },
    );

    let _ = sqlx::query("UPDATE documents SET extraction_status = 'processing' WHERE id = ?")
        .bind(&doc.id)
        .execute(pool)
        .await;

    // 2026-06-14:云端 OCR 单文档轮询进度 → 前端"排队 / 识别中(已 N 秒)"子状态。
    // 建一个单文档级回传通道 + 转发任务:OCR 轮询循环每拍 send 一次 OcrPollUpdate,
    // 转发任务补上 doc 上下文后 emit DocOcrStatus(**不动主进度条百分比**,前端单独渲染)。
    // tx 随 doc_ocr_ctx 在本函数末尾 drop,转发任务届时自然结束(rx 收到 None)。
    // 仅在可能走云端 OCR(cloud_enabled 或去水印强制后端)时才建,本地/跳过文档不浪费。
    let mut doc_ocr_ctx = ocr_ctx.clone();
    if ocr_ctx.cloud_enabled || doc.ocr_backend_override.is_some() {
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<crate::ingest::ocr::OcrPollUpdate>();
        doc_ocr_ctx.poll_tx = Some(tx);
        let app_fwd = app.clone();
        let case_id_fwd = case_id.to_string();
        let doc_id_fwd = doc.id.clone();
        let name_fwd = display_name.clone();
        tokio::spawn(async move {
            while let Some(u) = rx.recv().await {
                let _ = app_fwd.emit(
                    "extraction_progress",
                    ProgressEvent::DocOcrStatus {
                        case_id: case_id_fwd.clone(),
                        doc_id: doc_id_fwd.clone(),
                        filename: name_fwd.clone(),
                        index,
                        total,
                        phase: u.phase,
                        elapsed_secs: u.elapsed_secs,
                        pages_done: u.pages_done,
                        pages_total: u.pages_total,
                    },
                );
            }
        });
    }

    // 2026-05-31 抽取策略改版(作者:现在所有材料都要抽,做案件分析/对抗需要证据支撑)。
    // 三档:
    //   A. 完整抽(字段 + 文本,进 LLM 上下文):法院文书 + 我方文书 + **证据材料**
    //      (合同/催告函/对话记录等 —— 作者明确要进对抗分析)
    //   B. 仅文本归档(抽文本存着,但**不进** LLM 上下文):律所规范/程序材料
    //      (风险告知书/谈话笔录/反馈卡/送达确认书 等)+ 身份信息(隐私,无分析价值)。
    //      上下文排除在 constitution.rs 用 is_archival_category 把关;这里只负责抽文本归档。
    //   C. 纯跳过:AI 产物(已是结构化 .md,再抽回上下文会自证循环)。
    let result = if let Some(backend) = doc.ocr_backend_override.clone() {
        // 2026-06-13:用户对该文档点了「去水印重识别」→ 强制走该 OCR 后端(PP-OCRv6+去水印),
        // 绕过归档短路与文本层、不回退;让带水印的工商调档件也能完整抽。
        if ocr_ctx.cloud_enabled && might_hit_mineru(&doc.filename) {
            throttle.acquire().await;
        }
        doc_ocr_ctx.force_backend = Some(backend);
        extract_one(
            llm_config,
            &doc_ocr_ctx,
            Path::new(&doc.source_path),
            &doc.filename,
            doc.category.as_deref(),
            execution_guard,
        )
        .await
    } else if doc.is_ai_artifact {
        // C. AI 产物纯跳过
        ExtractResult::Skipped {
            reason: "AI 产物已是结构化总结,跳过(详情页直接渲染原文)".to_string(),
            metrics: Vec::new(),
        }
    } else if is_archival_category(doc.category.as_deref())
        || doc.stage.as_deref() == Some("身份信息")
    {
        // B. 律所规范/程序/身份材料:只抽文本归档(便宜直抽,扫描件不烧 OCR),不抽 LLM 字段。
        // 文本仍写盘 → read_case_doc / 全文搜索可读;但 constitution 不把它塞进 system prompt。
        match crate::ingest::extractor::extract_text_only_cheap(
            Path::new(&doc.source_path),
            &doc.filename,
        )
        .await
        {
            Ok(Some(text_md)) => ExtractResult::TextOnly {
                text_md,
                metrics: Vec::new(),
            },
            // 直抽拿不到(扫描件/图片需 OCR)或真错 → 纯跳过(无文本),归档类不值得烧 OCR
            _ => ExtractResult::Skipped {
                reason: "律所规范/程序/身份材料(归档,不进 AI 上下文)".to_string(),
                metrics: Vec::new(),
            },
        }
    } else {
        // A. 其余全部完整抽(含证据 stage / 合同 / 催告函等)。证据现在要支撑对抗分析,
        //    走完整 extract_one(字段 + 文本)。PDF/扫描件会触发云端 OCR(作者主动选择:
        //    分析价值 > OCR 成本;扫描件直抽失败才 OCR 的既有链路保留,不浪费)。
        // 节流闸门 —— 仅云端 OCR + PDF/图片才需要(避开 MinerU 50 文件/分钟限流)
        if ocr_ctx.cloud_enabled && might_hit_mineru(&doc.filename) {
            throttle.acquire().await;
        }
        extract_one(
            llm_config,
            &doc_ocr_ctx,
            Path::new(&doc.source_path),
            &doc.filename,
            doc.category.as_deref(),
            execution_guard,
        )
        .await
    };

    // 2026-05-26 V0.1.12:抽取性能埋点 — 拿到 metrics 后批量 insert 进表,反馈通道带出来
    let collected_metrics: Vec<crate::db::metrics::MetricEntry> = match &result {
        ExtractResult::Extracted { metrics, .. } => metrics.clone(),
        ExtractResult::Skipped { metrics, .. } => metrics.clone(),
        ExtractResult::TextOnly { metrics, .. } => metrics.clone(),
        ExtractResult::Failed { metrics, .. } => metrics.clone(),
    };
    if !collected_metrics.is_empty() {
        if let Err(e) = crate::db::metrics::insert_many(pool, &collected_metrics).await {
            crate::dlog!("[pipeline] 写 extraction_metrics 失败(不阻塞抽取): {}", e);
        }
    }

    let outcome = match result {
        ExtractResult::Extracted {
            fields,
            text_md,
            mut partial_error,
            metrics: _,
        } => {
            if let Some(warning) = crate::ingest::reliability::quality_warning(&text_md) {
                partial_error = Some(match partial_error {
                    Some(detail) => format!("{}；{}", detail, warning),
                    None => warning.to_string(),
                });
            }
            let json = serde_json::to_string(&fields).unwrap_or_else(|_| "null".into());
            let extracted_text_path = match write_extracted_md(case_id, &doc.id, &text_md) {
                Ok(p) => Some(p),
                Err(e) => {
                    let detail = format!("保存抽取正文失败: {}", e);
                    crate::dlog!("[pipeline] {}", detail);
                    let _ = sqlx::query(
                        "UPDATE documents SET extraction_status = 'failed', last_error = ? WHERE id = ?",
                    )
                    .bind(&detail)
                    .bind(&doc.id)
                    .execute(pool)
                    .await;
                    return DocOutcome::Failed { error: detail };
                }
            };
            // A DB write failure is not a successful extraction: report it back to the UI.
            if let Err(e) = sqlx::query(
                "UPDATE documents SET extracted_fields = ?, extracted_text_path = ?, \
                 extraction_status = ?, last_error = ? WHERE id = ?",
            )
            .bind(&json)
            .bind(&extracted_text_path)
            .bind(if partial_error.is_some() {
                "partial"
            } else {
                "done"
            })
            .bind(&partial_error)
            .bind(&doc.id)
            .execute(pool)
            .await
            {
                return DocOutcome::Failed {
                    error: format!("保存抽取结果失败: {}", e),
                };
            }
            // 刑事材料只进入可审核候选；后台识别永不直接写 criminal_case_profiles。
            if matches!(
                crate::ingest::reliability::classify_domain(fields.case_type.as_deref(), &text_md),
                crate::ingest::reliability::Domain::Criminal
            ) {
                if let Err(e) =
                    crate::db::criminal_extraction_candidates::persist_extraction_candidate(
                        pool,
                        case_id,
                        &doc.id,
                        &doc.filename,
                        &llm_config.model,
                        &text_md,
                        &fields,
                        partial_error.as_deref(),
                    )
                    .await
                {
                    let _ = crate::db::criminal_extraction_candidates::persist_failed_candidate(
                        pool,
                        case_id,
                        &doc.id,
                        &doc.filename,
                        &llm_config.model,
                        &text_md,
                        &e,
                    )
                    .await;
                    let detail = format!("刑事识别候选保存失败: {}", e);
                    let _ = sqlx::query("UPDATE documents SET extraction_status = 'partial', last_error = ? WHERE id = ?")
                        .bind(&detail).bind(&doc.id).execute(pool).await;
                    crate::dlog!("[pipeline] {}", detail);
                }
            }
            // Only the explicit work-record whitelist reaches the pending ledger.  This uses the
            // cached extraction text and never schedules OCR/LLM again.
            if crate::ingest::reliability::is_work_record_filename(
                &doc.filename,
                doc.category.as_deref(),
            ) {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                if let Err(e) = crate::db::case_work_items::upsert_document_candidate(
                    pool,
                    case_id,
                    &doc.id,
                    &doc.filename,
                    &today,
                    doc.stage.clone(),
                    text_md.clone(),
                    None,
                )
                .await
                {
                    let detail = format!("工作候选保存失败: {}", e);
                    let _ = sqlx::query("UPDATE documents SET extraction_status = 'partial', last_error = ? WHERE id = ?").bind(&detail).bind(&doc.id).execute(pool).await;
                    return DocOutcome::Failed { error: detail };
                }
            }
            // The invoice contract is part of the same LLM response; a successful document pass
            // therefore creates/updates one reviewable income draft without another OCR/LLM call.
            if let Some(invoice) = fields.invoice.clone() {
                if let Some(invoice_no) = invoice.invoice_no.filter(|v| !v.trim().is_empty()) {
                    if let Err(e) = crate::db::income_records::sync_invoice_draft(
                        pool,
                        crate::db::income_records::InvoiceDraftInput {
                            case_id: Some(case_id.into()),
                            source_document_id: doc.id.clone(),
                            source_filename: doc.filename.clone(),
                            invoice_date: invoice.invoice_date,
                            invoice_no,
                            invoice_total: invoice.invoice_total,
                            invoice_buyer: invoice.invoice_buyer,
                            invoice_seller: invoice.invoice_seller,
                            invoice_type: invoice.invoice_type,
                        },
                    )
                    .await
                    {
                        let detail = format!("发票收入草稿保存失败: {}", e);
                        let _ = sqlx::query("UPDATE documents SET extraction_status = 'partial', last_error = ? WHERE id = ?").bind(&detail).bind(&doc.id).execute(pool).await;
                        return DocOutcome::Failed { error: detail };
                    }
                }
            }
            DocOutcome::Extracted
        }
        ExtractResult::Skipped { reason, metrics: _ } => {
            let _ = sqlx::query(
                "UPDATE documents SET extraction_status = 'skipped', last_error = NULL WHERE id = ?",
            )
            .bind(&doc.id)
            .execute(pool)
            .await;
            DocOutcome::Skipped { reason }
        }
        // 只抽了文本、没抽字段:状态保持 'skipped'(透明 — 没跑 LLM 字段),但写
        // extracted_text_path,使 read_case_doc / find_in_document / 全文搜索可读。
        ExtractResult::TextOnly {
            text_md,
            metrics: _,
        } => {
            let extracted_text_path = match write_extracted_md(case_id, &doc.id, &text_md) {
                Ok(p) => Some(p),
                Err(e) => {
                    let detail = format!("保存抽取正文失败: {}", e);
                    crate::dlog!("[pipeline] TextOnly {}", detail);
                    let _ = sqlx::query(
                        "UPDATE documents SET extraction_status = 'failed', last_error = ? WHERE id = ?",
                    )
                    .bind(&detail)
                    .bind(&doc.id)
                    .execute(pool)
                    .await;
                    return DocOutcome::Failed { error: detail };
                }
            };
            let _ = sqlx::query(
                "UPDATE documents SET extracted_text_path = ?, \
                 extraction_status = 'skipped', last_error = NULL WHERE id = ?",
            )
            .bind(&extracted_text_path)
            .bind(&doc.id)
            .execute(pool)
            .await;
            DocOutcome::Skipped {
                reason: "已抽文本未抽字段(证据/低价值材料,可被 AI 读取但不占字段)".to_string(),
            }
        }
        ExtractResult::Failed { error, metrics: _ } => {
            if is_final_round {
                // 三轮都失败 → 真的 failed,落 last_error 给用户/事后排查看
                let _ = sqlx::query(
                    "UPDATE documents SET extraction_status = 'failed', last_error = ? WHERE id = ?",
                )
                .bind(&error)
                .bind(&doc.id)
                .execute(pool)
                .await;
                crate::dlog!(
                    "[pipeline] case={} doc={} 三轮全失败: {}",
                    case_id,
                    doc.filename,
                    error
                );
            } else {
                // 中间轮失败 → 回退 pending 状态,等下一轮 caller 再喂进来
                // (不写 last_error,因为下一轮可能成功;只在最终失败才落 error)
                let _ =
                    sqlx::query("UPDATE documents SET extraction_status = 'pending' WHERE id = ?")
                        .bind(&doc.id)
                        .execute(pool)
                        .await;
                crate::dlog!(
                    "[pipeline] case={} doc={} 第 {} 轮失败,排队下一轮: {}",
                    case_id,
                    doc.filename,
                    round_num,
                    error
                );
            }
            DocOutcome::Failed { error }
        }
    };

    // DocFinished emit 现在挪到调用方(stream wrapper),那里有 completed_count 计数器
    // 且只在 doc**最终**完成时 emit(中间轮失败静默,避免前端进度回弹)
    outcome
}

/// 把抽出的纯文本写盘到 `~/Library/Application Support/CaseBoard/extracts/<case_id>/<doc_id>.md`。
///
/// 2026-05-23 晚十 Q1 作者拍板:落盘,方便全文搜索、用户预览、未来加编辑。
fn write_extracted_md(case_id: &str, doc_id: &str, text: &str) -> Result<String, String> {
    let dir = extracts_dir_for_case(case_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("建目录 {} 失败: {}", dir.display(), e))?;
    let path = dir.join(format!("{}.md", doc_id));
    std::fs::write(&path, text).map_err(|e| format!("写 {} 失败: {}", path.display(), e))?;
    Ok(path.to_string_lossy().into_owned())
}

/// 判断这个文档类别是不是"律所规范 / 程序 / 身份归档类" —— 抽文本归档,但**不进 LLM 上下文**。
///
/// 2026-05-31 改版(作者):现在所有材料都要抽(证据也要,做案件分析/对抗需要证据支撑)。
/// 但有几类材料对实体分析无价值、只占 token / 加噪音,应"抽了存着可查、但不喂给 LLM":
///   - **律所规范材料**:风险告知书、谈话笔录、反馈卡(作者点名的三类)
///   - **程序性材料**:送达地址确认书、送达回证、回执、介绍信、收案呈批表、收案登记表
///   - **律师内部**:办案笔记
///   - **身份隐私**:身份证、户口(隐私 + 无分析价值)
///
/// ⚠️ 关键区别(与旧 `is_low_value_category` 的根本不同):
/// **证据材料不再在此列** —— 借条/欠条/发票/收据/票据/银行流水/营业执照/证据清单/合同 等
/// 现在要走**完整抽取并进上下文**(作者:对抗分析需要证据支撑)。
/// 本函数同时被 `constitution.rs` 用来把这些类别**排除出 system prompt**(归档不喂 LLM)。
pub(crate) fn is_archival_category(cat: Option<&str>) -> bool {
    matches!(
        cat,
        // 律所规范材料(作者点名)
        Some("风险告知")
            | Some("风险告知书")
            | Some("反馈卡")
            | Some("律师工作反馈卡")
            | Some("笔录")
            | Some("谈话笔录")
            | Some("首次谈话笔录")
            // 程序性材料
            | Some("送达回证")
            | Some("送达地址确认书")
            | Some("回执")
            | Some("介绍信")
            | Some("收案呈批表")
            | Some("收案登记表")
            // 律师内部
            | Some("办案笔记")
            // 身份隐私
            | Some("身份证")
            | Some("户口")
    )
}

fn extracts_dir_for_case(case_id: &str) -> Result<PathBuf, String> {
    // 跟 caseboard.db / settings.json 同一个 app data dir(~/Library/Application Support/CaseBoard/)
    let base = crate::db::app_data_dir().map_err(|e| format!("无法定位 app data dir: {}", e))?;
    Ok(base.join("extracts").join(case_id))
}

#[cfg(test)]
mod criminal_reextract_guard_tests {
    use super::{
        material_execution_gate, require_criminal_material_domain,
        settle_without_network_if_document_terminal,
    };
    use crate::db::material_queue::{MaterialDecisionInput, MaterialQueueItemInput};

    #[test]
    fn manual_criminal_domain_passes_the_backend_gate() {
        assert!(require_criminal_material_domain("criminal").is_ok());
    }

    #[test]
    fn non_criminal_domains_return_stable_error_prefix() {
        for domain in ["civil", "other", "unknown"] {
            let error = require_criminal_material_domain(domain).unwrap_err();
            assert!(error.starts_with("DOMAIN_MISMATCH:"));
            assert!(error.contains(domain));
        }
    }

    #[tokio::test]
    async fn application_material_gate_allows_only_one_batch_executor() {
        let first = material_execution_gate().acquire().await.unwrap();
        assert!(
            material_execution_gate().try_acquire().is_err(),
            "第二个案件批次不能同时进入供应商执行区"
        );
        drop(first);
        assert!(material_execution_gate().try_acquire().is_ok());
    }

    #[tokio::test]
    async fn terminal_document_claim_settles_without_entering_extraction() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        sqlx::query(
            "INSERT INTO cases(id,name,case_type,source_folder) \
             VALUES ('case-terminal','测试','诉讼','C:/terminal')",
        )
        .execute(&pool)
        .await
        .unwrap();
        crate::db::material_queue::save_decisions(
            &pool,
            "case-terminal",
            &[MaterialDecisionInput {
                source_path: "done.pdf".into(),
                disposition: "recognize".into(),
                document_id: None,
            }],
        )
        .await
        .unwrap();
        let batch = crate::db::material_queue::create_batch(
            &pool,
            "case-terminal",
            &[MaterialQueueItemInput {
                source_path: "done.pdf".into(),
                document_id: None,
            }],
        )
        .await
        .unwrap();
        crate::db::material_queue::start_batch(&pool, &batch.batch.id)
            .await
            .unwrap();
        let item = crate::db::material_queue::claim_next(&pool, &batch.batch.id)
            .await
            .unwrap()
            .unwrap();
        assert!(settle_without_network_if_document_terminal(
            &pool,
            &item.id,
            item.claim_token.as_deref().unwrap(),
            "done",
        )
        .await
        .unwrap());
        let after = crate::db::material_queue::get_batch_detail(&pool, &batch.batch.id)
            .await
            .unwrap();
        assert_eq!(after.items[0].status, "completed");
        assert_eq!(after.batch.status, "completed");
    }
}
