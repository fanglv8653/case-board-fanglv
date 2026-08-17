use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use uuid::Uuid;

const ATTEMPT_SCHEMA: u32 = 1;
const ATTEMPT_TTL_MINUTES: i64 = 30;
const NOTES_LIMIT: usize = 16 * 1024;
const HELPER_READY_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_ACK_TIMEOUT: Duration = Duration::from_secs(15);
const HELPER_TOTAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const UPDATE_ATTEMPT_ARG: &str = "--caseboard-update-attempt";

pub const UPD_CHECK_UNAVAILABLE: &str = "UPD_CHECK_UNAVAILABLE";
pub const UPD_METADATA_INVALID: &str = "UPD_METADATA_INVALID";
pub const UPD_DOWNLOAD_FAILED: &str = "UPD_DOWNLOAD_FAILED";
pub const UPD_SIGNATURE_INVALID: &str = "UPD_SIGNATURE_INVALID";
pub const UPD_ATTEMPT_PERSIST_FAILED: &str = "UPD_ATTEMPT_PERSIST_FAILED";
pub const UPD_SHUTDOWN_FAILED: &str = "UPD_SHUTDOWN_FAILED";
pub const UPD_SHUTDOWN_TIMEOUT: &str = "UPD_SHUTDOWN_TIMEOUT";
pub const UPD_SHUTDOWN_CHANNEL_CLOSED: &str = "UPD_SHUTDOWN_CHANNEL_CLOSED";
pub const UPD_INSTALL_PREPARE_FAILED: &str = "UPD_INSTALL_PREPARE_FAILED";
pub const UPD_INSTALL_LAUNCH_FAILED: &str = "UPD_INSTALL_LAUNCH_FAILED";
pub const UPD_INSTALL_EXIT_NONZERO: &str = "UPD_INSTALL_EXIT_NONZERO";
pub const UPD_TARGET_BINARY_INVALID: &str = "UPD_TARGET_BINARY_INVALID";
pub const UPD_RECEIPT_INVALID: &str = "UPD_RECEIPT_INVALID";
pub const UPD_RECEIPT_ACL_INVALID: &str = "UPD_RECEIPT_ACL_INVALID";
pub const UPD_RECEIPT_PERSIST_FAILED: &str = "UPD_RECEIPT_PERSIST_FAILED";
pub const UPD_RECEIPT_CONSUME_FAILED: &str = "UPD_RECEIPT_CONSUME_FAILED";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AttemptPhase {
    Prepared,
    ShutdownComplete,
    InstallerSucceeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptRecord {
    schema: u32,
    attempt_id: String,
    source_version: String,
    target_version: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    phase: AttemptPhase,
    installer_exit_code: Option<i32>,
    package_sha256: String,
    installed_exe_version: Option<String>,
    notes: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateLifecycleError {
    pub code: String,
    pub detail: Option<String>,
}

impl UpdateLifecycleError {
    fn new(code: &'static str) -> Self {
        Self {
            code: code.to_string(),
            detail: None,
        }
    }

    fn with_detail(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            detail: Some(redact_detail(detail.into())),
        }
    }
}

impl std::fmt::Display for UpdateLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.code)
    }
}

impl std::error::Error for UpdateLifecycleError {}

