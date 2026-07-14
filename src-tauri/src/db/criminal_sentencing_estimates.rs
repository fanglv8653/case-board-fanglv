use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

const INVALID_PAYLOAD: &str = "SENTENCING_ESTIMATE_INVALID_PAYLOAD";
const CASE_NOT_FOUND: &str = "SENTENCING_ESTIMATE_CASE_NOT_FOUND";
const PROFILE_NOT_FOUND: &str = "SENTENCING_ESTIMATE_PROFILE_NOT_FOUND";
const REVISION_CONFLICT: &str = "SENTENCING_ESTIMATE_REVISION_CONFLICT";
const RECORD_NOT_FOUND: &str = "SENTENCING_ESTIMATE_NOT_FOUND";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveCriminalSentencingEstimateInput {
    pub case_id: String,
    pub expected_profile_revision: i64,
    pub input_snapshot: Value,
    pub output_min_months: f64,
    pub output_max_months: Option<f64>,
    pub output_snapshot: Value,
    pub process_snapshot: Value,
    pub basis_snapshot: Value,
    pub created_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CriminalSentencingEstimate {
    pub id: String,
    pub case_id: String,
    pub profile_case_id: String,
    pub profile_revision: i64,
    pub input_snapshot: Value,
    pub output_min_months: f64,
    pub output_max_months: Option<f64>,
    pub output_snapshot: Value,
    pub process_snapshot: Value,
    pub basis_snapshot: Value,
    pub created_source: String,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
struct EstimateRow {
    id: String,
    case_id: String,
    profile_case_id: String,
    profile_revision: i64,
    input_snapshot_json: String,
    output_min_months: f64,
    output_max_months: Option<f64>,
    output_snapshot_json: String,
    process_snapshot_json: String,
    basis_snapshot_json: String,
    created_source: String,
    created_at: String,
}

pub async fn save_criminal_sentencing_estimate(
    pool: &SqlitePool,
    input: SaveCriminalSentencingEstimateInput,
) -> Result<CriminalSentencingEstimate, String> {
    validate_input(&input)?;
    let input_json = encode_snapshot(&input.input_snapshot)?;
    let output_json = encode_snapshot(&input.output_snapshot)?;
    let process_json = encode_snapshot(&input.process_snapshot)?;
    let basis_json = encode_snapshot(&input.basis_snapshot)?;
    let id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await.map_err(db_error)?;

    // The revision predicate and append happen in one SQL statement. Once the
    // INSERT starts, SQLite's write lock prevents a profile update racing the
    // validation before this transaction commits.
    let inserted = sqlx::query(
        "INSERT INTO criminal_sentencing_estimates (
             id, case_id, profile_case_id, profile_revision,
             input_snapshot_json, output_min_months, output_max_months,
             output_snapshot_json, process_snapshot_json, basis_snapshot_json,
             created_source
         )
         SELECT ?, p.case_id, p.case_id, p.profile_revision, ?, ?, ?, ?, ?, ?, ?
         FROM criminal_case_profiles p
         WHERE p.case_id = ? AND p.profile_revision = ?",
    )
    .bind(&id)
    .bind(input_json)
    .bind(input.output_min_months)
    .bind(input.output_max_months)
    .bind(output_json)
    .bind(process_json)
    .bind(basis_json)
    .bind(input.created_source.trim())
    .bind(input.case_id.trim())
    .bind(input.expected_profile_revision)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;

    if inserted.rows_affected() == 0 {
        let case_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cases WHERE id = ?)")
                .bind(input.case_id.trim())
                .fetch_one(&mut *tx)
                .await
                .map_err(db_error)?;
        if !case_exists {
            return Err(code(CASE_NOT_FOUND, "case does not exist"));
        }
        let revision: Option<i64> = sqlx::query_scalar(
            "SELECT profile_revision FROM criminal_case_profiles WHERE case_id = ?",
        )
        .bind(input.case_id.trim())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        return match revision {
            None => Err(code(PROFILE_NOT_FOUND, "criminal profile does not exist")),
            Some(actual) => Err(code(
                REVISION_CONFLICT,
                &format!(
                    "expected profile revision {}, actual {}",
                    input.expected_profile_revision, actual
                ),
            )),
        };
    }

    let row = load_row_tx(&mut tx, input.case_id.trim(), &id)
        .await?
        .ok_or_else(|| code(RECORD_NOT_FOUND, "saved estimate is not readable"))?;
    tx.commit().await.map_err(db_error)?;
    decode_row(row)
}

