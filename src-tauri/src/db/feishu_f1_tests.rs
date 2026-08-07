use serde_json::json;
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{cases, feishu_sync};
use crate::feishu::FeishuRemoteCaseRecord;

async fn fixture_pool() -> SqlitePool {
    super::init_pool(":memory:").await.expect("fixture pool")
}

async fn insert_case(pool: &SqlitePool, id: &str, name: &str, case_no: &str) {
    sqlx::query(
        "INSERT INTO cases
         (id,name,case_type,source_folder,case_no,legal_domain,management_status)
         VALUES (?1,?2,'诉讼',?3,?4,'civil','active')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("C:/fixtures/{id}"))
    .bind(case_no)
    .execute(pool)
    .await
    .expect("insert case");
}

async fn insert_binding(
    pool: &SqlitePool,
    suffix: &str,
    case_id: &str,
    record_id: &str,
    with_inbox: bool,
) {
    sqlx::query(
        "INSERT INTO feishu_sync_links
         (id,entity_type,local_entity_id,app_token,table_id,record_id,link_source,status)
         VALUES (?1,'case',?2,'app','table',?3,'manual','active')",
    )
    .bind(format!("link-{suffix}"))
    .bind(case_id)
    .bind(record_id)
    .execute(pool)
    .await
    .expect("insert link");
    if with_inbox {
        sqlx::query(
            "INSERT INTO feishu_sync_inbox
             (id,app_token,table_id,record_id,display_name,status,bound_case_id,resolved_at)
             VALUES (?1,'app','table',?2,?3,'bound',?4,datetime('now'))",
        )
        .bind(format!("inbox-{suffix}"))
        .bind(record_id)
        .bind(format!("remote-{suffix}"))
        .bind(case_id)
        .execute(pool)
        .await
        .expect("insert inbox");
    }
}

async fn insert_pending_artifacts(pool: &SqlitePool, suffix: &str, case_id: &str) {
    let run_id = format!("run-{suffix}");
    let link_id = format!("link-{suffix}");
    sqlx::query("INSERT INTO feishu_sync_runs (id,mode,status) VALUES (?1,'pull','succeeded')")
        .bind(&run_id)
        .execute(pool)
        .await
        .expect("insert run");
    sqlx::query(
        "INSERT INTO feishu_sync_field_previews
         (id,run_id,link_id,field_key,field_label,local_value_json,feishu_value_json,classification,proposed_action)
         VALUES (?1,?2,?3,'display_name','案件名称','\"same\"','\"remote\"','needs_review','review')",
    )
    .bind(format!("field-{suffix}"))
    .bind(&run_id)
    .bind(&link_id)
    .execute(pool)
    .await
    .expect("insert field preview");
    sqlx::query(
        "INSERT INTO feishu_sync_entity_previews
         (id,run_id,link_id,entity_type,app_token,table_id,record_id,case_id,case_name,change_kind)
         VALUES (?1,?2,?3,'work_item','app','table',?4,?5,'fixture','create')",
    )
    .bind(format!("entity-{suffix}"))
    .bind(&run_id)
    .bind(&link_id)
    .bind(format!("entity-record-{suffix}"))
    .bind(case_id)
    .execute(pool)
    .await
    .expect("insert entity preview");
    sqlx::query(
        "INSERT INTO feishu_sync_conflicts
         (id,link_id,field_key,status) VALUES (?1,?2,'display_name','pending')",
    )
    .bind(format!("conflict-{suffix}"))
    .bind(&link_id)
    .execute(pool)
    .await
    .expect("insert conflict");
}

fn remote(record_id: &str, name: &str, case_no: &str) -> FeishuRemoteCaseRecord {
    FeishuRemoteCaseRecord {
        record_id: record_id.to_string(),
        fields: json!({
            "案件名称": name,
            "类型": "民事诉讼",
            "案号": case_no,
            "☑状态": "在办"
        }),
        last_modified_time: Some("1784518994000".into()),
    }
}