#[derive(Debug, Clone, Serialize)]
struct UpdateProgressEvent {
    downloaded: u64,
    total: u64,
    phase: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedUpdate {
    pub version: String,
    pub notes: Option<String>,
}

struct ShutdownRequest {
    attempt_path: PathBuf,
    response: mpsc::Sender<Result<(), UpdateLifecycleError>>,
}

pub struct UpdateShutdownCoordinator {
    sender: mpsc::Sender<ShutdownRequest>,
    quiescing: Arc<AtomicBool>,
}

impl UpdateShutdownCoordinator {
    pub fn start(pool: SqlitePool) -> Result<Self, UpdateLifecycleError> {
        let (sender, receiver) = mpsc::channel::<ShutdownRequest>();
        let quiescing = Arc::new(AtomicBool::new(false));
        std::thread::Builder::new()
            .name("caseboard-update-shutdown".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => return,
                };
                while let Ok(request) = receiver.recv() {
                    crate::lifecycle::shutdown();
                    runtime.block_on(pool.close());
                    let result = transition_attempt(
                        &request.attempt_path,
                        AttemptPhase::ShutdownComplete,
                        None,
                        None,
                        None,
                    )
                    .map_err(|error| {
                        UpdateLifecycleError::with_detail(UPD_SHUTDOWN_FAILED, error.code)
                    });
                    let _ = request.response.send(result);
                    break;
                }
            })
            .map_err(|error| {
                UpdateLifecycleError::with_detail(UPD_SHUTDOWN_CHANNEL_CLOSED, error.to_string())
            })?;
        Ok(Self { sender, quiescing })
    }

    fn begin(
        &self,
        attempt_path: PathBuf,
    ) -> Result<mpsc::Receiver<Result<(), UpdateLifecycleError>>, UpdateLifecycleError> {
        if self.quiescing.swap(true, Ordering::SeqCst) {
            return Err(UpdateLifecycleError::new(UPD_SHUTDOWN_FAILED));
        }
        let (response, receiver) = mpsc::channel();
        if self
            .sender
            .send(ShutdownRequest {
                attempt_path,
                response,
            })
            .is_err()
        {
            self.quiescing.store(false, Ordering::SeqCst);
            return Err(UpdateLifecycleError::new(UPD_SHUTDOWN_CHANNEL_CLOSED));
        }
        Ok(receiver)
    }
}

#[tauri::command]
pub async fn start_app_update(
    app: tauri::AppHandle,
    coordinator: tauri::State<'_, UpdateShutdownCoordinator>,
    expected_version: String,
    notes: Option<String>,
) -> Result<(), UpdateLifecycleError> {
    if !valid_version(&expected_version) {
        return Err(UpdateLifecycleError::new(UPD_METADATA_INVALID));
    }
    if notes
        .as_deref()
        .is_some_and(|value| value.len() > NOTES_LIMIT)
    {
        return Err(UpdateLifecycleError::new(UPD_METADATA_INVALID));
    }

    let updater = app.updater().map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_CHECK_UNAVAILABLE, error.to_string())
    })?;
    let update = updater
        .check()
        .await
        .map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_CHECK_UNAVAILABLE, error.to_string())
        })?
        .ok_or_else(|| UpdateLifecycleError::new(UPD_CHECK_UNAVAILABLE))?;
    if update.version != expected_version
        || update.current_version == update.version
        || !valid_version(&update.current_version)
    {
        return Err(UpdateLifecycleError::new(UPD_METADATA_INVALID));
    }

    let progress_app = app.clone();
    let downloaded = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let downloaded_for_chunk = Arc::clone(&downloaded);
    let bytes = update
        .download(
            move |chunk, total| {
                let current =
                    downloaded_for_chunk.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;
                let _ = progress_app.emit(
                    "app-update-progress",
                    UpdateProgressEvent {
                        downloaded: current,
                        total: total.unwrap_or(0),
                        phase: "downloading",
                    },
                );
            },
            || {},
        )
        .await
        .map_err(map_download_error)?;
    let final_size = downloaded.load(Ordering::Relaxed);
    let _ = app.emit(
        "app-update-progress",
        UpdateProgressEvent {
            downloaded: final_size,
            total: final_size,
            phase: "verified",
        },
    );

    let attempt_id = Uuid::new_v4().to_string();
    let state_dir = crate::db::app_data_dir()
        .map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_ATTEMPT_PERSIST_FAILED, error.to_string())
        })?
        .join("update")
        .join("attempts");
    ensure_secure_dir(&state_dir)?;
    let attempt_dir = state_dir.join(&attempt_id);
    ensure_secure_dir(&attempt_dir)?;

    let installer_path = attempt_dir.join("installer.exe");
    write_new_secure(&installer_path, &bytes, UPD_INSTALL_PREPARE_FAILED)?;
    let package_sha256 = sha256_bytes(&bytes);
    let current_exe = std::env::current_exe().map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_INSTALL_PREPARE_FAILED, error.to_string())
    })?;
    let helper_source = helper_binary_path(&current_exe)?;
    let helper_path = attempt_dir.join(helper_filename());
    copy_secure(&helper_source, &helper_path)?;
    if sha256_file(&helper_source)? != sha256_file(&helper_path)? {
        return Err(UpdateLifecycleError::new(UPD_INSTALL_PREPARE_FAILED));
    }

    let now = Utc::now();
    let attempt = AttemptRecord {
        schema: ATTEMPT_SCHEMA,
        attempt_id: attempt_id.clone(),
        source_version: update.current_version,
        target_version: update.version,
        created_at: now,
        expires_at: now + chrono::Duration::minutes(ATTEMPT_TTL_MINUTES),
        phase: AttemptPhase::Prepared,
        installer_exit_code: None,
        package_sha256,
        installed_exe_version: None,
        notes,
        error_code: None,
    };
    let attempt_path = attempt_file(&state_dir, &attempt_id);
    write_attempt(&attempt_path, &attempt, UPD_ATTEMPT_PERSIST_FAILED)?;

    let ready_path = attempt_dir.join("helper.ready");
    Command::new(&helper_path)
        .arg("--attempt-id")
        .arg(&attempt_id)
        .arg("--state-dir")
        .arg(&state_dir)
        .arg("--installer")
        .arg(&installer_path)
        .arg("--old-pid")
        .arg(std::process::id().to_string())
        .arg("--target-exe")
        .arg(&current_exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_INSTALL_PREPARE_FAILED, error.to_string())
        })?;

    wait_for_path_async(&ready_path, HELPER_READY_TIMEOUT).await?;
    let receiver = coordinator.begin(attempt_path.clone())?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_enabled(false);
    }
    let received =
        tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(SHUTDOWN_ACK_TIMEOUT))
            .await
            .map_err(|_| UpdateLifecycleError::new(UPD_SHUTDOWN_FAILED))?;

    match received {
        Ok(Ok(())) => {
            app.exit(0);
            Ok(())
        }
        Ok(Err(error)) => Err(error),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            spawn_late_shutdown_watcher(app, attempt_path);
            Err(UpdateLifecycleError::new(UPD_SHUTDOWN_TIMEOUT))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            spawn_late_shutdown_watcher(app, attempt_path);
            Err(UpdateLifecycleError::new(UPD_SHUTDOWN_CHANNEL_CLOSED))
        }
    }
}

