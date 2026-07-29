use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncManifest {
    pub group_id: String,
    pub device_id: String,
    pub sequence: u64,
    pub event_ciphertext_sha256: String,
    pub previous_manifest_hash: Option<String>,
    pub generated_at: String,
}
