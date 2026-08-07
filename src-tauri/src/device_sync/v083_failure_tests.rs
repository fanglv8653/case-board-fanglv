use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use super::crypto::{generate_device_keys, generate_group_key};
use super::nas_folder::MountedFolder;
use super::operations::{apply_incoming_package, OperationAction, SyncOperation};
use super::{commands, engine, registry, SyncError};

async fn memory_pool() -> sqlx::SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

async fn synthetic_pool() -> sqlx::SqlitePool {
    let pool = memory_pool().await;
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

async fn synthetic_file_pool(label: &str) -> (std::path::PathBuf, sqlx::SqlitePool) {
    let path = std::env::temp_dir().join(format!(
        "caseboard-v083-{label}-{}.sqlite3",
        uuid::Uuid::new_v4().simple()
    ));
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
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
    (path, pool)
}

async fn remove_test_database_with_retry(path: &std::path::Path) {
    let mut last_error = None;
    for attempt in 0..20 {
        match fs::remove_file(path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => last_error = Some(error),
        }
        if attempt < 19 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    panic!(
        "failed to remove temporary SQLite database {}: {}",
        path.display(),
        last_error.expect("removal failure must retain its error")
    );
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
        capture_sequence: logical_time,
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

fn case_with_judge(operation_id: &str, case_id: &str, judge_id: &str) -> SyncOperation {
    operation(
        operation_id,
        "case",
        case_id,
        Some(case_id),
        10,
        BTreeMap::from([
            ("id".to_string(), json!(case_id)),
            ("name".to_string(), json!(format!("Synthetic {case_id}"))),
            ("judge_id".to_string(), json!(judge_id)),
        ]),
    )
}

fn contact_for_case(operation_id: &str, contact_id: &str, case_id: &str) -> SyncOperation {
    operation(
        operation_id,
        "contact",
        contact_id,
        Some(case_id),
        11,
        BTreeMap::from([
            ("id".to_string(), json!(contact_id)),
            ("case_id".to_string(), json!(case_id)),
            ("role".to_string(), json!("judge")),
            ("name".to_string(), json!(format!("Synthetic {contact_id}"))),
        ]),
    )
}

fn tombstone(
    operation_id: &str,
    entity_type: &str,
    entity_id: &str,
    case_id: Option<&str>,
    logical_time: i64,
) -> SyncOperation {
    let mut operation = operation(
        operation_id,
        entity_type,
        entity_id,
        case_id,
        logical_time,
        BTreeMap::new(),
    );
    operation.action = OperationAction::Tombstone;
    operation
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

async fn insert_outbox(
    pool: &sqlx::SqlitePool,
    operation: &SyncOperation,
    state: &str,
    exported_sequence: Option<i64>,
) {
    let capture_sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(capture_sequence),0)+1 FROM device_sync_outbox
         WHERE group_id='v083-synthetic-group'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO device_sync_outbox (
             operation_id,group_id,entity_type,entity_id,case_id,action,
             base_revision,changed_fields_json,base_field_hashes_json,
             atomic_group,author_device_id,logical_time,capture_sequence,schema_version,
             state,exported_sequence
         ) VALUES (?1,'v083-synthetic-group',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
    )
    .bind(&operation.operation_id)
    .bind(&operation.entity_type)
    .bind(&operation.entity_id)
    .bind(&operation.case_id)
    .bind(match operation.action {
        OperationAction::Upsert => "upsert",
        OperationAction::Tombstone => "tombstone",
    })
    .bind(operation.base_revision)
    .bind(serde_json::to_string(&operation.changed_fields).unwrap())
    .bind(serde_json::to_string(&operation.base_field_hashes).unwrap())
    .bind(&operation.atomic_group)
    .bind(&operation.author_device_id)
    .bind(operation.logical_time)
    .bind(capture_sequence)
    .bind(operation.schema_version as i64)
    .bind(state)
    .bind(exported_sequence)
    .execute(pool)
    .await
    .unwrap();
}

fn temporary_mounted_folder(label: &str) -> (std::path::PathBuf, MountedFolder) {
    let root = std::env::temp_dir().join(format!(
        "caseboard-v083-{label}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let folder = MountedFolder::connect(&root).unwrap();
    folder.initialize_group("v083-synthetic-group").unwrap();
    (root, folder)
}

fn draft_nas_paths(root: &Path, sequence: i64) -> (std::path::PathBuf, std::path::PathBuf) {
    let group_root = root
        .join("fanglv-caseboard-sync")
        .join("groups")
        .join("v083-synthetic-group");
    (
        group_root
            .join("manifests")
            .join("local-device")
            .join(format!("{sequence:020}.cbm")),
        group_root
            .join("events")
            .join("local-device")
            .join(format!("{sequence:020}.cbe")),
    )
}

fn count_files_with_extension(root: &Path, extension: &str) -> usize {
    if !root.exists() {
        return 0;
    }
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| {
            if path.is_dir() {
                count_files_with_extension(&path, extension)
            } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                1
            } else {
                0
            }
        })
        .sum()
}

#[tokio::test]
async fn cyclic_case_then_contact_succeeds_when_both_are_in_one_transaction() {
    let pool = synthetic_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let operations = vec![cyclic_case_operation(), cyclic_contact_operation()];
    let outcomes = apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "synthetic-payload-one",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(outcomes.len(), 2);
    assert_eq!(count(&pool, "SELECT count(*) FROM cases").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT judge_id FROM cases WHERE id='case-cyclic'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "contact-judge"
    );
    let hashes: String = sqlx::query_scalar(
        "SELECT field_hashes_json FROM device_sync_entity_revisions
         WHERE group_id='v083-synthetic-group' AND entity_type='case'
           AND entity_id='case-cyclic'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(serde_json::from_str::<BTreeMap<String, String>>(&hashes)
        .unwrap()
        .contains_key("judge_id"));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_dirty_entities
             WHERE entity_type='case' AND entity_id='case-cyclic'",
        )
        .await,
        0
    );
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap()
        .is_empty());

    let mut replay_tx = pool.begin().await.unwrap();
    let replay = apply_incoming_package(
        &mut replay_tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "synthetic-payload-one",
    )
    .await
    .unwrap();
    replay_tx.commit().await.unwrap();
    assert!(replay.iter().all(|outcome| outcome.duplicate));
    assert_eq!(count(&pool, "SELECT count(*) FROM cases").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 1);

    sqlx::raw_sql(
        "UPDATE cases SET judge_id=NULL WHERE id='case-cyclic';
         DELETE FROM contacts WHERE id='contact-judge';
         DELETE FROM cases WHERE id='case-cyclic';",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut deleted_dependency_replay = pool.begin().await.unwrap();
    let replay = apply_incoming_package(
        &mut deleted_dependency_replay,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "synthetic-payload-one",
    )
    .await
    .expect("an exact duplicate is independent of current business dependencies");
    deleted_dependency_replay.commit().await.unwrap();
    assert!(replay.iter().all(|outcome| outcome.duplicate));
    assert_eq!(count(&pool, "SELECT count(*) FROM cases").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 0);
}

#[tokio::test]
async fn reused_operation_id_with_different_payload_is_rejected_before_writes() {
    let pool = synthetic_pool().await;
    let operations = vec![cyclic_case_operation(), cyclic_contact_operation()];
    let mut tx = pool.begin().await.unwrap();
    apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "authenticated-payload-a",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let applied_before = count(&pool, "SELECT count(*) FROM device_sync_applied_operations").await;

    let mut mismatched = pool.begin().await.unwrap();
    let error = apply_incoming_package(
        &mut mismatched,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "authenticated-payload-b",
    )
    .await
    .expect_err("operation identity cannot be rebound to another authenticated payload");
    assert_eq!(error.code(), "SYNC_INTEGRITY");
    mismatched.rollback().await.unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM device_sync_applied_operations").await,
        applied_before
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM cases").await, 1);
    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 1);
}

#[tokio::test]
async fn duplicate_operation_id_inside_one_signed_event_is_rejected_before_writes() {
    let pool = synthetic_pool().await;
    for same_body in [true, false] {
        let first = operation(
            "op-duplicate-inside-event",
            "calendar_event",
            "calendar-duplicate-a",
            None,
            1,
            BTreeMap::from([
                ("id".to_string(), json!("calendar-duplicate-a")),
                ("date".to_string(), json!("2099-03-01")),
                ("title".to_string(), json!("Duplicate fixture")),
            ]),
        );
        let mut second = first.clone();
        if !same_body {
            second.entity_id = "calendar-duplicate-b".to_string();
            second
                .changed_fields
                .insert("id".to_string(), json!("calendar-duplicate-b"));
        }
        let mut tx = pool.begin().await.unwrap();
        let error = apply_incoming_package(
            &mut tx,
            "v083-synthetic-group",
            "remote-device",
            91,
            &[first, second],
            if same_body {
                "duplicate-same-body"
            } else {
                "duplicate-different-body"
            },
        )
        .await
        .expect_err("operation_id must be unique inside the authenticated array");
        assert_eq!(error.code(), "SYNC_INTEGRITY");
        tx.rollback().await.unwrap();
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
}

#[tokio::test]
async fn receiver_preserves_signed_order_for_multiple_actions_on_one_entity() {
    for tombstone_last in [true, false] {
        let pool = synthetic_pool().await;
        let entity_id = if tombstone_last {
            "calendar-upsert-then-tombstone"
        } else {
            "calendar-tombstone-then-upsert"
        };
        let upsert = operation(
            if tombstone_last {
                "op-calendar-upsert-first"
            } else {
                "op-calendar-upsert-second"
            },
            "calendar_event",
            entity_id,
            None,
            1,
            BTreeMap::from([
                ("id".to_string(), json!(entity_id)),
                ("date".to_string(), json!("2099-02-01")),
                ("title".to_string(), json!("Signed order fixture")),
            ]),
        );
        let mut tombstone = tombstone(
            if tombstone_last {
                "op-calendar-tombstone-second"
            } else {
                "op-calendar-tombstone-first"
            },
            "calendar_event",
            entity_id,
            None,
            2,
        );
        let mut upsert = upsert;
        let operations = if tombstone_last {
            tombstone.base_revision = 1;
            vec![upsert, tombstone]
        } else {
            upsert.base_revision = 1;
            vec![tombstone, upsert]
        };
        let mut tx = pool.begin().await.unwrap();
        let outcomes = apply_incoming_package(
            &mut tx,
            "v083-synthetic-group",
            "remote-device",
            2,
            &operations,
            if tombstone_last {
                "calendar-order-delete"
            } else {
                "calendar-order-restore"
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            count(
                &pool,
                &format!("SELECT count(*) FROM calendar_events WHERE id='{entity_id}'")
            )
            .await,
            if tombstone_last { 0 } else { 1 }
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT tombstoned FROM device_sync_entity_revisions
                 WHERE group_id='v083-synthetic-group' AND entity_type='calendar_event'
                   AND entity_id=?1",
            )
            .bind(entity_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            if tombstone_last { 1 } else { 0 }
        );
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().all(|outcome| !outcome.duplicate));
    }
}

#[tokio::test]
async fn consecutive_judge_operations_use_shadow_state_and_keep_final_hash_consistent() {
    for mode in ["success", "judge_conflict", "tombstone_conflict"] {
        let pool = synthetic_pool().await;
        let mut initial = pool.begin().await.unwrap();
        apply_incoming_package(
            &mut initial,
            "v083-synthetic-group",
            "remote-device",
            1,
            &[cyclic_case_operation(), cyclic_contact_operation()],
            "judge-shadow-initial",
        )
        .await
        .unwrap();
        initial.commit().await.unwrap();
        for (id, name) in [
            ("contact-judge-b", "Judge B"),
            ("contact-judge-c", "Judge C"),
        ] {
            sqlx::query(
                "INSERT INTO contacts(id,case_id,role,name) VALUES(?1,'case-cyclic','judge',?2)",
            )
            .bind(id)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query("DELETE FROM device_sync_dirty_entities")
            .execute(&pool)
            .await
            .unwrap();

        let judge_hash = |judge_id: &str| {
            super::operations::hash_fields(&serde_json::Map::from_iter([(
                "judge_id".to_string(),
                json!(judge_id),
            )]))["judge_id"]
                .clone()
        };
        let mut first = operation(
            "op-judge-shadow-first",
            "case",
            "case-cyclic",
            Some("case-cyclic"),
            10,
            BTreeMap::from([("judge_id".to_string(), json!("contact-judge-b"))]),
        );
        first.base_revision = 1;
        first
            .base_field_hashes
            .insert("judge_id".to_string(), judge_hash("contact-judge"));
        let second = if mode == "tombstone_conflict" {
            let mut operation = tombstone(
                "op-judge-shadow-second",
                "case",
                "case-cyclic",
                Some("case-cyclic"),
                11,
            );
            operation.base_revision = 0;
            operation
        } else {
            let mut operation = operation(
                "op-judge-shadow-second",
                "case",
                "case-cyclic",
                Some("case-cyclic"),
                11,
                BTreeMap::from([("judge_id".to_string(), json!("contact-judge-c"))]),
            );
            if mode == "success" {
                operation.base_revision = 2;
                operation
                    .base_field_hashes
                    .insert("judge_id".to_string(), judge_hash("contact-judge-b"));
            } else {
                operation.base_revision = 0;
                operation
                    .base_field_hashes
                    .insert("judge_id".to_string(), judge_hash("contact-judge"));
            }
            operation
        };
        let mut tx = pool.begin().await.unwrap();
        let outcomes = apply_incoming_package(
            &mut tx,
            "v083-synthetic-group",
            "remote-device",
            2,
            &[first, second],
            &format!("judge-shadow-{mode}"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let expected_judge = if mode == "success" {
            "contact-judge-c"
        } else {
            "contact-judge-b"
        };
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT judge_id FROM cases WHERE id='case-cyclic'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            expected_judge,
            "{mode}"
        );
        assert!(outcomes[0]
            .applied_fields
            .iter()
            .any(|field| field == "judge_id"));
        if mode == "judge_conflict" {
            assert!(outcomes[1]
                .conflict_fields
                .iter()
                .any(|field| field == "judge_id"));
            let local: String = sqlx::query_scalar(
                "SELECT local_value_json FROM device_sync_conflicts
                 WHERE operation_id='op-judge-shadow-second' AND field_key='judge_id'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&local).unwrap(),
                json!("contact-judge-b")
            );
        } else if mode == "tombstone_conflict" {
            assert!(outcomes[1]
                .conflict_fields
                .iter()
                .any(|field| field == "_tombstone"));
        } else {
            assert!(outcomes[1]
                .applied_fields
                .iter()
                .any(|field| field == "judge_id"));
        }
        let revision: (i64, String, i64) = sqlx::query_as(
            "SELECT revision,field_hashes_json,tombstoned
             FROM device_sync_entity_revisions
             WHERE group_id='v083-synthetic-group' AND entity_type='case' AND entity_id='case-cyclic'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(revision.0, 3);
        assert_eq!(revision.2, 0);
        let mut verify = pool.begin().await.unwrap();
        let actual = super::operations::fetch_entity(
            &mut verify,
            registry::policy("case").unwrap(),
            "case-cyclic",
        )
        .await
        .unwrap()
        .unwrap();
        verify.rollback().await.unwrap();
        assert_eq!(
            serde_json::from_str::<BTreeMap<String, String>>(&revision.1).unwrap(),
            super::operations::hash_fields(&actual),
            "{mode}"
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_dirty_entities
                 WHERE entity_type='case' AND entity_id='case-cyclic'",
            )
            .await,
            0
        );
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty());
    }
}

#[tokio::test]
async fn case_package_missing_contact_fails_before_any_write() {
    let pool = synthetic_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let operations = vec![cyclic_case_operation(), independent_calendar_operation()];
    let error = apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "synthetic-payload-split-one",
    )
    .await
    .expect_err("judge contact is absent from both receiver and package");
    assert_eq!(error.code(), "SYNC_PACKAGE_DEPENDENCY_MISSING");
    tx.rollback().await.unwrap();

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
    let operations = vec![cyclic_contact_operation()];
    let error = apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "synthetic-payload-contact-first",
    )
    .await
    .expect_err("contact.case_id points to a case that is not present yet");
    assert_eq!(error.code(), "SYNC_PACKAGE_DEPENDENCY_MISSING");
    tx.rollback().await.unwrap();

    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM device_sync_applied_operations").await,
        0
    );
}