fn map_download_error(error: tauri_plugin_updater::Error) -> UpdateLifecycleError {
    let code = match &error {
        tauri_plugin_updater::Error::Minisign(_)
        | tauri_plugin_updater::Error::Base64(_)
        | tauri_plugin_updater::Error::SignatureUtf8(_) => UPD_SIGNATURE_INVALID,
        _ => UPD_DOWNLOAD_FAILED,
    };
    UpdateLifecycleError::with_detail(code, error.to_string())
}

#[tauri::command]
pub fn claim_app_update_success() -> Result<Option<ClaimedUpdate>, UpdateLifecycleError> {
    let Some(attempt_id) = startup_attempt_id() else {
        return Ok(None);
    };
    claim_attempt(&attempt_id).map(Some)
}

fn spawn_late_shutdown_watcher(app: tauri::AppHandle, attempt_path: PathBuf) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + HELPER_TOTAL_TIMEOUT;
        while Instant::now() < deadline {
            if read_attempt(&attempt_path)
                .is_ok_and(|attempt| attempt.phase == AttemptPhase::ShutdownComplete)
            {
                app.exit(0);
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

async fn wait_for_path_async(path: &Path, timeout: Duration) -> Result<(), UpdateLifecycleError> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.is_file() {
            validate_secure_path(path)?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(UpdateLifecycleError::new(UPD_INSTALL_PREPARE_FAILED))
}

fn startup_attempt_id() -> Option<String> {
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg == UPDATE_ATTEMPT_ARG {
            return args.next().filter(|value| valid_attempt_id(value));
        }
    }
    None
}

static CLAIM_LOCK: Mutex<()> = Mutex::new(());

fn claim_attempt(attempt_id: &str) -> Result<ClaimedUpdate, UpdateLifecycleError> {
    let _guard = CLAIM_LOCK
        .lock()
        .map_err(|_| UpdateLifecycleError::new(UPD_RECEIPT_CONSUME_FAILED))?;
    let state_dir = crate::db::app_data_dir()
        .map_err(|error| UpdateLifecycleError::with_detail(UPD_RECEIPT_INVALID, error.to_string()))?
        .join("update")
        .join("attempts");
    validate_secure_path(&state_dir)?;
    let pending = attempt_file(&state_dir, attempt_id);
    let attempt = read_attempt(&pending)?;
    let current = env!("CARGO_PKG_VERSION");
    if attempt.schema != ATTEMPT_SCHEMA
        || attempt.attempt_id != attempt_id
        || attempt.phase != AttemptPhase::InstallerSucceeded
        || attempt.installer_exit_code != Some(0)
        || attempt.installed_exe_version.as_deref() != Some(attempt.target_version.as_str())
        || attempt.target_version != current
        || attempt.source_version == current
        || Utc::now() > attempt.expires_at
    {
        return Err(UpdateLifecycleError::new(UPD_RECEIPT_INVALID));
    }
    let consumed = state_dir.join(format!("update-attempt-v1.consumed-{attempt_id}.json"));
    rename_no_replace(&pending, &consumed).map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_RECEIPT_CONSUME_FAILED, error.to_string())
    })?;
    validate_secure_path(&consumed)?;
    Ok(ClaimedUpdate {
        version: attempt.target_version,
        notes: attempt.notes,
    })
}

