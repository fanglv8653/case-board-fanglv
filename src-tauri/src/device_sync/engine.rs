use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};

use super::capture::{capture_dirty_entities, ensure_initial_baseline};
use super::crypto::{open, seal, sha256_hex, EncryptedEnvelope, EnvelopeHeader, PROTOCOL_VERSION};
use super::identity::{load_group_key, load_signing_secret};
use super::manifest::SyncManifest;
use super::nas_folder::MountedFolder;
use super::operations::{apply_incoming_package, ApplyOutcome, OperationAction, SyncOperation};
use super::{SyncError, SyncStatus};

const MAX_OPERATIONS_PER_EVENT: usize = 500;
static SYNC_RUN_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[derive(Clone, Copy)]
enum ExportFault {
    None,
    #[cfg(test)]
    AfterManifest(i64),
    #[cfg(test)]
    AfterEvent(i64),
    #[cfg(test)]
    AfterCas(i64),
}

impl ExportFault {
    fn after_cas(self, _sequence: i64) -> bool {
        #[cfg(test)]
        if matches!(self, Self::AfterCas(value) if value == _sequence) {
            return true;
        }
        false
    }

    fn after_manifest(self, _sequence: i64) -> bool {
        #[cfg(test)]
        if matches!(self, Self::AfterManifest(value) if value == _sequence) {
            return true;
        }
        false
    }

    fn after_event(self, _sequence: i64) -> bool {
        #[cfg(test)]
        if matches!(self, Self::AfterEvent(value) if value == _sequence) {
            return true;
        }
        false
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn max_operations_per_event_for_test() -> usize {
    MAX_OPERATIONS_PER_EVENT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRunResult {
    pub exported_operations: usize,
    pub imported_operations: usize,
    pub conflicts_created: usize,
    pub duplicate_operations: usize,
    pub quarantined_packages: usize,
}

#[derive(Debug, FromRow)]
struct GroupRow {
    id: String,
    connector_root: String,
    local_device_id: String,
    key_epoch: i64,
    next_sequence: i64,
    paused: i64,
    auto_paused: i64,
    pause_reason_code: Option<String>,
    last_attempt_at: Option<String>,
    last_success_at: Option<String>,
    last_manifest_hash: Option<String>,
}

#[derive(Debug, FromRow)]
struct OutboxRow {
    operation_id: String,
    entity_type: String,
    entity_id: String,
    case_id: Option<String>,
    action: String,
    base_revision: i64,
    changed_fields_json: String,
    base_field_hashes_json: String,
    atomic_group: Option<String>,
    author_device_id: String,
    logical_time: i64,
    capture_sequence: i64,
    schema_version: i64,
}

#[derive(Debug, Clone, FromRow)]
struct ExportDraftRow {
    group_id: String,
    local_device_id: String,
    sequence: i64,
    key_epoch: i64,
    previous_manifest_hash: Option<String>,
    event_envelope_bytes: Vec<u8>,
    manifest_envelope_bytes: Vec<u8>,
    event_ciphertext_sha256: String,
    manifest_ciphertext_sha256: String,
    operation_ids_json: String,
    operation_fingerprint: String,
    state: String,
}

#[derive(Debug)]
pub(crate) struct PreparedExport {
    pub(crate) group_id: String,
    pub(crate) local_device_id: String,
    pub(crate) sequence: i64,
    pub(crate) key_epoch: i64,
    pub(crate) previous_manifest_hash: Option<String>,
    pub(crate) event_envelope_bytes: Vec<u8>,
    pub(crate) manifest_envelope_bytes: Vec<u8>,
    pub(crate) event_ciphertext_sha256: String,
    pub(crate) manifest_ciphertext_sha256: String,
    pub(crate) operation_ids: Vec<String>,
    pub(crate) operation_fingerprint: String,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum TestPublishPart {
    ManifestOnly,
    EventOnly,
    Complete,
    FailAfterManifest,
}

fn find_component(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        parent[index] = find_component(parent, parent[index]);
    }
    parent[index]
}

fn join_components(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find_component(parent, left);
    let right_root = find_component(parent, right);
    if left_root != right_root {
        parent[right_root] = left_root;
    }
}

fn dependency_value<'a>(
    operation: &'a SyncOperation,
    field: &str,
) -> Result<Option<&'a str>, SyncError> {
    match operation.changed_fields.get(field) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(_) => Err(SyncError::Protocol(
            "invalid package dependency field".to_string(),
        )),
    }
}

fn operation_dependency(operation: &SyncOperation) -> Result<Option<(String, String)>, SyncError> {
    if operation.action != OperationAction::Upsert {
        return Ok(None);
    }
    match operation.entity_type.as_str() {
        "case" => Ok(dependency_value(operation, "judge_id")?
            .map(|entity_id| ("contact".to_string(), entity_id.to_string()))),
        "contact" => Ok(dependency_value(operation, "case_id")?
            .or(operation.case_id.as_deref())
            .map(|entity_id| ("case".to_string(), entity_id.to_string()))),
        _ => Ok(None),
    }
}

fn pack_operation_indexes(
    operations: &[SyncOperation],
    historically_exported: &BTreeSet<(String, String)>,
    max_operations: usize,
) -> Result<Vec<Vec<usize>>, SyncError> {
    if operations.is_empty() {
        return Ok(Vec::new());
    }
    if max_operations == 0 {
        return Err(SyncError::PackageTooLarge);
    }

    let mut parent = (0..operations.len()).collect::<Vec<_>>();
    let mut entity_operations: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    let mut atomic_groups: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for (index, operation) in operations.iter().enumerate() {
        entity_operations
            .entry((operation.entity_type.clone(), operation.entity_id.clone()))
            .or_default()
            .push(index);
        if let Some(group) = operation.atomic_group.as_ref() {
            let key = (
                operation.entity_type.clone(),
                operation.entity_id.clone(),
                group.clone(),
            );
            if let Some(previous) = atomic_groups.insert(key, index) {
                join_components(&mut parent, previous, index);
            }
        }
    }

    let stable_key = |index: &usize| operations[*index].capture_sequence;
    let mut final_actions = BTreeMap::new();
    for (entity, indexes) in &mut entity_operations {
        indexes.sort_by_key(&stable_key);
        for pair in indexes.windows(2) {
            join_components(&mut parent, pair[0], pair[1]);
        }
        let final_index = *indexes.last().expect("entity has operations");
        final_actions.insert(entity.clone(), operations[final_index].action);
    }

    for (index, operation) in operations.iter().enumerate() {
        let Some(dependency) = operation_dependency(operation)? else {
            continue;
        };
        if let Some(matches) = entity_operations.get(&dependency) {
            if final_actions.get(&dependency) != Some(&OperationAction::Upsert) {
                return Err(SyncError::PackageDependencyConflict);
            }
            for dependency_index in matches {
                join_components(&mut parent, index, *dependency_index);
            }
        } else if !historically_exported.contains(&dependency) {
            return Err(SyncError::PackageDependencyMissing);
        }
    }

    let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..operations.len() {
        let root = find_component(&mut parent, index);
        by_root.entry(root).or_default().push(index);
    }
    let mut components = by_root.into_values().collect::<Vec<_>>();
    for component in &mut components {
        component.sort_by_key(&stable_key);
    }
    components.sort_by_key(|component| stable_key(&component[0]));
    if components
        .iter()
        .any(|component| component.len() > max_operations)
    {
        return Err(SyncError::PackageTooLarge);
    }

    let mut packages = Vec::new();
    let mut current = Vec::new();
    for component in components {
        if !current.is_empty() && current.len() + component.len() > max_operations {
            packages.push(current);
            current = Vec::new();
        }
        current.extend(component);
    }
    if !current.is_empty() {
        packages.push(current);
    }
    Ok(packages)
}