#[tokio::test]
async fn package_accepts_shared_and_independent_case_contact_cycles() {
    let pool = synthetic_pool().await;
    let shared = vec![
        contact_for_case("op-shared-contact", "contact-shared", "case-shared-a"),
        case_with_judge("op-shared-case-a", "case-shared-a", "contact-shared"),
        case_with_judge("op-shared-case-b", "case-shared-b", "contact-shared"),
    ];
    let mut tx = pool.begin().await.unwrap();
    apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        2,
        &shared,
        "synthetic-shared",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let independent = vec![
        contact_for_case(
            "op-independent-contact",
            "contact-independent",
            "case-independent",
        ),
        case_with_judge(
            "op-independent-case",
            "case-independent",
            "contact-independent",
        ),
    ];
    let mut tx = pool.begin().await.unwrap();
    apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        3,
        &independent,
        "synthetic-independent",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(count(&pool, "SELECT count(*) FROM cases").await, 3);
    assert_eq!(count(&pool, "SELECT count(*) FROM contacts").await, 2);
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn durable_export_recovers_empty_manifest_only_event_only_and_complete_sites() {
    for (label, initial_part) in [
        ("empty", None),
        ("manifest-only", Some(engine::TestPublishPart::ManifestOnly)),
        ("event-only", Some(engine::TestPublishPart::EventOnly)),
        ("complete", Some(engine::TestPublishPart::Complete)),
    ] {
        let pool = synthetic_pool().await;
        insert_outbox(&pool, &independent_calendar_operation(), "pending", None).await;
        let group_key = generate_group_key();
        let device = generate_device_keys();
        let (root, folder) = temporary_mounted_folder(label);
        let prepared = engine::prepare_next_export_for_test(
            &pool,
            "v083-synthetic-group",
            &group_key,
            &device.signing_secret,
        )
        .await
        .unwrap();
        let (manifest_path, event_path) = draft_nas_paths(&root, prepared.sequence);
        if label == "manifest-only" {
            let error = engine::publish_prepared_export_for_test(
                &folder,
                &prepared,
                engine::TestPublishPart::FailAfterManifest,
            )
            .unwrap_err();
            assert_eq!(error.code(), "SYNC_NAS_UNAVAILABLE");
        } else if let Some(part) = initial_part {
            engine::publish_prepared_export_for_test(&folder, &prepared, part).unwrap();
        }
        assert_eq!(
            manifest_path.exists(),
            label != "empty" && label != "event-only"
        );
        assert_eq!(
            event_path.exists(),
            label == "event-only" || label == "complete"
        );

        let recovered = engine::prepare_next_export_for_test(
            &pool,
            "v083-synthetic-group",
            &group_key,
            &device.signing_secret,
        )
        .await
        .unwrap();
        assert_eq!(
            recovered.event_envelope_bytes,
            prepared.event_envelope_bytes
        );
        assert_eq!(
            recovered.manifest_envelope_bytes,
            prepared.manifest_envelope_bytes
        );
        engine::publish_prepared_export_for_test(
            &folder,
            &recovered,
            engine::TestPublishPart::Complete,
        )
        .unwrap();
        engine::finalize_prepared_export_for_test(&pool, &recovered, false)
            .await
            .unwrap();

        assert_eq!(
            fs::read(&manifest_path).unwrap(),
            prepared.manifest_envelope_bytes
        );
        assert_eq!(
            fs::read(&event_path).unwrap(),
            prepared.event_envelope_bytes
        );
        assert_eq!(
            count(&pool, "SELECT count(*) FROM device_sync_export_drafts").await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_outbox WHERE state='exported' AND exported_sequence=1",
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_groups WHERE next_sequence=2 AND last_manifest_hash IS NOT NULL",
            )
            .await,
            1
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn durable_export_cas_failure_rolls_back_and_reuses_exact_draft_bytes() {
    let pool = synthetic_pool().await;
    insert_outbox(&pool, &independent_calendar_operation(), "pending", None).await;
    sqlx::query(
        "INSERT INTO device_sync_quarantine (
             id,group_id,source_path,source_device_id,source_sequence,
             reason_code,details_json,status,last_error_code
         ) VALUES (
             'local-export-quarantine','v083-synthetic-group',NULL,'local-device',1,
             'SYNC_INTEGRITY','{}','active','SYNC_INTEGRITY'
         )",
    )
    .execute(&pool)
    .await
    .unwrap();
    let group_key = generate_group_key();
    let device = generate_device_keys();
    let (root, folder) = temporary_mounted_folder("cas-rollback");
    let prepared = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    )
    .await
    .unwrap();
    engine::publish_prepared_export_for_test(&folder, &prepared, engine::TestPublishPart::Complete)
        .unwrap();
    let failure = engine::finalize_prepared_export_for_test(&pool, &prepared, true)
        .await
        .unwrap_err();
    assert_eq!(failure.code(), "SYNC_BUSY");
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_groups WHERE next_sequence=1 AND last_manifest_hash IS NULL",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='pending' AND exported_sequence IS NULL",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_export_drafts WHERE state='prepared'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine WHERE status='active'",
        )
        .await,
        1
    );

    let recovered = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    )
    .await
    .unwrap();
    assert_eq!(
        recovered.event_envelope_bytes,
        prepared.event_envelope_bytes
    );
    assert_eq!(
        recovered.manifest_envelope_bytes,
        prepared.manifest_envelope_bytes
    );
    engine::publish_prepared_export_for_test(
        &folder,
        &recovered,
        engine::TestPublishPart::Complete,
    )
    .unwrap();
    engine::finalize_prepared_export_for_test(&pool, &recovered, false)
        .await
        .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine WHERE status='resolved' AND resolved_at IS NOT NULL",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_audits WHERE action='quarantine_resolved' AND outcome='succeeded'",
        )
        .await,
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn durable_export_refuses_different_existing_bytes_without_database_progress() {
    let pool = synthetic_pool().await;
    insert_outbox(&pool, &independent_calendar_operation(), "pending", None).await;
    let group_key = generate_group_key();
    let device = generate_device_keys();
    let (root, folder) = temporary_mounted_folder("different-existing-bytes");
    let prepared = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    )
    .await
    .unwrap();
    let (manifest_path, event_path) = draft_nas_paths(&root, prepared.sequence);
    fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
    fs::write(&manifest_path, b"different-existing-bytes").unwrap();
    let error = engine::publish_prepared_export_for_test(
        &folder,
        &prepared,
        engine::TestPublishPart::Complete,
    )
    .unwrap_err();
    assert_eq!(error.code(), "SYNC_INTEGRITY");
    assert!(!error
        .to_string()
        .contains(&root.to_string_lossy().to_string()));
    assert_eq!(
        fs::read(&manifest_path).unwrap(),
        b"different-existing-bytes"
    );
    assert!(!event_path.exists());
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_groups WHERE next_sequence=1 AND last_manifest_hash IS NULL",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='pending'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_export_drafts WHERE state='prepared'",
        )
        .await,
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn durable_export_draft_binding_mismatches_fail_closed() {
    for (label, mutation) in [
        (
            "operations",
            "UPDATE device_sync_outbox SET changed_fields_json='{\"title\":\"mutated\"}'",
        ),
        (
            "previous-hash",
            "UPDATE device_sync_groups SET last_manifest_hash='unexpected'",
        ),
        (
            "device",
            "UPDATE device_sync_groups SET local_device_id='other-device'",
        ),
        ("sequence", "UPDATE device_sync_groups SET next_sequence=2"),
        ("key-epoch", "UPDATE device_sync_groups SET key_epoch=2"),
    ] {
        let pool = synthetic_pool().await;
        insert_outbox(&pool, &independent_calendar_operation(), "pending", None).await;
        let group_key = generate_group_key();
        let device = generate_device_keys();
        let prepared = engine::prepare_next_export_for_test(
            &pool,
            "v083-synthetic-group",
            &group_key,
            &device.signing_secret,
        )
        .await
        .unwrap();
        sqlx::query(mutation).execute(&pool).await.unwrap();
        let error = engine::prepare_next_export_for_test(
            &pool,
            "v083-synthetic-group",
            &group_key,
            &device.signing_secret,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), "SYNC_INTEGRITY", "{label}");
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_export_drafts WHERE state='prepared'",
            )
            .await,
            1,
            "{label}"
        );
        assert_eq!(prepared.sequence, 1, "{label}");
    }
}