fn attempt_file(state_dir: &Path, attempt_id: &str) -> PathBuf {
    state_dir.join(format!("update-attempt-v1-{attempt_id}.json"))
}

fn valid_attempt_id(value: &str) -> bool {
    Uuid::parse_str(value)
        .is_ok_and(|parsed| parsed.hyphenated().to_string() == value.to_ascii_lowercase())
}

fn valid_version(value: &str) -> bool {
    let pieces: Vec<&str> = value.split('.').collect();
    pieces.len() == 3
        && pieces.iter().all(|piece| {
            !piece.is_empty()
                && piece.chars().all(|ch| ch.is_ascii_digit())
                && (piece == &"0" || !piece.starts_with('0'))
        })
}

fn redact_detail(value: String) -> String {
    let value = value.replace(['\r', '\n'], " ");
    value.chars().take(240).collect()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String, UpdateLifecycleError> {
    let bytes = fs::read(path).map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_INSTALL_PREPARE_FAILED, error.to_string())
    })?;
    Ok(sha256_bytes(&bytes))
}

fn read_attempt(path: &Path) -> Result<AttemptRecord, UpdateLifecycleError> {
    validate_secure_path(path)?;
    let bytes = fs::read(path).map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_RECEIPT_INVALID, error.to_string())
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|error| UpdateLifecycleError::with_detail(UPD_RECEIPT_INVALID, error.to_string()))
}

fn transition_attempt(
    path: &Path,
    phase: AttemptPhase,
    exit_code: Option<i32>,
    installed_version: Option<String>,
    error_code: Option<String>,
) -> Result<(), UpdateLifecycleError> {
    let mut attempt = read_attempt(path)?;
    let allowed = matches!(
        (&attempt.phase, &phase),
        (AttemptPhase::Prepared, AttemptPhase::ShutdownComplete)
            | (AttemptPhase::Prepared, AttemptPhase::Failed)
            | (
                AttemptPhase::ShutdownComplete,
                AttemptPhase::InstallerSucceeded
            )
            | (AttemptPhase::ShutdownComplete, AttemptPhase::Failed)
    );
    if !allowed {
        return Err(UpdateLifecycleError::new(UPD_RECEIPT_INVALID));
    }
    attempt.phase = phase;
    attempt.installer_exit_code = exit_code;
    attempt.installed_exe_version = installed_version;
    attempt.error_code = error_code;
    write_attempt(path, &attempt, UPD_RECEIPT_PERSIST_FAILED)
}

fn write_attempt(
    path: &Path,
    attempt: &AttemptRecord,
    code: &'static str,
) -> Result<(), UpdateLifecycleError> {
    if !valid_attempt_id(&attempt.attempt_id)
        || !valid_version(&attempt.source_version)
        || !valid_version(&attempt.target_version)
        || attempt.source_version == attempt.target_version
        || attempt
            .notes
            .as_deref()
            .is_some_and(|value| value.len() > NOTES_LIMIT)
    {
        return Err(UpdateLifecycleError::new(UPD_METADATA_INVALID));
    }
    let bytes = serde_json::to_vec(attempt)
        .map_err(|error| UpdateLifecycleError::with_detail(code, error.to_string()))?;
    atomic_replace_secure(path, &bytes, code)
}

