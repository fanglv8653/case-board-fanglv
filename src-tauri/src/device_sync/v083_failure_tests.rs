use std::collections::BTreeMap;
use std::str::FromStr;

use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use super::operations::{apply_incoming, OperationAction, SyncOperation};
use super::{engine, registry};

async fn synthetic_pool() -> sqlx::SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO device_sync_groups (
             id, connector_root, local_device_id
         ) VALUES ('v083-synthetic-group','synthetic://mounted-folder','local-device')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn operation(
    operation_id: &str,
    entity_type: &str,
    entity_id: &str,
    case_id: Option<&str>,
    logical_time: i64,
    changed_fields: BTreeMap<String, Value>,
) -> SyncOperation {
    SyncOperation {
        operation_id: operation_id.to_string(),
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        case_id: case_id.map(str::to_string),
        action: OperationAction::Upsert,
        base_revision: 0,
        changed_fields,
        base_field_hashes: BTreeMap::new(),
        atomic_group: None,
        author_device_id: "remote-device".to_string(),
        logical_time,
        schema_version: 1,
    }
}

fn cyclic_case_operation() -> SyncOperation {
    operation(
        "op-case-cyclic",
        "case",
        "case-cyclic",
        Some("case-cyclic"),
        1,
        BTreeMap::from([
            ("id".to_string(), json!("case-cyclic")),
            ("name".to_string(), json!("Synthetic case")),
            ("judge_id".to_string(), json!("contact-judge")),
        ]),
    )
}

fn cyclic_contact_operation() -> SyncOperation {
    operation(
        "op-contact-judge",
        "contact",
        "contact-judge",
        Some("case-cyclic"),
        2,
        BTreeMap::from([
            ("id".to_string(), json!("contact-judge")),
            ("case_id".to_string(), json!("case-cyclic")),
            ("role".to_string(), json!("judge")),
            ("name".to_string(), json!("Synthetic contact")),
        ]),
    )
}

fn independent_calendar_operation() -> SyncOperation {
    operation(
        "op-calendar-independent",
        "calendar_event",
        "calendar-independent",
        None,
        3,
        BTreeMap::from([
            ("id".to_string(), json!("calendar-independent")),
            ("date".to_string(), json!("2099-01-01")),
            ("title".to_string(), json!("Synthetic independent row")),
        ]),
    )
}

async fn count(pool: &sqlx::SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn cyclic_case_then_contact_succeeds_when_both_are_in_one_transaction() {
    let pool = synthetic_pool().await;
    let mut tx = pool.begin().await.unwrap();

    apply_incoming(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &cyclic_case_operation(),
        "synthetic-payload-one",
    )
    .await
    .unwrap();
    apply_incoming(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &cyclic_contact_operation(),
        "synthetic-payload-one",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(count(&pool, "SELECT count(*) FROM cases").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 1);
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn case_package_commit_failure_rolls_back_every_operation() {
    let pool = synthetic_pool().await;
    let mut tx = pool.begin().await.unwrap();

    apply_incoming(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &cyclic_case_operation(),
        "synthetic-payload-split-one",
    )
    .await
    .unwrap();
    apply_incoming(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &independent_calendar_operation(),
        "synthetic-payload-split-one",
    )
    .await
    .unwrap();

    let commit_error = tx
        .commit()
        .await
        .expect_err("judge contact is absent from this package, so commit must fail");
    let sqlite_code = match &commit_error {
        sqlx::Error::Database(error) => error.code().map(|code| code.into_owned()),
        _ => None,
    };
    assert_eq!(sqlite_code.as_deref(), Some("787"));

    assert_eq!(count(&pool, "SELECT count(*) FROM cases").await, 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM calendar_events").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM device_sync_applied_operations").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM device_sync_entity_revisions").await,
        0
    );
}

#[tokio::test]
async fn contact_before_case_is_rejected_without_partial_write() {
    let pool = synthetic_pool().await;
    let mut tx = pool.begin().await.unwrap();

    let error = apply_incoming(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &cyclic_contact_operation(),
        "synthetic-payload-contact-first",
    )
    .await
    .expect_err("contact.case_id points to a case that is not present yet");
    assert_eq!(error.code(), "SYNC_DATABASE");
    tx.rollback().await.unwrap();

    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM device_sync_applied_operations").await,
        0
    );
}

#[tokio::test]
async fn repeated_package_quarantine_is_duplicated_and_audit_can_still_say_succeeded() {
    let pool = synthetic_pool().await;
    let source = "synthetic://remote-device/00000000000000000001.cbe";
    for attempt in 1..=2 {
        engine::quarantine_for_test(
            &pool,
            "v083-synthetic-group",
            Some(source),
            "SYNC_DATABASE",
            json!({"attempt": attempt, "fixture": "cyclic-foreign-key"}),
        )
        .await
        .unwrap();
    }
    engine::audit_for_test(
        &pool,
        Some("v083-synthetic-group"),
        Some("local-device"),
        "sync_once",
        "succeeded",
        json!({"quarantined": 2}),
    )
    .await
    .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE group_id='v083-synthetic-group'
               AND source_path='synthetic://remote-device/00000000000000000001.cbe'
               AND reason_code='SYNC_DATABASE'",
        )
        .await,
        2,
        "current schema/function inserts one row per retry instead of updating an active quarantine"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_audits
             WHERE group_id='v083-synthetic-group'
               AND action='sync_once' AND outcome='succeeded'
               AND json_extract(details_json,'$.quarantined')=2",
        )
        .await,
        1,
        "current audit contract permits succeeded even when packages were quarantined"
    );
}

#[test]
fn default_windows_split_case_contact_dependency_at_501_and_1001() {
    let limit = engine::max_operations_per_event_for_test();
    assert_eq!(limit, 500);
    let policies = registry::all_policies();
    let case_rank = policies
        .iter()
        .position(|policy| policy.entity_type == "case")
        .unwrap();
    let contact_rank = policies
        .iter()
        .position(|policy| policy.entity_type == "contact")
        .unwrap();
    assert!(
        case_rank < contact_rank,
        "baseline capture orders case first"
    );

    for (entity_total, expected_packages, dependency_is_split) in [
        (500_usize, 1_usize, false),
        (501, 2, true),
        (1000, 2, false),
        (1001, 3, true),
    ] {
        let mut registry_order = vec!["case-independent"; entity_total - 2];
        registry_order.extend(["case-cyclic", "contact-judge"]);
        let packages = registry_order.chunks(limit).collect::<Vec<_>>();
        let case_package = (entity_total - 2) / limit;
        let contact_package = (entity_total - 1) / limit;

        assert_eq!(packages.len(), expected_packages, "total={entity_total}");
        assert_eq!(
            case_package != contact_package,
            dependency_is_split,
            "total={entity_total}"
        );
        assert_eq!(registry_order[entity_total - 2], "case-cyclic");
        assert_eq!(registry_order[entity_total - 1], "contact-judge");
    }
}