pub async fn list_criminal_sentencing_estimates(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CriminalSentencingEstimate>, String> {
    validate_case_id(case_id)?;
    ensure_case_exists(pool, case_id).await?;
    let rows = sqlx::query_as::<_, EstimateRow>(
        "SELECT * FROM criminal_sentencing_estimates
         WHERE case_id = ? ORDER BY created_at DESC, id DESC",
    )
    .bind(case_id.trim())
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    rows.into_iter().map(decode_row).collect()
}

pub async fn get_criminal_sentencing_estimate(
    pool: &SqlitePool,
    case_id: &str,
    estimate_id: &str,
) -> Result<CriminalSentencingEstimate, String> {
    validate_case_id(case_id)?;
    if estimate_id.trim().is_empty() {
        return Err(code(INVALID_PAYLOAD, "estimate_id is required"));
    }
    ensure_case_exists(pool, case_id).await?;
    let row = sqlx::query_as::<_, EstimateRow>(
        "SELECT * FROM criminal_sentencing_estimates WHERE case_id = ? AND id = ?",
    )
    .bind(case_id.trim())
    .bind(estimate_id.trim())
    .fetch_optional(pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| code(RECORD_NOT_FOUND, "estimate does not exist for this case"))?;
    decode_row(row)
}

fn validate_input(input: &SaveCriminalSentencingEstimateInput) -> Result<(), String> {
    validate_case_id(&input.case_id)?;
    if input.expected_profile_revision < 0 {
        return Err(code(
            INVALID_PAYLOAD,
            "expected_profile_revision must be non-negative",
        ));
    }
    if !input.output_min_months.is_finite() || input.output_min_months < 0.0 {
        return Err(code(
            INVALID_PAYLOAD,
            "output_min_months must be finite and non-negative",
        ));
    }
    if input
        .output_max_months
        .is_some_and(|maximum| !maximum.is_finite() || maximum < input.output_min_months)
    {
        return Err(code(
            INVALID_PAYLOAD,
            "output_max_months must be null or not less than the minimum",
        ));
    }
    if input.created_source.trim().is_empty() || input.created_source.trim().len() > 64 {
        return Err(code(
            INVALID_PAYLOAD,
            "created_source must contain 1 to 64 bytes",
        ));
    }
    for (name, snapshot) in [
        ("input_snapshot", &input.input_snapshot),
        ("output_snapshot", &input.output_snapshot),
        ("process_snapshot", &input.process_snapshot),
        ("basis_snapshot", &input.basis_snapshot),
    ] {
        if snapshot.is_null() {
            return Err(code(INVALID_PAYLOAD, &format!("{name} must not be null")));
        }
    }
    Ok(())
}

fn validate_case_id(case_id: &str) -> Result<(), String> {
    if case_id.trim().is_empty() {
        Err(code(INVALID_PAYLOAD, "case_id is required"))
    } else {
        Ok(())
    }
}

async fn ensure_case_exists(pool: &SqlitePool, case_id: &str) -> Result<(), String> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cases WHERE id = ?)")
        .bind(case_id.trim())
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    exists
        .then_some(())
        .ok_or_else(|| code(CASE_NOT_FOUND, "case does not exist"))
}

async fn load_row_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    case_id: &str,
    id: &str,
) -> Result<Option<EstimateRow>, String> {
    sqlx::query_as("SELECT * FROM criminal_sentencing_estimates WHERE case_id = ? AND id = ?")
        .bind(case_id)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error)
}

fn encode_snapshot(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| code(INVALID_PAYLOAD, &error.to_string()))
}

fn decode_row(row: EstimateRow) -> Result<CriminalSentencingEstimate, String> {
    let parse = |value: &str| serde_json::from_str(value).map_err(db_error);
    Ok(CriminalSentencingEstimate {
        id: row.id,
        case_id: row.case_id,
        profile_case_id: row.profile_case_id,
        profile_revision: row.profile_revision,
        input_snapshot: parse(&row.input_snapshot_json)?,
        output_min_months: row.output_min_months,
        output_max_months: row.output_max_months,
        output_snapshot: parse(&row.output_snapshot_json)?,
        process_snapshot: parse(&row.process_snapshot_json)?,
        basis_snapshot: parse(&row.basis_snapshot_json)?,
        created_source: row.created_source,
        created_at: row.created_at,
    })
}

fn code(prefix: &str, message: &str) -> String {
    format!("{prefix}: {message}")
}

