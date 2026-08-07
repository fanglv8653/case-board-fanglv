use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteArguments, SqliteRow};
use sqlx::SqlitePool;
use sqlx::{Arguments, Row, Sqlite, Transaction};

use super::registry;
use super::SyncError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    pub operation_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub case_id: Option<String>,
    pub action: OperationAction,
    pub base_revision: i64,
    pub changed_fields: BTreeMap<String, Value>,
    pub base_field_hashes: BTreeMap<String, String>,
    pub atomic_group: Option<String>,
    pub author_device_id: String,
    pub logical_time: i64,
    #[serde(default)]
    pub capture_sequence: i64,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationAction {
    Upsert,
    Tombstone,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub operation_id: String,
    pub applied_fields: Vec<String>,
    pub conflict_fields: Vec<String>,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    KeepLocal,
    KeepRemote,
    Manual,
}

pub async fn resolve_operation_conflicts(
    pool: &SqlitePool,
    operation_id: &str,
    resolution: ConflictResolution,
    manual_fields: Option<BTreeMap<String, Value>>,
) -> Result<usize, SyncError> {
    #[derive(sqlx::FromRow)]
    struct ConflictRow {
        id: String,
        group_id: String,
        entity_type: String,
        entity_id: String,
        field_key: String,
        remote_value_json: Option<String>,
    }
    let mut tx = pool.begin().await?;
    let rows: Vec<ConflictRow> = sqlx::query_as(
        "SELECT id, group_id, entity_type, entity_id, field_key, remote_value_json
         FROM device_sync_conflicts
         WHERE operation_id=?1 AND status='pending'
         ORDER BY field_key",
    )
    .bind(operation_id)
    .fetch_all(&mut *tx)
    .await?;
    if rows.is_empty() {
        return Err(SyncError::NotFound(format!(
            "没有待处理冲突: {operation_id}"
        )));
    }
    let first = &rows[0];
    if rows.iter().any(|row| {
        row.group_id != first.group_id
            || row.entity_type != first.entity_type
            || row.entity_id != first.entity_id
    }) {
        return Err(SyncError::Integrity("同一操作的冲突实体不一致".to_string()));
    }
    let policy = registry::policy(&first.entity_type)?;
    let mut values = BTreeMap::new();
    match resolution {
        ConflictResolution::KeepLocal => {}
        ConflictResolution::KeepRemote => {
            for row in &rows {
                if row.field_key == "_tombstone" {
                    continue;
                }
                let raw = row.remote_value_json.as_deref().ok_or_else(|| {
                    SyncError::Integrity(format!("冲突缺少远端候选: {}", row.field_key))
                })?;
                values.insert(row.field_key.clone(), serde_json::from_str(raw)?);
            }
        }
        ConflictResolution::Manual => {
            let manual = manual_fields
                .ok_or_else(|| SyncError::Protocol("手工解决必须提供字段值".to_string()))?;
            let expected = rows
                .iter()
                .filter(|row| row.field_key != "_tombstone")
                .map(|row| row.field_key.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let actual = manual
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            if expected != actual {
                return Err(SyncError::Protocol(
                    "手工解决字段必须与本操作的待处理冲突完全一致".to_string(),
                ));
            }
            let map: Map<String, Value> = manual
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            values = registry::sanitize_fields(&first.entity_type, &map)?;
        }
    }
    if !values.is_empty() {
        apply_upsert(&mut tx, policy, &first.entity_id, &values, true).await?;
    }
    let status = match resolution {
        ConflictResolution::KeepLocal => "resolved_local",
        ConflictResolution::KeepRemote => "resolved_remote",
        ConflictResolution::Manual => "resolved_manual",
    };
    for row in &rows {
        let resolution_value = values.get(&row.field_key).map(Value::to_string);
        sqlx::query(
            "UPDATE device_sync_conflicts
             SET status=?1, resolution_value_json=?2, resolved_at=datetime('now'),
                 updated_at=datetime('now')
             WHERE id=?3 AND status='pending'",
        )
        .bind(status)
        .bind(resolution_value)
        .bind(&row.id)
        .execute(&mut *tx)
        .await?;
    }
    if !values.is_empty() {
        let after = fetch_entity(&mut tx, policy, &first.entity_id)
            .await?
            .ok_or_else(|| SyncError::NotFound("冲突实体不存在".to_string()))?;
        let hashes = hash_fields(&after);
        sqlx::query(
            "UPDATE device_sync_entity_revisions
             SET revision=revision+1, field_hashes_json=?1, tombstoned=0,
                 updated_at=datetime('now')
             WHERE group_id=?2 AND entity_type=?3 AND entity_id=?4",
        )
        .bind(serde_json::to_string(&hashes)?)
        .bind(&first.group_id)
        .bind(&first.entity_type)
        .bind(&first.entity_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(rows.len())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_operation(
    tx: &mut Transaction<'_, Sqlite>,
    group_id: &str,
    entity_type: &str,
    entity_id: &str,
    case_id: Option<&str>,
    action: OperationAction,
    fields: Map<String, Value>,
    changed_field_names: &[String],
) -> Result<String, SyncError> {
    super::capture::lock_capture_sequence_group(tx, group_id).await?;
    let clean = registry::sanitize_fields(entity_type, &fields)?;
    let current_revision: Option<(i64, String)> = sqlx::query_as(
        "SELECT revision, field_hashes_json
         FROM device_sync_entity_revisions
         WHERE group_id = ?1 AND entity_type = ?2 AND entity_id = ?3",
    )
    .bind(group_id)
    .bind(entity_type)
    .bind(entity_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (base_revision, prior_hashes) = current_revision
        .map(|(revision, hashes)| {
            let hashes =
                serde_json::from_str::<BTreeMap<String, String>>(&hashes).unwrap_or_default();
            (revision, hashes)
        })
        .unwrap_or_default();
    let changed: BTreeMap<String, Value> = changed_field_names
        .iter()
        .map(|name| {
            clean
                .get(name)
                .cloned()
                .map(|value| (name.clone(), value))
                .ok_or_else(|| SyncError::Protocol(format!("变更字段不存在: {name}")))
        })
        .collect::<Result<_, _>>()?;
    if changed.is_empty() && action == OperationAction::Upsert {
        return Err(SyncError::Protocol("变更字段不能为空".to_string()));
    }
    let base_field_hashes = changed
        .keys()
        .filter_map(|name| {
            prior_hashes
                .get(name)
                .map(|hash| (name.clone(), hash.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let atomic_group = changed
        .keys()
        .filter_map(|field| registry::atomic_group_for_field(entity_type, field))
        .next()
        .map(str::to_string);
    if let Some(group) = atomic_group.as_deref() {
        let required = registry::atomic_group_fields(entity_type, group)?;
        if !required.iter().all(|field| clean.contains_key(*field)) {
            return Err(SyncError::Protocol(format!(
                "原子字段组 {group} 必须携带完整当前值"
            )));
        }
    }

    let operation_id = uuid::Uuid::new_v4().to_string();
    let logical_time: i64 =
        sqlx::query_scalar("SELECT CAST(strftime('%s','now') AS INTEGER) * 1000")
            .fetch_one(&mut **tx)
            .await?;
    let local_device_id: String =
        sqlx::query_scalar("SELECT local_device_id FROM device_sync_groups WHERE id = ?1")
            .bind(group_id)
            .fetch_one(&mut **tx)
            .await?;
    let capture_sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(capture_sequence),0)+1
         FROM device_sync_outbox WHERE group_id=?1",
    )
    .bind(group_id)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO device_sync_outbox (
             operation_id, group_id, entity_type, entity_id, case_id, action,
             base_revision, changed_fields_json, base_field_hashes_json, atomic_group,
             author_device_id, logical_time, capture_sequence, schema_version
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,1)",
    )
    .bind(&operation_id)
    .bind(group_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(case_id)
    .bind(match action {
        OperationAction::Upsert => "upsert",
        OperationAction::Tombstone => "tombstone",
    })
    .bind(base_revision)
    .bind(serde_json::to_string(&changed)?)
    .bind(serde_json::to_string(&base_field_hashes)?)
    .bind(atomic_group)
    .bind(local_device_id)
    .bind(logical_time)
    .bind(capture_sequence)
    .execute(&mut **tx)
    .await?;
    Ok(operation_id)
}

#[derive(Debug, Clone)]
struct VirtualJudgeState {
    entity_exists: bool,
    judge_id: Option<String>,
}

fn dependency_value<'a>(
    operation: &'a SyncOperation,
    field: &str,
) -> Result<Option<&'a str>, SyncError> {
    match operation.changed_fields.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.as_str())),
        Some(Value::String(_)) => Err(SyncError::Protocol(format!(
            "依赖字段不能为空: {}/{}",
            operation.entity_type, field
        ))),
        Some(_) => Err(SyncError::Protocol(format!(
            "依赖字段必须是字符串或 null: {}/{}",
            operation.entity_type, field
        ))),
    }
}

fn hash_json_value(value: &Value) -> String {
    Sha256::digest(serde_json::to_vec(value).unwrap_or_default())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn load_virtual_judge_state(
    tx: &mut Transaction<'_, Sqlite>,
    case_id: &str,
) -> Result<VirtualJudgeState, SyncError> {
    let row: Option<Option<String>> = sqlx::query_scalar("SELECT judge_id FROM cases WHERE id=?1")
        .bind(case_id)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(match row {
        Some(judge_id) => VirtualJudgeState {
            entity_exists: true,
            judge_id,
        },
        None => VirtualJudgeState {
            entity_exists: false,
            judge_id: None,
        },
    })
}

async fn record_judge_conflict(
    tx: &mut Transaction<'_, Sqlite>,
    group_id: &str,
    operation: &SyncOperation,
    local_judge: Option<&str>,
    remote_judge: Option<&str>,
) -> Result<(), SyncError> {
    sqlx::query(
        "INSERT OR IGNORE INTO device_sync_conflicts (
             id, group_id, operation_id, entity_type, entity_id, case_id,
             field_key, atomic_group, base_value_hash, local_value_json, remote_value_json
         ) VALUES (?1,?2,?3,'case',?4,?5,'judge_id',?6,?7,?8,?9)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(&operation.operation_id)
    .bind(&operation.entity_id)
    .bind(&operation.case_id)
    .bind(&operation.atomic_group)
    .bind(operation.base_field_hashes.get("judge_id"))
    .bind(
        local_judge
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null)
            .to_string(),
    )
    .bind(
        remote_judge
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null)
            .to_string(),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn receiver_has_entity(
    tx: &mut Transaction<'_, Sqlite>,
    entity_type: &str,
    entity_id: &str,
) -> Result<bool, SyncError> {
    let sql = match entity_type {
        "case" => "SELECT EXISTS(SELECT 1 FROM cases WHERE id=?1)",
        "contact" => "SELECT EXISTS(SELECT 1 FROM contacts WHERE id=?1)",
        _ => return Err(SyncError::Protocol("未知依赖实体类型".to_string())),
    };
    Ok(sqlx::query_scalar::<_, i64>(sql)
        .bind(entity_id)
        .fetch_one(&mut **tx)
        .await?
        != 0)
}

async fn validate_package_dependencies(
    tx: &mut Transaction<'_, Sqlite>,
    operations: &[SyncOperation],
) -> Result<(), SyncError> {
    let mut package_final_actions = BTreeMap::new();
    for operation in operations {
        package_final_actions.insert(
            (operation.entity_type.as_str(), operation.entity_id.as_str()),
            operation.action,
        );
    }

    for operation in operations {
        if operation.action != OperationAction::Upsert {
            continue;
        }
        let dependency = match operation.entity_type.as_str() {
            "case" => {
                dependency_value(operation, "judge_id")?.map(|entity_id| ("contact", entity_id))
            }
            "contact" => {
                dependency_value(operation, "case_id")?.map(|entity_id| ("case", entity_id))
            }
            _ => None,
        };
        let Some((entity_type, entity_id)) = dependency else {
            continue;
        };
        if let Some(final_action) = package_final_actions.get(&(entity_type, entity_id)) {
            if *final_action == OperationAction::Upsert {
                continue;
            }
            return Err(SyncError::PackageDependencyConflict);
        }
        if receiver_has_entity(tx, entity_type, entity_id).await? {
            continue;
        }
        return Err(SyncError::PackageDependencyMissing);
    }
    Ok(())
}

async fn classify_package_operations(
    tx: &mut Transaction<'_, Sqlite>,
    source_device_id: &str,
    source_sequence: u64,
    operations: &[SyncOperation],
    payload_hash: &str,
) -> Result<(Vec<usize>, Vec<Option<ApplyOutcome>>), SyncError> {
    let mut package_ids = BTreeSet::new();
    if operations
        .iter()
        .any(|operation| !package_ids.insert(operation.operation_id.as_str()))
    {
        return Err(SyncError::Integrity(
            "signed event contains duplicate operation_id".to_string(),
        ));
    }
    let mut pending = Vec::new();
    let mut outcomes = vec![None; operations.len()];
    for (index, operation) in operations.iter().enumerate() {
        let already: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT source_device_id,source_sequence,payload_hash
             FROM device_sync_applied_operations WHERE operation_id=?1",
        )
        .bind(&operation.operation_id)
        .fetch_optional(&mut **tx)
        .await?;
        match already {
            None => pending.push(index),
            Some((device, sequence, hash))
                if device == source_device_id
                    && sequence == source_sequence as i64
                    && hash == payload_hash =>
            {
                outcomes[index] = Some(ApplyOutcome {
                    operation_id: operation.operation_id.clone(),
                    applied_fields: Vec::new(),
                    conflict_fields: Vec::new(),
                    duplicate: true,
                });
            }
            Some(_) => {
                return Err(SyncError::Integrity(
                    "operation_id 已被不同来源或载荷使用".to_string(),
                ));
            }
        }
    }
    Ok((pending, outcomes))
}

fn stable_dependency_order(
    operations: &[SyncOperation],
    pending: &[usize],
) -> Result<Vec<usize>, SyncError> {
    let pending_set = pending.iter().copied().collect::<BTreeSet<_>>();
    let mut entity_operations: BTreeMap<(&str, &str), Vec<usize>> = BTreeMap::new();
    for index in pending {
        let operation = &operations[*index];
        entity_operations
            .entry((&operation.entity_type, &operation.entity_id))
            .or_default()
            .push(*index);
    }
    let mut edges = BTreeSet::new();
    for indexes in entity_operations.values() {
        for pair in indexes.windows(2) {
            edges.insert((pair[0], pair[1]));
        }
    }
    for index in pending {
        let operation = &operations[*index];
        if operation.action != OperationAction::Upsert || operation.entity_type != "contact" {
            continue;
        }
        let Some(case_id) = dependency_value(operation, "case_id")? else {
            continue;
        };
        if let Some(provider) = entity_operations
            .get(&("case", case_id))
            .and_then(|indexes| indexes.last())
        {
            edges.insert((*provider, *index));
        }
    }

    let mut outgoing: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut indegree = pending
        .iter()
        .map(|index| (*index, 0_usize))
        .collect::<BTreeMap<_, _>>();
    for (from, to) in edges {
        if from == to || !pending_set.contains(&from) || !pending_set.contains(&to) {
            continue;
        }
        outgoing.entry(from).or_default().push(to);
        *indegree.get_mut(&to).expect("pending node") += 1;
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(index, degree)| (*degree == 0).then_some(*index))
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(pending.len());
    while let Some(index) = ready.pop_first() {
        order.push(index);
        for next in outgoing.get(&index).into_iter().flatten() {
            let degree = indegree.get_mut(next).expect("pending node");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(*next);
            }
        }
    }
    if order.len() != pending.len() {
        return Err(SyncError::PackageDependencyConflict);
    }
    Ok(order)
}

/// Applies one authenticated event as a dependency-checked atomic package.
///
/// The caller owns the transaction so member sequence advancement can commit
/// with the business rows. No writes occur before the complete package dependency
/// preflight succeeds.
pub(crate) async fn apply_incoming_package(
    tx: &mut Transaction<'_, Sqlite>,
    group_id: &str,
    source_device_id: &str,
    source_sequence: u64,
    operations: &[SyncOperation],
    payload_hash: &str,
) -> Result<Vec<ApplyOutcome>, SyncError> {
    let (pending, mut outcomes) = classify_package_operations(
        tx,
        source_device_id,
        source_sequence,
        operations,
        payload_hash,
    )
    .await?;
    if pending.is_empty() {
        return outcomes
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| SyncError::Integrity("同步包重复结果不完整".to_string()));
    }
    let pending_operations = pending
        .iter()
        .map(|index| operations[*index].clone())
        .collect::<Vec<_>>();
    validate_package_dependencies(tx, &pending_operations).await?;
    let order = stable_dependency_order(operations, &pending)?;
    let mut judge_states: BTreeMap<String, VirtualJudgeState> = BTreeMap::new();

    for index in order {
        let operation = &operations[index];
        let judge_change = if operation.entity_type == "case"
            && operation.action == OperationAction::Upsert
            && operation.changed_fields.contains_key("judge_id")
        {
            Some(dependency_value(operation, "judge_id")?.map(str::to_string))
        } else {
            None
        };
        if operation.entity_type == "case" && !judge_states.contains_key(&operation.entity_id) {
            judge_states.insert(
                operation.entity_id.clone(),
                load_virtual_judge_state(tx, &operation.entity_id).await?,
            );
        }
        let mut judge_conflict = false;
        if judge_change.is_some() {
            let local_revision: i64 = sqlx::query_scalar(
                "SELECT COALESCE((SELECT revision FROM device_sync_entity_revisions
                  WHERE group_id=?1 AND entity_type='case' AND entity_id=?2),0)",
            )
            .bind(group_id)
            .bind(&operation.entity_id)
            .fetch_one(&mut **tx)
            .await?;
            if local_revision > operation.base_revision {
                let state = judge_states
                    .get(&operation.entity_id)
                    .expect("case judge state loaded");
                let current_hash = state.entity_exists.then(|| {
                    hash_json_value(
                        &state
                            .judge_id
                            .as_ref()
                            .map(|value| Value::String(value.clone()))
                            .unwrap_or(Value::Null),
                    )
                });
                judge_conflict =
                    operation.base_field_hashes.get("judge_id") != current_hash.as_ref();
            }
        }
        let mut safe_operation = operation.clone();
        if judge_change.is_some() {
            safe_operation.changed_fields.remove("judge_id");
            safe_operation.base_field_hashes.remove("judge_id");
        }
        let mut outcome = apply_incoming(
            tx,
            group_id,
            source_device_id,
            source_sequence,
            &safe_operation,
            payload_hash,
        )
        .await?;
        if operation.entity_type == "case" {
            let state = judge_states
                .get_mut(&operation.entity_id)
                .expect("case judge state loaded");
            if operation.action == OperationAction::Tombstone {
                if outcome
                    .applied_fields
                    .iter()
                    .any(|field| field == "_tombstone")
                {
                    state.entity_exists = false;
                    state.judge_id = None;
                }
            } else {
                state.entity_exists = true;
            }
            if let Some(intended_judge) = judge_change {
                if judge_conflict {
                    record_judge_conflict(
                        tx,
                        group_id,
                        operation,
                        state.judge_id.as_deref(),
                        intended_judge.as_deref(),
                    )
                    .await?;
                    outcome.conflict_fields.push("judge_id".to_string());
                } else {
                    state.judge_id = intended_judge;
                    outcome.applied_fields.push("judge_id".to_string());
                }
            }
        }
        outcomes[index] = Some(outcome);
    }

    let case_policy = registry::policy("case")?;
    for (case_id, state) in judge_states {
        if !state.entity_exists {
            continue;
        }
        let affected = sqlx::query("UPDATE cases SET judge_id=?1 WHERE id=?2")
            .bind(&state.judge_id)
            .bind(&case_id)
            .execute(&mut **tx)
            .await?
            .rows_affected();
        if affected != 1 {
            return Err(SyncError::PackageDependencyMissing);
        }
        let after = fetch_entity(tx, case_policy, &case_id)
            .await?
            .ok_or(SyncError::PackageDependencyMissing)?;
        let hashes = hash_fields(&after);
        let revised = sqlx::query(
            "UPDATE device_sync_entity_revisions
             SET field_hashes_json=?1,updated_at=datetime('now')
             WHERE group_id=?2 AND entity_type='case' AND entity_id=?3",
        )
        .bind(serde_json::to_string(&hashes)?)
        .bind(group_id)
        .bind(&case_id)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if revised != 1 {
            return Err(SyncError::Integrity(
                "deferred case judge patch is missing revision".to_string(),
            ));
        }
        sqlx::query(
            "DELETE FROM device_sync_dirty_entities
             WHERE entity_type='case' AND entity_id=?1",
        )
        .bind(&case_id)
        .execute(&mut **tx)
        .await?;
    }

    outcomes
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| SyncError::Integrity("同步包结果不完整".to_string()))
}

