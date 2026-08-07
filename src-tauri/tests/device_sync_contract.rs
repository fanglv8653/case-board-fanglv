#![allow(dead_code)]

#[path = "../src/device_sync/mod.rs"]
mod device_sync;

use std::collections::BTreeMap;

use device_sync::capture::capture_dirty_entities;
use device_sync::operations::{apply_incoming, OperationAction, SyncOperation};
use serde_json::json;
use sha2::{Digest, Sha256};

fn value_hash(value: &serde_json::Value) -> String {
    Sha256::digest(serde_json::to_vec(value).unwrap())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn test_pool() -> sqlx::SqlitePool {
    let pool = caseboard_lib::db::init_pool(":memory:").await.unwrap();
    let migration_58: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=58")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(migration_58, 1, "完整迁移链必须真实执行 0058");
    let migration_59: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=59")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(migration_59, 1, "设备同步安全加固迁移必须真实执行 0059");
    sqlx::query(
        "INSERT INTO device_sync_groups (
             id, connector_root, local_device_id
         ) VALUES ('g1','D:\\nas','local')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO cases (
             id,name,cause,source_folder,legal_domain,domain_source,
             management_status,management_status_source
         ) VALUES ('c1','Local name',NULL,'D:\\case','civil','manual','active','manual')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn remote_operation(field: &str, value: serde_json::Value, base_hash: String) -> SyncOperation {
    SyncOperation {
        operation_id: uuid::Uuid::new_v4().to_string(),
        entity_type: "case".to_string(),
        entity_id: "c1".to_string(),
        case_id: Some("c1".to_string()),
        action: OperationAction::Upsert,
        base_revision: 1,
        changed_fields: BTreeMap::from([(field.to_string(), value)]),
        base_field_hashes: BTreeMap::from([(field.to_string(), base_hash)]),
        atomic_group: None,
        author_device_id: "remote".to_string(),
        logical_time: 2,
        capture_sequence: 2,
        schema_version: 1,
    }
}

#[tokio::test]
async fn different_field_remote_change_merges_and_duplicate_is_idempotent() {
    let pool = test_pool().await;
    let mut hashes = BTreeMap::new();
    hashes.insert("cause".to_string(), value_hash(&serde_json::Value::Null));
    sqlx::query(
        "INSERT INTO device_sync_entity_revisions (
             group_id,entity_type,entity_id,revision,field_hashes_json,updated_by_device_id
         ) VALUES ('g1','case','c1',2,?1,'local')",
    )
    .bind(serde_json::to_string(&hashes).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let operation = remote_operation("cause", json!("合同纠纷"), value_hash(&json!(null)));
    let mut tx = pool.begin().await.unwrap();
    let first = apply_incoming(&mut tx, "g1", "remote", 1, &operation, "payload")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(first.applied_fields, vec!["cause"]);
    assert!(first.conflict_fields.is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT cause FROM cases WHERE id='c1'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "合同纠纷"
    );

    let mut tx = pool.begin().await.unwrap();
    let duplicate = apply_incoming(&mut tx, "g1", "remote", 1, &operation, "payload")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(duplicate.duplicate);
}

#[tokio::test]
async fn concurrent_same_field_change_creates_conflict_without_overwrite() {
    let pool = test_pool().await;
    let old_hash = value_hash(&json!("Old name"));
    sqlx::query(
        "INSERT INTO device_sync_entity_revisions (
             group_id,entity_type,entity_id,revision,field_hashes_json,updated_by_device_id
         ) VALUES ('g1','case','c1',2,?1,'local')",
    )
    .bind(json!({"name": value_hash(&json!("Local name"))}).to_string())
    .execute(&pool)
    .await
    .unwrap();
    let operation = remote_operation("name", json!("Remote name"), old_hash);
    let mut tx = pool.begin().await.unwrap();
    let outcome = apply_incoming(&mut tx, "g1", "remote", 1, &operation, "payload")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(outcome.conflict_fields, vec!["name"]);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT name FROM cases WHERE id='c1'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "Local name"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM device_sync_conflicts WHERE status='pending'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn direct_case_income_and_feishu_writes_are_captured_but_excluded_tables_are_not() {
    let pool = test_pool().await;
    sqlx::query("UPDATE cases SET cause='买卖合同纠纷' WHERE id='c1'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO case_income_records (
             id,case_id,lawyer_fee_total,personal_share_amount,firm_deduction_amount,
             archive_holdback_amount,recognized_month,actual_income_amount
         ) VALUES ('income-1','c1',10000,10000,1500,500,'2026-07',8000)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO feishu_sync_links (
             id,entity_type,local_entity_id,app_token,table_id,record_id
         ) VALUES ('link-1','case','c1','app','table','record')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(capture_dirty_entities(&pool, "g1").await.unwrap(), 3);
    let types: Vec<String> =
        sqlx::query_scalar("SELECT entity_type FROM device_sync_outbox ORDER BY entity_type")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(types, vec!["case", "feishu_link", "income_record"]);
    assert!(!types.iter().any(|value| {
        value.contains("document") || value.contains("chat") || value.contains("memory")
    }));
    let triggered_tables: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT tbl_name FROM sqlite_master
         WHERE type='trigger' AND name LIKE 'device_sync_%'
         ORDER BY tbl_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for excluded in [
        "documents",
        "chat_messages",
        "case_memory_items",
        "case_memory_candidates",
        "material_processing_queue",
    ] {
        assert!(!triggered_tables.iter().any(|table| table == excluded));
    }
}

#[tokio::test]
async fn baseline_orders_parent_before_child_and_syncs_skill_suppressions() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO parties (id,case_id,role,name)
         VALUES ('party-1','c1','plaintiff','Party')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO legal_skill_binding_suppressions
         (id,legal_domain,task_type,reason)
         VALUES ('suppression-1','civil','analysis','user_unbound')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM device_sync_dirty_entities")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO device_sync_dirty_entities
         (entity_type,entity_id,case_id,action,changed_at) VALUES
         ('party','party-1','c1','upsert','2026-07-29T00:00:00Z'),
         ('case','c1','c1','upsert','2026-07-29T00:00:00Z'),
         ('legal_skill_binding_suppression','suppression-1',NULL,'upsert','2026-07-29T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(capture_dirty_entities(&pool, "g1").await.unwrap(), 3);
    let ordered: Vec<String> =
        sqlx::query_scalar("SELECT entity_type FROM device_sync_outbox ORDER BY logical_time")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        ordered,
        vec!["case", "party", "legal_skill_binding_suppression"]
    );
}

#[tokio::test]
async fn incoming_tombstone_removes_business_row_and_db_rejects_unknown_entity() {
    let pool = test_pool().await;
    let operation = SyncOperation {
        operation_id: uuid::Uuid::new_v4().to_string(),
        entity_type: "case".to_string(),
        entity_id: "c1".to_string(),
        case_id: Some("c1".to_string()),
        action: OperationAction::Tombstone,
        base_revision: 0,
        changed_fields: BTreeMap::new(),
        base_field_hashes: BTreeMap::new(),
        atomic_group: None,
        author_device_id: "remote".to_string(),
        logical_time: 1,
        capture_sequence: 1,
        schema_version: 1,
    };
    let mut tx = pool.begin().await.unwrap();
    let outcome = apply_incoming(&mut tx, "g1", "remote", 1, &operation, "payload")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(outcome.applied_fields, vec!["_tombstone"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cases WHERE id='c1'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT tombstoned FROM device_sync_entity_revisions
             WHERE group_id='g1' AND entity_type='case' AND entity_id='c1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert!(sqlx::query(
        "INSERT INTO device_sync_dirty_entities
             (entity_type,entity_id,action) VALUES ('unknown','x','upsert')"
    )
    .execute(&pool)
    .await
    .is_err());
}

#[tokio::test]
async fn invite_consumption_is_single_use_and_replay_safe() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO device_sync_invites (
             id,group_id,inviter_device_id,code_hash,expires_at
         ) VALUES ('invite-1','g1','local','hash','2099-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let first = sqlx::query(
        "UPDATE device_sync_invites
         SET status='consumed',consumed_by_device_id='remote'
         WHERE id='invite-1' AND status='active'",
    )
    .execute(&pool)
    .await
    .unwrap()
    .rows_affected();
    let replay = sqlx::query(
        "UPDATE device_sync_invites
         SET status='consumed',consumed_by_device_id='attacker'
         WHERE id='invite-1' AND status='active'",
    )
    .execute(&pool)
    .await
    .unwrap()
    .rows_affected();
    assert_eq!(first, 1);
    assert_eq!(replay, 0);
}

