//! 案件全局抽取的编排层(2026-05-24 h)。
//!
//! 输入:case_id
//! 流程:
//!   1. 拉所有 done 文档 + 各自 extracted_text_path
//!   2. 读 MD 文件内容
//!   3. 拼 corpus + 两次并发 LLM 调用(call A 表格 / call B 报告)
//!   4. 写 cases.agg_* 全套 + case_summary + case_report_path + case_report_generated_at
//!   5. 报告 MD 落盘到 ~/Library/.../reports/<case_id>.md
//!
//! 替代了 `db/aggregator.rs::aggregate_case_facts`,**不再做规则去污**,全部交给 LLM。

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::llm::global_extract::{
    build_corpus, extract_combined, report_path_for_case, DocInput, GlobalExtractTable,
};
use crate::llm::LlmConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalExtractReport {
    pub case_id: String,
    pub docs_included: usize,
    pub table_ok: bool,
    pub report_ok: bool,
    pub report_path: Option<String>,
    pub elapsed_ms: u128,
    pub error: Option<String>,
}

/// 批量重抽所有案件后的汇报(给前端 Toast 用)。
///
/// 2026-05-24 h:从 `db::aggregator::ReaggregateReport` 搬过来,接口保持兼容
/// (前端 `reaggregateAllCases` 仍能用),但底层从规则聚合换成 LLM 全局抽。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaggregateReport {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    /// (case_id, 错误消息) 列表
    pub failures: Vec<(String, String)>,
}

/// 跑一次案件全局抽。两次 LLM call **并发跑**(call A 表格 + call B 报告)。
pub async fn run_global_extract(
    pool: &SqlitePool,
    case_id: &str,
    llm_config: &LlmConfig,
) -> GlobalExtractReport {
    let start = std::time::Instant::now();

    // 1. 拿 done 文档清单 + extracted_text_path
    type DocRow = (String, Option<String>, Option<String>, Option<String>);
    let rows: Vec<DocRow> = match sqlx::query_as(
        "SELECT filename, category, stage, extracted_text_path \
         FROM documents \
         WHERE case_id = ? AND deleted_at IS NULL AND extraction_status = 'done' \
         ORDER BY filename",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return GlobalExtractReport {
                case_id: case_id.into(),
                docs_included: 0,
                table_ok: false,
                report_ok: false,
                report_path: None,
                elapsed_ms: start.elapsed().as_millis(),
                error: Some(format!("查文档列表失败:{}", e)),
            }
        }
    };

    if rows.is_empty() {
        return GlobalExtractReport {
            case_id: case_id.into(),
            docs_included: 0,
            table_ok: false,
            report_ok: false,
            report_path: None,
            elapsed_ms: start.elapsed().as_millis(),
            error: Some("无已 done 文档,无法全局抽取".into()),
        };
    }

    // D3-1:检测语料是否为完整集的子集 —— 有未 done 的文档说明本次基于**不完整语料**抽取。
    // 数组字段(当事人/日期/费用)可能比完整抽取更短;COALESCE 只防"整列被空值抹除",
    // **防不了"变短覆盖"**(P1 残留:完整性 gate 待定)。这里落 dlog 让 partial-shrink 可观测,不再静默。
    if let Ok(not_done) = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM documents \
         WHERE case_id = ? AND deleted_at IS NULL AND extraction_status != 'done'",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await
    {
        if not_done > 0 {
            crate::dlog!(
                "[global_extract] case={} 有 {} 份文档未 done → 基于不完整语料抽取,\
                 数组字段可能比完整抽取更短(D3-1 残留:仅防空覆盖,未防变短)",
                case_id,
                not_done
            );
        }
    }

    // 2. 读 MD 文件内容(本地 IO,blocking,但量小可接受)
    let mut docs: Vec<DocInput> = Vec::with_capacity(rows.len());
    for (filename, category, stage, text_path) in &rows {
        let Some(p) = text_path else {
            crate::dlog!("[global_extract] {} 无 extracted_text_path,跳过", filename);
            continue;
        };
        match std::fs::read_to_string(p) {
            Ok(content) => docs.push(DocInput {
                filename: filename.clone(),
                category: category.clone(),
                stage: stage.clone(),
                text_md: content,
            }),
            Err(e) => crate::dlog!("[global_extract] 读 {} 失败:{}", p, e),
        }
    }

    if docs.is_empty() {
        return GlobalExtractReport {
            case_id: case_id.into(),
            docs_included: 0,
            table_ok: false,
            report_ok: false,
            report_path: None,
            elapsed_ms: start.elapsed().as_millis(),
            error: Some("MD 文件都读不到,无法全局抽取".into()),
        };
    }

    let docs_count = docs.len();
    let corpus = build_corpus(&docs);
    crate::dlog!(
        "[global_extract] case={} 拼了 {} 份 MD,{} chars(~{} tokens)",
        case_id,
        docs_count,
        corpus.len(),
        corpus.len() / 4
    );

    // 3. 单次 LLM call 同时拿表格 + 报告(2026-05-24 i 合并)
    let combined = extract_combined(llm_config, &corpus).await;

    let (table_ok, report_ok, report_path_str, err) = match combined {
        Ok(r) => {
            // 报告 MD 落盘
            let report_path = match report_path_for_case(case_id) {
                Ok(p) => match std::fs::write(&p, &r.report_md) {
                    Ok(_) => Some(p.to_string_lossy().to_string()),
                    Err(e) => {
                        crate::dlog!("[global_extract] 写报告 MD 失败:{}", e);
                        None
                    }
                },
                Err(e) => {
                    crate::dlog!("[global_extract] 算报告路径失败:{}", e);
                    None
                }
            };
            // 写 cases 表
            if let Err(e) =
                write_table_to_cases(pool, case_id, &r.table, report_path.as_deref()).await
            {
                crate::dlog!("[global_extract] 写 cases 失败:{}", e);
            }
            (true, report_path.is_some(), report_path, None)
        }
        Err(e) => {
            crate::dlog!("[global_extract] LLM 调用失败:{}", e);
            (false, false, None, Some(e.to_string()))
        }
    };

    GlobalExtractReport {
        case_id: case_id.into(),
        docs_included: docs_count,
        table_ok,
        report_ok,
        report_path: report_path_str,
        elapsed_ms: start.elapsed().as_millis(),
        error: err,
    }
}