pub async fn apply_incoming(
    tx: &mut Transaction<'_, Sqlite>,
    group_id: &str,
    source_device_id: &str,
    source_sequence: u64,
    operation: &SyncOperation,
    payload_hash: &str,
) -> Result<ApplyOutcome, SyncError> {
    let already: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT source_device_id,source_sequence,payload_hash
         FROM device_sync_applied_operations WHERE operation_id=?1",
    )
    .bind(&operation.operation_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((device, sequence, hash)) = already {
        if device == source_device_id && sequence == source_sequence as i64 && hash == payload_hash
        {
            return Ok(ApplyOutcome {
                operation_id: operation.operation_id.clone(),
                applied_fields: Vec::new(),
                conflict_fields: Vec::new(),
                duplicate: true,
            });
        }
        return Err(SyncError::Integrity(
            "operation_id 已被不同来源或载荷使用".to_string(),
        ));
    }
    if operation.author_device_id != source_device_id {
        return Err(SyncError::Integrity("操作作者与信封设备不一致".to_string()));
    }
    let field_map: Map<String, Value> = operation
        .changed_fields
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let clean = registry::sanitize_fields(&operation.entity_type, &field_map)?;
    let policy = registry::policy(&operation.entity_type)?;
    let current = fetch_entity(tx, policy, &operation.entity_id).await?;
    let revision: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT revision, field_hashes_json, tombstoned
         FROM device_sync_entity_revisions
         WHERE group_id=?1 AND entity_type=?2 AND entity_id=?3",
    )
    .bind(group_id)
    .bind(&operation.entity_type)
    .bind(&operation.entity_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (local_revision, stored_hashes, locally_tombstoned) = revision
        .map(|(revision, hashes, tombstoned)| {
            (
                revision,
                serde_json::from_str::<BTreeMap<String, String>>(&hashes).unwrap_or_default(),
                tombstoned != 0,
            )
        })
        .unwrap_or_default();
    let current_hashes = current
        .as_ref()
        .map(hash_fields)
        .unwrap_or_else(|| stored_hashes.clone());

    let mut conflicts = Vec::new();
    if operation.action == OperationAction::Tombstone
        && local_revision > operation.base_revision
        && !locally_tombstoned
    {
        conflicts.push("_tombstone".to_string());
    } else if operation.action == OperationAction::Upsert {
        for field in clean.keys() {
            if local_revision <= operation.base_revision {
                continue;
            }
            let base = operation.base_field_hashes.get(field);
            let now = current_hashes.get(field);
            if base != now {
                conflicts.push(field.clone());
            }
        }
        if let Some(group) = operation.atomic_group.as_deref() {
            if !conflicts.is_empty() {
                conflicts = registry::atomic_group_fields(&operation.entity_type, group)?
                    .iter()
                    .filter(|field| clean.contains_key(**field))
                    .map(|field| (*field).to_string())
                    .collect();
            }
        }
    }

    for field in &conflicts {
        let (local, remote) = if field == "_tombstone" {
            (
                current.clone().map(Value::Object),
                Some(Value::String("tombstone".into())),
            )
        } else {
            (
                current.as_ref().and_then(|map| map.get(field)).cloned(),
                clean.get(field).cloned(),
            )
        };
        let base_hash = operation.base_field_hashes.get(field);
        sqlx::query(
            "INSERT OR IGNORE INTO device_sync_conflicts (
                 id, group_id, operation_id, entity_type, entity_id, case_id,
                 field_key, atomic_group, base_value_hash, local_value_json, remote_value_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(group_id)
        .bind(&operation.operation_id)
        .bind(&operation.entity_type)
        .bind(&operation.entity_id)
        .bind(&operation.case_id)
        .bind(field)
        .bind(&operation.atomic_group)
        .bind(base_hash)
        .bind(local.map(|value| value.to_string()))
        .bind(remote.map(|value| value.to_string()))
        .execute(&mut **tx)
        .await?;
    }

    let conflict_set = conflicts
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let safe_fields = clean
        .into_iter()
        .filter(|(field, _)| !conflict_set.contains(field))
        .collect::<BTreeMap<_, _>>();
    let mut applied_fields = Vec::new();
    let mut tombstoned = locally_tombstoned;
    match operation.action {
        OperationAction::Upsert if !safe_fields.is_empty() => {
            apply_upsert(
                tx,
                policy,
                &operation.entity_id,
                &safe_fields,
                current.is_some(),
            )
            .await?;
            applied_fields.extend(safe_fields.keys().cloned());
            tombstoned = false;
        }
        OperationAction::Tombstone if conflicts.is_empty() => {
            apply_tombstone(tx, policy, &operation.entity_id).await?;
            tombstoned = true;
            applied_fields.push("_tombstone".to_string());
        }
        _ => {}
    }
    let after = fetch_entity(tx, policy, &operation.entity_id).await?;
    let hashes = after.as_ref().map(hash_fields).unwrap_or_default();
    let next_revision = local_revision.max(operation.base_revision) + 1;
    sqlx::query(
        "INSERT INTO device_sync_entity_revisions (
             group_id, entity_type, entity_id, revision, field_hashes_json,
             tombstoned, updated_by_device_id
         ) VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(group_id,entity_type,entity_id) DO UPDATE SET
             revision=excluded.revision,
             field_hashes_json=excluded.field_hashes_json,
             tombstoned=excluded.tombstoned,
             updated_by_device_id=excluded.updated_by_device_id,
             updated_at=datetime('now')",
    )
    .bind(group_id)
    .bind(&operation.entity_type)
    .bind(&operation.entity_id)
    .bind(next_revision)
    .bind(serde_json::to_string(&hashes)?)
    .bind(if tombstoned { 1 } else { 0 })
    .bind(source_device_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO device_sync_applied_operations (
             operation_id, group_id, source_device_id, source_sequence, payload_hash
         ) VALUES (?1,?2,?3,?4,?5)",
    )
    .bind(&operation.operation_id)
    .bind(group_id)
    .bind(source_device_id)
    .bind(source_sequence as i64)
    .bind(payload_hash)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM device_sync_dirty_entities
         WHERE entity_type=?1 AND entity_id=?2",
    )
    .bind(&operation.entity_type)
    .bind(&operation.entity_id)
    .execute(&mut **tx)
    .await?;
    Ok(ApplyOutcome {
        operation_id: operation.operation_id.clone(),
        applied_fields,
        conflict_fields: conflicts,
        duplicate: false,
    })
}