fn write_new_secure(
    path: &Path,
    bytes: &[u8],
    code: &'static str,
) -> Result<(), UpdateLifecycleError> {
    if path.exists() {
        return Err(UpdateLifecycleError::new(code));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| UpdateLifecycleError::with_detail(code, error.to_string()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| UpdateLifecycleError::with_detail(code, error.to_string()))?;
    secure_path(path)?;
    validate_secure_path(path)
}

fn atomic_replace_secure(
    path: &Path,
    bytes: &[u8],
    code: &'static str,
) -> Result<(), UpdateLifecycleError> {
    let parent = path
        .parent()
        .ok_or_else(|| UpdateLifecycleError::new(code))?;
    ensure_secure_dir(parent)?;
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    write_new_secure(&temporary, bytes, code)?;
    rename_replace(&temporary, path)
        .map_err(|error| UpdateLifecycleError::with_detail(code, error.to_string()))?;
    validate_secure_path(path)
}

fn copy_secure(source: &Path, target: &Path) -> Result<(), UpdateLifecycleError> {
    if target.exists() {
        return Err(UpdateLifecycleError::new(UPD_INSTALL_PREPARE_FAILED));
    }
    fs::copy(source, target).map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_INSTALL_PREPARE_FAILED, error.to_string())
    })?;
    secure_path(target)?;
    validate_secure_path(target)
}

fn ensure_secure_dir(path: &Path) -> Result<(), UpdateLifecycleError> {
    if path.exists() && !path.is_dir() {
        return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
    }
    fs::create_dir_all(path).map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_ATTEMPT_PERSIST_FAILED, error.to_string())
    })?;
    secure_path(path)?;
    validate_secure_path(path)
}

fn helper_filename() -> &'static str {
    if cfg!(windows) {
        "caseboard-updater-helper.exe"
    } else {
        "caseboard-updater-helper"
    }
}

fn helper_binary_path(current_exe: &Path) -> Result<PathBuf, UpdateLifecycleError> {
    let path = current_exe
        .parent()
        .map(|parent| parent.join(helper_filename()))
        .ok_or_else(|| UpdateLifecycleError::new(UPD_INSTALL_PREPARE_FAILED))?;
    if path.is_file() {
        Ok(path)
    } else {
        Err(UpdateLifecycleError::new(UPD_INSTALL_PREPARE_FAILED))
    }
}

#[cfg(windows)]
fn rename_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MOVE_FILE_FLAGS,
    };
    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = MOVE_FILE_FLAGS(MOVEFILE_REPLACE_EXISTING.0 | MOVEFILE_WRITE_THROUGH.0);
    unsafe {
        MoveFileExW(
            PCWSTR(source_wide.as_ptr()),
            PCWSTR(target_wide.as_ptr()),
            flags,
        )
    }
    .map_err(std::io::Error::other)
}

#[cfg(not(windows))]
fn rename_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

fn rename_no_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    if target.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "claim target exists",
        ));
    }
    fs::rename(source, target)
}

#[cfg(windows)]
fn secure_path(path: &Path) -> Result<(), UpdateLifecycleError> {
    let sid = windows_acl::current_user_sid_string()?;
    let grant = if path.is_dir() {
        format!("*{sid}:(OI)(CI)F")
    } else {
        format!("*{sid}:F")
    };
    let status = Command::new("icacls.exe")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(grant)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_RECEIPT_ACL_INVALID, error.to_string())
        })?;
    if !status.success() {
        return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
    }
    Ok(())
}

#[cfg(not(windows))]
fn secure_path(path: &Path) -> Result<(), UpdateLifecycleError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if path.is_dir() { 0o700 } else { 0o600 }),
    )
    .map_err(|error| UpdateLifecycleError::with_detail(UPD_RECEIPT_ACL_INVALID, error.to_string()))
}

fn validate_secure_path(path: &Path) -> Result<(), UpdateLifecycleError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        UpdateLifecycleError::with_detail(UPD_RECEIPT_ACL_INVALID, error.to_string())
    })?;
    if metadata.file_type().is_symlink() {
        return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
        windows_acl::validate(path)?;
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
    }
    Ok(())
}