#[cfg(test)]
pub(crate) fn pack_operations_for_test(
    operations: &[SyncOperation],
    historically_exported: &[(&str, &str)],
) -> Result<Vec<Vec<String>>, SyncError> {
    let historically_exported = historically_exported
        .iter()
        .map(|(entity_type, entity_id)| (entity_type.to_string(), entity_id.to_string()))
        .collect();
    pack_operation_indexes(operations, &historically_exported, MAX_OPERATIONS_PER_EVENT).map(
        |packages| {
            packages
                .into_iter()
                .map(|package| {
                    package
                        .into_iter()
                        .map(|index| operations[index].operation_id.clone())
                        .collect()
                })
                .collect()
        },
    )
}

#[derive(Debug, FromRow)]
struct MemberRow {
    device_id: String,
    signing_public_key: String,
    status: String,
    last_seen_sequence: i64,
    last_manifest_hash: Option<String>,
}

pub async fn sync_once(pool: &SqlitePool, group_id: &str) -> Result<SyncRunResult, SyncError> {
    sync_once_coordinated(pool, group_id, || std::future::ready(())).await
}

async fn sync_once_coordinated<G, Fut>(
    pool: &SqlitePool,
    group_id: &str,
    after_lifecycle_lock: G,
) -> Result<SyncRunResult, SyncError>
where
    G: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let lock = SYNC_RUN_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = lock.try_lock().map_err(|_| SyncError::Busy)?;
    super::feishu_binding_lifecycle::run_device_sync_action(|| async {
        after_lifecycle_lock().await;
        mark_sync_attempt(pool, group_id).await?;
        sync_once_inner(pool, group_id).await
    })
    .await
}

#[cfg(test)]
pub(crate) async fn sync_once_with_entry_gate_for_test<G, Fut>(
    pool: &SqlitePool,
    group_id: &str,
    after_lifecycle_lock: G,
) -> Result<SyncRunResult, SyncError>
where
    G: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    sync_once_coordinated(pool, group_id, after_lifecycle_lock).await
}

async fn mark_sync_attempt(pool: &SqlitePool, group_id: &str) -> Result<(), SyncError> {
    sqlx::query(
        "UPDATE device_sync_groups
         SET last_attempt_at=datetime('now'),updated_at=datetime('now')
         WHERE id=?1",
    )
    .bind(group_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn sync_once_inner(pool: &SqlitePool, group_id: &str) -> Result<SyncRunResult, SyncError> {
    let mut group = load_group(pool, group_id).await?;
    if group.paused != 0 {
        return Err(if group.auto_paused != 0 {
            SyncError::GroupAutoPaused
        } else {
            SyncError::Paused
        });
    }
    let folder = MountedFolder::connect(PathBuf::from(&group.connector_root))?;
    folder.initialize_group(group_id)?;
    if super::pairing::accept_pending_key_rotation(pool, group_id)
        .await?
        .is_some()
    {
        group = load_group(pool, group_id).await?;
    }

    ensure_initial_baseline(pool, group_id).await?;
    capture_dirty_entities(pool, group_id).await?;
    let exported_operations = match export_pending(pool, &folder, &group).await {
        Ok(exported_operations) => exported_operations,
        Err(error) if is_deterministic_export_error(&error) => {
            quarantine_export_error(pool, &group, &error).await?;
            return Err(SyncError::GroupAutoPaused);
        }
        Err(error) => return Err(error),
    };
    let mut imported_operations = 0;
    let mut conflicts_created = 0;
    let mut duplicate_operations = 0;

    let members: Vec<MemberRow> = sqlx::query_as(
        "SELECT device_id, signing_public_key, status, last_seen_sequence,
                last_manifest_hash
         FROM device_sync_members
         WHERE group_id=?1 AND device_id<>?2",
    )
    .bind(group_id)
    .bind(&group.local_device_id)
    .fetch_all(pool)
    .await?;
    for mut member in members {
        if member.status != "trusted" {
            continue;
        }
        let events = match folder.list_events_after(
            group_id,
            &member.device_id,
            member.last_seen_sequence as u64,
        ) {
            Ok(events) => events,
            Err(error) if is_deterministic_event_error(&error) => {
                quarantine_and_auto_pause(
                    pool,
                    group_id,
                    Some(&member.device_id),
                    None,
                    None,
                    error.code(),
                )
                .await?;
                return Err(SyncError::GroupAutoPaused);
            }
            Err(error) => return Err(error),
        };
        let mut expected = member.last_seen_sequence as u64 + 1;
        for (sequence, path) in events {
            if sequence != expected {
                quarantine_and_auto_pause(
                    pool,
                    group_id,
                    Some(&member.device_id),
                    Some(sequence),
                    Some(path.to_string_lossy().as_ref()),
                    "SYNC_SEQUENCE_GAP",
                )
                .await?;
                return Err(SyncError::GroupAutoPaused);
            }
            match import_event(pool, &folder, &group, &member, sequence, &path).await {
                Ok((outcomes, manifest_hash)) => {
                    imported_operations += outcomes.len();
                    for outcome in outcomes {
                        conflicts_created += outcome.conflict_fields.len();
                        duplicate_operations += usize::from(outcome.duplicate);
                    }
                    member.last_manifest_hash = Some(manifest_hash);
                    expected += 1;
                }
                Err(error) if is_deterministic_event_error(&error) => {
                    quarantine_and_auto_pause(
                        pool,
                        group_id,
                        Some(&member.device_id),
                        Some(sequence),
                        Some(path.to_string_lossy().as_ref()),
                        error.code(),
                    )
                    .await?;
                    return Err(SyncError::GroupAutoPaused);
                }
                Err(error) => return Err(error),
            }
        }
    }
    ensure_no_active_quarantine(pool, group_id, &group.local_device_id).await?;
    let daily_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM device_sync_snapshots
             WHERE group_id=?1 AND snapshot_kind='daily'
               AND date(created_at)=date('now')
         )",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    if daily_exists == 0 {
        super::snapshot::create_encrypted_snapshot(pool, group_id, "daily").await?;
    }
    let monthly_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM device_sync_snapshots
             WHERE group_id=?1 AND snapshot_kind='monthly'
               AND strftime('%Y-%m',created_at)=strftime('%Y-%m','now')
         )",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    if monthly_exists == 0 {
        super::snapshot::create_encrypted_snapshot(pool, group_id, "monthly").await?;
    }
    record_sync_success(
        pool,
        group_id,
        &group.local_device_id,
        serde_json::json!({
            "exported": exported_operations,
            "imported": imported_operations,
            "conflicts": conflicts_created,
            "duplicates": duplicate_operations,
            "quarantined": 0
        }),
    )
    .await?;
    Ok(SyncRunResult {
        exported_operations,
        imported_operations,
        conflicts_created,
        duplicate_operations,
        quarantined_packages: 0,
    })
}

