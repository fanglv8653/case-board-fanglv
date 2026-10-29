//! Graceful application exit without blocking the native event loop.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::Manager;

const RUNNING: u8 = 0;
const DRAINING: u8 = 1;
const READY: u8 = 2;

#[derive(Default)]
pub struct ShutdownCoordinator {
    phase: AtomicU8,
}

impl ShutdownCoordinator {
    fn begin(&self) -> bool {
        self.phase
            .compare_exchange(RUNNING, DRAINING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn ready(&self) -> bool {
        self.phase.load(Ordering::Acquire) == READY
    }

    fn allow_exit(&self) {
        self.phase.store(READY, Ordering::Release);
    }
}

async fn drain(pool: &SqlitePool) -> Result<(), String> {
    // Child::wait must not run in a window event callback either.
    tokio::task::spawn_blocking(crate::lifecycle::shutdown)
        .await
        .map_err(|error| error.to_string())?;
    crate::dlog!("[shutdown] local services stopped; draining database");
    // Marks the pool closed to new work, then waits for checked-out connections,
    // including in-flight writes. Do not replace this with a forced process exit.
    pool.close().await;
    crate::dlog!("[shutdown] database drained");
    Ok(())
}

pub fn on_exit_requested(app: &tauri::AppHandle, code: Option<i32>, api: tauri::ExitRequestApi) {
    let coordinator = Arc::clone(app.state::<Arc<ShutdownCoordinator>>().inner());
    if coordinator.ready() {
        return;
    }
    // Tauri ignores prevent_exit for its forced restart code. Keep a synchronous
    // barrier for that unpausable API instead of starting cleanup that it can
    // bypass. Normal window close and the existing updater (exit code 0) use
    // the asynchronous path below.
    if code == Some(tauri::RESTART_EXIT_CODE) {
        let pool = app.state::<SqlitePool>().inner().clone();
        if let Err(error) = tauri::async_runtime::block_on(drain(&pool)) {
            crate::dlog!("[shutdown] forced-restart cleanup failed: {error}");
        }
        coordinator.allow_exit();
        return;
    }
    api.prevent_exit();
    if !coordinator.begin() {
        return;
    }
    crate::dlog!("[shutdown] exit requested; starting asynchronous cleanup");
    let app = app.clone();
    let pool = app.state::<SqlitePool>().inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = drain(&pool).await {
            crate::dlog!("[shutdown] cleanup failed; exit remains blocked: {error}");
            return;
        }
        coordinator.allow_exit();
        crate::dlog!("[shutdown] cleanup complete; allowing exit");
        // Preserve explicit exit codes. This posts to the still-running
        // native event loop; the second ExitRequested is allowed through.
        app.exit(code.unwrap_or(0));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    use std::time::Duration;

    #[test]
    fn repeated_exit_requests_start_exactly_one_cleanup() {
        let coordinator = Arc::new(ShutdownCoordinator::default());
        let attempts: Vec<_> = (0..16)
            .map(|_| {
                let coordinator = Arc::clone(&coordinator);
                std::thread::spawn(move || coordinator.begin())
            })
            .collect();
        let started = attempts
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|v| *v)
            .count();
        assert_eq!(started, 1);
        assert!(!coordinator.ready());
        coordinator.allow_exit();
        assert!(coordinator.ready());
        assert!(!coordinator.begin());
    }

    #[tokio::test]
    async fn cleanup_keeps_runtime_live_until_an_in_flight_write_commits() {
        let temp = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(temp.path().join("shutdown.db"))
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options.clone())
            .await
            .unwrap();
        sqlx::query("CREATE TABLE saved (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        let mut write = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO saved VALUES ('must-survive-exit')")
            .execute(&mut *write)
            .await
            .unwrap();
        let closing_pool = pool.clone();
        let closing = tokio::spawn(async move { drain(&closing_pool).await.unwrap() });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !pool.is_closed() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            !closing.is_finished(),
            "exit must wait for the checked-out write"
        );
        assert!(
            pool.acquire().await.is_err(),
            "no new writes after draining starts"
        );
        // On the same runtime, work needed to finish the write remains runnable.
        write.commit().await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), closing)
            .await
            .unwrap()
            .unwrap();
        let reopened = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let value: String = sqlx::query_scalar("SELECT value FROM saved")
            .fetch_one(&reopened)
            .await
            .unwrap();
        assert_eq!(value, "must-survive-exit");
        reopened.close().await;
    }

    #[tokio::test]
    async fn cleanup_accepts_a_pool_already_closed_by_the_updater() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.close().await;
        tokio::time::timeout(Duration::from_secs(5), drain(&pool))
            .await
            .unwrap()
            .unwrap();
    }
}