async fn lifecycle_fingerprint(pool: &SqlitePool, case_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT json_object(
            'case_count',(SELECT count(*) FROM cases WHERE id=?1),
            'links',(SELECT COALESCE(group_concat(id||':'||status,'|'),'') FROM feishu_sync_links WHERE local_entity_id=?1),
            'inboxes',(SELECT COALESCE(group_concat(id||':'||status||':'||COALESCE(bound_case_id,'NULL')||':'||auto_bind_suppressed,'|'),'') FROM feishu_sync_inbox),
            'fields',(SELECT COALESCE(group_concat(id||':'||review_status,'|'),'') FROM feishu_sync_field_previews),
            'entities',(SELECT COALESCE(group_concat(id||':'||review_status,'|'),'') FROM feishu_sync_entity_previews),
            'conflicts',(SELECT COALESCE(group_concat(id||':'||status,'|'),'') FROM feishu_sync_conflicts),
            'audits',(SELECT count(*) FROM feishu_sync_binding_audits)
         )",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await
    .expect("fingerprint")
}

#[tokio::test]
async fn ce1_delete_is_one_local_transaction_and_invalidates_every_artifact() {
    let pool = fixture_pool().await;
    insert_case(&pool, "case-a", "案件A", "A-001").await;
    insert_binding(&pool, "a", "case-a", "remote-a", true).await;
    insert_pending_artifacts(&pool, "a", "case-a").await;

    cases::delete_case(&pool, "case-a").await.expect("delete");

    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cases WHERE id='case-a'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM feishu_sync_links WHERE id='link-a'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "archived"
    );
    let inbox: (String, Option<String>, i64) = sqlx::query_as("SELECT status,bound_case_id,auto_bind_suppressed FROM feishu_sync_inbox WHERE id='inbox-a'").fetch_one(&pool).await.unwrap();
    assert_eq!(inbox, ("pending_binding".into(), None, 1));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT review_status FROM feishu_sync_field_previews WHERE id='field-a'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "superseded"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM feishu_sync_entity_previews WHERE id='entity-a'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM feishu_sync_conflicts WHERE id='conflict-a'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "dismissed"
    );
    let audit: (String, Option<String>) = sqlx::query_as(
        "SELECT action,previous_case_id FROM feishu_sync_binding_audits WHERE inbox_id='inbox-a'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit, ("unbind".into(), None));
}

#[tokio::test]
async fn ce2_orphan_is_partial_while_valid_remote_preview_commits() {
    let pool = fixture_pool().await;
    insert_case(&pool, "case-a", "案件A", "A-001").await;
    insert_binding(&pool, "orphan", "case-a", "remote-orphan", true).await;
    sqlx::query("DELETE FROM cases WHERE id='case-a'")
        .execute(&pool)
        .await
        .unwrap();
    insert_case(&pool, "case-b", "案件B", "B-001").await;
    insert_binding(&pool, "b", "case-b", "remote-b", true).await;
    let run_id = feishu_sync::start_pull_run(&pool).await.unwrap();

    feishu_sync::complete_pull_preview(
        &pool,
        &run_id,
        "app",
        "table",
        vec![
            remote("remote-orphan", "案件A", "A-001"),
            remote("remote-b", "案件B-远端", "B-001"),
        ],
    )
    .await
    .expect("partial pull must commit");

    let run: (String, Option<String>) =
        sqlx::query_as("SELECT status,error_code FROM feishu_sync_runs WHERE id=?1")
            .bind(&run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        run,
        ("partial".into(), Some("FEISHU_ORPHAN_BINDING".into()))
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM feishu_sync_links WHERE id='link-orphan'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "archived"
    );
    assert!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM feishu_sync_field_previews WHERE link_id='link-b'"
        )
        .fetch_one(&pool)
        .await
        .unwrap()
            > 0
    );
}