pub async fn get_status(pool: &SqlitePool, group_id: &str) -> Result<SyncStatus, SyncError> {
    let group = load_group(pool, group_id).await?;
    let pending_upload: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_outbox WHERE group_id=?1 AND state='pending'",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    let conflicts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_conflicts WHERE group_id=?1 AND status='pending'",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    let quarantined: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_quarantine WHERE group_id=?1 AND status='active'",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    let manual_review: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_quarantine WHERE group_id=?1 AND status='manual_review'",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    Ok(SyncStatus {
        group_id: group.id,
        connector_root: group.connector_root,
        local_device_id: group.local_device_id,
        key_epoch: group.key_epoch as u32,
        paused: group.paused != 0,
        auto_paused: group.auto_paused != 0,
        pause_reason_code: group.pause_reason_code,
        last_attempt_at: group.last_attempt_at,
        last_success_at: group.last_success_at,
        pending_upload: pending_upload as u64,
        conflicts: conflicts as u64,
        quarantined: quarantined as u64,
        manual_review: manual_review as u64,
    })
}

fn operation_from_outbox(row: &OutboxRow) -> Result<SyncOperation, SyncError> {
    Ok(SyncOperation {
        operation_id: row.operation_id.clone(),
        entity_type: row.entity_type.clone(),
        entity_id: row.entity_id.clone(),
        case_id: row.case_id.clone(),
        action: match row.action.as_str() {
            "upsert" => OperationAction::Upsert,
            "tombstone" => OperationAction::Tombstone,
            other => {
                return Err(SyncError::Protocol(format!(
                    "unknown operation type: {other}"
                )))
            }
        },
        base_revision: row.base_revision,
        changed_fields: serde_json::from_str(&row.changed_fields_json)?,
        base_field_hashes: serde_json::from_str(&row.base_field_hashes_json)?,
        atomic_group: row.atomic_group.clone(),
        author_device_id: row.author_device_id.clone(),
        logical_time: row.logical_time,
        capture_sequence: row.capture_sequence,
        schema_version: row.schema_version as u32,
    })
}

async fn load_historical_dependency_proof(
    pool: &SqlitePool,
    group: &GroupRow,
    operations: &[SyncOperation],
) -> Result<BTreeSet<(String, String)>, SyncError> {
    let pending_entities = operations
        .iter()
        .map(|operation| (operation.entity_type.clone(), operation.entity_id.clone()))
        .collect::<BTreeSet<_>>();
    let mut missing_from_pending = BTreeSet::new();
    for operation in operations {
        if let Some(dependency) = operation_dependency(operation)? {
            if !pending_entities.contains(&dependency) {
                missing_from_pending.insert(dependency);
            }
        }
    }

    let mut proven = BTreeSet::new();
    for (entity_type, entity_id) in missing_from_pending {
        let last_action: Option<String> = sqlx::query_scalar(
            "SELECT action FROM device_sync_outbox
             WHERE group_id=?1 AND entity_type=?2 AND entity_id=?3
               AND state IN ('exported','acknowledged')
               AND exported_sequence IS NOT NULL
               AND exported_sequence < ?4
             ORDER BY exported_sequence DESC,capture_sequence DESC
             LIMIT 1",
        )
        .bind(&group.id)
        .bind(&entity_type)
        .bind(&entity_id)
        .bind(group.next_sequence)
        .fetch_optional(pool)
        .await?;
        if last_action.as_deref() == Some("upsert") {
            proven.insert((entity_type, entity_id));
        }
    }
    Ok(proven)
}

async fn plan_pending_export(
    pool: &SqlitePool,
    group: &GroupRow,
) -> Result<(Vec<OutboxRow>, Vec<SyncOperation>, Vec<Vec<usize>>), SyncError> {
    let rows: Vec<OutboxRow> = sqlx::query_as(
        "SELECT operation_id, entity_type, entity_id, case_id, action, base_revision,
                changed_fields_json, base_field_hashes_json, atomic_group,
                author_device_id, logical_time, capture_sequence, schema_version
         FROM device_sync_outbox
         WHERE group_id=?1 AND state='pending'
         ORDER BY capture_sequence",
    )
    .bind(&group.id)
    .fetch_all(pool)
    .await?;
    let operations = rows
        .iter()
        .map(operation_from_outbox)
        .collect::<Result<Vec<_>, SyncError>>()?;
    let historically_exported = load_historical_dependency_proof(pool, group, &operations).await?;
    let packages = pack_operation_indexes(
        &operations,
        &historically_exported,
        MAX_OPERATIONS_PER_EVENT,
    )?;
    Ok((rows, operations, packages))
}

#[cfg(test)]
pub(crate) async fn plan_pending_export_for_test(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<Vec<Vec<String>>, SyncError> {
    let group = load_group(pool, group_id).await?;
    let (_rows, operations, packages) = plan_pending_export(pool, &group).await?;
    Ok(packages
        .into_iter()
        .map(|package| {
            package
                .into_iter()
                .map(|index| operations[index].operation_id.clone())
                .collect()
        })
        .collect())
}

#[cfg(test)]
pub(crate) async fn export_pending_for_test(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group_id: &str,
) -> Result<usize, SyncError> {
    let group = load_group(pool, group_id).await?;
    export_pending(pool, folder, &group).await
}

#[cfg(test)]
pub(crate) async fn export_pending_with_fault_for_test(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group_id: &str,
    group_key: &[u8],
    signing_secret: &[u8],
    phase: &str,
    sequence: i64,
) -> Result<usize, SyncError> {
    let signing_public_key = super::crypto::signing_public_from_secret(signing_secret)?;
    sqlx::query(
        "INSERT INTO device_sync_members (
             group_id,device_id,display_name,signing_public_key,
             exchange_public_key,fingerprint,key_epoch,status
         ) VALUES (?1,'local-device','Local test',?2,?2,'test-local',1,'trusted')
         ON CONFLICT(group_id,device_id) DO UPDATE SET signing_public_key=excluded.signing_public_key,status='trusted'",
    )
    .bind(group_id)
    .bind(signing_public_key)
    .execute(pool)
    .await?;
    let fault = match phase {
        "after_manifest" => ExportFault::AfterManifest(sequence),
        "after_event" => ExportFault::AfterEvent(sequence),
        "after_cas" => ExportFault::AfterCas(sequence),
        _ => ExportFault::None,
    };
    let group = load_group(pool, group_id).await?;
    export_pending_inner(
        pool,
        folder,
        &group,
        Some((group_key, signing_secret)),
        fault,
    )
    .await
}

fn operation_ids(operations: &[SyncOperation]) -> Vec<String> {
    operations
        .iter()
        .map(|operation| operation.operation_id.clone())
        .collect()
}

fn operation_fingerprint(operations: &[SyncOperation]) -> Result<String, SyncError> {
    Ok(sha256_hex(&serde_json::to_vec(operations)?))
}