async fn apply_tombstone(
    tx: &mut Transaction<'_, Sqlite>,
    policy: &registry::EntityPolicy,
    entity_id: &str,
) -> Result<(), SyncError> {
    let sql = format!("DELETE FROM \"{}\" WHERE id=?1", policy.table);
    sqlx::query(&sql).bind(entity_id).execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn fetch_entity(
    tx: &mut Transaction<'_, Sqlite>,
    policy: &registry::EntityPolicy,
    entity_id: &str,
) -> Result<Option<Map<String, Value>>, SyncError> {
    let fields = policy
        .columns
        .iter()
        .flat_map(|field| [format!("'{field}'"), format!("\"{field}\"")])
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT json_object({fields}) AS payload FROM \"{}\" WHERE id=?1",
        policy.table
    );
    let row: Option<SqliteRow> = sqlx::query(&sql)
        .bind(entity_id)
        .fetch_optional(&mut **tx)
        .await?;
    row.map(|row| {
        let payload: String = row.try_get("payload")?;
        let value: Value = serde_json::from_str(&payload)?;
        value
            .as_object()
            .cloned()
            .ok_or_else(|| SyncError::Serialization("实体投影不是对象".to_string()))
    })
    .transpose()
}

async fn apply_upsert(
    tx: &mut Transaction<'_, Sqlite>,
    policy: &registry::EntityPolicy,
    entity_id: &str,
    fields: &BTreeMap<String, Value>,
    exists: bool,
) -> Result<(), SyncError> {
    let mut fields = fields.clone();
    fields.insert("id".to_string(), Value::String(entity_id.to_string()));
    if exists {
        fields.remove("id");
        if fields.is_empty() {
            return Ok(());
        }
        let assignments = fields
            .keys()
            .enumerate()
            .map(|(index, field)| format!("\"{field}\"=?{}", index + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE \"{}\" SET {assignments} WHERE id=?{}",
            policy.table,
            fields.len() + 1
        );
        let mut args = SqliteArguments::default();
        for value in fields.values() {
            add_json_arg(&mut args, value)?;
        }
        args.add(entity_id)
            .map_err(|error| SyncError::Database(error.to_string()))?;
        sqlx::query_with(&sql, args).execute(&mut **tx).await?;
    } else {
        if policy.table == "cases" {
            fields.insert(
                "source_folder".to_string(),
                Value::String(format!("device-sync-unbound://{entity_id}")),
            );
        }
        let columns = fields
            .keys()
            .map(|field| format!("\"{field}\""))
            .collect::<Vec<_>>()
            .join(",");
        let placeholders = (1..=fields.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "INSERT INTO \"{}\" ({columns}) VALUES ({placeholders})",
            policy.table
        );
        let mut args = SqliteArguments::default();
        for value in fields.values() {
            add_json_arg(&mut args, value)?;
        }
        sqlx::query_with(&sql, args).execute(&mut **tx).await?;
    }
    Ok(())
}

fn add_json_arg(args: &mut SqliteArguments<'_>, value: &Value) -> Result<(), SyncError> {
    match value {
        Value::Null => args.add(Option::<String>::None),
        Value::Bool(value) => args.add(if *value { 1_i64 } else { 0_i64 }),
        Value::Number(value) if value.is_i64() => args.add(value.as_i64().unwrap_or_default()),
        Value::Number(value) if value.is_u64() => {
            let value = i64::try_from(value.as_u64().unwrap_or_default())
                .map_err(|_| SyncError::Protocol("整数超过 SQLite 范围".to_string()))?;
            args.add(value)
        }
        Value::Number(value) => args.add(
            value
                .as_f64()
                .ok_or_else(|| SyncError::Protocol("无效数字".to_string()))?,
        ),
        Value::String(value) => args.add(value.clone()),
        Value::Array(_) | Value::Object(_) => args.add(serde_json::to_string(value)?),
    }
    .map_err(|error| SyncError::Database(error.to_string()))
}

pub(crate) fn hash_fields(fields: &Map<String, Value>) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|(field, value)| {
            let bytes = serde_json::to_vec(value).unwrap_or_default();
            let hash = Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            (field.clone(), hash)
        })
        .collect()
}