#[cfg(windows)]
mod windows_acl {
    use super::{UpdateLifecycleError, UPD_RECEIPT_ACL_INVALID};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL, WIN32_ERROR};
    use windows::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{
        AclSizeInformation, EqualSid, GetAce, GetAclInformation, GetSecurityDescriptorControl,
        TokenUser, ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    struct LocalDescriptor(PSECURITY_DESCRIPTOR);
    impl Drop for LocalDescriptor {
        fn drop(&mut self) {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0 .0)));
            }
        }
    }

    fn current_user_sid_buffer() -> Result<Vec<u8>, UpdateLifecycleError> {
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.map_err(
            |error| UpdateLifecycleError::with_detail(UPD_RECEIPT_ACL_INVALID, error.to_string()),
        )?;
        let token = Handle(token);
        let mut needed = 0u32;
        let _ = unsafe {
            windows::Win32::Security::GetTokenInformation(token.0, TokenUser, None, 0, &mut needed)
        };
        if needed < size_of::<TOKEN_USER>() as u32 {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
        let mut buffer = vec![0u8; needed as usize];
        unsafe {
            windows::Win32::Security::GetTokenInformation(
                token.0,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
        }
        .map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_RECEIPT_ACL_INVALID, error.to_string())
        })?;
        Ok(buffer)
    }

    fn sid_from_buffer(buffer: &[u8]) -> PSID {
        let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
        token_user.User.Sid
    }

    pub(super) fn current_user_sid_string() -> Result<String, UpdateLifecycleError> {
        let buffer = current_user_sid_buffer()?;
        let sid = sid_from_buffer(&buffer);
        let mut value = PWSTR::null();
        unsafe { ConvertSidToStringSidW(sid, &mut value) }.map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_RECEIPT_ACL_INVALID, error.to_string())
        })?;
        let text = unsafe { value.to_string() }.map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_RECEIPT_ACL_INVALID, error.to_string())
        })?;
        unsafe {
            let _ = LocalFree(Some(HLOCAL(value.0.cast())));
        }
        Ok(text)
    }

    pub(super) fn validate(path: &Path) -> Result<(), UpdateLifecycleError> {
        let sid_buffer = current_user_sid_buffer()?;
        let current_sid = sid_from_buffer(&sid_buffer);
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut owner = PSID::default();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let info = windows::Win32::Security::OBJECT_SECURITY_INFORMATION(
            OWNER_SECURITY_INFORMATION.0 | DACL_SECURITY_INFORMATION.0,
        );
        let status: WIN32_ERROR = unsafe {
            GetNamedSecurityInfoW(
                PCWSTR(wide.as_ptr()),
                SE_FILE_OBJECT,
                info,
                Some(&mut owner),
                None,
                Some(&mut dacl),
                None,
                &mut descriptor,
            )
        };
        if status.0 != 0 || descriptor.is_invalid() || dacl.is_null() {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
        let descriptor_guard = LocalDescriptor(descriptor);
        unsafe { EqualSid(owner, current_sid) }
            .map_err(|_| UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID))?;
        let mut control = 0u16;
        let mut revision = 0u32;
        unsafe { GetSecurityDescriptorControl(descriptor_guard.0, &mut control, &mut revision) }
            .map_err(|_| UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID))?;
        if control & SE_DACL_PROTECTED.0 == 0 {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
        let mut size = ACL_SIZE_INFORMATION::default();
        unsafe {
            GetAclInformation(
                dacl,
                (&mut size as *mut ACL_SIZE_INFORMATION).cast::<c_void>(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        }
        .map_err(|_| UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID))?;
        if size.AceCount != 1 {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
        let mut ace_pointer: *mut c_void = std::ptr::null_mut();
        unsafe { GetAce(dacl, 0, &mut ace_pointer) }
            .map_err(|_| UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID))?;
        let ace = unsafe { &*(ace_pointer.cast::<ACCESS_ALLOWED_ACE>()) };
        if ace.Header.AceType as u32 != ACCESS_ALLOWED_ACE_TYPE {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
        if ace.Mask != windows::Win32::Storage::FileSystem::FILE_ALL_ACCESS.0 {
            return Err(UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID));
        }
        let ace_sid = PSID((&ace.SidStart as *const u32).cast_mut().cast());
        unsafe { EqualSid(ace_sid, current_sid) }
            .map_err(|_| UpdateLifecycleError::new(UPD_RECEIPT_ACL_INVALID))?;
        Ok(())
    }
}

#[derive(Debug)]
struct HelperArgs {
    attempt_id: String,
    state_dir: PathBuf,
    installer: PathBuf,
    old_pid: u32,
    target_exe: PathBuf,
}