fn operations_for_ids(
    rows: &[OutboxRow],
    expected_ids: &[String],
) -> Result<Vec<SyncOperation>, SyncError> {
    let by_id = rows
        .iter()
        .map(|row| (row.operation_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    expected_ids
        .iter()
        .map(|operation_id| {
            by_id
                .get(operation_id.as_str())
                .ok_or_else(|| {
                    SyncError::Integrity("导出草稿绑定的操作已不再处于待导出状态".to_string())
                })
                .and_then(|row| operation_from_outbox(row))
        })
        .collect()
}

fn validate_export_draft(
    group: &GroupRow,
    draft: ExportDraftRow,
    pending_rows: &[OutboxRow],
    crypto_validation: Option<(&[u8], &str)>,
) -> Result<PreparedExport, SyncError> {
    if draft.state != "prepared"
        || draft.group_id != group.id
        || draft.local_device_id != group.local_device_id
        || draft.sequence != group.next_sequence
        || draft.key_epoch != group.key_epoch
        || draft.previous_manifest_hash != group.last_manifest_hash
    {
        return Err(SyncError::Integrity(
            "导出草稿与当前同步组状态不一致".to_string(),
        ));
    }
    let expected_operation_ids: Vec<String> = serde_json::from_str(&draft.operation_ids_json)?;
    if expected_operation_ids.is_empty()
        || expected_operation_ids.len() > MAX_OPERATIONS_PER_EVENT
        || expected_operation_ids.iter().collect::<BTreeSet<_>>().len()
            != expected_operation_ids.len()
    {
        return Err(SyncError::Integrity("导出草稿的操作标识集无效".to_string()));
    }
    let operations = operations_for_ids(pending_rows, &expected_operation_ids)?;
    if operation_fingerprint(&operations)? != draft.operation_fingerprint {
        return Err(SyncError::Integrity(
            "导出草稿与当前待导出操作不一致".to_string(),
        ));
    }
    let event: EncryptedEnvelope = serde_json::from_slice(&draft.event_envelope_bytes)?;
    let manifest: EncryptedEnvelope = serde_json::from_slice(&draft.manifest_envelope_bytes)?;
    let sequence = u64::try_from(draft.sequence)
        .map_err(|_| SyncError::Integrity("导出草稿序列无效".to_string()))?;
    for (envelope, payload_kind, expected_hash) in [
        (&event, "operations", &draft.event_ciphertext_sha256),
        (&manifest, "manifest", &draft.manifest_ciphertext_sha256),
    ] {
        if envelope.header.protocol_version != PROTOCOL_VERSION
            || envelope.header.group_id != group.id
            || envelope.header.device_id != group.local_device_id
            || envelope.header.sequence != sequence
            || envelope.header.key_epoch != group.key_epoch as u32
            || envelope.header.payload_kind != payload_kind
            || &envelope.ciphertext_sha256 != expected_hash
        {
            return Err(SyncError::Integrity(
                "导出草稿加密信封与草稿身份不一致".to_string(),
            ));
        }
    }
    if let Some((group_key, signing_public_key)) = crypto_validation {
        let event_plaintext = open(&event, group_key, signing_public_key)?;
        let decrypted_operations: Vec<SyncOperation> = serde_json::from_slice(&event_plaintext)?;
        if operation_ids(&decrypted_operations) != expected_operation_ids
            || operation_fingerprint(&decrypted_operations)? != draft.operation_fingerprint
        {
            return Err(SyncError::Integrity(
                "导出草稿事件正文与待导出操作不一致".to_string(),
            ));
        }
        let manifest_plaintext = open(&manifest, group_key, signing_public_key)?;
        let manifest_payload: SyncManifest = serde_json::from_slice(&manifest_plaintext)?;
        if manifest_payload.group_id != group.id
            || manifest_payload.device_id != group.local_device_id
            || manifest_payload.sequence != sequence
            || manifest_payload.event_ciphertext_sha256 != event.ciphertext_sha256
            || manifest_payload.previous_manifest_hash != group.last_manifest_hash
        {
            return Err(SyncError::Integrity(
                "导出草稿 manifest 链与当前状态不一致".to_string(),
            ));
        }
    }
    Ok(PreparedExport {
        group_id: draft.group_id,
        local_device_id: draft.local_device_id,
        sequence: draft.sequence,
        key_epoch: draft.key_epoch,
        previous_manifest_hash: draft.previous_manifest_hash,
        event_envelope_bytes: draft.event_envelope_bytes,
        manifest_envelope_bytes: draft.manifest_envelope_bytes,
        event_ciphertext_sha256: draft.event_ciphertext_sha256,
        manifest_ciphertext_sha256: draft.manifest_ciphertext_sha256,
        operation_ids: expected_operation_ids,
        operation_fingerprint: draft.operation_fingerprint,
    })
}

async fn load_pending_rows(
    tx: &mut Transaction<'_, Sqlite>,
    group_id: &str,
) -> Result<Vec<OutboxRow>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT operation_id, entity_type, entity_id, case_id, action, base_revision,
                changed_fields_json, base_field_hashes_json, atomic_group,
                author_device_id, logical_time, capture_sequence, schema_version
         FROM device_sync_outbox
         WHERE group_id=?1 AND state='pending'
         ORDER BY capture_sequence",
    )
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await?)
}

