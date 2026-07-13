//! 收入台账(2026-06-29 · case_income_records 表)
//!
//! 私人财务记录,只提供独立 CRUD 与 summary,不进入工作台/团队/AI 主链。

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

const SOURCE_PERSONAL: &str = "personal";
const SOURCE_COLLABORATION: &str = "collaboration";
const HOLDBACK_HOLDING: &str = "holding";
const HOLDBACK_RETURNED: &str = "returned";
const HOLDBACK_NOT_RETURNED: &str = "not_returned";
const INVOICE_STATUS_ALL: &str = "all";
const INVOICE_STATUS_INVOICED: &str = "invoiced";
const INVOICE_STATUS_NOT_INVOICED: &str = "not_invoiced";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IncomeRecord {
    pub id: String,
    pub case_id: Option<String>,
    pub case_name: Option<String>,
    pub manual_case_name: Option<String>,
    pub lawyer_fee_total: f64,
    pub source_type: String,
    pub collaborator_name: Option<String>,
    pub share_ratio: f64,
    pub firm_deduction_rate: f64,
    pub archive_holdback_rate: f64,
    pub personal_share_amount: f64,
    pub firm_deduction_amount: f64,
    pub archive_holdback_amount: f64,
    pub archive_holdback_status: String,
    pub archive_returned_at: Option<String>,
    pub archive_returned_amount: f64,
    pub invoice_date: Option<String>,
    pub invoice_no: Option<String>,
    pub record_status: String,
    pub invoice_total: Option<f64>,
    pub invoice_buyer: Option<String>,
    pub invoice_seller: Option<String>,
    pub invoice_type: Option<String>,
    pub auto_source_document_id: Option<String>,
    pub auto_source_filename: Option<String>,
    pub auto_fields_json: String,
    pub manual_fields_json: String,
    pub recognized_month: String,
    pub actual_income_amount: f64,
    pub actual_income_overridden: i64,
    pub actual_income_override_note: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IncomeRecordFilter {
    pub month_from: Option<String>,
    pub month_to: Option<String>,
    pub source_type: Option<String>,
    pub archive_holdback_status: Option<String>,
    pub invoice_status: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertIncomeRecordInput {
    pub id: Option<String>,
    pub case_id: Option<String>,
    pub manual_case_name: Option<String>,
    pub lawyer_fee_total: f64,
    pub source_type: Option<String>,
    pub collaborator_name: Option<String>,
    pub share_ratio: Option<f64>,
    pub firm_deduction_rate: Option<f64>,
    pub archive_holdback_rate: Option<f64>,
    pub archive_holdback_status: Option<String>,
    pub archive_returned_at: Option<String>,
    pub archive_returned_amount: Option<f64>,
    pub invoice_date: Option<String>,
    pub invoice_no: Option<String>,
    pub record_status: Option<String>,
    pub recognized_month: Option<String>,
    pub actual_income_amount: Option<f64>,
    pub actual_income_overridden: Option<i64>,
    pub actual_income_override_note: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InvoiceDraftInput {
    pub case_id: Option<String>,
    pub source_document_id: String,
    pub source_filename: String,
    pub invoice_date: Option<String>,
    pub invoice_no: String,
    pub invoice_total: Option<f64>,
    pub invoice_buyer: Option<String>,
    pub invoice_seller: Option<String>,
    pub invoice_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IncomeSummary {
    pub record_count: i64,
    pub lawyer_fee_total_sum: f64,
    pub personal_share_sum: f64,
    pub firm_deduction_sum: f64,
    pub archive_holdback_sum: f64,
    pub actual_income_sum: f64,
    pub holding_amount_sum: f64,
    pub returned_holdback_sum: f64,
    pub invoiced_fee_sum: f64,
    pub overridden_count: i64,
}

struct ComputedInput {
    id: String,
    case_id: Option<String>,
    manual_case_name: Option<String>,
    lawyer_fee_total: f64,
    source_type: String,
    collaborator_name: Option<String>,
    share_ratio: f64,
    firm_deduction_rate: f64,
    archive_holdback_rate: f64,
    personal_share_amount: f64,
    firm_deduction_amount: f64,
    archive_holdback_amount: f64,
    archive_holdback_status: String,
    archive_returned_at: Option<String>,
    archive_returned_amount: f64,
    invoice_date: Option<String>,
    invoice_no: Option<String>,
    recognized_month: String,
    actual_income_amount: f64,
    actual_income_overridden: i64,
    actual_income_override_note: Option<String>,
    note: Option<String>,
    record_status: String,
}

const RECORD_SELECT: &str = r#"
SELECT
    ir.id,
    ir.case_id,
    COALESCE(NULLIF(trim(ir.manual_case_name), ''), c.name) AS case_name,
    ir.manual_case_name,
    ir.lawyer_fee_total,
    ir.source_type,
    ir.collaborator_name,
    ir.share_ratio,
    ir.firm_deduction_rate,
    ir.archive_holdback_rate,
    ir.personal_share_amount,
    ir.firm_deduction_amount,
    ir.archive_holdback_amount,
    ir.archive_holdback_status,
    ir.archive_returned_at,
    ir.archive_returned_amount,
    ir.invoice_date,
    ir.invoice_no,
    ir.record_status, ir.invoice_total, ir.invoice_buyer, ir.invoice_seller, ir.invoice_type,
    ir.auto_source_document_id, ir.auto_source_filename, ir.auto_fields_json, ir.manual_fields_json,
    ir.recognized_month,
    ir.actual_income_amount,
    ir.actual_income_overridden,
    ir.actual_income_override_note,
    ir.note,
    ir.created_at,
    ir.updated_at
FROM case_income_records ir
LEFT JOIN cases c ON ir.case_id = c.id
"#;

const FILTER_SQL: &str = r#"
WHERE
    (?1 IS NULL OR ir.recognized_month >= ?1)
    AND (?2 IS NULL OR ir.recognized_month <= ?2)
    AND (?3 IS NULL OR ir.source_type = ?3)
    AND (?4 IS NULL OR ir.archive_holdback_status = ?4)
    AND (
        ?5 IS NULL OR ?5 = 'all'
        OR (?5 = 'invoiced' AND ir.invoice_date IS NOT NULL AND trim(ir.invoice_date) <> '')
        OR (?5 = 'not_invoiced' AND (ir.invoice_date IS NULL OR trim(ir.invoice_date) = ''))
    )
    AND (
        ?6 IS NULL
        OR COALESCE(NULLIF(trim(ir.manual_case_name), ''), c.name, '') LIKE ?7
        OR COALESCE(ir.collaborator_name, '') LIKE ?7
        OR COALESCE(ir.invoice_no, '') LIKE ?7
    )
"#;

pub async fn list(
    pool: &SqlitePool,
    filter: IncomeRecordFilter,
) -> Result<Vec<IncomeRecord>, String> {
    let prepared = prepare_filter(filter)?;
    let sql = format!(
        "{RECORD_SELECT} {FILTER_SQL} ORDER BY ir.recognized_month DESC, ir.invoice_date DESC, ir.updated_at DESC"
    );
    sqlx::query_as::<_, IncomeRecord>(&sql)
        .bind(prepared.month_from)
        .bind(prepared.month_to)
        .bind(prepared.source_type)
        .bind(prepared.archive_holdback_status)
        .bind(prepared.invoice_status)
        .bind(prepared.query_term)
        .bind(prepared.query_like)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<IncomeRecord>, String> {
    let sql = format!("{RECORD_SELECT} WHERE ir.id = ?");
    sqlx::query_as::<_, IncomeRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert(
    pool: &SqlitePool,
    input: UpsertIncomeRecordInput,
) -> Result<IncomeRecord, String> {
    let is_manual_edit = input.id.is_some();
    let computed = compute_input(input)?;
    sqlx::query(
        "INSERT INTO case_income_records (
            id, case_id, manual_case_name, lawyer_fee_total, source_type, collaborator_name,
            share_ratio, firm_deduction_rate, archive_holdback_rate,
            personal_share_amount, firm_deduction_amount, archive_holdback_amount,
            archive_holdback_status, archive_returned_at, archive_returned_amount,
            invoice_date, invoice_no, recognized_month,
            actual_income_amount, actual_income_overridden, actual_income_override_note,
            note, record_status
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            case_id = excluded.case_id,
            manual_case_name = excluded.manual_case_name,
            lawyer_fee_total = excluded.lawyer_fee_total,
            source_type = excluded.source_type,
            collaborator_name = excluded.collaborator_name,
            share_ratio = excluded.share_ratio,
            firm_deduction_rate = excluded.firm_deduction_rate,
            archive_holdback_rate = excluded.archive_holdback_rate,
            personal_share_amount = excluded.personal_share_amount,
            firm_deduction_amount = excluded.firm_deduction_amount,
            archive_holdback_amount = excluded.archive_holdback_amount,
            archive_holdback_status = excluded.archive_holdback_status,
            archive_returned_at = excluded.archive_returned_at,
            archive_returned_amount = excluded.archive_returned_amount,
            invoice_date = excluded.invoice_date,
            invoice_no = excluded.invoice_no,
            recognized_month = excluded.recognized_month,
            actual_income_amount = excluded.actual_income_amount,
            actual_income_overridden = excluded.actual_income_overridden,
            actual_income_override_note = excluded.actual_income_override_note,
            note = excluded.note,
            record_status = excluded.record_status,
            updated_at = datetime('now')",
    )
    .bind(&computed.id)
    .bind(&computed.case_id)
    .bind(&computed.manual_case_name)
    .bind(computed.lawyer_fee_total)
    .bind(&computed.source_type)
    .bind(&computed.collaborator_name)
    .bind(computed.share_ratio)
    .bind(computed.firm_deduction_rate)
    .bind(computed.archive_holdback_rate)
    .bind(computed.personal_share_amount)
    .bind(computed.firm_deduction_amount)
    .bind(computed.archive_holdback_amount)
    .bind(&computed.archive_holdback_status)
    .bind(&computed.archive_returned_at)
    .bind(computed.archive_returned_amount)
    .bind(&computed.invoice_date)
    .bind(&computed.invoice_no)
    .bind(&computed.recognized_month)
    .bind(computed.actual_income_amount)
    .bind(computed.actual_income_overridden)
    .bind(&computed.actual_income_override_note)
    .bind(&computed.note)
    .bind(&computed.record_status)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    if is_manual_edit {
        // JSON array, not a substring convention: later automatic invoice rescans query it via json_each.
        let protected = serde_json::json!([
            "case_id",
            "manual_case_name",
            "lawyer_fee_total",
            "invoice_date",
            "invoice_no",
            "invoice_total",
            "invoice_buyer",
            "invoice_seller",
            "invoice_type"
        ]);
        sqlx::query("UPDATE case_income_records SET manual_fields_json = ?, record_status = 'confirmed', updated_at = datetime('now') WHERE id = ?")
            .bind(protected.to_string()).bind(&computed.id).execute(pool).await.map_err(|e| e.to_string())?;
    }

    get(pool, &computed.id)
        .await?
        .ok_or_else(|| "收入记录写入后读取失败".to_string())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, String> {
    let result = sqlx::query("DELETE FROM case_income_records WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected())
}

/// OCR invoice sync is idempotent by the normalized invoice number.  Existing rows retain
/// human-confirmed fields; an unmatched invoice becomes a draft and is therefore excluded from summaries.
pub async fn sync_invoice_draft(
    pool: &SqlitePool,
    input: InvoiceDraftInput,
) -> Result<IncomeRecord, String> {
    let invoice_no: String = input
        .invoice_no
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if invoice_no.is_empty() {
        return Err("电子发票缺少发票号码，不能自动建账".into());
    }
    let id =
        sqlx::query_scalar::<_, String>("SELECT id FROM case_income_records WHERE invoice_no = ?")
            .bind(&invoice_no)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
    let amount = input.invoice_total.unwrap_or(0.0).max(0.0);
    let month = input
        .invoice_date
        .as_deref()
        .filter(|v| v.len() >= 7)
        .map(|v| v[..7].to_string())
        .unwrap_or_else(|| Local::now().format("%Y-%m").to_string());
    sqlx::query("INSERT INTO case_income_records (id, case_id, manual_case_name, lawyer_fee_total, source_type, share_ratio, firm_deduction_rate, archive_holdback_rate, personal_share_amount, firm_deduction_amount, archive_holdback_amount, archive_holdback_status, archive_returned_amount, invoice_date, invoice_no, recognized_month, actual_income_amount, actual_income_overridden, record_status, invoice_total, invoice_buyer, invoice_seller, invoice_type, auto_source_document_id, auto_source_filename, auto_fields_json, manual_fields_json) VALUES (?, ?, NULL, ?, 'personal', 1, .15, .05, ?, ?, ?, 'holding', 0, ?, ?, ?, ?, 0, 'draft', ?, ?, ?, ?, ?, ?, '[\"case_id\",\"invoice_date\",\"invoice_no\",\"invoice_total\",\"invoice_buyer\",\"invoice_seller\",\"invoice_type\"]', '[]') ON CONFLICT(invoice_no) WHERE invoice_no IS NOT NULL AND trim(invoice_no) <> '' DO UPDATE SET case_id=CASE WHEN NOT EXISTS(SELECT 1 FROM json_each(case_income_records.manual_fields_json) WHERE value='case_id') THEN excluded.case_id ELSE case_income_records.case_id END, invoice_date=CASE WHEN NOT EXISTS(SELECT 1 FROM json_each(case_income_records.manual_fields_json) WHERE value='invoice_date') THEN excluded.invoice_date ELSE case_income_records.invoice_date END, invoice_total=CASE WHEN NOT EXISTS(SELECT 1 FROM json_each(case_income_records.manual_fields_json) WHERE value='invoice_total') THEN excluded.invoice_total ELSE case_income_records.invoice_total END, invoice_buyer=CASE WHEN NOT EXISTS(SELECT 1 FROM json_each(case_income_records.manual_fields_json) WHERE value='invoice_buyer') THEN excluded.invoice_buyer ELSE case_income_records.invoice_buyer END, invoice_seller=CASE WHEN NOT EXISTS(SELECT 1 FROM json_each(case_income_records.manual_fields_json) WHERE value='invoice_seller') THEN excluded.invoice_seller ELSE case_income_records.invoice_seller END, invoice_type=CASE WHEN NOT EXISTS(SELECT 1 FROM json_each(case_income_records.manual_fields_json) WHERE value='invoice_type') THEN excluded.invoice_type ELSE case_income_records.invoice_type END, auto_source_document_id=excluded.auto_source_document_id, auto_source_filename=excluded.auto_source_filename, auto_fields_json='[\"case_id\",\"invoice_date\",\"invoice_no\",\"invoice_total\",\"invoice_buyer\",\"invoice_seller\",\"invoice_type\"]', updated_at=datetime('now')")
        .bind(&id).bind(&input.case_id).bind(amount).bind(amount).bind(amount * 0.15).bind(amount * 0.05).bind(&input.invoice_date).bind(&invoice_no).bind(&month).bind(amount * 0.80).bind(input.invoice_total).bind(&input.invoice_buyer).bind(&input.invoice_seller).bind(&input.invoice_type).bind(&input.source_document_id).bind(&input.source_filename).execute(pool).await.map_err(|e| e.to_string())?;
    get(pool, &id)
        .await?
        .ok_or_else(|| "发票草稿写入后读取失败".into())
}

pub async fn summarize(
    pool: &SqlitePool,
    filter: IncomeRecordFilter,
) -> Result<IncomeSummary, String> {
    let prepared = prepare_filter(filter)?;
    let sql = format!(
        r#"
SELECT
    COUNT(*) AS record_count,
    COALESCE(SUM(ir.lawyer_fee_total), 0.0) AS lawyer_fee_total_sum,
    COALESCE(SUM(ir.personal_share_amount), 0.0) AS personal_share_sum,
    COALESCE(SUM(ir.firm_deduction_amount), 0.0) AS firm_deduction_sum,
    COALESCE(SUM(ir.archive_holdback_amount), 0.0) AS archive_holdback_sum,
    COALESCE(SUM(ir.actual_income_amount), 0.0) AS actual_income_sum,
    COALESCE(SUM(CASE
        WHEN ir.archive_holdback_status = 'holding'
        THEN MAX(ir.archive_holdback_amount - ir.archive_returned_amount, 0.0)
        ELSE 0.0
    END), 0.0) AS holding_amount_sum,
    COALESCE(SUM(ir.archive_returned_amount), 0.0) AS returned_holdback_sum,
    COALESCE(SUM(CASE
        WHEN ir.invoice_date IS NOT NULL AND trim(ir.invoice_date) <> ''
        THEN ir.lawyer_fee_total ELSE 0.0 END), 0.0) AS invoiced_fee_sum,
    COALESCE(SUM(CASE WHEN ir.actual_income_overridden = 1 THEN 1 ELSE 0 END), 0) AS overridden_count
FROM case_income_records ir
LEFT JOIN cases c ON ir.case_id = c.id
{FILTER_SQL}
AND ir.record_status = 'confirmed'
"#
    );
    sqlx::query_as::<_, IncomeSummary>(&sql)
        .bind(prepared.month_from)
        .bind(prepared.month_to)
        .bind(prepared.source_type)
        .bind(prepared.archive_holdback_status)
        .bind(prepared.invoice_status)
        .bind(prepared.query_term)
        .bind(prepared.query_like)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())
}

struct PreparedFilter {
    month_from: Option<String>,
    month_to: Option<String>,
    source_type: Option<String>,
    archive_holdback_status: Option<String>,
    invoice_status: Option<String>,
    query_term: Option<String>,
    query_like: Option<String>,
}

fn prepare_filter(filter: IncomeRecordFilter) -> Result<PreparedFilter, String> {
    let month_from = normalize_opt(filter.month_from);
    let month_to = normalize_opt(filter.month_to);
    if let Some(ref month) = month_from {
        validate_month(month)?;
    }
    if let Some(ref month) = month_to {
        validate_month(month)?;
    }
    let source_type = normalize_opt(filter.source_type);
    if let Some(ref source) = source_type {
        validate_source_type(source)?;
    }
    let archive_holdback_status = normalize_opt(filter.archive_holdback_status);
    if let Some(ref status) = archive_holdback_status {
        validate_holdback_status(status)?;
    }
    let invoice_status = normalize_opt(filter.invoice_status);
    if let Some(ref status) = invoice_status {
        validate_invoice_status(status)?;
    }
    let query_term = normalize_opt(filter.query);
    let query_like = query_term.as_ref().map(|s| format!("%{s}%"));
    Ok(PreparedFilter {
        month_from,
        month_to,
        source_type,
        archive_holdback_status,
        invoice_status,
        query_term,
        query_like,
    })
}

fn compute_input(input: UpsertIncomeRecordInput) -> Result<ComputedInput, String> {
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let case_id = normalize_opt(input.case_id);
    let manual_case_name = normalize_opt(input.manual_case_name);
    if case_id.is_none() && manual_case_name.is_none() {
        return Err("未关联案件时,必须填写手工案件名称".to_string());
    }

    let lawyer_fee_total = round_money(input.lawyer_fee_total);
    if lawyer_fee_total < 0.0 {
        return Err("律师费总额不能为负数".to_string());
    }

    let source_type =
        normalize_opt(input.source_type).unwrap_or_else(|| SOURCE_PERSONAL.to_string());
    validate_source_type(&source_type)?;
    let record_status =
        normalize_opt(input.record_status).unwrap_or_else(|| "confirmed".to_string());
    if !matches!(record_status.as_str(), "draft" | "confirmed") {
        return Err("收入状态必须是 draft 或 confirmed".to_string());
    }

    let share_ratio = input.share_ratio.unwrap_or(1.0);
    validate_ratio("分成比例", share_ratio)?;

    let firm_deduction_rate = input.firm_deduction_rate.unwrap_or(0.15);
    validate_ratio("律所固定扣除比例", firm_deduction_rate)?;

    let archive_holdback_rate = input.archive_holdback_rate.unwrap_or(0.05);
    validate_ratio("归档暂押费比例", archive_holdback_rate)?;

    let archive_holdback_status = normalize_opt(input.archive_holdback_status)
        .unwrap_or_else(|| HOLDBACK_HOLDING.to_string());
    validate_holdback_status(&archive_holdback_status)?;

    let invoice_date = normalize_opt(input.invoice_date);
    if let Some(ref value) = invoice_date {
        validate_date(value, "开票日期")?;
    }

    let recognized_month = match normalize_opt(input.recognized_month) {
        Some(month) => {
            validate_month(&month)?;
            month
        }
        None => invoice_date
            .as_deref()
            .map(|value| value[..7].to_string())
            .unwrap_or_else(|| Local::now().format("%Y-%m").to_string()),
    };

    let archive_returned_at = normalize_opt(input.archive_returned_at);
    if let Some(ref value) = archive_returned_at {
        validate_date(value, "归档返还日期")?;
    }

    let personal_share_amount = round_money(lawyer_fee_total * share_ratio);
    let firm_deduction_amount = round_money(lawyer_fee_total * firm_deduction_rate);
    let archive_holdback_amount = round_money(lawyer_fee_total * archive_holdback_rate);

    let archive_returned_amount_input = input.archive_returned_amount.unwrap_or_else(|| {
        if archive_holdback_status == HOLDBACK_RETURNED {
            archive_holdback_amount
        } else {
            0.0
        }
    });
    let archive_returned_amount = round_money(archive_returned_amount_input);
    if archive_returned_amount < 0.0 {
        return Err("归档返还金额不能为负数".to_string());
    }
    if archive_returned_amount > archive_holdback_amount {
        return Err("归档返还金额不能大于归档暂押费".to_string());
    }

    let default_actual_income =
        round_money(personal_share_amount - firm_deduction_amount - archive_holdback_amount);
    let overridden = if input.actual_income_overridden.unwrap_or(0) == 1
        || input.actual_income_amount.is_some()
    {
        1
    } else {
        0
    };
    let actual_income_override_note = normalize_opt(input.actual_income_override_note);
    let actual_income_amount = if overridden == 1 {
        let value = input
            .actual_income_amount
            .ok_or_else(|| "手工覆盖实际收入时,必须传 actual_income_amount".to_string())?;
        round_money(value)
    } else {
        default_actual_income
    };

    Ok(ComputedInput {
        id,
        case_id,
        manual_case_name,
        lawyer_fee_total,
        source_type,
        collaborator_name: normalize_opt(input.collaborator_name),
        share_ratio,
        firm_deduction_rate,
        archive_holdback_rate,
        personal_share_amount,
        firm_deduction_amount,
        archive_holdback_amount,
        archive_holdback_status,
        archive_returned_at,
        archive_returned_amount,
        invoice_date,
        invoice_no: normalize_opt(input.invoice_no),
        recognized_month,
        actual_income_amount,
        actual_income_overridden: overridden,
        actual_income_override_note,
        note: normalize_opt(input.note),
        record_status,
    })
}

fn normalize_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn validate_source_type(value: &str) -> Result<(), String> {
    if matches!(value, SOURCE_PERSONAL | SOURCE_COLLABORATION) {
        Ok(())
    } else {
        Err(format!("不支持的案源类型: {value}"))
    }
}

fn validate_holdback_status(value: &str) -> Result<(), String> {
    if matches!(
        value,
        HOLDBACK_HOLDING | HOLDBACK_RETURNED | HOLDBACK_NOT_RETURNED
    ) {
        Ok(())
    } else {
        Err(format!("不支持的归档暂押状态: {value}"))
    }
}

fn validate_invoice_status(value: &str) -> Result<(), String> {
    if matches!(
        value,
        INVOICE_STATUS_ALL | INVOICE_STATUS_INVOICED | INVOICE_STATUS_NOT_INVOICED
    ) {
        Ok(())
    } else {
        Err(format!("不支持的开票状态筛选: {value}"))
    }
}

fn validate_ratio(field: &str, value: f64) -> Result<(), String> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{field} 必须在 0 到 1 之间"))
    }
}

fn validate_date(value: &str, field: &str) -> Result<(), String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| format!("{field} 必须是 YYYY-MM-DD"))
}

fn validate_month(value: &str) -> Result<(), String> {
    if value.len() != 7 || !value.is_char_boundary(4) || &value[4..5] != "-" {
        return Err("收入确认月份必须是 YYYY-MM".to_string());
    }
    let normalized = format!("{value}-01");
    NaiveDate::parse_from_str(&normalized, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| "收入确认月份必须是 YYYY-MM".to_string())
}

fn round_money(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoice(buyer: &str, total: f64) -> InvoiceDraftInput {
        InvoiceDraftInput {
            case_id: None,
            source_document_id: "doc-invoice-1".into(),
            source_filename: "电子发票.pdf".into(),
            invoice_date: Some("2026-07-13".into()),
            invoice_no: "12 34-56".into(),
            invoice_total: Some(total),
            invoice_buyer: Some(buyer.into()),
            invoice_seller: Some("测试律所".into()),
            invoice_type: Some("电子发票".into()),
        }
    }

    #[tokio::test]
    async fn invoice_sync_is_idempotent_and_respects_manual_fields() {
        let pool = crate::db::init_pool(":memory:")
            .await
            .expect("migrate database");
        let first = sync_invoice_draft(&pool, invoice("甲公司", 10_000.0))
            .await
            .expect("first sync");
        let second = sync_invoice_draft(&pool, invoice("乙公司", 12_000.0))
            .await
            .expect("repeat sync");

        assert_eq!(first.id, second.id);
        assert_eq!(second.invoice_buyer.as_deref(), Some("乙公司"));
        assert_eq!(second.invoice_total, Some(12_000.0));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM case_income_records")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);

        sqlx::query("UPDATE case_income_records SET invoice_buyer = '人工购买方', manual_fields_json = '[\"invoice_buyer\"]' WHERE id = ?")
            .bind(&second.id).execute(&pool).await.unwrap();
        let protected = sync_invoice_draft(&pool, invoice("丙公司", 15_000.0))
            .await
            .expect("protected rescan");
        assert_eq!(protected.invoice_buyer.as_deref(), Some("人工购买方"));
        assert_eq!(protected.invoice_total, Some(15_000.0));
    }

    #[tokio::test]
    async fn draft_is_excluded_until_confirmed() {
        let pool = crate::db::init_pool(":memory:")
            .await
            .expect("migrate database");
        let draft = sync_invoice_draft(&pool, invoice("甲公司", 10_000.0))
            .await
            .expect("draft sync");
        assert_eq!(
            summarize(&pool, IncomeRecordFilter::default())
                .await
                .unwrap()
                .record_count,
            0
        );

        upsert(
            &pool,
            UpsertIncomeRecordInput {
                id: Some(draft.id),
                manual_case_name: Some("发票待关联案件".into()),
                lawyer_fee_total: 10_000.0,
                invoice_date: draft.invoice_date,
                invoice_no: draft.invoice_no,
                record_status: Some("confirmed".into()),
                recognized_month: Some("2026-07".into()),
                ..Default::default()
            },
        )
        .await
        .expect("confirm record");

        let summary = summarize(&pool, IncomeRecordFilter::default())
            .await
            .unwrap();
        assert_eq!(summary.record_count, 1);
        assert_eq!(summary.lawyer_fee_total_sum, 10_000.0);
    }
}
