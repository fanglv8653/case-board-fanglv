use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::crypto::EncryptedEnvelope;
use super::SyncError;

const ROOT_DIR: &str = "fanglv-caseboard-sync";

#[derive(Debug, Clone)]
pub struct MountedFolder {
    selected_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDescriptor {
    pub protocol_version: u32,
    pub product: String,
    pub connector_type: String,
}

impl MountedFolder {
    pub fn connect(selected_root: impl AsRef<Path>) -> Result<Self, SyncError> {
        let selected_root = selected_root.as_ref();
        if !selected_root.is_absolute() {
            return Err(SyncError::InvalidNasPath(
                "NAS 目录必须是绝对盘符路径或 UNC 路径".to_string(),
            ));
        }
        if selected_root
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(SyncError::InvalidNasPath(
                "NAS 目录不能包含上级路径片段".to_string(),
            ));
        }
        fs::create_dir_all(selected_root).map_err(|error| {
            SyncError::NasUnavailable(format!("无法创建或访问 NAS 目录: {error}"))
        })?;
        let probe = selected_root.join(".caseboard-write-probe");
        atomic_write(&probe, b"probe")?;
        fs::remove_file(&probe).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        Ok(Self {
            selected_root: selected_root.to_path_buf(),
        })
    }

    pub fn initialize_group(&self, group_id: &str) -> Result<PathBuf, SyncError> {
        validate_segment(group_id)?;
        let group = self.group_root(group_id)?;
        for relative in [
            "members",
            "invites",
            "events",
            "receipts",
            "snapshots",
            "manifests",
            "quarantine",
        ] {
            fs::create_dir_all(group.join(relative))
                .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        }
        let protocol = ProtocolDescriptor {
            protocol_version: super::crypto::PROTOCOL_VERSION,
            product: "Fanglv CaseBoard".to_string(),
            connector_type: "mounted_folder".to_string(),
        };
        let bytes = serde_json::to_vec_pretty(&protocol)
            .map_err(|error| SyncError::Serialization(error.to_string()))?;
        atomic_write(&group.join("protocol.json"), &bytes)?;
        Ok(group)
    }

    pub fn write_event(
        &self,
        group_id: &str,
        device_id: &str,
        sequence: u64,
        envelope: &EncryptedEnvelope,
    ) -> Result<PathBuf, SyncError> {
        validate_segment(device_id)?;
        let directory = self.group_root(group_id)?.join("events").join(device_id);
        fs::create_dir_all(&directory)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        let target = directory.join(format!("{sequence:020}.cbe"));
        let bytes = serde_json::to_vec(envelope)
            .map_err(|error| SyncError::Serialization(error.to_string()))?;
        atomic_write(&target, &bytes)?;
        Ok(target)
    }

    pub(crate) fn write_event_bytes(
        &self,
        group_id: &str,
        device_id: &str,
        sequence: u64,
        bytes: &[u8],
    ) -> Result<PathBuf, SyncError> {
        validate_segment(device_id)?;
        let directory = self.group_root(group_id)?.join("events").join(device_id);
        fs::create_dir_all(&directory)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        let target = directory.join(format!("{sequence:020}.cbe"));
        atomic_write(&target, bytes)?;
        Ok(target)
    }

    pub fn list_events_after(
        &self,
        group_id: &str,
        device_id: &str,
        after_sequence: u64,
    ) -> Result<Vec<(u64, PathBuf)>, SyncError> {
        validate_segment(device_id)?;
        let directory = self.group_root(group_id)?.join("events").join(device_id);
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        for entry in fs::read_dir(&directory)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?
        {
            let entry = entry.map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
            if !entry
                .file_type()
                .map_err(|error| SyncError::NasUnavailable(error.to_string()))?
                .is_file()
            {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("cbe") {
                continue;
            }
            let sequence = path
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or_else(|| {
                    SyncError::Integrity(format!("事件文件名非法: {}", path.display()))
                })?;
            if sequence > after_sequence {
                events.push((sequence, path));
            }
        }
        events.sort_by_key(|(sequence, _)| *sequence);
        Ok(events)
    }

    pub fn write_manifest(
        &self,
        group_id: &str,
        device_id: &str,
        sequence: u64,
        envelope: &EncryptedEnvelope,
    ) -> Result<PathBuf, SyncError> {
        validate_segment(device_id)?;
        let directory = self.group_root(group_id)?.join("manifests").join(device_id);
        fs::create_dir_all(&directory)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        let target = directory.join(format!("{sequence:020}.cbm"));
        let bytes = serde_json::to_vec(envelope)
            .map_err(|error| SyncError::Serialization(error.to_string()))?;
        atomic_write(&target, &bytes)?;
        Ok(target)
    }

    pub(crate) fn write_manifest_bytes(
        &self,
        group_id: &str,
        device_id: &str,
        sequence: u64,
        bytes: &[u8],
    ) -> Result<PathBuf, SyncError> {
        validate_segment(device_id)?;
        let directory = self.group_root(group_id)?.join("manifests").join(device_id);
        fs::create_dir_all(&directory)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        let target = directory.join(format!("{sequence:020}.cbm"));
        atomic_write(&target, bytes)?;
        Ok(target)
    }

    pub fn manifest_path(
        &self,
        group_id: &str,
        device_id: &str,
        sequence: u64,
    ) -> Result<PathBuf, SyncError> {
        validate_segment(device_id)?;
        Ok(self
            .group_root(group_id)?
            .join("manifests")
            .join(device_id)
            .join(format!("{sequence:020}.cbm")))
    }

    pub fn read_envelope(&self, path: &Path) -> Result<EncryptedEnvelope, SyncError> {
        let events_root = self.sync_root().join("groups");
        let canonical_events = fs::canonicalize(&events_root)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        let canonical_path =
            fs::canonicalize(path).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        if !canonical_path.starts_with(&canonical_events) {
            return Err(SyncError::InvalidNasPath(
                "拒绝读取同步目录之外的信封".to_string(),
            ));
        }
        let bytes = fs::read(&canonical_path)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| SyncError::Serialization(format!("信封 JSON 无效: {error}")))
    }