async fn prepare_or_load_export(
    pool: &SqlitePool,
    group_id: &str,
    expected_local_device_id: &str,
    expected_key_epoch: i64,
    candidate_ids: &[String],
    group_key: &[u8],
    signing_secret: &[u8],
) -> Result<PreparedExport, SyncError> {
    let mut tx = pool.begin().await?;
    let locked = sqlx::query("UPDATE device_sync_groups SET updated_at=updated_at WHERE id=?1")
        .bind(group_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if locked != 1 {
        return Err(SyncError::NotFound("同步组不存在".to_string()));
    }
    let group: GroupRow = sqlx::query_as(
        "SELECT id, connector_root, local_device_id, key_epoch, next_sequence,
                paused, auto_paused, pause_reason_code, last_attempt_at,
                last_success_at, last_manifest_hash
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_one(&mut *tx)
    .await?;
    if group.local_device_id != expected_local_device_id || group.key_epoch != expected_key_epoch {
        return Err(SyncError::Busy);
    }
    let pending_rows = load_pending_rows(&mut tx, group_id).await?;
    let existing: Vec<ExportDraftRow> = sqlx::query_as(
        "SELECT group_id, local_device_id, sequence, key_epoch,
                previous_manifest_hash, event_envelope_bytes,
                manifest_envelope_bytes, event_ciphertext_sha256,
                manifest_ciphertext_sha256, operation_ids_json,
                operation_fingerprint, state
         FROM device_sync_export_drafts
         WHERE group_id=?1 AND state='prepared'
         ORDER BY sequence",
    )
    .bind(group_id)
    .fetch_all(&mut *tx)
    .await?;
    if existing.len() > 1 {
        return Err(SyncError::Integrity(
            "同步组存在多个待收尾导出草稿".to_string(),
        ));
    }
    if let Some(draft) = existing.into_iter().next() {
        validate_export_draft(&group, draft.clone(), &pending_rows, None)?;
        let signing_public_key: Option<String> = sqlx::query_scalar(
            "SELECT signing_public_key FROM device_sync_members
             WHERE group_id=?1 AND device_id=?2 AND status='trusted'",
        )
        .bind(&group.id)
        .bind(&group.local_device_id)
        .fetch_optional(&mut *tx)
        .await?;
        let signing_public_key = signing_public_key
            .ok_or_else(|| SyncError::Integrity("导出草稿缺少可信的本机签名公钥".to_string()))?;
        let prepared = validate_export_draft(
            &group,
            draft,
            &pending_rows,
            Some((group_key, &signing_public_key)),
        )?;
        tx.commit().await?;
        return Ok(prepared);
    }
    if candidate_ids.is_empty() {
        return Err(SyncError::Busy);
    }
    let operations = operations_for_ids(&pending_rows, candidate_ids)?;
    let ids = operation_ids(&operations);
    if ids != candidate_ids {
        return Err(SyncError::Integrity(
            "待导出操作的稳定顺序已变化".to_string(),
        ));
    }
    let operation_fingerprint = operation_fingerprint(&operations)?;
    let sequence = u64::try_from(group.next_sequence)
        .map_err(|_| SyncError::Integrity("导出序列无效".to_string()))?;
    let created_at = Utc::now().to_rfc3339();
    let event = seal(
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: group.id.clone(),
            device_id: group.local_device_id.clone(),
            sequence,
            key_epoch: group.key_epoch as u32,
            payload_kind: "operations".to_string(),
            created_at: created_at.clone(),
        },
        &serde_json::to_vec(&operations)?,
        group_key,
        signing_secret,
    )?;
    let manifest_plaintext = SyncManifest {
        group_id: group.id.clone(),
        device_id: group.local_device_id.clone(),
        sequence,
        event_ciphertext_sha256: event.ciphertext_sha256.clone(),
        previous_manifest_hash: group.last_manifest_hash.clone(),
        generated_at: created_at.clone(),
    };
    let manifest = seal(
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: group.id.clone(),
            device_id: group.local_device_id.clone(),
            sequence,
            key_epoch: group.key_epoch as u32,
            payload_kind: "manifest".to_string(),
            created_at,
        },
        &serde_json::to_vec(&manifest_plaintext)?,
        group_key,
        signing_secret,
    )?;
    let event_envelope_bytes = serde_json::to_vec(&event)?;
    let manifest_envelope_bytes = serde_json::to_vec(&manifest)?;
    sqlx::query(
        "INSERT INTO device_sync_export_drafts (
             group_id, local_device_id, sequence, key_epoch,
             previous_manifest_hash, event_envelope_bytes,
             manifest_envelope_bytes, event_ciphertext_sha256,
             manifest_ciphertext_sha256, operation_ids_json,
             operation_fingerprint, state
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'prepared')",
    )
    .bind(&group.id)
    .bind(&group.local_device_id)
    .bind(group.next_sequence)
    .bind(group.key_epoch)
    .bind(&group.last_manifest_hash)
    .bind(&event_envelope_bytes)
    .bind(&manifest_envelope_bytes)
    .bind(&event.ciphertext_sha256)
    .bind(&manifest.ciphertext_sha256)
    .bind(serde_json::to_string(&ids)?)
    .bind(&operation_fingerprint)
    .execute(&mut *tx)
    .await?;
    let prepared = PreparedExport {
        group_id: group.id,
        local_device_id: group.local_device_id,
        sequence: group.next_sequence,
        key_epoch: group.key_epoch,
        previous_manifest_hash: group.last_manifest_hash,
        event_envelope_bytes,
        manifest_envelope_bytes,
        event_ciphertext_sha256: event.ciphertext_sha256,
        manifest_ciphertext_sha256: manifest.ciphertext_sha256,
        operation_ids: ids,
        operation_fingerprint,
    };
    tx.commit().await?;
    Ok(prepared)
}

#[cfg(test)]
pub(crate) async fn prepare_next_export_for_test(
    pool: &SqlitePool,
    group_id: &str,
    group_key: &[u8],
    signing_secret: &[u8],
) -> Result<PreparedExport, SyncError> {
    let signing_public_key = super::crypto::signing_public_from_secret(signing_secret)?;
    sqlx::query(
        "INSERT INTO device_sync_members (
             group_id,device_id,display_name,signing_public_key,
             exchange_public_key,fingerprint,key_epoch,status
         ) VALUES (?1,'local-device','Local test',?2,?2,'test-local',1,'trusted')
         ON CONFLICT(group_id,device_id) DO UPDATE SET
             signing_public_key=excluded.signing_public_key,status='trusted'",
    )
    .bind(group_id)
    .bind(&signing_public_key)
    .execute(pool)
    .await?;
    let group = load_group(pool, group_id).await?;
    let existing_draft: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_export_drafts
         WHERE group_id=?1 AND state='prepared'",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    let candidate_ids = if existing_draft > 0 {
        Vec::new()
    } else {
        let (rows, operations, packages) = plan_pending_export(pool, &group).await?;
        if rows.is_empty() {
            return Err(SyncError::NotFound("没有待导出操作".to_string()));
        }
        packages
            .first()
            .ok_or_else(|| SyncError::Integrity("导出规划未产生数据包".to_string()))?
            .iter()
            .map(|index| operations[*index].operation_id.clone())
            .collect()
    };
    prepare_or_load_export(
        pool,
        group_id,
        &group.local_device_id,
        group.key_epoch,
        &candidate_ids,
        group_key,
        signing_secret,
    )
    .await
}

#[cfg(test)]
pub(crate) fn publish_prepared_export_for_test(
    folder: &MountedFolder,
    prepared: &PreparedExport,
    part: TestPublishPart,
) -> Result<(), SyncError> {
    let sequence = u64::try_from(prepared.sequence)
        .map_err(|_| SyncError::Integrity("导出草稿序列无效".to_string()))?;
    if matches!(
        part,
        TestPublishPart::ManifestOnly
            | TestPublishPart::Complete
            | TestPublishPart::FailAfterManifest
    ) {
        folder.write_manifest_bytes(
            &prepared.group_id,
            &prepared.local_device_id,
            sequence,
            &prepared.manifest_envelope_bytes,
        )?;
    }
    if matches!(part, TestPublishPart::FailAfterManifest) {
        return Err(SyncError::NasUnavailable(
            "injected failure after manifest publish".to_string(),
        ));
    }
    if matches!(part, TestPublishPart::EventOnly | TestPublishPart::Complete) {
        folder.write_event_bytes(
            &prepared.group_id,
            &prepared.local_device_id,
            sequence,
            &prepared.event_envelope_bytes,
        )?;
    }
    Ok(())
}

async fn export_pending(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group: &GroupRow,
) -> Result<usize, SyncError> {
    export_pending_inner(pool, folder, group, None, ExportFault::None).await
}