fn db_error(error: impl std::fmt::Display) -> String {
    format!("SENTENCING_ESTIMATE_DATABASE_ERROR: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn fixture() -> (SqlitePool, String, String) {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        let first = crate::db::cases::create_case(
            &pool,
            crate::db::cases::NewCase {
                name: "sentencing estimate case A".into(),
                case_type: "criminal".into(),
                source_folder: format!("D:/tmp/{}", Uuid::new_v4()),
            },
        )
        .await
        .unwrap();
        let second = crate::db::cases::create_case(
            &pool,
            crate::db::cases::NewCase {
                name: "sentencing estimate case B".into(),
                case_type: "criminal".into(),
                source_folder: format!("D:/tmp/{}", Uuid::new_v4()),
            },
        )
        .await
        .unwrap();
        for case_id in [&first.id, &second.id] {
            sqlx::query(
                "INSERT INTO criminal_case_profiles
                 (case_id, suspected_charge, sentencing_recommendation, sentence_term, notes, profile_revision)
                 VALUES (?, 'fraud', 'three years', 'not sentenced', 'keep me', 7)",
            )
            .bind(case_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        (pool, first.id, second.id)
    }

    fn request(case_id: &str) -> SaveCriminalSentencingEstimateInput {
        SaveCriminalSentencingEstimateInput {
            case_id: case_id.into(),
            expected_profile_revision: 7,
            input_snapshot: json!({"crimeName":"fraud","amount":100000}),
            output_min_months: 18.0,
            output_max_months: Some(30.0),
            output_snapshot: json!({"finalPenaltyRange":[18,30]}),
            process_snapshot: json!([{"step":"starting point"}]),
            basis_snapshot: json!(["local rule snapshot"]),
            created_source: "explicit_user_save".into(),
        }
    }

    #[tokio::test]
    async fn migration_has_required_table_columns_and_index() {
        let (pool, _, _) = fixture().await;
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM pragma_table_info('criminal_sentencing_estimates') ORDER BY cid",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        for required in [
            "case_id",
            "profile_case_id",
            "profile_revision",
            "input_snapshot_json",
            "output_min_months",
            "output_max_months",
            "output_snapshot_json",
            "process_snapshot_json",
            "basis_snapshot_json",
            "created_source",
            "created_at",
        ] {
            assert!(
                columns.iter().any(|column| column == required),
                "missing {required}"
            );
        }
        let indexes: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='criminal_sentencing_estimates'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(indexes
            .iter()
            .any(|name| name == "idx_criminal_sentencing_estimates_case_created"));
    }

    #[tokio::test]
    async fn save_is_append_only_and_profile_and_case_are_unchanged() {
        let (pool, case_id, _) = fixture().await;
        let before_profile: (Option<String>, Option<String>, Option<String>, Option<String>, i64) =
            sqlx::query_as("SELECT suspected_charge, sentencing_recommendation, sentence_term, notes, profile_revision FROM criminal_case_profiles WHERE case_id=?")
                .bind(&case_id).fetch_one(&pool).await.unwrap();
        let before_case: Option<String> = sqlx::query_scalar("SELECT stage FROM cases WHERE id=?")
            .bind(&case_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let first = save_criminal_sentencing_estimate(&pool, request(&case_id))
            .await
            .unwrap();
        let second = save_criminal_sentencing_estimate(&pool, request(&case_id))
            .await
            .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(
            list_criminal_sentencing_estimates(&pool, &case_id)
                .await
                .unwrap()
                .len(),
            2
        );
        let after_profile: (Option<String>, Option<String>, Option<String>, Option<String>, i64) =
            sqlx::query_as("SELECT suspected_charge, sentencing_recommendation, sentence_term, notes, profile_revision FROM criminal_case_profiles WHERE case_id=?")
                .bind(&case_id).fetch_one(&pool).await.unwrap();
        let after_case: Option<String> = sqlx::query_scalar("SELECT stage FROM cases WHERE id=?")
            .bind(&case_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before_profile, after_profile);
        assert_eq!(before_case, after_case);
    }

    #[tokio::test]
    async fn revision_conflict_rejects_without_append() {
        let (pool, case_id, _) = fixture().await;
        let mut stale = request(&case_id);
        stale.expected_profile_revision = 6;
        let error = save_criminal_sentencing_estimate(&pool, stale)
            .await
            .unwrap_err();
        assert!(error.starts_with(REVISION_CONFLICT));
        assert!(list_criminal_sentencing_estimates(&pool, &case_id)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn reads_fail_closed_across_cases() {
        let (pool, first_case, second_case) = fixture().await;
        let saved = save_criminal_sentencing_estimate(&pool, request(&first_case))
            .await
            .unwrap();
        let error = get_criminal_sentencing_estimate(&pool, &second_case, &saved.id)
            .await
            .unwrap_err();
        assert!(error.starts_with(RECORD_NOT_FOUND));
        assert!(list_criminal_sentencing_estimates(&pool, &second_case)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn missing_case_profile_and_invalid_payload_have_distinct_codes() {
        let (pool, case_id, _) = fixture().await;
        let mut invalid = request(&case_id);
        invalid.output_max_months = Some(10.0);
        assert!(save_criminal_sentencing_estimate(&pool, invalid)
            .await
            .unwrap_err()
            .starts_with(INVALID_PAYLOAD));

        let mut missing_case = request("missing-case");
        missing_case.expected_profile_revision = 0;
        assert!(save_criminal_sentencing_estimate(&pool, missing_case)
            .await
            .unwrap_err()
            .starts_with(CASE_NOT_FOUND));

        sqlx::query("DELETE FROM criminal_case_profiles WHERE case_id=?")
            .bind(&case_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(save_criminal_sentencing_estimate(&pool, request(&case_id))
            .await
            .unwrap_err()
            .starts_with(PROFILE_NOT_FOUND));
    }
}