#[tokio::test]
async fn durable_export_second_package_failure_preserves_first_and_recovers_second() {
    let pool = synthetic_pool().await;
    for index in 0..501_i64 {
        let operation = operation(
            &format!("durable-package-{index:04}"),
            "calendar_event",
            &format!("durable-calendar-{index:04}"),
            None,
            index + 1,
            BTreeMap::from([
                (
                    "id".to_string(),
                    json!(format!("durable-calendar-{index:04}")),
                ),
                ("date".to_string(), json!("2099-01-01")),
                ("title".to_string(), json!(format!("Durable {index:04}"))),
            ]),
        );
        insert_outbox(&pool, &operation, "pending", None).await;
    }
    let group_key = generate_group_key();
    let device = generate_device_keys();
    let (root, folder) = temporary_mounted_folder("second-package");

    let first = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    )
    .await
    .unwrap();
    assert_eq!(first.operation_ids.len(), 500);
    engine::publish_prepared_export_for_test(&folder, &first, engine::TestPublishPart::Complete)
        .unwrap();
    engine::finalize_prepared_export_for_test(&pool, &first, false)
        .await
        .unwrap();
    let (first_manifest_path, first_event_path) = draft_nas_paths(&root, 1);
    let first_manifest_bytes = fs::read(&first_manifest_path).unwrap();
    let first_event_bytes = fs::read(&first_event_path).unwrap();

    let second = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    )
    .await
    .unwrap();
    assert_eq!(second.sequence, 2);
    assert_eq!(second.operation_ids.len(), 1);
    engine::publish_prepared_export_for_test(
        &folder,
        &second,
        engine::TestPublishPart::ManifestOnly,
    )
    .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='exported' AND exported_sequence=1",
        )
        .await,
        500
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='pending'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_groups WHERE next_sequence=2",
        )
        .await,
        1
    );

    let recovered = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    )
    .await
    .unwrap();
    assert_eq!(recovered.event_envelope_bytes, second.event_envelope_bytes);
    assert_eq!(
        recovered.manifest_envelope_bytes,
        second.manifest_envelope_bytes
    );
    engine::publish_prepared_export_for_test(
        &folder,
        &recovered,
        engine::TestPublishPart::Complete,
    )
    .unwrap();
    engine::finalize_prepared_export_for_test(&pool, &recovered, false)
        .await
        .unwrap();
    assert_eq!(
        fs::read(&first_manifest_path).unwrap(),
        first_manifest_bytes
    );
    assert_eq!(fs::read(&first_event_path).unwrap(), first_event_bytes);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='exported'",
        )
        .await,
        501
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_groups WHERE next_sequence=3",
        )
        .await,
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn concurrent_draft_prepare_has_one_winner_and_one_exact_recovery() {
    let (database_path, pool) = synthetic_file_pool("concurrent-draft").await;
    insert_outbox(&pool, &independent_calendar_operation(), "pending", None).await;
    let group_key = generate_group_key();
    let device = generate_device_keys();
    let first_future = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    );
    let second_future = engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    );
    let (first, second) = tokio::join!(first_future, second_future);
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.sequence, second.sequence);
    assert_eq!(first.operation_ids, second.operation_ids);
    assert_eq!(first.event_envelope_bytes, second.event_envelope_bytes);
    assert_eq!(
        first.manifest_envelope_bytes,
        second.manifest_envelope_bytes
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_export_drafts WHERE state='prepared'",
        )
        .await,
        1
    );
    pool.close().await;
    remove_test_database_with_retry(&database_path).await;
}