    pub fn write_encrypted_snapshot(
        &self,
        group_id: &str,
        snapshot_id: &str,
        envelope: &EncryptedEnvelope,
    ) -> Result<PathBuf, SyncError> {
        validate_segment(snapshot_id)?;
        let target = self
            .group_root(group_id)?
            .join("snapshots")
            .join(format!("{snapshot_id}.cbs"));
        let bytes = serde_json::to_vec(envelope)
            .map_err(|error| SyncError::Serialization(error.to_string()))?;
        atomic_write(&target, &bytes)?;
        Ok(target)
    }

    pub fn remove_snapshot(
        &self,
        group_id: &str,
        encrypted_file_name: &str,
    ) -> Result<(), SyncError> {
        if !encrypted_file_name.ends_with(".cbs")
            || encrypted_file_name.contains('/')
            || encrypted_file_name.contains('\\')
        {
            return Err(SyncError::InvalidNasPath("快照文件名非法".to_string()));
        }
        let target = self
            .group_root(group_id)?
            .join("snapshots")
            .join(encrypted_file_name);
        if !target.exists() {
            return Ok(());
        }
        fs::remove_file(target).map_err(|error| SyncError::NasUnavailable(error.to_string()))
    }

    pub fn write_invite_json(
        &self,
        group_id: &str,
        invite_id: &str,
        suffix: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, SyncError> {
        validate_segment(invite_id)?;
        validate_segment(suffix)?;
        let target = self
            .group_root(group_id)?
            .join("invites")
            .join(format!("{invite_id}.{suffix}.json"));
        atomic_write(&target, bytes)?;
        Ok(target)
    }

    pub fn invite_path(
        &self,
        group_id: &str,
        invite_id: &str,
        suffix: &str,
    ) -> Result<PathBuf, SyncError> {
        validate_segment(invite_id)?;
        validate_segment(suffix)?;
        Ok(self
            .group_root(group_id)?
            .join("invites")
            .join(format!("{invite_id}.{suffix}.json")))
    }

    pub fn write_member_envelope(
        &self,
        group_id: &str,
        device_id: &str,
        epoch: u32,
        envelope: &EncryptedEnvelope,
    ) -> Result<PathBuf, SyncError> {
        validate_segment(device_id)?;
        let target = self
            .group_root(group_id)?
            .join("members")
            .join(format!("{device_id}.epoch-{epoch}.key.cbe"));
        let bytes = serde_json::to_vec(envelope)
            .map_err(|error| SyncError::Serialization(error.to_string()))?;
        atomic_write(&target, &bytes)?;
        Ok(target)
    }

    pub fn member_envelope_path(
        &self,
        group_id: &str,
        device_id: &str,
        epoch: u32,
    ) -> Result<PathBuf, SyncError> {
        validate_segment(device_id)?;
        Ok(self
            .group_root(group_id)?
            .join("members")
            .join(format!("{device_id}.epoch-{epoch}.key.cbe")))
    }

    pub fn read_group_file(&self, path: &Path) -> Result<Vec<u8>, SyncError> {
        let groups_root = self.sync_root().join("groups");
        let canonical_root = fs::canonicalize(&groups_root)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        let canonical_path =
            fs::canonicalize(path).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        if !canonical_path.starts_with(canonical_root) {
            return Err(SyncError::InvalidNasPath(
                "拒绝读取同步组目录之外的文件".to_string(),
            ));
        }
        fs::read(canonical_path).map_err(|error| SyncError::NasUnavailable(error.to_string()))
    }

    fn sync_root(&self) -> PathBuf {
        self.selected_root.join(ROOT_DIR)
    }

    fn group_root(&self, group_id: &str) -> Result<PathBuf, SyncError> {
        validate_segment(group_id)?;
        Ok(self.sync_root().join("groups").join(group_id))
    }
}

fn validate_segment(value: &str) -> Result<(), SyncError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(SyncError::InvalidNasPath(format!(
            "同步标识包含非法字符: {value}"
        )));
    }
    Ok(())
}

fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), SyncError> {
    atomic_write_inner(target, bytes, None)
}

fn atomic_write_inner(
    target: &Path,
    bytes: &[u8],
    #[cfg(test)] before_publish: Option<&std::sync::Barrier>,
    #[cfg(not(test))] _before_publish: Option<&()>,
) -> Result<(), SyncError> {
    if target.exists() {
        let existing =
            fs::read(target).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        if existing == bytes {
            return Ok(());
        }
        return Err(SyncError::Integrity(
            "同序列同步对象已存在但内容不一致".to_string(),
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| SyncError::InvalidNasPath("目标文件没有父目录".to_string()))?;
    fs::create_dir_all(parent).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        target
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("caseboard"),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        file.sync_all()
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        #[cfg(test)]
        if let Some(barrier) = before_publish {
            barrier.wait();
        }
        publish_no_replace(&temp, target, parent).or_else(|error| {
            if target.exists() {
                let winner = fs::read(target)
                    .map_err(|read_error| SyncError::NasUnavailable(read_error.to_string()))?;
                if winner == bytes {
                    return Ok(());
                }
                return Err(SyncError::Integrity(
                    "同序列同步对象已存在但内容不一致".to_string(),
                ));
            }
            Err(error)
        })
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(target_os = "windows")]
fn publish_no_replace(temp: &Path, target: &Path, _parent: &Path) -> Result<(), SyncError> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    let temp_wide = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(temp_wide.as_ptr()),
            PCWSTR(target_wide.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| SyncError::NasUnavailable(error.to_string()))
}

#[cfg(not(target_os = "windows"))]
fn publish_no_replace(temp: &Path, target: &Path, parent: &Path) -> Result<(), SyncError> {
    fs::hard_link(temp, target).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    fs::remove_file(temp).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| SyncError::NasUnavailable(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_sync::crypto::{
        generate_device_keys, generate_group_key, seal, EnvelopeHeader, PROTOCOL_VERSION,
    };

    #[test]
    fn mounted_folder_roundtrip_and_path_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let folder = MountedFolder::connect(temp.path()).unwrap();
        folder.initialize_group("g1").unwrap();
        let device = generate_device_keys();
        let key = generate_group_key();
        let envelope = seal(
            EnvelopeHeader {
                protocol_version: PROTOCOL_VERSION,
                group_id: "g1".to_string(),
                device_id: "d1".to_string(),
                sequence: 1,
                key_epoch: 1,
                payload_kind: "operations".to_string(),
                created_at: "2026-07-29T00:00:00Z".to_string(),
            },
            b"[]",
            &key,
            &device.signing_secret,
        )
        .unwrap();
        let path = folder.write_event("g1", "d1", 1, &envelope).unwrap();
        assert_eq!(folder.list_events_after("g1", "d1", 0).unwrap().len(), 1);
        assert_eq!(folder.read_envelope(&path).unwrap().header.sequence, 1);
        assert!(folder
            .read_envelope(&temp.path().join("outside.cbe"))
            .is_err());
    }

    #[test]
    fn concurrent_no_replace_keeps_one_complete_winner_and_rejects_other_bytes() {
        let directory = tempfile::tempdir().unwrap();
        for round in 0..20 {
            let target = directory.path().join(format!("race-{round}.cbe"));
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let left_target = target.clone();
            let left_barrier = barrier.clone();
            let right_target = target.clone();
            let right_barrier = barrier.clone();
            let left = std::thread::spawn(move || {
                atomic_write_inner(&left_target, b"complete-left", Some(&left_barrier))
            });
            let right = std::thread::spawn(move || {
                atomic_write_inner(&right_target, b"complete-right", Some(&right_barrier))
            });
            let results = [left.join().unwrap(), right.join().unwrap()];
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
            assert_eq!(
                results
                    .iter()
                    .filter(|result| matches!(result, Err(SyncError::Integrity(_))))
                    .count(),
                1
            );
            let winner = fs::read(&target).unwrap();
            assert!(winner == b"complete-left" || winner == b"complete-right");
        }
    }

    #[test]
    fn concurrent_no_replace_accepts_identical_bytes_idempotently() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("same.cbe");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let left_target = target.clone();
        let left_barrier = barrier.clone();
        let right_target = target.clone();
        let right_barrier = barrier.clone();
        let left = std::thread::spawn(move || {
            atomic_write_inner(&left_target, b"same-complete-bytes", Some(&left_barrier))
        });
        let right = std::thread::spawn(move || {
            atomic_write_inner(&right_target, b"same-complete-bytes", Some(&right_barrier))
        });
        assert!(left.join().unwrap().is_ok());
        assert!(right.join().unwrap().is_ok());
        assert_eq!(fs::read(target).unwrap(), b"same-complete-bytes");
    }
}