/// 对所有案件依次跑一遍全局抽。**串行**(每个案件单 LLM call 已经够慢),
/// 失败不阻断后续案件,失败列表通过 ReaggregateReport.failures 返回。
pub async fn rerun_all_cases(
    pool: &SqlitePool,
    llm_config: &LlmConfig,
) -> Result<ReaggregateReport, sqlx::Error> {
    let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM cases")
        .fetch_all(pool)
        .await?;
    let total = ids.len();
    let mut succeeded = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();
    for (id,) in ids {
        let r = run_global_extract(pool, &id, llm_config).await;
        if r.table_ok {
            succeeded += 1;
        } else {
            failures.push((id, r.error.unwrap_or_else(|| "table 抽取失败".into())));
        }
    }
    Ok(ReaggregateReport {
        total,
        succeeded,
        failed: failures.len(),
        failures,
    })
}

/// D3-1:空集合 → None(配合 SQL COALESCE 跳过覆盖),非空才序列化为 JSON。
fn non_empty_json<T: serde::Serialize>(v: &[T]) -> Option<String> {
    if v.is_empty() {
        None
    } else {
        Some(serde_json::to_string(v).unwrap_or_else(|_| "[]".into()))
    }
}

/// D9-1:`cases.workflow_status` 单一英文口径。LLM 输出的中文 9 档 → 前端 `StatusId`(英文)。
/// 不在表内 → None(写库时 COALESCE 保留 DB 现值)。**与前端 `inferStatus.ts::StatusId` 严格对齐**。
pub fn workflow_status_zh_to_en(zh: &str) -> Option<&'static str> {
    match zh.trim() {
        "接案" => Some("intake"),
        "立案中" => Some("filing"),
        "待开庭" => Some("awaiting_hearing"),
        "审理中" => Some("trial"),
        "已调解" => Some("mediated"),
        "上诉期" => Some("appeal_window"),
        "二审中" => Some("appeal"),
        "执行中" => Some("execution"),
        "已结案" => Some("closed"),
        _ => None,
    }
}