#[tokio::test]
async fn persisted_export_draft_contains_no_business_plaintext_key_or_absolute_path() {
    let pool = synthetic_pool().await;
    let sensitive_operation = operation(
        "draft-safe-metadata-id",
        "calendar_event",
        "draft-sensitive-calendar",
        None,
        1,
        BTreeMap::from([
            ("id".to_string(), json!("draft-sensitive-calendar")),
            ("date".to_string(), json!("2099-01-01")),
            (
                "title".to_string(),
                json!("TOP-SECRET-DRAFT-PLAINTEXT C:\\private\\case\\secret.txt"),
            ),
        ]),
    );
    insert_outbox(&pool, &sensitive_operation, "pending", None).await;
    let group_key = generate_group_key();
    let device = generate_device_keys();
    engine::prepare_next_export_for_test(
        &pool,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
    )
    .await
    .unwrap();
    let row: (Vec<u8>, Vec<u8>, String, String) = sqlx::query_as(
        "SELECT event_envelope_bytes, manifest_envelope_bytes,
                operation_ids_json, operation_fingerprint
         FROM device_sync_export_drafts WHERE state='prepared'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut persisted = Vec::new();
    persisted.extend_from_slice(&row.0);
    persisted.extend_from_slice(&row.1);
    persisted.extend_from_slice(row.2.as_bytes());
    persisted.extend_from_slice(row.3.as_bytes());
    assert!(!persisted
        .windows(b"TOP-SECRET-DRAFT-PLAINTEXT".len())
        .any(|window| window == b"TOP-SECRET-DRAFT-PLAINTEXT"));
    assert!(!persisted
        .windows(b"C:\\private\\case\\secret.txt".len())
        .any(|window| window == b"C:\\private\\case\\secret.txt"));
    assert!(!persisted
        .windows(group_key.len())
        .any(|window| window == group_key.as_slice()));
    assert!(!persisted
        .windows(device.signing_secret.len())
        .any(|window| window == device.signing_secret.as_slice()));
}

#[test]
fn public_sync_errors_keep_internal_paths_database_text_and_secrets_private() {
    let secret = r"C:\private\case\folder\sync.cbe API_KEY=do-not-leak sqlite failure";
    let errors = [
        SyncError::Database(secret.to_string()),
        SyncError::Serialization(secret.to_string()),
        SyncError::Integrity(secret.to_string()),
        SyncError::InvalidNasPath(secret.to_string()),
        SyncError::NasUnavailable(secret.to_string()),
        SyncError::CredentialStore(secret.to_string()),
    ];
    for error in errors {
        let code = error.code();
        let public = commands::command_error(error);
        assert!(public.starts_with(&format!("[{code}] ")));
        assert!(!public.contains("C:\\private"));
        assert!(!public.contains("API_KEY"));
        assert!(!public.contains("sqlite failure"));
    }
    let serialized = serde_json::to_string(&SyncError::NasUnavailable(secret.to_string())).unwrap();
    assert!(serialized.contains("SYNC_NAS_UNAVAILABLE"));
    assert!(!serialized.contains("C:\\\\private"));
    assert!(!serialized.contains("API_KEY"));
}

#[tokio::test]
async fn persisted_draft_envelope_tamper_matrix_fails_before_nas_or_database_progress() {
    for envelope_column in ["event_envelope_bytes", "manifest_envelope_bytes"] {
        for mutation in [
            "ciphertext",
            "ciphertext_hash",
            "signature",
            "nonce",
            "protocol",
            "payload_kind",
            "group",
            "device",
            "sequence",
            "epoch",
        ] {
            let pool = synthetic_pool().await;
            insert_outbox(&pool, &independent_calendar_operation(), "pending", None).await;
            let group_key = generate_group_key();
            let device = generate_device_keys();
            engine::prepare_next_export_for_test(
                &pool,
                "v083-synthetic-group",
                &group_key,
                &device.signing_secret,
            )
            .await
            .unwrap();
            let bytes: Vec<u8> = sqlx::query_scalar(&format!(
                "SELECT {envelope_column} FROM device_sync_export_drafts"
            ))
            .fetch_one(&pool)
            .await
            .unwrap();
            let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            match mutation {
                "ciphertext" => envelope["ciphertext_b64"] = json!("AAAA"),
                "ciphertext_hash" => envelope["ciphertext_sha256"] = json!("00".repeat(32)),
                "signature" => envelope["signature_b64"] = json!("AAAA"),
                "nonce" => envelope["nonce_b64"] = json!("AAAA"),
                "protocol" => envelope["header"]["protocol_version"] = json!(999),
                "payload_kind" => {
                    envelope["header"]["payload_kind"] =
                        json!(if envelope_column.starts_with("event") {
                            "manifest"
                        } else {
                            "operations"
                        })
                }
                "group" => envelope["header"]["group_id"] = json!("other-group"),
                "device" => envelope["header"]["device_id"] = json!("other-device"),
                "sequence" => envelope["header"]["sequence"] = json!(99),
                "epoch" => envelope["header"]["key_epoch"] = json!(99),
                _ => unreachable!(),
            }
            let mutated = serde_json::to_vec(&envelope).unwrap();
            sqlx::query(&format!(
                "UPDATE device_sync_export_drafts SET {envelope_column}=?1"
            ))
            .bind(mutated)
            .execute(&pool)
            .await
            .unwrap();
            let error = engine::prepare_next_export_for_test(
                &pool,
                "v083-synthetic-group",
                &group_key,
                &device.signing_secret,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(
                    &error,
                    SyncError::Integrity(_)
                        | SyncError::Protocol(_)
                        | SyncError::Crypto(_)
                        | SyncError::Serialization(_)
                ),
                "{envelope_column}/{mutation}: {error}"
            );
            assert_eq!(
                count(
                    &pool,
                    "SELECT count(*) FROM device_sync_groups WHERE next_sequence=1 AND last_manifest_hash IS NULL",
                )
                .await,
                1,
                "{envelope_column}/{mutation}"
            );
            assert_eq!(
                count(
                    &pool,
                    "SELECT count(*) FROM device_sync_outbox WHERE state='pending'",
                )
                .await,
                1,
                "{envelope_column}/{mutation}"
            );
        }
    }
}

#[tokio::test]
async fn production_export_orchestration_recovers_each_injected_failure_phase() {
    for phase in ["after_manifest", "after_event", "after_cas"] {
        let pool = synthetic_pool().await;
        insert_outbox(&pool, &independent_calendar_operation(), "pending", None).await;
        let group_key = generate_group_key();
        let device = generate_device_keys();
        let (root, folder) = temporary_mounted_folder(&format!("production-{phase}"));
        let error = engine::export_pending_with_fault_for_test(
            &pool,
            &folder,
            "v083-synthetic-group",
            &group_key,
            &device.signing_secret,
            phase,
            1,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            SyncError::NasUnavailable(_) | SyncError::Busy
        ));
        assert_eq!(
            count(&pool, "SELECT count(*) FROM device_sync_export_drafts").await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_outbox WHERE state='pending'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_groups WHERE next_sequence=1"
            )
            .await,
            1
        );
        let recovered = engine::export_pending_with_fault_for_test(
            &pool,
            &folder,
            "v083-synthetic-group",
            &group_key,
            &device.signing_secret,
            "none",
            0,
        )
        .await
        .unwrap();
        assert_eq!(recovered, 1);
        assert_eq!(
            count(&pool, "SELECT count(*) FROM device_sync_export_drafts").await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_outbox WHERE state='exported'"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_groups WHERE next_sequence=2"
            )
            .await,
            1
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn production_export_orchestration_recovers_second_package_only() {
    let pool = synthetic_pool().await;
    for index in 0..501_i64 {
        let operation = operation(
            &format!("production-package-{index:04}"),
            "calendar_event",
            &format!("production-calendar-{index:04}"),
            None,
            index + 1,
            BTreeMap::from([
                (
                    "id".to_string(),
                    json!(format!("production-calendar-{index:04}")),
                ),
                ("date".to_string(), json!("2099-06-01")),
                ("title".to_string(), json!(format!("Production {index:04}"))),
            ]),
        );
        insert_outbox(&pool, &operation, "pending", None).await;
    }
    let group_key = generate_group_key();
    let device = generate_device_keys();
    let (root, folder) = temporary_mounted_folder("production-second-package");
    engine::export_pending_with_fault_for_test(
        &pool,
        &folder,
        "v083-synthetic-group",
        &group_key,
        &device.signing_secret,
        "after_manifest",
        2,
    )
    .await
    .unwrap_err();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='exported'"
        )
        .await,
        500
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='pending'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_groups WHERE next_sequence=2"
        )
        .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM device_sync_export_drafts").await,
        1
    );
    assert_eq!(
        engine::export_pending_with_fault_for_test(
            &pool,
            &folder,
            "v083-synthetic-group",
            &group_key,
            &device.signing_secret,
            "none",
            0,
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='exported'"
        )
        .await,
        501
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_groups WHERE next_sequence=3"
        )
        .await,
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn receiver_existing_dependencies_satisfy_package_preflight() {
    let pool = synthetic_pool().await;
    sqlx::query(
        "INSERT INTO cases(id,name,source_folder)
         VALUES('receiver-case','Receiver case','synthetic://receiver-case')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contacts(id,case_id,role,name)
         VALUES('receiver-contact','receiver-case','judge','Receiver contact')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM device_sync_dirty_entities")
        .execute(&pool)
        .await
        .unwrap();

    let operations = vec![
        contact_for_case(
            "op-contact-existing-case",
            "contact-existing-case",
            "receiver-case",
        ),
        case_with_judge(
            "op-case-existing-contact",
            "case-existing-contact",
            "receiver-contact",
        ),
    ];
    let mut tx = pool.begin().await.unwrap();
    apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        4,
        &operations,
        "synthetic-existing-dependencies",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM contacts WHERE id='contact-existing-case'",
        )
        .await,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT judge_id FROM cases WHERE id='case-existing-contact'",
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "receiver-contact"
    );
}