pub fn helper_main() -> i32 {
    let parsed = parse_helper_args(
        std::env::args()
            .skip(1)
            .filter(|argument| argument != "--caseboard-updater-helper"),
    );
    let result = match parsed {
        Ok(args) => {
            let attempt_path = attempt_file(&args.state_dir, &args.attempt_id);
            let result = run_helper(args);
            if let Err(error) = &result {
                // 所有 helper 失败（含屏障超时、启动安装器失败）都尽力落盘，避免留下
                // 看似仍可继续的 prepared/shutdown_complete 状态。已成功或已 failed 时
                // 状态机会拒绝二次转换，且这里不会覆盖原始错误。
                let _ = transition_attempt(
                    &attempt_path,
                    AttemptPhase::Failed,
                    None,
                    None,
                    Some(error.code.clone()),
                );
            }
            result
        }
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{}", error.code);
            1
        }
    }
}

fn parse_helper_args(
    args: impl Iterator<Item = String>,
) -> Result<HelperArgs, UpdateLifecycleError> {
    let values: Vec<String> = args.collect();
    if values.len() != 10 {
        return Err(UpdateLifecycleError::new(UPD_METADATA_INVALID));
    }
    let value = |name: &str| -> Result<String, UpdateLifecycleError> {
        values
            .chunks_exact(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].clone())
            .ok_or_else(|| UpdateLifecycleError::new(UPD_METADATA_INVALID))
    };
    let attempt_id = value("--attempt-id")?;
    if !valid_attempt_id(&attempt_id) {
        return Err(UpdateLifecycleError::new(UPD_METADATA_INVALID));
    }
    Ok(HelperArgs {
        attempt_id,
        state_dir: PathBuf::from(value("--state-dir")?),
        installer: PathBuf::from(value("--installer")?),
        old_pid: value("--old-pid")?
            .parse()
            .map_err(|_| UpdateLifecycleError::new(UPD_METADATA_INVALID))?,
        target_exe: PathBuf::from(value("--target-exe")?),
    })
}