async fn export_pending_inner(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group: &GroupRow,
    test_key_material: Option<(&[u8], &[u8])>,
    fault: ExportFault,
) -> Result<usize, SyncError> {
    let mut exported = 0;
    loop {
        let current_group = load_group(pool, &group.id).await?;
        if current_group.local_device_id != group.local_device_id
            || current_group.key_epoch != group.key_epoch
        {
            return Err(SyncError::Busy);
        }
        let existing_draft: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM device_sync_export_drafts
             WHERE group_id=?1 AND state='prepared'",
        )
        .bind(&group.id)
        .fetch_one(pool)
        .await?;
        let loaded_signing_secret;
        let (candidate_ids, signing_secret) = if existing_draft > 0 {
            (Vec::new(), &[][..])
        } else {
            let (rows, operations, packages) = plan_pending_export(pool, &current_group).await?;
            if rows.is_empty() {
                break;
            }
            let candidate_ids = packages
                .first()
                .ok_or_else(|| SyncError::Integrity("导出规划未产生数据包".to_string()))?
                .iter()
                .map(|index| operations[*index].operation_id.clone())
                .collect();
            let signing_secret = if let Some((_, secret)) = test_key_material {
                secret
            } else {
                loaded_signing_secret = load_signing_secret(&group.id, &group.local_device_id)?;
                loaded_signing_secret.as_slice()
            };
            (candidate_ids, signing_secret)
        };
        let loaded_group_key;
        let group_key = if let Some((key, _)) = test_key_material {
            key
        } else {
            loaded_group_key =
                load_group_key(&group.id, &group.local_device_id, group.key_epoch as u32)?;
            loaded_group_key.as_slice()
        };
        let prepared = prepare_or_load_export(
            pool,
            &group.id,
            &group.local_device_id,
            group.key_epoch,
            &candidate_ids,
            group_key,
            signing_secret,
        )
        .await?;
        publish_prepared_export(folder, &prepared, fault)?;
        let operation_count = prepared.operation_ids.len();
        let fail_after_cas = fault.after_cas(prepared.sequence);
        finalize_prepared_export_inner(pool, &prepared, fail_after_cas).await?;
        exported += operation_count;
    }
    Ok(exported)
}