#[cfg(target_os = "windows")]
#[tokio::test]
async fn pairing_two_pool_sync_conflict_revocation_and_isolated_restore_contract() {
    let first = caseboard_lib::db::init_pool(":memory:").await.unwrap();
    let second = caseboard_lib::db::init_pool(":memory:").await.unwrap();
    let nas = tempfile::tempdir().unwrap();
    let offline = tempfile::tempdir().unwrap();
    let recovery_path = offline.path().join("offline-recovery.cbr");
    let created = device_sync::recovery::create_group_with_recovery(
        &first,
        nas.path(),
        "电脑 A",
        &recovery_path,
        "correct horse battery staple",
    )
    .await
    .unwrap();
    assert!(recovery_path.exists());
    let recovery_preview = device_sync::recovery::preview_recovery_package(
        &recovery_path,
        "correct horse battery staple",
    )
    .unwrap();
    assert!(recovery_preview.formal_database_unchanged);

    let invite = device_sync::pairing::create_pairing_invite(&first, &created.identity.group_id)
        .await
        .unwrap();
    let request = device_sync::pairing::create_join_request(
        nas.path(),
        &invite.group_id,
        &invite.invite_id,
        &invite.pairing_code,
        "电脑 B",
    )
    .unwrap();
    let wrong = device_sync::pairing::approve_join(
        &first,
        &invite.group_id,
        &invite.invite_id,
        "wrong-fingerprint",
    )
    .await
    .unwrap_err();
    assert_eq!(wrong.code(), "SYNC_INTEGRITY");
    let member = device_sync::pairing::approve_join(
        &first,
        &invite.group_id,
        &invite.invite_id,
        &request.fingerprint,
    )
    .await
    .unwrap();
    let replay = device_sync::pairing::approve_join(
        &first,
        &invite.group_id,
        &invite.invite_id,
        &request.fingerprint,
    )
    .await
    .unwrap_err();
    assert_eq!(replay.code(), "SYNC_PROTOCOL");
    device_sync::pairing::complete_join(&second, nas.path(), &invite.invite_id, &request)
        .await
        .unwrap();

    let expired_invite =
        device_sync::pairing::create_pairing_invite(&first, &created.identity.group_id)
            .await
            .unwrap();
    let expired_request = device_sync::pairing::create_join_request(
        nas.path(),
        &expired_invite.group_id,
        &expired_invite.invite_id,
        &expired_invite.pairing_code,
        "过期设备",
    )
    .unwrap();
    sqlx::query(
        "UPDATE device_sync_invites SET expires_at='2020-01-01T00:00:00Z'
         WHERE id=?1",
    )
    .bind(&expired_invite.invite_id)
    .execute(&first)
    .await
    .unwrap();
    let expired = device_sync::pairing::approve_join(
        &first,
        &expired_invite.group_id,
        &expired_invite.invite_id,
        &expired_request.fingerprint,
    )
    .await
    .unwrap_err();
    assert_eq!(expired.code(), "SYNC_PROTOCOL");
    assert_eq!(
        device_sync::pairing::expire_pairing_invites(&first)
            .await
            .unwrap(),
        1
    );

    sqlx::query(
        "INSERT INTO cases (
             id,name,case_type,cause,source_folder,legal_domain,domain_source
         ) VALUES ('paired-case','双机案件','诉讼','合同纠纷','D:\\case-a','civil','manual')",
    )
    .execute(&first)
    .await
    .unwrap();
    device_sync::engine::sync_once(&first, &invite.group_id)
        .await
        .unwrap();
    device_sync::engine::sync_once(&second, &invite.group_id)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT name FROM cases WHERE id='paired-case'")
            .fetch_one(&second)
            .await
            .unwrap(),
        "双机案件"
    );

    sqlx::query("UPDATE cases SET name='电脑 A 修改' WHERE id='paired-case'")
        .execute(&first)
        .await
        .unwrap();
    sqlx::query("UPDATE cases SET name='电脑 B 修改' WHERE id='paired-case'")
        .execute(&second)
        .await
        .unwrap();
    device_sync::engine::sync_once(&first, &invite.group_id)
        .await
        .unwrap();
    let second_run = device_sync::engine::sync_once(&second, &invite.group_id)
        .await
        .unwrap();
    assert!(second_run.conflicts_created >= 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT name FROM cases WHERE id='paired-case'")
            .fetch_one(&second)
            .await
            .unwrap(),
        "电脑 B 修改"
    );

    let snapshot_path: String = sqlx::query_scalar(
        "SELECT ?1 || '\\fanglv-caseboard-sync\\groups\\' || ?2 ||
                '\\snapshots\\' || encrypted_file_name
         FROM device_sync_snapshots
         WHERE group_id=?2 AND snapshot_kind='daily'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(nas.path().to_string_lossy().as_ref())
    .bind(&invite.group_id)
    .fetch_one(&first)
    .await
    .unwrap();
    let isolated_path = offline.path().join("restore-preview.sqlite");
    let isolated = device_sync::snapshot::prepare_isolated_restore(
        &first,
        &invite.group_id,
        std::path::Path::new(&snapshot_path),
        &isolated_path,
    )
    .await
    .unwrap();
    assert!(isolated.preview.formal_database_unchanged);
    assert!(isolated_path.exists());

    let new_epoch = device_sync::pairing::revoke_device(
        &first,
        &invite.group_id,
        &member.device_id,
        &member.fingerprint,
    )
    .await
    .unwrap();
    assert_eq!(new_epoch, 2);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM device_sync_members WHERE group_id=?1 AND device_id=?2"
        )
        .bind(&invite.group_id)
        .bind(&member.device_id)
        .fetch_one(&first)
        .await
        .unwrap(),
        "revoked"
    );

    device_sync::identity::delete_device_secrets(&invite.group_id, &created.identity.device_id, 2);
    device_sync::identity::delete_device_secrets(&invite.group_id, &request.device_id, 1);
    device_sync::identity::delete_device_secrets(&invite.group_id, &expired_request.device_id, 1);
}

#[test]
fn device_sync_core_has_no_feishu_network_client_path() {
    let sources = [
        include_str!("../src/device_sync/engine.rs"),
        include_str!("../src/device_sync/capture.rs"),
        include_str!("../src/device_sync/operations.rs"),
        include_str!("../src/device_sync/registry.rs"),
    ]
    .join("\n");
    for forbidden in ["reqwest", "feishu_oauth", "access_token", "https://"] {
        assert!(
            !sources.contains(forbidden),
            "device sync core must not invoke Feishu/network path: {forbidden}"
        );
    }
}