fn run_helper(args: HelperArgs) -> Result<(), UpdateLifecycleError> {
    validate_secure_path(&args.state_dir)?;
    let attempt_path = attempt_file(&args.state_dir, &args.attempt_id);
    let attempt_dir = args.state_dir.join(&args.attempt_id);
    if args.installer.parent() != Some(attempt_dir.as_path()) {
        return Err(UpdateLifecycleError::new(UPD_METADATA_INVALID));
    }
    validate_secure_path(&args.installer)?;
    let ready = attempt_dir.join("helper.ready");
    write_new_secure(&ready, b"ready", UPD_RECEIPT_PERSIST_FAILED)?;

    let deadline = Instant::now() + HELPER_TOTAL_TIMEOUT;
    loop {
        if Instant::now() >= deadline {
            return Err(UpdateLifecycleError::new(UPD_SHUTDOWN_TIMEOUT));
        }
        let attempt = read_attempt(&attempt_path)?;
        if attempt.phase == AttemptPhase::ShutdownComplete && !process_is_running(args.old_pid)? {
            if sha256_file(&args.installer)? != attempt.package_sha256 {
                return fail_helper_attempt(&attempt_path, UPD_INSTALL_PREPARE_FAILED, None);
            }
            let status = Command::new(&args.installer)
                .args(["/P", "/UPDATE"])
                .status()
                .map_err(|error| {
                    UpdateLifecycleError::with_detail(UPD_INSTALL_LAUNCH_FAILED, error.to_string())
                })?;
            let exit_code = status.code().unwrap_or(-1);
            if exit_code != 0 {
                return fail_helper_attempt(
                    &attempt_path,
                    UPD_INSTALL_EXIT_NONZERO,
                    Some(exit_code),
                );
            }
            let installed_version = target_binary_version(&args.target_exe)?;
            if installed_version != attempt.target_version {
                return fail_helper_attempt(
                    &attempt_path,
                    UPD_TARGET_BINARY_INVALID,
                    Some(exit_code),
                );
            }
            transition_attempt(
                &attempt_path,
                AttemptPhase::InstallerSucceeded,
                Some(0),
                Some(installed_version),
                None,
            )?;
            Command::new(&args.target_exe)
                .arg(UPDATE_ATTEMPT_ARG)
                .arg(&args.attempt_id)
                .spawn()
                .map_err(|error| {
                    UpdateLifecycleError::with_detail(UPD_TARGET_BINARY_INVALID, error.to_string())
                })?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn fail_helper_attempt(
    path: &Path,
    code: &'static str,
    exit_code: Option<i32>,
) -> Result<(), UpdateLifecycleError> {
    let _ = transition_attempt(
        path,
        AttemptPhase::Failed,
        exit_code,
        None,
        Some(code.to_string()),
    );
    Err(UpdateLifecycleError::new(code))
}

fn target_binary_version(path: &Path) -> Result<String, UpdateLifecycleError> {
    let output = Command::new(path)
        .arg("--caseboard-print-version")
        .output()
        .map_err(|error| {
            UpdateLifecycleError::with_detail(UPD_TARGET_BINARY_INVALID, error.to_string())
        })?;
    if !output.status.success() {
        return Err(UpdateLifecycleError::new(UPD_TARGET_BINARY_INVALID));
    }
    let version = String::from_utf8(output.stdout)
        .map_err(|_| UpdateLifecycleError::new(UPD_TARGET_BINARY_INVALID))?
        .trim()
        .to_string();
    if !valid_version(&version) {
        return Err(UpdateLifecycleError::new(UPD_TARGET_BINARY_INVALID));
    }
    Ok(version)
}

#[cfg(windows)]
fn process_is_running(pid: u32) -> Result<bool, UpdateLifecycleError> {
    use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    let process = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )
    };
    let Ok(process) = process else {
        return Ok(false);
    };
    let wait = unsafe { WaitForSingleObject(process, 0) };
    unsafe {
        let _ = CloseHandle(process);
    }
    Ok(wait == WAIT_TIMEOUT)
}

#[cfg(not(windows))]
fn process_is_running(_pid: u32) -> Result<bool, UpdateLifecycleError> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_attempt(root: &Path) -> (PathBuf, AttemptRecord) {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let record = AttemptRecord {
            schema: ATTEMPT_SCHEMA,
            attempt_id: id.clone(),
            source_version: "0.8.3".into(),
            target_version: "0.8.4".into(),
            created_at: now,
            expires_at: now + chrono::Duration::minutes(30),
            phase: AttemptPhase::Prepared,
            installer_exit_code: None,
            package_sha256: sha256_bytes(b"installer"),
            installed_exe_version: None,
            notes: Some("notes".into()),
            error_code: None,
        };
        (attempt_file(root, &id), record)
    }

    #[test]
    fn state_machine_rejects_skipping_shutdown_barrier() {
        let temp = tempfile::tempdir().unwrap();
        ensure_secure_dir(temp.path()).unwrap();
        let (path, attempt) = test_attempt(temp.path());
        write_attempt(&path, &attempt, UPD_ATTEMPT_PERSIST_FAILED).unwrap();
        let error = transition_attempt(
            &path,
            AttemptPhase::InstallerSucceeded,
            Some(0),
            Some("0.8.4".into()),
            None,
        )
        .unwrap_err();
        assert_eq!(error.code, UPD_RECEIPT_INVALID);
    }

    #[test]
    fn prepared_shutdown_success_path_is_durable() {
        let temp = tempfile::tempdir().unwrap();
        ensure_secure_dir(temp.path()).unwrap();
        let (path, attempt) = test_attempt(temp.path());
        write_attempt(&path, &attempt, UPD_ATTEMPT_PERSIST_FAILED).unwrap();
        transition_attempt(&path, AttemptPhase::ShutdownComplete, None, None, None).unwrap();
        transition_attempt(
            &path,
            AttemptPhase::InstallerSucceeded,
            Some(0),
            Some("0.8.4".into()),
            None,
        )
        .unwrap();
        let saved = read_attempt(&path).unwrap();
        assert_eq!(saved.phase, AttemptPhase::InstallerSucceeded);
        assert_eq!(saved.installer_exit_code, Some(0));
    }

    #[test]
    fn invalid_attempt_ids_and_versions_are_rejected() {
        assert!(!valid_attempt_id("not-an-id"));
        assert!(!valid_version("0.08.4"));
        assert!(!valid_version("0.8"));
        assert!(valid_version("0.8.4"));
    }

    #[test]
    fn helper_arguments_contain_no_secret_fields() {
        let id = Uuid::new_v4().to_string();
        let parsed = parse_helper_args(
            [
                "--attempt-id",
                &id,
                "--state-dir",
                "C:/state",
                "--installer",
                "C:/state/installer.exe",
                "--old-pid",
                "42",
                "--target-exe",
                "C:/app.exe",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert_eq!(parsed.attempt_id, id);
    }
}