#[tokio::test]
async fn package_midway_failure_rolls_back_all_sync_and_business_rows() {
    let pool = synthetic_pool().await;
    let invalid_calendar = operation(
        "op-invalid-calendar",
        "calendar_event",
        "invalid-calendar",
        None,
        12,
        BTreeMap::from([
            ("id".to_string(), json!("invalid-calendar")),
            ("date".to_string(), json!("2099-01-02")),
        ]),
    );
    let operations = vec![
        cyclic_case_operation(),
        cyclic_contact_operation(),
        invalid_calendar,
    ];
    let mut tx = pool.begin().await.unwrap();
    let error = apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        5,
        &operations,
        "synthetic-midway-failure",
    )
    .await
    .expect_err("calendar title is required");
    assert_eq!(error.code(), "SYNC_DATABASE");
    tx.rollback().await.unwrap();

    for table in [
        "cases",
        "contacts",
        "calendar_events",
        "device_sync_applied_operations",
        "device_sync_entity_revisions",
        "device_sync_conflicts",
        "device_sync_dirty_entities",
    ] {
        assert_eq!(
            count(&pool, &format!("SELECT count(*) FROM {table}")).await,
            0,
            "table={table}"
        );
    }
}

#[tokio::test]
async fn repeated_package_quarantine_updates_one_active_row() {
    let pool = synthetic_pool().await;
    let source = "synthetic://remote-device/00000000000000000001.cbe";
    for attempt in 1..=2 {
        engine::quarantine_for_test(
            &pool,
            "v083-synthetic-group",
            "remote-device",
            1,
            Some(source),
            "SYNC_DATABASE",
            json!({"attempt": attempt, "fixture": "cyclic-foreign-key"}),
        )
        .await
        .unwrap();
    }
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE group_id='v083-synthetic-group'
               AND source_path='00000000000000000001.cbe'
               AND reason_code='SYNC_DATABASE' AND status='active'",
        )
        .await,
        1,
        "one deterministic package failure must have one active quarantine"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT retry_count FROM device_sync_quarantine
             WHERE group_id='v083-synthetic-group' AND status='active'",
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
}

#[tokio::test]
async fn quarantine_identity_isolated_by_device_and_resolve_covers_all_reasons() {
    let pool = synthetic_pool().await;
    let source = "D:/private/device-folder/00000000000000000007.cbe";
    for (device, reason) in [
        ("device-a", "SYNC_DATABASE"),
        ("device-a", "SYNC_PACKAGE_DEPENDENCY_MISSING"),
        ("device-b", "SYNC_DATABASE"),
    ] {
        engine::auto_pause_failure_for_test(
            &pool,
            "v083-synthetic-group",
            device,
            7,
            source,
            reason,
        )
        .await
        .expect_err("fixture deliberately auto-pauses");
    }
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine WHERE status='active'",
        )
        .await,
        3,
        "same filename never merges different devices or reason codes"
    );

    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        engine::resolve_active_quarantine_for_test(
            &mut tx,
            "v083-synthetic-group",
            "device-a",
            7,
            Path::new(source),
        )
        .await
        .unwrap(),
        2,
        "one authenticated package resolves every reason for its exact identity"
    );
    tx.commit().await.unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE source_device_id='device-a' AND source_sequence=7 AND status='resolved'",
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE source_device_id='device-b' AND source_sequence=7 AND status='active'",
        )
        .await,
        1,
        "resolving device-a must not touch device-b's same-named package"
    );
    let resolution_audit: String = sqlx::query_scalar(
        "SELECT details_json FROM device_sync_audits
         WHERE action='quarantine_resolved' AND outcome='succeeded' AND device_id='device-a'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let resolution_audit: Value = serde_json::from_str(&resolution_audit).unwrap();
    assert_eq!(resolution_audit["device_id"], json!("device-a"));
    assert_eq!(resolution_audit["sequence"], json!(7));
    assert_eq!(
        resolution_audit["source_file"],
        json!("00000000000000000007.cbe")
    );
    assert_eq!(resolution_audit["resolved_count"], json!(2));
}

#[tokio::test]
async fn deterministic_export_planning_failure_pauses_local_sequence_once() {
    let pool = synthetic_pool().await;
    let error = engine::auto_pause_export_failure_for_test(
        &pool,
        "v083-synthetic-group",
        super::SyncError::PackageTooLarge,
    )
    .await
    .expect_err("deterministic export planning errors stop scheduler retries");
    assert_eq!(error.code(), "SYNC_GROUP_AUTO_PAUSED");
    let identity: (String, i64, String) = sqlx::query_as(
        "SELECT source_device_id,source_sequence,reason_code
         FROM device_sync_quarantine WHERE status='active'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        identity,
        (
            "local-device".to_string(),
            1,
            "SYNC_PACKAGE_TOO_LARGE".to_string()
        )
    );
    assert!(super::scheduler::eligible_group_ids(&pool).await.is_empty());
}