#[tokio::test]
async fn ce3_historical_orphan_unbinds_with_null_fk_and_clean_foreign_keys() {
    let pool = fixture_pool().await;
    insert_case(&pool, "case-a", "案件A", "A-001").await;
    insert_binding(&pool, "a", "case-a", "remote-a", true).await;
    insert_pending_artifacts(&pool, "a", "case-a").await;
    sqlx::query("DELETE FROM cases WHERE id='case-a'")
        .execute(&pool)
        .await
        .unwrap();

    feishu_sync::unbind_case(&pool, "link-a")
        .await
        .expect("orphan unbind");

    let audit: (String, Option<String>) = sqlx::query_as("SELECT action,previous_case_id FROM feishu_sync_binding_audits WHERE inbox_id='inbox-a' ORDER BY created_at DESC LIMIT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(audit, ("unbind".into(), None));
    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());
}

#[tokio::test]
async fn ce4_ce5_unbind_rebind_supersedes_old_authority_and_network_spy_stays_zero() {
    let pool = fixture_pool().await;
    insert_case(&pool, "case-a", "same", "A-001").await;
    insert_case(&pool, "case-b", "same", "B-001").await;
    insert_binding(&pool, "a", "case-a", "remote-a", true).await;
    insert_pending_artifacts(&pool, "a", "case-a").await;

    feishu_sync::unbind_case(&pool, "link-a").await.unwrap();
    feishu_sync::bind_case(&pool, "inbox-a", "case-b")
        .await
        .unwrap();

    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT review_status FROM feishu_sync_field_previews WHERE id='field-a'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "superseded"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT review_status FROM feishu_sync_entity_previews WHERE id='entity-a'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "superseded"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM feishu_sync_conflicts WHERE id='conflict-a'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "dismissed"
    );
    assert!(feishu_sync::get_field_resolution_plan(&pool, "field-a")
        .await
        .unwrap_err()
        .starts_with("FEISHU_REVIEW_ALREADY_RESOLVED"));
    assert!(feishu_sync::get_entity_resolution_plan(&pool, "entity-a")
        .await
        .unwrap_err()
        .starts_with("FEISHU_REVIEW_ALREADY_RESOLVED"));

    let preview = feishu_sync::get_preview(&pool).await.unwrap();
    assert!(preview.proposed_changes.is_empty());
    assert!(preview.entity_changes.is_empty());
    assert!(preview.conflicts.is_empty());

    let network_reads = AtomicUsize::new(0);
    let network_writes = AtomicUsize::new(0);
    let field_result =
        feishu_sync::run_authorized_field_network_action(&pool, "field-a", |_| async {
            network_reads.fetch_add(1, Ordering::Relaxed);
            network_writes.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .await;
    assert!(field_result
        .unwrap_err()
        .starts_with("FEISHU_REVIEW_ALREADY_RESOLVED"));
    let entity_result =
        feishu_sync::run_authorized_entity_network_action(&pool, "entity-a", false, |_| async {
            network_reads.fetch_add(1, Ordering::Relaxed);
            network_writes.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .await;
    assert!(entity_result
        .unwrap_err()
        .starts_with("FEISHU_REVIEW_ALREADY_RESOLVED"));
    assert_eq!(network_reads.load(Ordering::Relaxed), 0);
    assert_eq!(network_writes.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn ce7_active_orphan_is_actionable_but_pull_archived_orphan_is_not_bound() {
    let pool = fixture_pool().await;
    insert_case(&pool, "case-a", "案件A", "A-001").await;
    insert_binding(&pool, "orphan", "case-a", "remote-orphan", true).await;
    insert_pending_artifacts(&pool, "orphan", "case-a").await;
    sqlx::query("DELETE FROM cases WHERE id='case-a'")
        .execute(&pool)
        .await
        .unwrap();

    let active_preview = feishu_sync::get_preview(&pool).await.unwrap();
    assert_eq!(active_preview.bound_cases.len(), 1);
    let active_orphan = &active_preview.bound_cases[0];
    assert!(active_orphan.is_orphaned);
    assert_eq!(active_orphan.local_case_name, "本地案件已删除");
    assert_eq!(
        active_orphan.error_code.as_deref(),
        Some("FEISHU_ORPHAN_BINDING")
    );
    assert!(
        feishu_sync::authorize_field_network_action(&pool, "field-orphan")
            .await
            .unwrap_err()
            .starts_with("FEISHU_ORPHAN_BINDING")
    );
    assert!(active_preview.proposed_changes.is_empty());
    assert!(active_preview.entity_changes.is_empty());
    assert!(active_preview.conflicts.is_empty());

    let run_id = feishu_sync::start_pull_run(&pool).await.unwrap();
    feishu_sync::complete_pull_preview(
        &pool,
        &run_id,
        "app",
        "table",
        vec![remote("remote-orphan", "案件A", "A-001")],
    )
    .await
    .expect("orphan pull is partial, not failed");

    let archived_preview = feishu_sync::get_preview(&pool).await.unwrap();
    assert!(archived_preview.bound_cases.is_empty());
    assert_eq!(archived_preview.pending_cases.len(), 1);
    assert_eq!(archived_preview.pending_cases[0].record_id, "remote-orphan");
    assert!(archived_preview.proposed_changes.is_empty());
    assert!(archived_preview.entity_changes.is_empty());
    assert!(archived_preview.conflicts.is_empty());
    assert_eq!(archived_preview.recent_runs[0].status, "partial");
    assert_eq!(
        archived_preview.recent_runs[0].error_code.as_deref(),
        Some("FEISHU_ORPHAN_BINDING")
    );
    assert!(
        feishu_sync::authorize_field_network_action(&pool, "field-orphan")
            .await
            .unwrap_err()
            .starts_with("FEISHU_REVIEW_ALREADY_RESOLVED")
    );
}

#[tokio::test]
async fn ce5_resolution_errors_distinguish_missing_orphan_and_resolved_before_network() {
    let pool = fixture_pool().await;
    assert!(
        feishu_sync::authorize_field_network_action(&pool, "missing-field")
            .await
            .unwrap_err()
            .starts_with("FEISHU_REVIEW_NOT_FOUND")
    );
    assert!(
        feishu_sync::authorize_entity_network_action(&pool, "missing-entity", false)
            .await
            .unwrap_err()
            .starts_with("FEISHU_REVIEW_NOT_FOUND")
    );

    insert_case(&pool, "case-a", "案件A", "A-001").await;
    insert_binding(&pool, "a", "case-a", "remote-a", true).await;
    insert_pending_artifacts(&pool, "a", "case-a").await;
    let mut connection = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("DELETE FROM cases WHERE id='case-a'")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);

    let calls = AtomicUsize::new(0);
    let field_error =
        feishu_sync::run_authorized_field_network_action(&pool, "field-a", |_| async {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .await
        .unwrap_err();
    let entity_error =
        feishu_sync::run_authorized_entity_network_action(&pool, "entity-a", false, |_| async {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .await
        .unwrap_err();
    assert!(field_error.starts_with("FEISHU_ORPHAN_BINDING"));
    assert!(entity_error.starts_with("FEISHU_ORPHAN_BINDING"));
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    insert_case(&pool, "case-a", "案件A", "A-001").await;
    feishu_sync::unbind_case(&pool, "link-a").await.unwrap();
    assert!(
        feishu_sync::authorize_field_network_action(&pool, "field-a")
            .await
            .unwrap_err()
            .starts_with("FEISHU_REVIEW_ALREADY_RESOLVED")
    );
    assert!(
        feishu_sync::authorize_entity_network_action(&pool, "entity-a", false)
            .await
            .unwrap_err()
            .starts_with("FEISHU_REVIEW_ALREADY_RESOLVED")
    );
    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());
}

#[tokio::test]
async fn ce8_multiple_links_missing_inbox_rolls_back_without_half_cleanup() {
    let pool = fixture_pool().await;
    insert_case(&pool, "case-a", "案件A", "A-001").await;
    insert_binding(&pool, "a1", "case-a", "remote-a1", true).await;
    sqlx::query(
        "INSERT INTO feishu_sync_links
         (id,entity_type,local_entity_id,app_token,table_id,record_id,link_source,status)
         VALUES ('link-a2','case','case-a','app','table-2','remote-a2','manual','active')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let before = lifecycle_fingerprint(&pool, "case-a").await;

    let error = cases::delete_case(&pool, "case-a").await.unwrap_err();
    assert!(error.to_string().contains("FEISHU_BINDING_NOT_FOUND"));
    assert_eq!(lifecycle_fingerprint(&pool, "case-a").await, before);
}

#[tokio::test]
async fn ce6_delete_bind_unbind_and_rebind_make_zero_http_calls() {
    let _spy_guard = crate::feishu::f1_http_spy_test_guard().await;
    let pool = fixture_pool().await;
    insert_case(&pool, "case-a", "案件A", "A-001").await;
    insert_case(&pool, "case-b", "案件B", "B-001").await;
    insert_case(&pool, "case-delete", "待删除案件", "D-001").await;
    insert_binding(&pool, "a", "case-a", "remote-a", true).await;
    insert_binding(&pool, "delete", "case-delete", "remote-delete", true).await;

    crate::feishu::reset_f1_http_spy();
    feishu_sync::unbind_case(&pool, "link-a").await.unwrap();
    feishu_sync::bind_case(&pool, "inbox-a", "case-b")
        .await
        .unwrap();
    feishu_sync::unbind_case(&pool, "link-a").await.unwrap();
    feishu_sync::bind_case(&pool, "inbox-a", "case-a")
        .await
        .unwrap();
    cases::delete_case(&pool, "case-delete").await.unwrap();

    assert_eq!(crate::feishu::f1_http_spy_counts(), (0, 0));
}

#[tokio::test]
async fn r2_active_orphan_missing_inbox_recovers_but_normal_missing_inbox_fails_closed() {
    let _spy_guard = crate::feishu::f1_http_spy_test_guard().await;
    let pool = fixture_pool().await;
    insert_case(&pool, "case-normal", "正常案件", "N-001").await;
    insert_binding(&pool, "normal", "case-normal", "remote-normal", false).await;
    let normal_before = lifecycle_fingerprint(&pool, "case-normal").await;

    let normal_error = feishu_sync::unbind_case(&pool, "link-normal")
        .await
        .unwrap_err();
    assert!(normal_error.starts_with("FEISHU_BINDING_NOT_FOUND"));
    assert_eq!(
        lifecycle_fingerprint(&pool, "case-normal").await,
        normal_before
    );

    insert_case(&pool, "case-orphan", "孤立案件", "O-001").await;
    insert_binding(
        &pool,
        "orphan-missing",
        "case-orphan",
        "remote-orphan",
        false,
    )
    .await;
    insert_pending_artifacts(&pool, "orphan-missing", "case-orphan").await;
    sqlx::query("DELETE FROM cases WHERE id='case-orphan'")
        .execute(&pool)
        .await
        .unwrap();

    crate::feishu::reset_f1_http_spy();
    let preview = feishu_sync::get_preview(&pool).await.unwrap();
    let orphan = preview
        .bound_cases
        .iter()
        .find(|item| item.id == "link-orphan-missing")
        .expect("active orphan is visible to the UI action");
    assert!(orphan.is_orphaned);

    feishu_sync::unbind_case(&pool, "link-orphan-missing")
        .await
        .expect("the UI's only orphan action must recover locally");

    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM feishu_sync_links WHERE id='link-orphan-missing'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "archived"
    );
    let inbox: (String, Option<String>, i64) = sqlx::query_as(
        "SELECT status,bound_case_id,auto_bind_suppressed FROM feishu_sync_inbox
         WHERE app_token='app' AND table_id='table' AND record_id='remote-orphan'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(inbox, ("pending_binding".into(), None, 1));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT review_status FROM feishu_sync_field_previews
             WHERE id='field-orphan-missing'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "superseded"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM feishu_sync_conflicts WHERE id='conflict-orphan-missing'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "dismissed"
    );
    let audit: (String, Option<String>) = sqlx::query_as(
        "SELECT action,previous_case_id FROM feishu_sync_binding_audits
         WHERE action='unbind' ORDER BY created_at DESC,rowid DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit, ("unbind".into(), None));
    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());
    assert_eq!(crate::feishu::f1_http_spy_counts(), (0, 0));
    assert!(feishu_sync::get_preview(&pool)
        .await
        .unwrap()
        .bound_cases
        .iter()
        .all(|item| item.id != "link-orphan-missing"));
}