/// D9-1 反向:英文 `StatusId` → 中文 label。给 chat context 喂 LLM 时还原可读中文用。
/// 未知值原样返回(兼容历史脏数据)。
pub fn workflow_status_en_to_zh(en: &str) -> &str {
    match en.trim() {
        "intake" => "接案",
        "filing" => "立案中",
        "awaiting_hearing" => "待开庭",
        "trial" => "审理中",
        "mediated" => "已调解",
        "appeal_window" => "上诉期",
        "appeal" => "二审中",
        "execution" => "执行中",
        "closed" => "已结案",
        other => other,
    }
}

/// 把 LLM 抽出来的 GlobalExtractTable 写到 cases 表里。
async fn write_table_to_cases(
    pool: &SqlitePool,
    case_id: &str,
    t: &GlobalExtractTable,
    report_path: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().to_rfc3339();

    // D3-1:数组/文本 agg_* 字段空值时返回 None → 配合下方 SQL 的 COALESCE 跳过覆盖,
    // 防"重抽期间个别文档失败、语料变子集"用更小结果把已抽到的当事人/日期/费用静默抹掉。
    let plaintiffs_json = non_empty_json(&t.plaintiffs);
    let defendants_json = non_empty_json(&t.defendants);
    let third_json = non_empty_json(&t.third_parties);
    let judges_json = non_empty_json(&t.judges);
    let party_contacts_json = non_empty_json(&t.party_contacts);
    let court_contacts_json = non_empty_json(&t.court_contacts);
    let key_dates_json = non_empty_json(&t.key_dates);
    let fees_json = non_empty_json(&t.fees);
    let resolution_opt = t.resolution.as_deref().filter(|s| !s.trim().is_empty());
    let status_text_opt = t.status_text.as_deref().filter(|s| !s.trim().is_empty());
    let summary_opt = t.summary.as_deref().filter(|s| !s.trim().is_empty());

    // D9-1:LLM 输出中文状态 → 前端/DB 统一英文 StatusId(单一口径);不在表内则 None(保留 DB 现值,
    // 用户可能手工标过)。修复"LLM 写中文、前端只认英文 → 推断状态在看板/执行 tab 落不了地"。
    let workflow_status_to_set = t
        .workflow_status
        .as_deref()
        .and_then(workflow_status_zh_to_en);

    sqlx::query(
        "UPDATE cases SET \
            agg_case_no = COALESCE(?, agg_case_no), \
            agg_court = COALESCE(?, agg_court), \
            agg_cause = COALESCE(?, agg_cause), \
            agg_filed_at = COALESCE(?, agg_filed_at), \
            agg_claim_amount = COALESCE(?, agg_claim_amount), \
            agg_plaintiffs = COALESCE(?, agg_plaintiffs), \
            agg_defendants = COALESCE(?, agg_defendants), \
            agg_third_parties = COALESCE(?, agg_third_parties), \
            agg_judges = COALESCE(?, agg_judges), \
            agg_party_contacts = COALESCE(?, agg_party_contacts), \
            agg_court_contacts = COALESCE(?, agg_court_contacts), \
            agg_key_dates = COALESCE(?, agg_key_dates), \
            agg_fees = COALESCE(?, agg_fees), \
            agg_resolution = COALESCE(?, agg_resolution), \
            agg_status_text = COALESCE(?, agg_status_text), \
            case_summary = COALESCE(?, case_summary), \
            case_report_path = COALESCE(?, case_report_path), \
            case_report_generated_at = ?, \
            workflow_status = COALESCE(?, workflow_status), \
            agg_computed_at = ? \
         WHERE id = ?",
    )
    .bind(&t.case_no)
    .bind(&t.court)
    .bind(&t.cause)
    .bind(&t.filed_at)
    .bind(t.claim_amount)
    .bind(&plaintiffs_json)
    .bind(&defendants_json)
    .bind(&third_json)
    .bind(&judges_json)
    .bind(&party_contacts_json)
    .bind(&court_contacts_json)
    .bind(&key_dates_json)
    .bind(&fees_json)
    .bind(resolution_opt)
    .bind(status_text_opt)
    .bind(summary_opt)
    .bind(report_path)
    .bind(if report_path.is_some() {
        Some(now.clone())
    } else {
        None
    })
    .bind(workflow_status_to_set)
    .bind(&now)
    .bind(case_id)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_status_zh_en_roundtrip_all_9() {
        // D9-1:9 档中英映射必须双向一致(写库 zh→en,喂 LLM en→zh)。
        // 英文侧必须与前端 src/modules/litigation/lib/inferStatus.ts::StatusId 严格相同。
        let pairs = [
            ("接案", "intake"),
            ("立案中", "filing"),
            ("待开庭", "awaiting_hearing"),
            ("审理中", "trial"),
            ("已调解", "mediated"),
            ("上诉期", "appeal_window"),
            ("二审中", "appeal"),
            ("执行中", "execution"),
            ("已结案", "closed"),
        ];
        for (zh, en) in pairs {
            assert_eq!(workflow_status_zh_to_en(zh), Some(en), "zh→en: {}", zh);
            assert_eq!(workflow_status_en_to_zh(en), zh, "en→zh: {}", en);
            // 容忍首尾空白
            assert_eq!(workflow_status_zh_to_en(&format!("  {}  ", zh)), Some(en));
        }
        // 表外值 → None(保留 DB 现值)
        assert_eq!(workflow_status_zh_to_en("不存在的状态"), None);
        assert_eq!(workflow_status_zh_to_en(""), None);
        // 反向未知值原样返回(兼容历史脏数据)
        assert_eq!(workflow_status_en_to_zh("unknown"), "unknown");
    }

    #[test]
    fn non_empty_json_skips_empty() {
        // D3-1:空集合 → None(COALESCE 保留现值),非空 → Some(JSON)
        let empty: Vec<String> = vec![];
        assert_eq!(non_empty_json(&empty), None);
        assert_eq!(
            non_empty_json(&["张三".to_string(), "李四".to_string()]),
            Some(r#"["张三","李四"]"#.to_string())
        );
    }

    /// D3-1 集成测试:① 空数组不抹除已有值 ② 非空数组正常覆盖 ③ 顺带验证 write_table_to_cases
    /// 那条 21-bind COALESCE SQL 的占位/绑定数对齐(sqlx 运行时查询,五绿/编译期查不出,
    /// 且现有测试从不执行这条 query —— 这是唯一的运行时覆盖)。
    #[tokio::test]
    async fn write_table_empty_arrays_do_not_wipe_existing() {
        use crate::db::cases::{create_case, NewCase};
        use crate::db::init_pool;
        use crate::llm::global_extract::GlobalExtractTable;

        let pool = init_pool(":memory:").await.expect("init pool");
        let case = create_case(
            &pool,
            NewCase {
                name: "张三 诉 李四".into(),
                case_type: "诉讼".into(),
                source_folder: "/tmp/test-d31".into(),
            },
        )
        .await
        .expect("create case");

        // 预置一份"已抽全"的当事人 + 法院
        sqlx::query("UPDATE cases SET agg_plaintiffs = ?, agg_court = ? WHERE id = ?")
            .bind(r#"["张三","李四"]"#)
            .bind("旧法院")
            .bind(&case.id)
            .execute(&pool)
            .await
            .unwrap();

        // 模拟"不完整语料"回来的结果:plaintiffs 为空(应跳过保留),court 非空(应覆盖)
        let table = GlobalExtractTable {
            plaintiffs: vec![],
            court: Some("新法院".into()),
            ..Default::default()
        };
        write_table_to_cases(&pool, &case.id, &table, None)
            .await
            .expect("write_table_to_cases 应成功(若 panic 多半是 bind/占位数不齐)");

        let (plaintiffs, court): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT agg_plaintiffs, agg_court FROM cases WHERE id = ?")
                .bind(&case.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        // 空数组 → 保留原值(D3-1 防整列抹除)
        assert_eq!(plaintiffs.as_deref(), Some(r#"["张三","李四"]"#));
        // 非空标量 → 正常覆盖
        assert_eq!(court.as_deref(), Some("新法院"));

        // 再来一次:非空数组应正常覆盖(确认没把字段冻死)
        let table2 = GlobalExtractTable {
            plaintiffs: vec!["王五".into()],
            ..Default::default()
        };
        write_table_to_cases(&pool, &case.id, &table2, None)
            .await
            .unwrap();
        let plaintiffs2: Option<String> =
            sqlx::query_scalar("SELECT agg_plaintiffs FROM cases WHERE id = ?")
                .bind(&case.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(plaintiffs2.as_deref(), Some(r#"["王五"]"#));
    }
}
