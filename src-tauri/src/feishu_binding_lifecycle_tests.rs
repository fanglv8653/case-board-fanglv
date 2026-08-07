use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{oneshot, Barrier};

use crate::device_sync::feishu_binding_lifecycle::run_explicit_action;

async fn fixture_pool() -> sqlx::SqlitePool {
    crate::db::init_pool(":memory:")
        .await
        .expect("fixture pool")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r2_barriers_cross_explicit_device_sync_and_lifecycle_production_entries() {
    let pool = fixture_pool().await;
    sqlx::query(
        "INSERT INTO cases
         (id,name,case_type,source_folder,legal_domain,management_status)
         VALUES ('case-lock','lock fixture','诉讼','C:/fixtures/lock','civil','active')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let explicit_entered = Arc::new(Barrier::new(2));
    let explicit_network_calls = Arc::new(AtomicUsize::new(0));
    let (release_explicit, wait_for_release) = oneshot::channel::<()>();
    let explicit_task = {
        let explicit_entered = Arc::clone(&explicit_entered);
        let explicit_network_calls = Arc::clone(&explicit_network_calls);
        tokio::spawn(async move {
            run_explicit_action(|| async move {
                explicit_network_calls.fetch_add(1, Ordering::Relaxed);
                explicit_entered.wait().await;
                wait_for_release.await.expect("release explicit action");
                Ok(())
            })
            .await
        })
    };
    explicit_entered.wait().await;

    let sync_error = crate::device_sync::engine::sync_once(&pool, "missing-group")
        .await
        .unwrap_err();
    assert_eq!(sync_error.code(), "SYNC_FEISHU_LIFECYCLE_BUSY");
    assert_eq!(
        sync_error.public_message(),
        "飞书绑定正在变更，请稍后重试设备同步"
    );
    let conflict_error = crate::device_sync::operations::resolve_operation_conflicts(
        &pool,
        "missing-operation",
        crate::device_sync::operations::ConflictResolution::KeepRemote,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(conflict_error.code(), "SYNC_FEISHU_LIFECYCLE_BUSY");
    let lifecycle_calls = AtomicUsize::new(0);
    let lifecycle_error = run_explicit_action(|| async {
        lifecycle_calls.fetch_add(1, Ordering::Relaxed);
        crate::db::cases::delete_case(&pool, "case-lock").await
    })
    .await
    .unwrap_err();
    assert!(lifecycle_error.starts_with("FEISHU_WRITE_IN_PROGRESS"));
    assert_eq!(lifecycle_calls.load(Ordering::Relaxed), 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cases WHERE id='case-lock'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    release_explicit.send(()).unwrap();
    explicit_task.await.unwrap().unwrap();
    assert_eq!(explicit_network_calls.load(Ordering::Relaxed), 1);

    let sync_entered = Arc::new(Barrier::new(2));
    let (release_sync, wait_for_sync_release) = oneshot::channel::<()>();
    let sync_task = {
        let pool = pool.clone();
        let sync_entered = Arc::clone(&sync_entered);
        tokio::spawn(async move {
            crate::device_sync::engine::sync_once_with_entry_gate_for_test(
                &pool,
                "missing-group",
                || async move {
                    sync_entered.wait().await;
                    wait_for_sync_release.await.expect("release sync entry");
                },
            )
            .await
        })
    };
    sync_entered.wait().await;

    let blocked_network_calls = AtomicUsize::new(0);
    let explicit_error = run_explicit_action(|| async {
        blocked_network_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    })
    .await
    .unwrap_err();
    assert!(explicit_error.starts_with("FEISHU_WRITE_IN_PROGRESS"));
    assert_eq!(blocked_network_calls.load(Ordering::Relaxed), 0);

    release_sync.send(()).unwrap();
    let sync_after_release = sync_task.await.unwrap().unwrap_err();
    assert_eq!(sync_after_release.code(), "SYNC_NOT_FOUND");
}