fn publish_prepared_export(
    folder: &MountedFolder,
    prepared: &PreparedExport,
    fault: ExportFault,
) -> Result<(), SyncError> {
    let sequence = u64::try_from(prepared.sequence)
        .map_err(|_| SyncError::Integrity("导出草稿序列无效".to_string()))?;
    folder.write_manifest_bytes(
        &prepared.group_id,
        &prepared.local_device_id,
        sequence,
        &prepared.manifest_envelope_bytes,
    )?;
    if fault.after_manifest(prepared.sequence) {
        return Err(SyncError::NasUnavailable(
            "injected failure after manifest".to_string(),
        ));
    }
    folder.write_event_bytes(
        &prepared.group_id,
        &prepared.local_device_id,
        sequence,
        &prepared.event_envelope_bytes,
    )?;
    if fault.after_event(prepared.sequence) {
        return Err(SyncError::NasUnavailable(
            "injected failure after event".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn finalize_prepared_export_for_test(
    pool: &SqlitePool,
    prepared: &PreparedExport,
    fail_after_cas: bool,
) -> Result<(), SyncError> {
    finalize_prepared_export_inner(pool, prepared, fail_after_cas).await
}

async fn finalize_prepared_export_inner(
    pool: &SqlitePool,
    prepared: &PreparedExport,
    fail_after_cas: bool,
) -> Result<(), SyncError> {
    let mut tx = pool.begin().await?;
    let group: GroupRow = sqlx::query_as(
        "SELECT id, connector_root, local_device_id, key_epoch, next_sequence,
                paused, auto_paused, pause_reason_code, last_attempt_at,
                last_success_at, last_manifest_hash
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(&prepared.group_id)
    .fetch_one(&mut *tx)
    .await?;
    let draft: ExportDraftRow = sqlx::query_as(
        "SELECT group_id, local_device_id, sequence, key_epoch,
                previous_manifest_hash, event_envelope_bytes,
                manifest_envelope_bytes, event_ciphertext_sha256,
                manifest_ciphertext_sha256, operation_ids_json,
                operation_fingerprint, state
         FROM device_sync_export_drafts
         WHERE group_id=?1 AND local_device_id=?2 AND sequence=?3 AND state='prepared'",
    )
    .bind(&prepared.group_id)
    .bind(&prepared.local_device_id)
    .bind(prepared.sequence)
    .fetch_one(&mut *tx)
    .await?;
    let pending_rows = load_pending_rows(&mut tx, &prepared.group_id).await?;
    let current = validate_export_draft(&group, draft, &pending_rows, None)?;
    if current.event_envelope_bytes != prepared.event_envelope_bytes
        || current.manifest_envelope_bytes != prepared.manifest_envelope_bytes
        || current.event_ciphertext_sha256 != prepared.event_ciphertext_sha256
        || current.manifest_ciphertext_sha256 != prepared.manifest_ciphertext_sha256
        || current.operation_ids != prepared.operation_ids
        || current.operation_fingerprint != prepared.operation_fingerprint
    {
        return Err(SyncError::Integrity("导出草稿在发布期间已变化".to_string()));
    }
    let advanced = sqlx::query(
        "UPDATE device_sync_groups
         SET next_sequence=next_sequence+1, last_manifest_hash=?6, updated_at=datetime('now')
         WHERE id=?1 AND local_device_id=?2 AND key_epoch=?3 AND next_sequence=?4
           AND ((last_manifest_hash IS NULL AND ?5 IS NULL) OR last_manifest_hash=?5)",
    )
    .bind(&prepared.group_id)
    .bind(&prepared.local_device_id)
    .bind(prepared.key_epoch)
    .bind(prepared.sequence)
    .bind(&prepared.previous_manifest_hash)
    .bind(&prepared.manifest_ciphertext_sha256)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if advanced != 1 {
        return Err(SyncError::Busy);
    }
    if fail_after_cas {
        tx.rollback().await?;
        return Err(SyncError::Busy);
    }
    for operation_id in &prepared.operation_ids {
        let changed = sqlx::query(
            "UPDATE device_sync_outbox
             SET state='exported', exported_sequence=?1, updated_at=datetime('now')
             WHERE group_id=?2 AND operation_id=?3 AND state='pending'",
        )
        .bind(prepared.sequence)
        .bind(&prepared.group_id)
        .bind(operation_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(SyncError::Integrity("导出草稿操作状态已变化".to_string()));
        }
    }
    let sequence = u64::try_from(prepared.sequence)
        .map_err(|_| SyncError::Integrity("导出草稿序列无效".to_string()))?;
    resolve_active_quarantine(
        &mut tx,
        &prepared.group_id,
        &prepared.local_device_id,
        sequence,
        None,
    )
    .await?;
    let deleted = sqlx::query(
        "DELETE FROM device_sync_export_drafts
         WHERE group_id=?1 AND local_device_id=?2 AND sequence=?3 AND state='prepared'",
    )
    .bind(&prepared.group_id)
    .bind(&prepared.local_device_id)
    .bind(prepared.sequence)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if deleted != 1 {
        return Err(SyncError::Integrity("导出草稿收尾状态已变化".to_string()));
    }
    tx.commit().await?;
    Ok(())
}

async fn import_event(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group: &GroupRow,
    member: &MemberRow,
    sequence: u64,
    path: &std::path::Path,
) -> Result<(Vec<ApplyOutcome>, String), SyncError> {
    let envelope = folder.read_envelope(path)?;
    if envelope.header.group_id != group.id
        || envelope.header.device_id != member.device_id
        || envelope.header.sequence != sequence
        || envelope.header.key_epoch == 0
        || envelope.header.key_epoch as i64 > group.key_epoch
        || envelope.header.payload_kind != "operations"
    {
        return Err(SyncError::Integrity(
            "信封头与成员、组、序号或密钥时代不一致".to_string(),
        ));
    }
    let key = load_group_key(&group.id, &group.local_device_id, envelope.header.key_epoch)?;
    let manifest_path = folder.manifest_path(&group.id, &member.device_id, sequence)?;
    let manifest_envelope = folder.read_envelope(&manifest_path)?;
    if manifest_envelope.header.group_id != group.id
        || manifest_envelope.header.device_id != member.device_id
        || manifest_envelope.header.sequence != sequence
        || manifest_envelope.header.key_epoch != envelope.header.key_epoch
        || manifest_envelope.header.payload_kind != "manifest"
    {
        return Err(SyncError::Integrity(
            "加密 manifest 头与事件不一致".to_string(),
        ));
    }
    let manifest_plaintext = open(&manifest_envelope, &key, &member.signing_public_key)?;
    let manifest: SyncManifest = serde_json::from_slice(&manifest_plaintext)?;
    if manifest.group_id != group.id
        || manifest.device_id != member.device_id
        || manifest.sequence != sequence
        || manifest.event_ciphertext_sha256 != envelope.ciphertext_sha256
        || manifest.previous_manifest_hash != member.last_manifest_hash
    {
        return Err(SyncError::Integrity(
            "manifest 链回退、分叉或事件哈希不匹配".to_string(),
        ));
    }
    let plaintext = open(&envelope, &key, &member.signing_public_key)?;
    let operations: Vec<SyncOperation> = serde_json::from_slice(&plaintext)?;
    if operations.len() > MAX_OPERATIONS_PER_EVENT {
        return Err(SyncError::FuseTriggered(
            "单个事件超过 500 项变化".to_string(),
        ));
    }
    let entity_total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_entity_revisions
         WHERE group_id=?1 AND tombstoned=0",
    )
    .bind(&group.id)
    .fetch_one(pool)
    .await?;
    let tombstones = operations
        .iter()
        .filter(|operation| operation.action == OperationAction::Tombstone)
        .count() as i64;
    if entity_total >= 20 && tombstones * 5 > entity_total {
        return Err(SyncError::FuseTriggered(
            "单轮删除超过当前同步实体的 20%".to_string(),
        ));
    }
    if entity_total >= 20 && (operations.len() as i64) * 5 > entity_total {
        return Err(SyncError::FuseTriggered(
            "单轮修改超过当前同步实体的 20%".to_string(),
        ));
    }

    let mut tx = pool.begin().await?;
    let outcomes = match apply_incoming_package(
        &mut tx,
        &group.id,
        &member.device_id,
        sequence,
        &operations,
        &envelope.ciphertext_sha256,
    )
    .await
    {
        Ok(outcomes) => outcomes,
        Err(error) => {
            tx.rollback().await?;
            return Err(error);
        }
    };
    sqlx::query(
        "UPDATE device_sync_members
         SET last_seen_sequence=?1, last_manifest_hash=?4, updated_at=datetime('now')
         WHERE group_id=?2 AND device_id=?3 AND last_seen_sequence<?1",
    )
    .bind(sequence as i64)
    .bind(&group.id)
    .bind(&member.device_id)
    .bind(&manifest_envelope.ciphertext_sha256)
    .execute(&mut *tx)
    .await?;
    resolve_active_quarantine(&mut tx, &group.id, &member.device_id, sequence, Some(path)).await?;
    tx.commit().await?;
    Ok((outcomes, manifest_envelope.ciphertext_sha256))
}

async fn load_group(pool: &SqlitePool, group_id: &str) -> Result<GroupRow, SyncError> {
    sqlx::query_as(
        "SELECT id, connector_root, local_device_id, key_epoch, next_sequence, paused,
                auto_paused,pause_reason_code,last_attempt_at,last_success_at,
                last_manifest_hash
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))
}

fn safe_source_file(source_path: Option<&str>) -> Option<String> {
    source_path.and_then(|path| {
        std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
    })
}

fn is_deterministic_event_error(error: &SyncError) -> bool {
    matches!(
        error,
        SyncError::Serialization(_)
            | SyncError::Crypto(_)
            | SyncError::Integrity(_)
            | SyncError::Protocol(_)
            | SyncError::EntityNotAllowed(_)
            | SyncError::FieldNotAllowed { .. }
            | SyncError::FuseTriggered(_)
            | SyncError::PackageDependencyMissing
            | SyncError::PackageTooLarge
            | SyncError::PackageDependencyConflict
            | SyncError::NotFound(_)
    )
}

fn is_deterministic_export_error(error: &SyncError) -> bool {
    matches!(
        error,
        SyncError::Serialization(_)
            | SyncError::Integrity(_)
            | SyncError::Protocol(_)
            | SyncError::EntityNotAllowed(_)
            | SyncError::FieldNotAllowed { .. }
            | SyncError::PackageDependencyMissing
            | SyncError::PackageTooLarge
            | SyncError::PackageDependencyConflict
    )
}

async fn quarantine_export_error(
    pool: &SqlitePool,
    group: &GroupRow,
    error: &SyncError,
) -> Result<(), SyncError> {
    quarantine_and_auto_pause(
        pool,
        &group.id,
        Some(&group.local_device_id),
        Some(group.next_sequence as u64),
        None,
        error.code(),
    )
    .await
}

async fn quarantine_and_auto_pause(
    pool: &SqlitePool,
    group_id: &str,
    device_id: Option<&str>,
    sequence: Option<u64>,
    source_path: Option<&str>,
    reason_code: &str,
) -> Result<(), SyncError> {
    let safe_source = safe_source_file(source_path);
    let package_device_id = device_id.unwrap_or("__event_list__");
    let package_sequence = sequence.map(|value| value as i64).unwrap_or(-1);
    let details = serde_json::json!({
        "error_code": reason_code,
        "device_id": device_id,
        "sequence": sequence,
        "source_file": safe_source
    });
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO device_sync_quarantine (
             id, group_id, source_path, source_device_id, source_sequence,
             reason_code, details_json,
             status,first_seen_at,last_seen_at,retry_count,last_error_code
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,'active',datetime('now'),datetime('now'),1,?6)
         ON CONFLICT DO UPDATE SET
             details_json=excluded.details_json,
             last_seen_at=datetime('now'),
             retry_count=device_sync_quarantine.retry_count+1,
             last_error_code=excluded.last_error_code",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(&safe_source)
    .bind(package_device_id)
    .bind(package_sequence)
    .bind(reason_code)
    .bind(details.to_string())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE device_sync_groups
         SET paused=1,auto_paused=1,pause_reason_code=?2,updated_at=datetime('now')
         WHERE id=?1",
    )
    .bind(group_id)
    .bind(reason_code)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO device_sync_audits (
             id,group_id,device_id,action,outcome,details_json
         ) VALUES (?1,?2,?3,'sync_package','paused',?4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(device_id)
    .bind(details.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn ensure_no_active_quarantine(
    pool: &SqlitePool,
    group_id: &str,
    local_device_id: &str,
) -> Result<(), SyncError> {
    let active: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT reason_code,source_path FROM device_sync_quarantine
         WHERE group_id=?1 AND status='active'
         ORDER BY first_seen_at,id LIMIT 1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?;
    let Some((reason_code, source_file)) = active else {
        return Ok(());
    };
    let details = serde_json::json!({
        "error_code": reason_code,
        "device_id": local_device_id,
        "source_file": source_file,
        "active_quarantine": true
    });
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE device_sync_groups
         SET paused=1,auto_paused=1,pause_reason_code=?2,updated_at=datetime('now')
         WHERE id=?1",
    )
    .bind(group_id)
    .bind(&reason_code)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO device_sync_audits (
             id,group_id,device_id,action,outcome,details_json
         ) VALUES (?1,?2,?3,'active_quarantine','paused',?4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(local_device_id)
    .bind(details.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Err(SyncError::GroupAutoPaused)
}

async fn record_sync_success(
    pool: &SqlitePool,
    group_id: &str,
    local_device_id: &str,
    details: serde_json::Value,
) -> Result<(), SyncError> {
    let mut tx = pool.begin().await?;
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_quarantine
         WHERE group_id=?1 AND status='active'",
    )
    .bind(group_id)
    .fetch_one(&mut *tx)
    .await?;
    if active != 0 {
        tx.rollback().await?;
        return ensure_no_active_quarantine(pool, group_id, local_device_id).await;
    }
    sqlx::query(
        "UPDATE device_sync_groups
         SET last_synced_at=datetime('now'),last_success_at=datetime('now'),
             updated_at=datetime('now')
         WHERE id=?1",
    )
    .bind(group_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO device_sync_audits (
             id,group_id,device_id,action,outcome,details_json
         ) VALUES (?1,?2,?3,'sync_once','succeeded',?4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(local_device_id)
    .bind(details.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn resolve_active_quarantine(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    group_id: &str,
    source_device_id: &str,
    source_sequence: u64,
    source_path: Option<&std::path::Path>,
) -> Result<u64, SyncError> {
    let safe_source =
        source_path.and_then(|path| safe_source_file(Some(path.to_string_lossy().as_ref())));
    let affected = sqlx::query(
        "UPDATE device_sync_quarantine
         SET status='resolved',resolved_at=datetime('now'),last_seen_at=datetime('now')
         WHERE group_id=?1 AND source_device_id=?2 AND source_sequence=?3
           AND status='active'",
    )
    .bind(group_id)
    .bind(source_device_id)
    .bind(source_sequence as i64)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if affected != 0 {
        sqlx::query(
            "INSERT INTO device_sync_audits (
                 id,group_id,device_id,action,outcome,details_json
             ) VALUES (?1,?2,?3,'quarantine_resolved','succeeded',?4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(group_id)
        .bind(source_device_id)
        .bind(
            serde_json::json!({
                "device_id": source_device_id,
                "sequence": source_sequence,
                "source_file": safe_source,
                "resolved_count": affected
            })
            .to_string(),
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(affected)
}

#[cfg(test)]
async fn quarantine(
    pool: &SqlitePool,
    group_id: &str,
    source_device_id: &str,
    source_sequence: u64,
    source_path: Option<&str>,
    reason_code: &str,
    details: serde_json::Value,
) -> Result<(), SyncError> {
    let safe_source = safe_source_file(source_path);
    sqlx::query(
        "INSERT INTO device_sync_quarantine (
             id, group_id, source_path, source_device_id, source_sequence,
             reason_code, details_json,
             status,first_seen_at,last_seen_at,retry_count,last_error_code
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,'active',datetime('now'),datetime('now'),1,?6)
         ON CONFLICT DO UPDATE SET
             details_json=excluded.details_json,
             last_seen_at=datetime('now'),
             retry_count=device_sync_quarantine.retry_count+1,
             last_error_code=excluded.last_error_code",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(safe_source)
    .bind(source_device_id)
    .bind(source_sequence as i64)
    .bind(reason_code)
    .bind(details.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn audit(
    pool: &SqlitePool,
    group_id: Option<&str>,
    device_id: Option<&str>,
    action: &str,
    outcome: &str,
    details: serde_json::Value,
) -> Result<(), SyncError> {
    sqlx::query(
        "INSERT INTO device_sync_audits (
             id, group_id, device_id, action, outcome, details_json
         ) VALUES (?1,?2,?3,?4,?5,?6)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(device_id)
    .bind(action)
    .bind(outcome)
    .bind(details.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) async fn quarantine_for_test(
    pool: &SqlitePool,
    group_id: &str,
    source_device_id: &str,
    source_sequence: u64,
    source_path: Option<&str>,
    reason_code: &str,
    details: serde_json::Value,
) -> Result<(), SyncError> {
    quarantine(
        pool,
        group_id,
        source_device_id,
        source_sequence,
        source_path,
        reason_code,
        details,
    )
    .await
}

#[cfg(test)]
pub(crate) async fn auto_pause_failure_for_test(
    pool: &SqlitePool,
    group_id: &str,
    device_id: &str,
    sequence: u64,
    source_path: &str,
    reason_code: &str,
) -> Result<(), SyncError> {
    quarantine_and_auto_pause(
        pool,
        group_id,
        Some(device_id),
        Some(sequence),
        Some(source_path),
        reason_code,
    )
    .await?;
    Err(SyncError::GroupAutoPaused)
}

#[cfg(test)]
pub(crate) async fn auto_pause_export_failure_for_test(
    pool: &SqlitePool,
    group_id: &str,
    error: SyncError,
) -> Result<(), SyncError> {
    let group = load_group(pool, group_id).await?;
    if !is_deterministic_export_error(&error) {
        return Err(error);
    }
    quarantine_export_error(pool, &group, &error).await?;
    Err(SyncError::GroupAutoPaused)
}

#[cfg(test)]
pub(crate) async fn resolve_active_quarantine_for_test(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    group_id: &str,
    source_device_id: &str,
    source_sequence: u64,
    source_path: &std::path::Path,
) -> Result<u64, SyncError> {
    resolve_active_quarantine(
        tx,
        group_id,
        source_device_id,
        source_sequence,
        Some(source_path),
    )
    .await
}

#[cfg(test)]
pub(crate) async fn record_sync_success_for_test(
    pool: &SqlitePool,
    group_id: &str,
    local_device_id: &str,
) -> Result<(), SyncError> {
    record_sync_success(
        pool,
        group_id,
        local_device_id,
        serde_json::json!({"fixture": "v083-lifecycle"}),
    )
    .await
}

#[cfg(test)]
pub(crate) async fn mark_sync_attempt_for_test(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<(), SyncError> {
    mark_sync_attempt(pool, group_id).await
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) async fn audit_for_test(
    pool: &SqlitePool,
    group_id: Option<&str>,
    device_id: Option<&str>,
    action: &str,
    outcome: &str,
    details: serde_json::Value,
) -> Result<(), SyncError> {
    audit(pool, group_id, device_id, action, outcome, details).await
}