#[tokio::test]
async fn real_sync_once_quarantines_persisted_planner_corruption_without_retry_loop() {
    for corruption in ["json", "dependency_type", "action"] {
        let pool = synthetic_pool().await;
        let (root, _folder) = temporary_mounted_folder(&format!("planner-{corruption}"));
        sqlx::query(
            "UPDATE device_sync_groups SET connector_root=?1
             WHERE id='v083-synthetic-group'",
        )
        .bind(root.to_string_lossy().as_ref())
        .execute(&pool)
        .await
        .unwrap();
        let operation = if corruption == "dependency_type" {
            operation(
                "op-corrupt-dependency",
                "case",
                "case-corrupt-dependency",
                Some("case-corrupt-dependency"),
                1,
                BTreeMap::from([
                    ("id".to_string(), json!("case-corrupt-dependency")),
                    ("name".to_string(), json!("Corrupt dependency")),
                    ("judge_id".to_string(), json!(42)),
                ]),
            )
        } else {
            independent_calendar_operation()
        };
        insert_outbox(&pool, &operation, "pending", None).await;
        match corruption {
            "json" => {
                sqlx::query(
                    "UPDATE device_sync_outbox SET changed_fields_json='{'
                     WHERE operation_id=?1",
                )
                .bind(&operation.operation_id)
                .execute(&pool)
                .await
                .unwrap();
            }
            "action" => {
                sqlx::query("PRAGMA ignore_check_constraints=ON")
                    .execute(&pool)
                    .await
                    .unwrap();
                sqlx::query(
                    "UPDATE device_sync_outbox SET action='invalid_action'
                     WHERE operation_id=?1",
                )
                .bind(&operation.operation_id)
                .execute(&pool)
                .await
                .unwrap();
                sqlx::query("PRAGMA ignore_check_constraints=OFF")
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "dependency_type" => {}
            _ => unreachable!(),
        }

        let error = engine::sync_once(&pool, "v083-synthetic-group")
            .await
            .expect_err("persisted planner corruption is deterministic");
        assert_eq!(error.code(), "SYNC_GROUP_AUTO_PAUSED", "{corruption}");
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM device_sync_quarantine WHERE status='active'",
            )
            .await,
            1,
            "{corruption}"
        );
        assert!(super::scheduler::eligible_group_ids(&pool).await.is_empty());
        let retry_before = count(
            &pool,
            "SELECT retry_count FROM device_sync_quarantine WHERE status='active'",
        )
        .await;
        assert_eq!(
            engine::sync_once(&pool, "v083-synthetic-group")
                .await
                .unwrap_err()
                .code(),
            "SYNC_GROUP_AUTO_PAUSED"
        );
        assert_eq!(
            count(
                &pool,
                "SELECT retry_count FROM device_sync_quarantine WHERE status='active'",
            )
            .await,
            retry_before,
            "paused scheduler/manual retries must not recreate the quarantine"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn status_keeps_manual_review_history_visible() {
    let pool = synthetic_pool().await;
    sqlx::query(
        "INSERT INTO device_sync_quarantine(
             id,group_id,source_path,source_device_id,source_sequence,
             reason_code,details_json,status,last_error_code
         ) VALUES('manual-review-row','v083-synthetic-group','legacy.cbe','__legacy__',-1,
                  'SYNC_DATABASE','{}','manual_review','SYNC_DATABASE')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let status = engine::get_status(&pool, "v083-synthetic-group")
        .await
        .unwrap();
    assert_eq!(status.quarantined, 0);
    assert_eq!(status.manual_review, 1);
}

#[tokio::test]
async fn explicit_pause_and_resume_clear_auto_pause_metadata() {
    let pool = synthetic_pool().await;
    super::queries::set_paused(&pool, "v083-synthetic-group", true)
        .await
        .unwrap();
    let manual: (i64, i64, Option<String>) = sqlx::query_as(
        "SELECT paused,auto_paused,pause_reason_code
         FROM device_sync_groups WHERE id='v083-synthetic-group'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(manual, (1, 0, Some("USER_PAUSED".to_string())));

    sqlx::query(
        "UPDATE device_sync_groups
         SET paused=1,auto_paused=1,pause_reason_code='SYNC_PACKAGE_DEPENDENCY_MISSING'
         WHERE id='v083-synthetic-group'",
    )
    .execute(&pool)
    .await
    .unwrap();
    super::queries::set_paused(&pool, "v083-synthetic-group", false)
        .await
        .unwrap();
    let resumed: (i64, i64, Option<String>) = sqlx::query_as(
        "SELECT paused,auto_paused,pause_reason_code
         FROM device_sync_groups WHERE id='v083-synthetic-group'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(resumed, (0, 0, None));
}

#[tokio::test]
async fn auto_pause_retry_resume_replay_resolves_and_records_only_real_success() {
    let pool = synthetic_pool().await;
    let source_path = Path::new("D:/private-case-folder/00000000000000000001.cbe");
    engine::mark_sync_attempt_for_test(&pool, "v083-synthetic-group")
        .await
        .unwrap();
    for _ in 0..2 {
        let error = engine::auto_pause_failure_for_test(
            &pool,
            "v083-synthetic-group",
            "remote-device",
            1,
            source_path.to_str().unwrap(),
            "SYNC_PACKAGE_DEPENDENCY_MISSING",
        )
        .await
        .expect_err("deterministic package failure must return the auto-paused code");
        assert_eq!(error.code(), "SYNC_GROUP_AUTO_PAUSED");
    }

    let paused: (i64, i64, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT paused,auto_paused,pause_reason_code,last_attempt_at,last_success_at
         FROM device_sync_groups WHERE id='v083-synthetic-group'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(paused.0, 1);
    assert_eq!(paused.1, 1);
    assert_eq!(paused.2.as_deref(), Some("SYNC_PACKAGE_DEPENDENCY_MISSING"));
    assert!(
        paused.3.is_some(),
        "every sync attempt advances last_attempt_at"
    );
    assert!(
        paused.4.is_none(),
        "failure must not advance last_success_at"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine WHERE status='active'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT retry_count FROM device_sync_quarantine WHERE status='active'",
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_audits WHERE outcome='succeeded'",
        )
        .await,
        0
    );
    let paused_details: Vec<String> = sqlx::query_scalar(
        "SELECT details_json FROM device_sync_audits WHERE outcome='paused' ORDER BY created_at,id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(paused_details.len(), 2);
    assert!(paused_details
        .iter()
        .all(|details| !details.contains("private-case-folder")));
    assert!(paused_details
        .iter()
        .all(|details| details.contains("00000000000000000001.cbe")));

    assert!(super::scheduler::eligible_group_ids(&pool).await.is_empty());
    assert_eq!(
        count(
            &pool,
            "SELECT retry_count FROM device_sync_quarantine WHERE status='active'",
        )
        .await,
        2,
        "scheduler selection must not retry an auto-paused group"
    );

    let empty_run =
        engine::record_sync_success_for_test(&pool, "v083-synthetic-group", "local-device")
            .await
            .expect_err("an unresolved active quarantine is not a successful empty run");
    assert_eq!(empty_run.code(), "SYNC_GROUP_AUTO_PAUSED");
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_audits WHERE outcome='succeeded'",
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT retry_count FROM device_sync_quarantine WHERE status='active'",
        )
        .await,
        2,
        "empty run must not mutate retry_count"
    );

    super::queries::set_paused(&pool, "v083-synthetic-group", false)
        .await
        .unwrap();
    let operations = vec![cyclic_case_operation(), cyclic_contact_operation()];
    let mut tx = pool.begin().await.unwrap();
    apply_incoming_package(
        &mut tx,
        "v083-synthetic-group",
        "remote-device",
        1,
        &operations,
        "synthetic-resume-replay",
    )
    .await
    .unwrap();
    assert_eq!(
        engine::resolve_active_quarantine_for_test(
            &mut tx,
            "v083-synthetic-group",
            "remote-device",
            1,
            source_path,
        )
        .await
        .unwrap(),
        1
    );
    tx.commit().await.unwrap();
    engine::record_sync_success_for_test(&pool, "v083-synthetic-group", "local-device")
        .await
        .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine WHERE status='active'",
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE status='resolved' AND resolved_at IS NOT NULL",
        )
        .await,
        1,
        "resolved quarantine remains as history"
    );
    let success_times: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT last_success_at,last_synced_at FROM device_sync_groups
         WHERE id='v083-synthetic-group'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(success_times.0.is_some());
    assert!(success_times.1.is_some());
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_audits
             WHERE action='sync_once' AND outcome='succeeded'",
        )
        .await,
        1
    );
    assert_eq!(
        engine::get_status(&pool, "v083-synthetic-group")
            .await
            .unwrap()
            .quarantined,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA quick_check")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "ok"
    );
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap()
        .is_empty());
}

#[test]
fn device_sync_ui_contract_exposes_auto_pause_times_and_success_only_notice() {
    let component = include_str!("../../../src/components/settings/DeviceSyncSettingsCard.tsx");
    let types = include_str!("../../../src/lib/types.ts");
    for expected in [
        "已自动暂停",
        "pause_reason_code",
        "最近尝试",
        "最近成功",
        "活动隔离",
    ] {
        assert!(
            component.contains(expected),
            "missing UI contract: {expected}"
        );
    }
    for expected in [
        "auto_paused: boolean",
        "pause_reason_code: string | null",
        "last_attempt_at: string | null",
        "last_success_at: string | null",
    ] {
        assert!(
            types.contains(expected),
            "missing type contract: {expected}"
        );
    }
    let action_await = component.find("await action();").unwrap();
    let success_notice = component.find("setNotice(message);").unwrap();
    let catch_start = component[action_await..].find("} catch (e) {").unwrap() + action_await;
    let catch_end = component[catch_start..].find("} finally {").unwrap() + catch_start;
    assert!(action_await < success_notice && success_notice < catch_start);
    assert!(!component[catch_start..catch_end].contains("setNotice(message)"));
    assert!(component.contains("手动双向同步完成。"));
}

#[tokio::test]
async fn migration_0063_preserves_unidentified_legacy_rows_for_manual_review() {
    let pool = memory_pool().await;
    sqlx::raw_sql(
        r#"CREATE TABLE device_sync_groups (
             id TEXT PRIMARY KEY NOT NULL,
             last_synced_at TEXT
         );
         CREATE TABLE device_sync_quarantine (
             id TEXT PRIMARY KEY NOT NULL,
             group_id TEXT,
             source_path TEXT,
             reason_code TEXT NOT NULL,
             details_json TEXT NOT NULL DEFAULT '{}',
             created_at TEXT NOT NULL,
             FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE SET NULL
         );
         CREATE TABLE device_sync_outbox (
             operation_id TEXT PRIMARY KEY NOT NULL,
             group_id TEXT NOT NULL,
             author_device_id TEXT NOT NULL,
             logical_time INTEGER NOT NULL,
             state TEXT NOT NULL
         );
         INSERT INTO device_sync_groups(id,last_synced_at)
         VALUES('legacy-group','2026-08-01T00:00:00Z');
         INSERT INTO device_sync_outbox(
             operation_id,group_id,author_device_id,logical_time,state
         ) VALUES
         ('z-captured-first','legacy-group','legacy-device',1000,'pending'),
         ('a-captured-second','legacy-group','legacy-device',1000,'pending'),
         ('next-millisecond','legacy-group','legacy-device',1001,'pending');
         INSERT INTO device_sync_quarantine(
             id,group_id,source_path,reason_code,details_json,created_at
         ) VALUES
         ('legacy-a','legacy-group','D:/private/client/event.cbe','SYNC_DATABASE','{"error":"database path D:/private/client/case.db"}','2026-08-01T00:00:01Z'),
         ('legacy-b','legacy-group','\\server\secret\event.cbe','SYNC_DATABASE','{"payload":"client narrative"}','2026-08-01T00:00:02Z');"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(
        "../../migrations/0063_device_sync_quarantine_lifecycle.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine WHERE status='manual_review'",
        )
        .await,
        2
    );
    let legacy_safe: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT source_path,details_json FROM device_sync_quarantine
         WHERE status='manual_review' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(legacy_safe.iter().all(|(path, details)| {
        path.is_none()
            && details == "{\"legacy_record\":true,\"identity\":\"unknown\",\"sensitive_content\":\"redacted\"}"
            && !details.contains("private")
            && !details.contains("narrative")
    }));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE source_device_id='__legacy__' AND source_sequence=-1",
        )
        .await,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT sum(retry_count) FROM device_sync_quarantine WHERE status='manual_review'",
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    sqlx::query(
        "INSERT INTO device_sync_quarantine(
             id,group_id,source_path,source_device_id,source_sequence,
             reason_code,details_json,last_error_code
         ) VALUES('active-a','legacy-group','event.cbe','device-a',1,
                  'SYNC_DATABASE','{}','SYNC_DATABASE')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(sqlx::query(
        "INSERT INTO device_sync_quarantine(
             id,group_id,source_path,source_device_id,source_sequence,
             reason_code,details_json,last_error_code
         ) VALUES('active-duplicate','legacy-group','other-name.cbe','device-a',1,
                  'SYNC_DATABASE','{}','SYNC_DATABASE')",
    )
    .execute(&pool)
    .await
    .is_err());
    sqlx::query(
        "INSERT INTO device_sync_quarantine(
             id,group_id,source_path,source_device_id,source_sequence,
             reason_code,details_json,last_error_code
         ) VALUES('active-device-b','legacy-group','event.cbe','device-b',1,
                  'SYNC_DATABASE','{}','SYNC_DATABASE')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let normalized: Vec<(String, i64)> = sqlx::query_as(
        "SELECT operation_id,capture_sequence FROM device_sync_outbox
         ORDER BY capture_sequence",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        normalized,
        vec![
            ("a-captured-second".to_string(), 1),
            ("z-captured-first".to_string(), 2),
            ("next-millisecond".to_string(), 3),
        ],
        "legacy normalization freezes the exact old (logical_time, operation_id) planner order"
    );
    assert!(sqlx::query(
        "INSERT INTO device_sync_outbox(
             operation_id,group_id,author_device_id,logical_time,state,capture_sequence
         ) VALUES('duplicate-sequence','legacy-group','other-device',2000,'pending',3)",
    )
    .execute(&pool)
    .await
    .is_err());
}

