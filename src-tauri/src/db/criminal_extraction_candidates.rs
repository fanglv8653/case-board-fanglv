use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::llm::{CriminalDocumentExtraction, CriminalExtractValue, ExtractedFields};

pub const CRIMINAL_SCHEMA_VERSION: &str = "criminal-document-v1";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalExtractionCandidateBatch {
    pub id: String,
    pub case_id: String,
    pub source_document_id: Option<String>,
    pub source_filename: String,
    pub document_type: Option<String>,
    pub model_name: String,
    pub schema_version: String,
    pub source_fingerprint: String,
    pub result_fingerprint: String,
    pub technical_status: String,
    pub review_status: String,
    pub warning_json: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalExtractionCandidateField {
    pub id: String,
    pub batch_id: String,
    pub field_key: String,
    pub value_json: String,
    pub source_document_id: Option<String>,
    pub source_filename: String,
    pub evidence_excerpt: Option<String>,
    pub confidence: Option<f64>,
    pub review_status: String,
    pub decision_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriminalExtractionCandidateDetail {
    pub batch: CriminalExtractionCandidateBatch,
    pub fields: Vec<CriminalExtractionCandidateField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateDecision {
    pub field_key: String,
    pub decision: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmCandidateBatchInput {
    pub batch_id: String,
    pub expected_profile_revision: i64,
    pub decisions: Vec<CandidateDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateReviewResult {
    pub batch: CriminalExtractionCandidateBatch,
    pub profile_revision: i64,
    pub applied_fields: Vec<String>,
    pub protected_fields: Vec<String>,
}

#[derive(Debug, Clone)]
struct ProposedField {
    key: String,
    value: Value,
    confidence: Option<f64>,
    evidence: Option<String>,
}

pub async fn list_case_candidates(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CriminalExtractionCandidateDetail>, String> {
    let batches = sqlx::query_as::<_, CriminalExtractionCandidateBatch>(
        "SELECT * FROM criminal_extraction_candidate_batches WHERE case_id = ? ORDER BY created_at DESC, id DESC",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut result = Vec::with_capacity(batches.len());
    for batch in batches {
        let fields = list_batch_fields(pool, &batch.id).await?;
        result.push(CriminalExtractionCandidateDetail { batch, fields });
    }
    Ok(result)
}

pub async fn list_batch_fields(
    pool: &SqlitePool,
    batch_id: &str,
) -> Result<Vec<CriminalExtractionCandidateField>, String> {
    sqlx::query_as::<_, CriminalExtractionCandidateField>(
        "SELECT * FROM criminal_extraction_candidate_fields WHERE batch_id = ? ORDER BY field_key",
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn persist_extraction_candidate(
    pool: &SqlitePool,
    case_id: &str,
    source_document_id: &str,
    source_filename: &str,
    model_name: &str,
    source_text: &str,
    fields: &ExtractedFields,
    partial_error: Option<&str>,
) -> Result<CriminalExtractionCandidateDetail, String> {
    let criminal = fields
        .criminal
        .as_ref()
        .ok_or_else(|| "刑事抽取结果缺少 criminal 对象".to_string())?;
    let proposed = proposed_fields(criminal)?;
    let source_fingerprint = fingerprint_bytes(source_text.as_bytes());
    let result_fingerprint = fingerprint_bytes(
        serde_json::to_vec(criminal)
            .map_err(|e| e.to_string())?
            .as_slice(),
    );
    let mut warnings = Vec::new();
    for item in &proposed {
        if item.evidence.as_deref().is_none_or(|v| v.trim().is_empty()) {
            warnings.push(format!("{} 缺少可核对证据摘录", item.key));
        }
    }
    if let Some(error) = partial_error {
        warnings.push(error.to_string());
    }
    let technical_status = if partial_error.is_some() || proposed.is_empty() {
        "partial"
    } else {
        "success"
    };
    let document_type = criminal.document_type.value.clone();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    if let Some(existing) = sqlx::query_as::<_, CriminalExtractionCandidateBatch>(
        "SELECT * FROM criminal_extraction_candidate_batches
         WHERE source_document_id = ? AND source_fingerprint = ? AND result_fingerprint = ? AND schema_version = ?",
    )
    .bind(source_document_id)
    .bind(&source_fingerprint)
    .bind(&result_fingerprint)
    .bind(CRIMINAL_SCHEMA_VERSION)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    {
        tx.commit().await.map_err(|e| e.to_string())?;
        let fields = list_batch_fields(pool, &existing.id).await?;
        return Ok(CriminalExtractionCandidateDetail { batch: existing, fields });
    }

    sqlx::query(
        "UPDATE criminal_extraction_candidate_batches
         SET review_status = 'superseded', updated_at = datetime('now')
         WHERE source_document_id = ? AND review_status = 'pending'",
    )
    .bind(source_document_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    let batch_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO criminal_extraction_candidate_batches
         (id, case_id, source_document_id, source_filename, document_type, model_name,
          schema_version, source_fingerprint, result_fingerprint, technical_status,
          review_status, warning_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
    )
    .bind(&batch_id)
    .bind(case_id)
    .bind(source_document_id)
    .bind(source_filename)
    .bind(document_type)
    .bind(model_name)
    .bind(CRIMINAL_SCHEMA_VERSION)
    .bind(source_fingerprint)
    .bind(result_fingerprint)
    .bind(technical_status)
    .bind((!warnings.is_empty()).then(|| serde_json::to_string(&warnings).unwrap()))
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    for item in proposed {
        sqlx::query(
            "INSERT INTO criminal_extraction_candidate_fields
             (id, batch_id, field_key, value_json, source_document_id, source_filename,
              evidence_excerpt, confidence) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&batch_id)
        .bind(&item.key)
        .bind(serde_json::to_string(&item.value).map_err(|e| e.to_string())?)
        .bind(source_document_id)
        .bind(source_filename)
        .bind(item.evidence.map(|v| truncate_chars(&v, 500)))
        .bind(item.confidence)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    get_candidate_detail(pool, &batch_id).await
}

pub async fn persist_failed_candidate(
    pool: &SqlitePool,
    case_id: &str,
    source_document_id: &str,
    source_filename: &str,
    model_name: &str,
    source_text: &str,
    error: &str,
) -> Result<CriminalExtractionCandidateDetail, String> {
    let source_fingerprint = fingerprint_bytes(source_text.as_bytes());
    let result_fingerprint = fingerprint_bytes(error.as_bytes());
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO criminal_extraction_candidate_batches
         (id, case_id, source_document_id, source_filename, model_name, schema_version,
          source_fingerprint, result_fingerprint, technical_status, review_status, error_message)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'failed', 'rejected', ?)",
    )
    .bind(&id)
    .bind(case_id)
    .bind(source_document_id)
    .bind(source_filename)
    .bind(model_name)
    .bind(CRIMINAL_SCHEMA_VERSION)
    .bind(&source_fingerprint)
    .bind(&result_fingerprint)
    .bind(error)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    let actual_id: String = sqlx::query_scalar(
        "SELECT id FROM criminal_extraction_candidate_batches
         WHERE source_document_id=? AND source_fingerprint=? AND result_fingerprint=? AND schema_version=?",
    )
    .bind(source_document_id)
    .bind(source_fingerprint)
    .bind(result_fingerprint)
    .bind(CRIMINAL_SCHEMA_VERSION)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    get_candidate_detail(pool, &actual_id).await
}

pub async fn confirm_candidate_batch(
    pool: &SqlitePool,
    input: ConfirmCandidateBatchInput,
) -> Result<CandidateReviewResult, String> {
    if input.decisions.is_empty() {
        return Err("至少选择一个候选字段".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let batch = load_batch_tx(&mut tx, &input.batch_id).await?;
    if matches!(batch.review_status.as_str(), "rejected" | "superseded") {
        return Err("该候选批次已拒绝或已被新结果替代".into());
    }
    let candidates = sqlx::query_as::<_, CriminalExtractionCandidateField>(
        "SELECT * FROM criminal_extraction_candidate_fields WHERE batch_id = ?",
    )
    .bind(&input.batch_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let by_key: HashMap<_, _> = candidates
        .iter()
        .map(|f| (f.field_key.as_str(), f))
        .collect();
    let mut seen = HashSet::new();
    for decision in &input.decisions {
        if !seen.insert(decision.field_key.as_str()) {
            return Err(format!("字段 {} 重复决定", decision.field_key));
        }
        if !matches!(decision.decision.as_str(), "accept" | "reject") {
            return Err(format!(
                "字段 {} 的决定必须为 accept 或 reject",
                decision.field_key
            ));
        }
        let candidate = by_key
            .get(decision.field_key.as_str())
            .ok_or_else(|| format!("批次中不存在字段 {}", decision.field_key))?;
        let same_direction_replay =
            (decision.decision == "accept"
                && matches!(candidate.review_status.as_str(), "accepted" | "protected"))
                || (decision.decision == "reject" && candidate.review_status == "rejected");
        if candidate.review_status != "pending" && !same_direction_replay {
            return Err(format!(
                "字段 {} 已决定为 {}，不可改为 {}",
                decision.field_key, candidate.review_status, decision.decision
            ));
        }
        validate_candidate_value(&candidate.field_key, &candidate.value_json)?;
    }

    let replay = input.decisions.iter().all(|d| {
        by_key.get(d.field_key.as_str()).is_some_and(|f| {
            (d.decision == "accept" && matches!(f.review_status.as_str(), "accepted" | "protected"))
                || (d.decision == "reject" && f.review_status == "rejected")
        })
    });
    let mut profile_revision: i64 = ensure_profile_and_revision(&mut tx, &batch.case_id).await?;
    if replay {
        tx.commit().await.map_err(|e| e.to_string())?;
        return Ok(CandidateReviewResult {
            batch,
            profile_revision,
            applied_fields: vec![],
            protected_fields: vec![],
        });
    }
    if profile_revision != input.expected_profile_revision {
        return Err(format!(
            "刑事画像已被其他操作更新（当前 revision={}，请求 revision={}）",
            profile_revision, input.expected_profile_revision
        ));
    }
    let protected = protected_fields_tx(&mut tx, &batch.case_id).await?;
    let mut extraction_meta = extraction_meta_tx(&mut tx, &batch.case_id).await?;
    let meta_fields = extraction_meta
        .as_object_mut()
        .expect("extraction meta normalized")
        .entry("fields")
        .or_insert_with(|| Value::Object(Map::new()));
    if !meta_fields.is_object() {
        *meta_fields = Value::Object(Map::new());
    }
    let mut applied_fields = Vec::new();
    let mut protected_fields = Vec::new();
    for decision in &input.decisions {
        let candidate = by_key[decision.field_key.as_str()];
        if decision.decision == "reject" {
            mark_field_decision(&mut tx, &candidate.id, "rejected", decision.note.as_deref())
                .await?;
            continue;
        }
        if protected.contains(&candidate.field_key) {
            mark_field_decision(
                &mut tx,
                &candidate.id,
                "protected",
                Some("人工字段受保护，未覆盖"),
            )
            .await?;
            protected_fields.push(candidate.field_key.clone());
            continue;
        }
        apply_profile_field(
            &mut tx,
            &batch.case_id,
            &candidate.field_key,
            &candidate.value_json,
        )
        .await?;
        mark_field_decision(&mut tx, &candidate.id, "accepted", decision.note.as_deref()).await?;
        meta_fields.as_object_mut().unwrap().insert(
            candidate.field_key.clone(),
            json!({
                "batch_id": batch.id,
                "source_document_id": candidate.source_document_id,
                "source_filename": candidate.source_filename,
                "confidence": candidate.confidence,
                "evidence": candidate.evidence_excerpt,
                "confirmed_at": Utc::now().to_rfc3339(),
            }),
        );
        applied_fields.push(candidate.field_key.clone());
    }
    if !applied_fields.is_empty() {
        profile_revision += 1;
        sqlx::query(
            "UPDATE criminal_case_profiles SET extraction_meta_json=?, profile_revision=?, updated_at=datetime('now') WHERE case_id=?",
        )
        .bind(serde_json::to_string(&extraction_meta).map_err(|e| e.to_string())?)
        .bind(profile_revision)
        .bind(&batch.case_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM criminal_extraction_candidate_fields WHERE batch_id=? AND review_status='pending'",
    )
    .bind(&batch.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let new_status = if pending == 0 {
        "confirmed"
    } else {
        "partially_confirmed"
    };
    sqlx::query(
        "UPDATE criminal_extraction_candidate_batches SET review_status=?, reviewed_at=datetime('now'), updated_at=datetime('now') WHERE id=?",
    )
    .bind(new_status)
    .bind(&batch.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(CandidateReviewResult {
        batch: get_candidate_detail(pool, &batch.id).await?.batch,
        profile_revision,
        applied_fields,
        protected_fields,
    })
}

pub async fn reject_candidate_batch(
    pool: &SqlitePool,
    batch_id: &str,
) -> Result<CriminalExtractionCandidateBatch, String> {
    sqlx::query(
        "UPDATE criminal_extraction_candidate_batches SET review_status='rejected', reviewed_at=datetime('now'), updated_at=datetime('now')
         WHERE id=? AND review_status NOT IN ('confirmed','superseded')",
    )
    .bind(batch_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query_as("SELECT * FROM criminal_extraction_candidate_batches WHERE id=?")
        .bind(batch_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "候选批次不存在".into())
}

async fn get_candidate_detail(
    pool: &SqlitePool,
    id: &str,
) -> Result<CriminalExtractionCandidateDetail, String> {
    let batch = sqlx::query_as("SELECT * FROM criminal_extraction_candidate_batches WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "候选批次不存在".to_string())?;
    let fields = list_batch_fields(pool, id).await?;
    Ok(CriminalExtractionCandidateDetail { batch, fields })
}

fn proposed_fields(extract: &CriminalDocumentExtraction) -> Result<Vec<ProposedField>, String> {
    let mut out = Vec::new();
    macro_rules! add {
        ($key:literal, $field:expr) => {
            push_value(&mut out, $key, &$field)?;
        };
    }
    add!("current_stage", extract.current_stage);
    add!("procedure_type", extract.procedure_type);
    add!("suspected_charge", extract.suspected_charge);
    add!(
        "suspect_or_defendant_name",
        extract.suspect_or_defendant_name
    );
    add!("victim_name", extract.victim_name);
    add!("detention_center", extract.detention_center);
    add!("coercive_measure_type", extract.coercive_measure_type);
    add!("guilty_plea_status", extract.guilty_plea_status);
    add!(
        "sentencing_recommendation",
        extract.sentencing_recommendation
    );
    add!("sentence_term", extract.sentence_term);
    add!("restitution_amount", extract.restitution_amount);
    add!("restitution_status", extract.restitution_status);
    add!("victim_forgiveness", extract.victim_forgiveness);
    add!("surrender_status", extract.surrender_status);
    add!(
        "meritorious_service_status",
        extract.meritorious_service_status
    );
    if !extract.charge_changes.is_empty() {
        let confidence = extract
            .charge_changes
            .iter()
            .filter_map(|v| v.confidence)
            .reduce(f64::min);
        let evidence = extract
            .charge_changes
            .iter()
            .filter_map(|v| v.evidence.clone())
            .collect::<Vec<_>>()
            .join("；");
        out.push(ProposedField {
            key: "charge_history_json".into(),
            value: serde_json::to_value(&extract.charge_changes).map_err(|e| e.to_string())?,
            confidence: normalize_confidence(confidence)?,
            evidence: (!evidence.is_empty()).then_some(evidence),
        });
    }
    let allowed_dates = allowed_date_fields();
    for date in &extract.key_dates {
        let Some(key) = date.event_type.as_deref() else {
            continue;
        };
        if !allowed_dates.contains(key) {
            continue;
        }
        let Some(value) = date.date.as_ref() else {
            continue;
        };
        validate_date(value)?;
        out.push(ProposedField {
            key: key.to_string(),
            value: Value::String(value.clone()),
            confidence: normalize_confidence(date.confidence)?,
            evidence: date.evidence.clone(),
        });
    }
    let mut unique = BTreeMap::new();
    for item in out {
        unique.insert(item.key.clone(), item);
    }
    Ok(unique.into_values().collect())
}

fn push_value<T: Serialize>(
    out: &mut Vec<ProposedField>,
    key: &'static str,
    field: &CriminalExtractValue<T>,
) -> Result<(), String> {
    let Some(value) = field.value.as_ref() else {
        return Ok(());
    };
    out.push(ProposedField {
        key: key.to_string(),
        value: serde_json::to_value(value).map_err(|e| e.to_string())?,
        confidence: normalize_confidence(field.confidence)?,
        evidence: field.evidence.clone(),
    });
    Ok(())
}

fn normalize_confidence(value: Option<f64>) -> Result<Option<f64>, String> {
    if value.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v)) {
        return Err("confidence 必须在 0..1".into());
    }
    Ok(value)
}

fn allowed_string_fields() -> HashSet<&'static str> {
    HashSet::from([
        "current_stage",
        "procedure_type",
        "suspected_charge",
        "suspect_or_defendant_name",
        "victim_name",
        "detention_center",
        "coercive_measure_type",
        "guilty_plea_status",
        "sentencing_recommendation",
        "sentence_term",
        "restitution_status",
        "victim_forgiveness",
        "surrender_status",
        "meritorious_service_status",
    ])
}
fn allowed_date_fields() -> HashSet<&'static str> {
    HashSet::from([
        "detention_date",
        "arrest_request_date",
        "arrest_review_received_date",
        "arrest_decision_date",
        "arrest_date",
        "bail_start_date",
        "residential_surveillance_start_date",
        "transfer_for_prosecution_date",
        "prosecution_received_date",
        "first_instance_accepted_date",
        "second_instance_accepted_date",
        "judgment_received_date",
        "ruling_received_date",
        "supplementary_investigation_1_date",
        "supplementary_investigation_2_date",
        "judgment_effective_date",
        "death_penalty_review_start_date",
    ])
}

fn validate_candidate_value(key: &str, raw: &str) -> Result<(), String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("{} 值不是有效 JSON: {}", key, e))?;
    if allowed_string_fields().contains(key) {
        if !value.is_string() {
            return Err(format!("{} 必须是字符串", key));
        }
    } else if allowed_date_fields().contains(key) {
        let value = value
            .as_str()
            .ok_or_else(|| format!("{} 必须是日期字符串", key))?;
        validate_date(value)?;
    } else if key == "restitution_amount" {
        if value.as_f64().is_none_or(|v| !v.is_finite() || v < 0.0) {
            return Err("restitution_amount 必须是非负数".into());
        }
    } else if key == "charge_history_json" {
        if !value.is_array() {
            return Err("charge_history_json 必须是数组".into());
        }
    } else {
        return Err(format!("非法刑事画像字段 {}", key));
    }
    Ok(())
}

fn validate_date(value: &str) -> Result<(), String> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| format!("日期必须为 YYYY-MM-DD: {}", value))
}

async fn apply_profile_field(
    tx: &mut Transaction<'_, Sqlite>,
    case_id: &str,
    key: &str,
    raw: &str,
) -> Result<(), String> {
    validate_candidate_value(key, raw)?;
    let value: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    if key == "restitution_amount" {
        return sqlx::query(
            "UPDATE criminal_case_profiles SET restitution_amount=? WHERE case_id=?",
        )
        .bind(value.as_f64().unwrap())
        .bind(case_id)
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string());
    }
    let text = if key == "charge_history_json" {
        serde_json::to_string(&value).unwrap()
    } else {
        value.as_str().unwrap().to_string()
    };
    macro_rules! update {
        ($column:literal) => {
            sqlx::query(concat!(
                "UPDATE criminal_case_profiles SET ",
                $column,
                "=? WHERE case_id=?"
            ))
            .bind(&text)
            .bind(case_id)
            .execute(&mut **tx)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
        };
    }
    match key {
        "current_stage" => update!("current_stage"),
        "procedure_type" => update!("procedure_type"),
        "suspected_charge" => update!("suspected_charge"),
        "suspect_or_defendant_name" => update!("suspect_or_defendant_name"),
        "victim_name" => update!("victim_name"),
        "detention_center" => update!("detention_center"),
        "coercive_measure_type" => update!("coercive_measure_type"),
        "guilty_plea_status" => update!("guilty_plea_status"),
        "sentencing_recommendation" => update!("sentencing_recommendation"),
        "sentence_term" => update!("sentence_term"),
        "restitution_status" => update!("restitution_status"),
        "victim_forgiveness" => update!("victim_forgiveness"),
        "surrender_status" => update!("surrender_status"),
        "meritorious_service_status" => update!("meritorious_service_status"),
        "charge_history_json" => update!("charge_history_json"),
        "detention_date" => update!("detention_date"),
        "arrest_request_date" => update!("arrest_request_date"),
        "arrest_review_received_date" => update!("arrest_review_received_date"),
        "arrest_decision_date" => update!("arrest_decision_date"),
        "arrest_date" => update!("arrest_date"),
        "bail_start_date" => update!("bail_start_date"),
        "residential_surveillance_start_date" => update!("residential_surveillance_start_date"),
        "transfer_for_prosecution_date" => update!("transfer_for_prosecution_date"),
        "prosecution_received_date" => update!("prosecution_received_date"),
        "first_instance_accepted_date" => update!("first_instance_accepted_date"),
        "second_instance_accepted_date" => update!("second_instance_accepted_date"),
        "judgment_received_date" => update!("judgment_received_date"),
        "ruling_received_date" => update!("ruling_received_date"),
        "supplementary_investigation_1_date" => update!("supplementary_investigation_1_date"),
        "supplementary_investigation_2_date" => update!("supplementary_investigation_2_date"),
        "judgment_effective_date" => update!("judgment_effective_date"),
        "death_penalty_review_start_date" => update!("death_penalty_review_start_date"),
        _ => Err(format!("非法刑事画像字段 {}", key)),
    }
}

async fn ensure_profile_and_revision(
    tx: &mut Transaction<'_, Sqlite>,
    case_id: &str,
) -> Result<i64, String> {
    sqlx::query(
        "INSERT INTO criminal_case_profiles(case_id) VALUES (?) ON CONFLICT(case_id) DO NOTHING",
    )
    .bind(case_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query_scalar("SELECT profile_revision FROM criminal_case_profiles WHERE case_id=?")
        .bind(case_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| e.to_string())
}

async fn protected_fields_tx(
    tx: &mut Transaction<'_, Sqlite>,
    case_id: &str,
) -> Result<HashSet<String>, String> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT user_overrides_json FROM criminal_case_profiles WHERE case_id=?",
    )
    .bind(case_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    let Some(raw) = raw else {
        return Ok(HashSet::new());
    };
    let value: Value =
        serde_json::from_str(&raw).map_err(|e| format!("人工覆盖记录损坏，已停止确认: {}", e))?;
    let fields = value
        .get("fields")
        .and_then(Value::as_object)
        .ok_or_else(|| "人工覆盖记录缺少 fields 对象，已停止确认".to_string())?;
    Ok(fields.keys().cloned().collect())
}

async fn extraction_meta_tx(
    tx: &mut Transaction<'_, Sqlite>,
    case_id: &str,
) -> Result<Value, String> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT extraction_meta_json FROM criminal_case_profiles WHERE case_id=?",
    )
    .bind(case_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    match raw {
        None => Ok(json!({"fields": {}})),
        Some(raw) => {
            let value: Value =
                serde_json::from_str(&raw).map_err(|e| format!("识别来源记录损坏: {}", e))?;
            if !value.is_object() {
                return Err("识别来源记录必须是对象".into());
            }
            Ok(value)
        }
    }
}

async fn mark_field_decision(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    status: &str,
    note: Option<&str>,
) -> Result<(), String> {
    sqlx::query("UPDATE criminal_extraction_candidate_fields SET review_status=?, decision_note=?, updated_at=datetime('now') WHERE id=?")
        .bind(status).bind(note).bind(id).execute(&mut **tx).await.map(|_| ()).map_err(|e| e.to_string())
}

async fn load_batch_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
) -> Result<CriminalExtractionCandidateBatch, String> {
    sqlx::query_as("SELECT * FROM criminal_extraction_candidate_batches WHERE id=?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "候选批次不存在".into())
}

fn fingerprint_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (SqlitePool, String, String) {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        let case = crate::db::cases::create_case(
            &pool,
            crate::db::cases::NewCase {
                name: "刑事候选测试".into(),
                case_type: "criminal".into(),
                source_folder: format!("D:/tmp/{}", Uuid::new_v4()),
            },
        )
        .await
        .unwrap();
        let doc_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO documents(id,case_id,source_path,filename,size_bytes,extraction_status) VALUES(?,?,?,?,1,'done')")
            .bind(&doc_id).bind(&case.id).bind(format!("D:/tmp/{doc_id}.pdf")).bind("起诉书.pdf")
            .execute(&pool).await.unwrap();
        (pool, case.id, doc_id)
    }

    fn extraction(charge: &str) -> ExtractedFields {
        ExtractedFields {
            case_type: Some("刑事".into()),
            criminal: Some(CriminalDocumentExtraction {
                document_type: CriminalExtractValue {
                    value: Some("起诉书".into()),
                    confidence: Some(0.9),
                    evidence: Some("人民检察院起诉书".into()),
                },
                suspected_charge: CriminalExtractValue {
                    value: Some(charge.into()),
                    confidence: Some(0.8),
                    evidence: Some("涉嫌诈骗罪".into()),
                },
                current_stage: CriminalExtractValue {
                    value: Some("审查起诉".into()), confidence: Some(0.7), evidence: Some("审查起诉阶段".into()),
                },
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn candidate_is_idempotent_and_does_not_write_profile() {
        let (pool, case_id, doc_id) = fixture().await;
        let first = persist_extraction_candidate(
            &pool,
            &case_id,
            &doc_id,
            "起诉书.pdf",
            "test",
            "same",
            &extraction("诈骗罪"),
            None,
        )
        .await
        .unwrap();
        let second = persist_extraction_candidate(
            &pool,
            &case_id,
            &doc_id,
            "起诉书.pdf",
            "test",
            "same",
            &extraction("诈骗罪"),
            None,
        )
        .await
        .unwrap();
        assert_eq!(first.batch.id, second.batch.id);
        assert!(
            crate::db::criminal_cases::get_criminal_case_profile(&pool, &case_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    async fn create_candidate(pool: &SqlitePool, case_id: &str, doc_id: &str) -> CriminalExtractionCandidateDetail {
        persist_extraction_candidate(pool, case_id, doc_id, "起诉书.pdf", "test", "source", &extraction("诈骗罪"), None).await.unwrap()
    }

    #[tokio::test]
    async fn partial_confirmation_applies_accepted_and_records_meta() {
        let (pool, case_id, doc_id) = fixture().await;
        let detail = create_candidate(&pool, &case_id, &doc_id).await;
        let result = confirm_candidate_batch(&pool, ConfirmCandidateBatchInput {
            batch_id: detail.batch.id, expected_profile_revision: 0,
            decisions: vec![
                CandidateDecision { field_key: "suspected_charge".into(), decision: "accept".into(), note: None },
                CandidateDecision { field_key: "current_stage".into(), decision: "reject".into(), note: None },
            ],
        }).await.unwrap();
        assert_eq!(result.profile_revision, 1);
        let profile = crate::db::criminal_cases::get_criminal_case_profile(&pool, &case_id).await.unwrap().unwrap();
        assert_eq!(profile.suspected_charge.as_deref(), Some("诈骗罪"));
        assert!(profile.current_stage.is_none());
        assert!(profile.extraction_meta_json.unwrap().contains(&doc_id));
    }

    #[tokio::test]
    async fn decided_fields_reject_opposite_replay_without_mutation() {
        let (pool, case_id, doc_id) = fixture().await;
        let detail = create_candidate(&pool, &case_id, &doc_id).await;
        confirm_candidate_batch(&pool, ConfirmCandidateBatchInput {
            batch_id: detail.batch.id.clone(), expected_profile_revision: 0,
            decisions: vec![
                CandidateDecision { field_key: "suspected_charge".into(), decision: "accept".into(), note: None },
                CandidateDecision { field_key: "current_stage".into(), decision: "reject".into(), note: None },
            ],
        }).await.unwrap();

        let accepted_to_reject = confirm_candidate_batch(&pool, ConfirmCandidateBatchInput {
            batch_id: detail.batch.id.clone(), expected_profile_revision: 1,
            decisions: vec![CandidateDecision { field_key: "suspected_charge".into(), decision: "reject".into(), note: None }],
        }).await.unwrap_err();
        assert!(accepted_to_reject.contains("不可改为 reject"));

        let rejected_to_accept = confirm_candidate_batch(&pool, ConfirmCandidateBatchInput {
            batch_id: detail.batch.id.clone(), expected_profile_revision: 1,
            decisions: vec![CandidateDecision { field_key: "current_stage".into(), decision: "accept".into(), note: None }],
        }).await.unwrap_err();
        assert!(rejected_to_accept.contains("不可改为 accept"));

        let fields = list_batch_fields(&pool, &detail.batch.id).await.unwrap();
        assert_eq!(fields.iter().find(|field| field.field_key == "suspected_charge").unwrap().review_status, "accepted");
        assert_eq!(fields.iter().find(|field| field.field_key == "current_stage").unwrap().review_status, "rejected");
        let profile = crate::db::criminal_cases::get_criminal_case_profile(&pool, &case_id).await.unwrap().unwrap();
        assert_eq!(profile.profile_revision, 1);
        assert_eq!(profile.suspected_charge.as_deref(), Some("诈骗罪"));
        assert!(profile.current_stage.is_none());
    }

    #[tokio::test]
    async fn manual_value_and_manual_null_keys_are_protected() {
        let (pool, case_id, doc_id) = fixture().await;
        let detail = create_candidate(&pool, &case_id, &doc_id).await;
        sqlx::query("INSERT INTO criminal_case_profiles(case_id,suspected_charge,user_overrides_json) VALUES(?,?,?)")
            .bind(&case_id).bind("人工罪名")
            .bind(r#"{"fields":{"suspected_charge":{"value":"人工罪名"},"current_stage":{"value":null}}}"#)
            .execute(&pool).await.unwrap();
        let result = confirm_candidate_batch(&pool, ConfirmCandidateBatchInput {
            batch_id: detail.batch.id, expected_profile_revision: 0,
            decisions: vec![
                CandidateDecision { field_key: "suspected_charge".into(), decision: "accept".into(), note: None },
                CandidateDecision { field_key: "current_stage".into(), decision: "accept".into(), note: None },
            ],
        }).await.unwrap();
        assert_eq!(result.protected_fields, vec!["suspected_charge", "current_stage"]);
        let profile = crate::db::criminal_cases::get_criminal_case_profile(&pool, &case_id).await.unwrap().unwrap();
        assert_eq!(profile.suspected_charge.as_deref(), Some("人工罪名"));
        assert_eq!(profile.profile_revision, 0);
    }

    #[tokio::test]
    async fn invalid_type_rolls_back_all_field_decisions() {
        let (pool, case_id, doc_id) = fixture().await;
        let detail = create_candidate(&pool, &case_id, &doc_id).await;
        sqlx::query("UPDATE criminal_extraction_candidate_fields SET value_json='123' WHERE batch_id=? AND field_key='current_stage'")
            .bind(&detail.batch.id).execute(&pool).await.unwrap();
        let error = confirm_candidate_batch(&pool, ConfirmCandidateBatchInput {
            batch_id: detail.batch.id.clone(), expected_profile_revision: 0,
            decisions: vec![
                CandidateDecision { field_key: "suspected_charge".into(), decision: "accept".into(), note: None },
                CandidateDecision { field_key: "current_stage".into(), decision: "accept".into(), note: None },
            ],
        }).await.unwrap_err();
        assert!(error.contains("必须是字符串"));
        assert!(crate::db::criminal_cases::get_criminal_case_profile(&pool, &case_id).await.unwrap().is_none());
        let fields = list_batch_fields(&pool, &detail.batch.id).await.unwrap();
        assert!(fields.iter().all(|field| field.review_status == "pending"));
    }

    #[tokio::test]
    async fn stale_revision_rolls_back_field_decisions() {
        let (pool, case_id, doc_id) = fixture().await;
        let detail = create_candidate(&pool, &case_id, &doc_id).await;
        sqlx::query("INSERT INTO criminal_case_profiles(case_id,profile_revision) VALUES(?,2)")
            .bind(&case_id).execute(&pool).await.unwrap();
        let error = confirm_candidate_batch(&pool, ConfirmCandidateBatchInput {
            batch_id: detail.batch.id.clone(), expected_profile_revision: 1,
            decisions: vec![CandidateDecision { field_key: "suspected_charge".into(), decision: "accept".into(), note: None }],
        }).await.unwrap_err();
        assert!(error.contains("revision=2"));
        let fields = list_batch_fields(&pool, &detail.batch.id).await.unwrap();
        assert!(fields.iter().all(|field| field.review_status == "pending"));
    }
}
