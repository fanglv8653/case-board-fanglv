use std::time::{Duration, Instant};

use sqlx::SqlitePool;
use tokio::task::JoinHandle;

use super::engine;

/// Starts the single background scheduler used by the desktop runtime.
///
/// - first tick is immediate (application startup);
/// - a dirty business row becomes eligible after five seconds;
/// - every group is retried at least every 60 seconds, which also detects NAS
///   recovery without blocking local writes;
/// - `engine::sync_once` has a process-wide try-lock, so manual and scheduled
///   runs never write the mounted folder concurrently.
pub fn start(pool: SqlitePool) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_periodic = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        loop {
            interval.tick().await;
            let _ = super::pairing::expire_pairing_invites(&pool).await;
            let periodic_due = last_periodic.elapsed() >= Duration::from_secs(60);
            let dirty_ready: bool = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(
                     SELECT 1 FROM device_sync_dirty_entities
                     WHERE changed_at <= datetime('now','-5 seconds')
                 )",
            )
            .fetch_one(&pool)
            .await
            .map(|value| value != 0)
            .unwrap_or(false);
            if !periodic_due && !dirty_ready {
                continue;
            }
            let groups: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM device_sync_groups WHERE paused=0 ORDER BY created_at",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            for group_id in groups {
                match engine::sync_once(&pool, &group_id).await {
                    Ok(_)
                    | Err(super::SyncError::Busy)
                    | Err(super::SyncError::NasUnavailable(_)) => {}
                    Err(error) => {
                        eprintln!(
                            "[device-sync] scheduled group {} failed [{}]: {}",
                            group_id,
                            error.code(),
                            error
                        );
                    }
                }
            }
            if periodic_due {
                last_periodic = Instant::now();
            }
        }
    })
}