#[tokio::test]
async fn manual_review_list_is_redacted_and_retain_archive_are_audited() {
    let pool = synthetic_pool().await;
    for id in ["manual-retain", "manual-archive"] {
        sqlx::query(
            "INSERT INTO device_sync_quarantine(
                 id,group_id,source_path,source_device_id,source_sequence,
                 reason_code,details_json,status,last_error_code
             ) VALUES(?1,'v083-synthetic-group',NULL,'__legacy__',-1,
                      'SYNC_DATABASE','{\"legacy_record\":true}',
                      'manual_review','SYNC_DATABASE')",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    let listed = super::queries::list_manual_reviews(&pool, "v083-synthetic-group")
        .await
        .unwrap();
    let serialized = serde_json::to_string(&listed).unwrap();
    assert_eq!(listed.len(), 2);
    for forbidden in ["source_path", "details_json", "legacy_record", "private"] {
        assert!(!serialized.contains(forbidden), "forbidden={forbidden}");
    }

    super::queries::review_manual_quarantine(
        &pool,
        "v083-synthetic-group",
        "manual-retain",
        "retain",
    )
    .await
    .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE id='manual-retain' AND status='manual_review'",
        )
        .await,
        1
    );
    super::queries::review_manual_quarantine(
        &pool,
        "v083-synthetic-group",
        "manual-archive",
        "archive",
    )
    .await
    .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_quarantine
             WHERE id='manual-archive' AND status='resolved' AND resolved_at IS NOT NULL",
        )
        .await,
        1
    );
    let audits: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT action,outcome,details_json FROM device_sync_audits
         WHERE action IN ('manual_review_retained','manual_review_archived') ORDER BY action",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(audits.len(), 2);
    assert!(audits.iter().all(|(_, outcome, details)| {
        outcome == "succeeded"
            && !details.contains("source_path")
            && !details.contains("details_json")
    }));
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

    for (entity_total, expected_sizes) in [
        (500_usize, vec![500_usize]),
        (501, vec![499, 2]),
        (1000, vec![500, 500]),
        (1001, vec![500, 499, 2]),
    ] {
        let mut operations = (0..entity_total - 2)
            .map(|index| {
                operation(
                    &format!("op-independent-{index:04}"),
                    "calendar_event",
                    &format!("calendar-{index:04}"),
                    None,
                    index as i64,
                    BTreeMap::new(),
                )
            })
            .collect::<Vec<_>>();
        let mut case = cyclic_case_operation();
        case.logical_time = (entity_total - 2) as i64;
        case.capture_sequence = (entity_total - 2) as i64;
        let mut contact = cyclic_contact_operation();
        contact.logical_time = (entity_total - 1) as i64;
        contact.capture_sequence = (entity_total - 1) as i64;
        operations.extend([case, contact]);

        let packages = engine::pack_operations_for_test(&operations, &[]).unwrap();
        let mut reversed = operations.clone();
        reversed.reverse();
        assert_eq!(
            packages,
            engine::pack_operations_for_test(&reversed, &[]).unwrap(),
            "packing must be deterministic across retries and query order"
        );
        assert_eq!(
            packages.iter().map(Vec::len).collect::<Vec<_>>(),
            expected_sizes,
            "total={entity_total}"
        );
        assert!(packages.iter().all(|package| package.len() <= limit));
        let dependency_package = packages
            .iter()
            .find(|package| package.iter().any(|id| id == "op-case-cyclic"))
            .unwrap();
        assert!(dependency_package.iter().any(|id| id == "op-contact-judge"));
    }
}

