//! Process-wide coordination for every writer that can change or rely on a Feishu binding.
//!
//! Lock order is fixed: device sync takes its run lock first and this lock second. Explicit
//! Feishu actions never take the device-sync run lock, so the order cannot form a cycle.

use std::future::Future;
use std::sync::OnceLock;

use tokio::sync::{Mutex, MutexGuard};

use super::SyncError;

static BINDING_LIFECYCLE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn try_acquire() -> Result<MutexGuard<'static, ()>, ()> {
    BINDING_LIFECYCLE_LOCK
        .get_or_init(|| Mutex::new(()))
        .try_lock()
        .map_err(|_| ())
}

pub(crate) fn try_acquire_explicit() -> Result<MutexGuard<'static, ()>, String> {
    try_acquire().map_err(|_| {
        "FEISHU_WRITE_IN_PROGRESS: 正在处理另一项飞书绑定生命周期操作，请稍后重试".to_string()
    })
}

pub(crate) async fn run_explicit_action<T, F, Fut>(action: F) -> Result<T, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let _guard = try_acquire_explicit()?;
    action().await
}

pub(crate) async fn run_device_sync_action<T, F, Fut>(action: F) -> Result<T, SyncError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, SyncError>>,
{
    let _guard = try_acquire().map_err(|_| SyncError::FeishuLifecycleBusy)?;
    action().await
}