#[tokio::test]
async fn oversized_atomic_dependency_closure_fails_before_mounted_folder_writes() {
    let pool = synthetic_pool().await;
    for index in 0..=500 {
        let mut operation = operation(
            &format!("op-oversized-{index:04}"),
            "calendar_event",
            "one-atomic-entity",
            None,
            index,
            BTreeMap::new(),
        );
        operation.atomic_group = Some("oversized-atomic-group".to_string());
        insert_outbox(&pool, &operation, "pending", None).await;
    }
    let (root, folder) = temporary_mounted_folder("oversized");
    let before_events = count_files_with_extension(&root, "cbe");
    let before_manifests = count_files_with_extension(&root, "cbm");

    let error = engine::export_pending_for_test(&pool, &folder, "v083-synthetic-group")
        .await
        .expect_err("a 501-operation atomic closure must fail before credentials or NAS I/O");
    assert_eq!(error.code(), "SYNC_PACKAGE_TOO_LARGE");
    assert_eq!(count_files_with_extension(&root, "cbe"), before_events);
    assert_eq!(count_files_with_extension(&root, "cbm"), before_manifests);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE state='pending'",
        )
        .await,
        501
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM device_sync_outbox WHERE exported_sequence IS NOT NULL",
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT next_sequence FROM device_sync_groups WHERE id='v083-synthetic-group'",
        )
        .await,
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn export_dependency_requires_pending_or_strictly_earlier_outbox_proof() {
    let pool = synthetic_pool().await;
    let pending_case = case_with_judge(
        "op-case-needs-proof",
        "case-needs-proof",
        "contact-historical",
    );
    insert_outbox(&pool, &pending_case, "pending", None).await;
    let (root, folder) = temporary_mounted_folder("missing-proof");
    let error = engine::export_pending_for_test(&pool, &folder, "v083-synthetic-group")
        .await
        .expect_err("sender cannot silently assume the receiver already has the contact");
    assert_eq!(error.code(), "SYNC_PACKAGE_DEPENDENCY_MISSING");
    assert_eq!(count_files_with_extension(&root, "cbe"), 0);
    assert_eq!(count_files_with_extension(&root, "cbm"), 0);
    assert_eq!(
        count(
            &pool,
            "SELECT next_sequence FROM device_sync_groups WHERE id='v083-synthetic-group'",
        )
        .await,
        1
    );
    fs::remove_dir_all(root).unwrap();

    sqlx::query("UPDATE device_sync_groups SET next_sequence=2 WHERE id='v083-synthetic-group'")
        .execute(&pool)
        .await
        .unwrap();
    let historical_contact = contact_for_case(
        "op-contact-historical",
        "contact-historical",
        "case-needs-proof",
    );
    insert_outbox(&pool, &historical_contact, "exported", Some(2)).await;
    let error = engine::plan_pending_export_for_test(&pool, "v083-synthetic-group")
        .await
        .expect_err("same-sequence proof is not earlier than the next package");
    assert_eq!(error.code(), "SYNC_PACKAGE_DEPENDENCY_MISSING");

    sqlx::query(
        "UPDATE device_sync_outbox SET exported_sequence=3
         WHERE operation_id='op-contact-historical'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let error = engine::plan_pending_export_for_test(&pool, "v083-synthetic-group")
        .await
        .expect_err("future-sequence proof is invalid");
    assert_eq!(error.code(), "SYNC_PACKAGE_DEPENDENCY_MISSING");

    sqlx::query(
        "UPDATE device_sync_outbox SET state='acknowledged',exported_sequence=1
         WHERE operation_id='op-contact-historical'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let packages = engine::plan_pending_export_for_test(&pool, "v083-synthetic-group")
        .await
        .expect("strictly earlier acknowledged upsert proves receiver history");
    assert_eq!(packages, vec![vec!["op-case-needs-proof".to_string()]]);

    sqlx::query(
        "UPDATE device_sync_outbox SET state='exported'
         WHERE operation_id='op-contact-historical'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        engine::plan_pending_export_for_test(&pool, "v083-synthetic-group")
            .await
            .unwrap(),
        packages,
        "strictly earlier exported proof is accepted as well"
    );
}

#[tokio::test]
async fn historical_dependency_uses_last_authenticated_action_within_sequence() {
    for (first_action, second_action, should_pass) in [
        (OperationAction::Upsert, OperationAction::Tombstone, false),
        (OperationAction::Tombstone, OperationAction::Upsert, true),
    ] {
        let pool = synthetic_pool().await;
        sqlx::query(
            "UPDATE device_sync_groups SET next_sequence=2
             WHERE id='v083-synthetic-group'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let pending_case = case_with_judge(
            "op-case-historical-order",
            "case-historical-order",
            "contact-historical-order",
        );
        insert_outbox(&pool, &pending_case, "pending", None).await;
        let mut first = contact_for_case(
            "op-contact-history-first",
            "contact-historical-order",
            "case-historical-order",
        );
        first.logical_time = 10;
        first.capture_sequence = 10;
        first.action = first_action;
        if first.action == OperationAction::Tombstone {
            first.changed_fields.clear();
        }
        let mut second = contact_for_case(
            "op-contact-history-second",
            "contact-historical-order",
            "case-historical-order",
        );
        second.logical_time = 11;
        second.capture_sequence = 11;
        second.action = second_action;
        if second.action == OperationAction::Tombstone {
            second.changed_fields.clear();
        }
        insert_outbox(&pool, &first, "exported", Some(1)).await;
        insert_outbox(&pool, &second, "exported", Some(1)).await;

        let planned = engine::plan_pending_export_for_test(&pool, "v083-synthetic-group").await;
        if should_pass {
            assert_eq!(
                planned.unwrap(),
                vec![vec!["op-case-historical-order".to_string()]]
            );
        } else {
            assert_eq!(
                planned.unwrap_err().code(),
                "SYNC_PACKAGE_DEPENDENCY_MISSING"
            );
        }
    }
}

#[test]
fn pending_entity_action_order_is_atomic_at_the_500_boundary() {
    let build = |first_action: OperationAction, second_action: OperationAction| {
        let mut operations = (0..498)
            .map(|index| {
                operation(
                    &format!("op-action-filler-{index:04}"),
                    "calendar_event",
                    &format!("calendar-action-filler-{index:04}"),
                    None,
                    index,
                    BTreeMap::new(),
                )
            })
            .collect::<Vec<_>>();
        let mut case = case_with_judge(
            "op-action-case",
            "case-action-order",
            "contact-action-order",
        );
        case.logical_time = 498;
        case.capture_sequence = 498;
        let mut first = contact_for_case(
            "op-action-contact-first",
            "contact-action-order",
            "case-action-order",
        );
        first.logical_time = 499;
        first.capture_sequence = 499;
        first.action = first_action;
        if first.action == OperationAction::Tombstone {
            first.changed_fields.clear();
        }
        let mut second = contact_for_case(
            "op-action-contact-second",
            "contact-action-order",
            "case-action-order",
        );
        second.logical_time = 500;
        second.capture_sequence = 500;
        second.action = second_action;
        if second.action == OperationAction::Tombstone {
            second.changed_fields.clear();
        }
        operations.extend([case, first, second]);
        operations
    };

    let valid = build(OperationAction::Tombstone, OperationAction::Upsert);
    let packages = engine::pack_operations_for_test(&valid, &[]).unwrap();
    assert_eq!(
        packages.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![498, 3]
    );
    assert_eq!(
        &packages[1],
        &[
            "op-action-case".to_string(),
            "op-action-contact-first".to_string(),
            "op-action-contact-second".to_string(),
        ]
    );
    let mut reversed = valid.clone();
    reversed.reverse();
    assert_eq!(
        packages,
        engine::pack_operations_for_test(&reversed, &[]).unwrap()
    );

    let invalid = build(OperationAction::Upsert, OperationAction::Tombstone);
    assert_eq!(
        engine::pack_operations_for_test(&invalid, &[])
            .unwrap_err()
            .code(),
        "SYNC_PACKAGE_DEPENDENCY_CONFLICT"
    );
}

#[tokio::test]
async fn local_capture_sequence_preserves_same_tick_causality_when_ids_sort_backwards() {
    let pool = synthetic_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let first_id = super::operations::enqueue_operation(
        &mut tx,
        "v083-synthetic-group",
        "calendar_event",
        "calendar-capture-order",
        None,
        OperationAction::Upsert,
        serde_json::Map::from_iter([
            ("id".to_string(), json!("calendar-capture-order")),
            ("date".to_string(), json!("2099-04-01")),
            ("title".to_string(), json!("Capture order")),
        ]),
        &["id".to_string(), "date".to_string(), "title".to_string()],
    )
    .await
    .unwrap();
    let second_id = super::operations::enqueue_operation(
        &mut tx,
        "v083-synthetic-group",
        "calendar_event",
        "calendar-capture-order",
        None,
        OperationAction::Tombstone,
        serde_json::Map::new(),
        &[],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    sqlx::query(
        "UPDATE device_sync_outbox
         SET operation_id=CASE operation_id WHEN ?1 THEN 'z-captured-first'
                                           WHEN ?2 THEN 'a-captured-second' END,
             logical_time=1000
         WHERE operation_id IN (?1,?2)",
    )
    .bind(first_id)
    .bind(second_id)
    .execute(&pool)
    .await
    .unwrap();

    let captured: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT operation_id,logical_time,capture_sequence
         FROM device_sync_outbox ORDER BY capture_sequence",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(captured[0].0, "z-captured-first");
    assert_eq!(captured[1].0, "a-captured-second");
    assert_eq!(captured[0].1, captured[1].1);
    assert_eq!(captured[1].2, captured[0].2 + 1);
    assert_eq!(
        engine::plan_pending_export_for_test(&pool, "v083-synthetic-group")
            .await
            .unwrap(),
        vec![vec![
            "z-captured-first".to_string(),
            "a-captured-second".to_string(),
        ]],
        "operation_id lexical order must never reverse captured causality"
    );
}

#[tokio::test]
async fn concurrent_connections_allocate_unique_monotonic_capture_sequences() {
    let (database_path, pool) = synthetic_file_pool("capture-sequence-race").await;
    let enqueue = |entity_id: &'static str| {
        let pool = pool.clone();
        async move {
            let mut tx = pool.begin().await.unwrap();
            let operation_id = super::operations::enqueue_operation(
                &mut tx,
                "v083-synthetic-group",
                "calendar_event",
                entity_id,
                None,
                OperationAction::Upsert,
                serde_json::Map::from_iter([
                    ("id".to_string(), json!(entity_id)),
                    ("date".to_string(), json!("2099-05-01")),
                    ("title".to_string(), json!(entity_id)),
                ]),
                &["id".to_string(), "date".to_string(), "title".to_string()],
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            operation_id
        }
    };
    let (left, right) = tokio::join!(enqueue("capture-race-z"), enqueue("capture-race-a"));
    assert_ne!(left, right);
    let sequences: Vec<i64> = sqlx::query_scalar(
        "SELECT capture_sequence FROM device_sync_outbox ORDER BY capture_sequence",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(sequences, vec![1, 2]);
    pool.close().await;
    remove_test_database_with_retry(&database_path).await;
}

#[test]
fn atomic_groups_and_case_contact_edges_form_one_transitive_closure() {
    let mut operations = (0..498)
        .map(|index| {
            operation(
                &format!("op-filler-{index:04}"),
                "calendar_event",
                &format!("calendar-filler-{index:04}"),
                None,
                index,
                BTreeMap::new(),
            )
        })
        .collect::<Vec<_>>();
    let mut case_a_first = case_with_judge(
        "op-shared-case-a-first",
        "case-shared-export-a",
        "contact-shared-export",
    );
    case_a_first.logical_time = 498;
    case_a_first.capture_sequence = 498;
    case_a_first.atomic_group = Some("case-atomic-fields".to_string());
    let mut case_a_second = operation(
        "op-shared-case-a-second",
        "case",
        "case-shared-export-a",
        Some("case-shared-export-a"),
        499,
        BTreeMap::new(),
    );
    case_a_second.atomic_group = Some("case-atomic-fields".to_string());
    let mut case_b = case_with_judge(
        "op-shared-case-b",
        "case-shared-export-b",
        "contact-shared-export",
    );
    case_b.logical_time = 500;
    case_b.capture_sequence = 500;
    let mut shared_contact = contact_for_case(
        "op-shared-contact",
        "contact-shared-export",
        "case-shared-export-a",
    );
    shared_contact.logical_time = 501;
    shared_contact.capture_sequence = 501;
    let mut independent_case = case_with_judge(
        "op-independent-case-export",
        "case-independent-export",
        "contact-independent-export",
    );
    independent_case.logical_time = 502;
    independent_case.capture_sequence = 502;
    let mut independent_contact = contact_for_case(
        "op-independent-contact-export",
        "contact-independent-export",
        "case-independent-export",
    );
    independent_contact.logical_time = 503;
    independent_contact.capture_sequence = 503;
    operations.extend([
        case_a_first,
        case_a_second,
        case_b,
        shared_contact,
        independent_case,
        independent_contact,
    ]);

    let packages = engine::pack_operations_for_test(&operations, &[]).unwrap();
    assert_eq!(
        packages.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![498, 6]
    );
    for operation_id in [
        "op-shared-case-a-first",
        "op-shared-case-a-second",
        "op-shared-case-b",
        "op-shared-contact",
    ] {
        assert!(packages[1].iter().any(|id| id == operation_id));
    }
    assert!(packages[1]
        .iter()
        .any(|id| id == "op-independent-case-export"));
    assert!(packages[1]
        .iter()
        .any(|id| id == "op-independent-contact-export"));
}
