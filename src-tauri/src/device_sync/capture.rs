use std::collections::{BTreeMap, BTreeSet};

use sqlx::{FromRow, SqlitePool};

use super::operations::{fetch_entity, hash_fields, OperationAction, SyncOperation};
use super::registry;
use super::SyncError;

pub(crate) async fn lock_capture_sequence_group(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    group_id: &str,
) -> Result<(), SyncError> {
    let locked = sqlx::query("UPDATE device_sync_groups SET updated_at=updated_at WHERE id=?1")
        .bind(group_id)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    if locked != 1 {
        return Err(SyncError::NotFound("同步组不存在".to_string()));
    }
    Ok(())
}

#[derive(Debug, FromRow)]
struct DirtyRow {
    entity_type: String,
    entity_id: String,
    case_id: Option<String>,
    action: String,
}

pub async fn ensure_initial_baseline(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<usize, SyncError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT
           EXISTS(SELECT 1 FROM device_sync_entity_revisions WHERE group_id=?1)
           OR EXISTS(SELECT 1 FROM device_sync_outbox WHERE group_id=?1)",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    if exists != 0 {
        return Ok(0);
    }
    let mut inserted = 0;
    let mut tx = pool.begin().await?;
    for policy in registry::all_policies() {
        let case_expr = policy
            .case_column
            .map(|column| format!("\"{column}\""))
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            "INSERT OR IGNORE INTO device_sync_dirty_entities (
                 entity_type, entity_id, case_id, action, changed_at
             )
             SELECT ?1, id, {case_expr}, 'upsert', datetime('now') FROM \"{}\"",
            policy.table
        );
        inserted += sqlx::query(&sql)
            .bind(policy.entity_type)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(inserted)
}

pub async fn capture_dirty_entities(pool: &SqlitePool, group_id: &str) -> Result<usize, SyncError> {
    let local_device_id: String =
        sqlx::query_scalar("SELECT local_device_id FROM device_sync_groups WHERE id=?1")
            .bind(group_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))?;
    let dependency_order = registry::all_policies()
        .iter()
        .enumerate()
        .map(|(rank, policy)| format!("WHEN '{}' THEN {rank}", policy.entity_type))
        .collect::<Vec<_>>()
        .join(" ");
    let dirty_sql = format!(
        "SELECT entity_type, entity_id, case_id, action
         FROM device_sync_dirty_entities
         ORDER BY changed_at,
           CASE WHEN action='tombstone'
             THEN -(CASE entity_type {dependency_order} ELSE 10000 END)
             ELSE  (CASE entity_type {dependency_order} ELSE 10000 END)
           END,
           entity_id
         LIMIT 1000"
    );
    let dirty: Vec<DirtyRow> = sqlx::query_as(&dirty_sql).fetch_all(pool).await?;
    let mut captured = 0;
    for row in dirty {
        let mut tx = pool.begin().await?;
        lock_capture_sequence_group(&mut tx, group_id).await?;
        let revision: Option<(i64, String, i64)> = sqlx::query_as(
            "SELECT revision, field_hashes_json, tombstoned
             FROM device_sync_entity_revisions
             WHERE group_id=?1 AND entity_type=?2 AND entity_id=?3",
        )
        .bind(group_id)
        .bind(&row.entity_type)
        .bind(&row.entity_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (base_revision, prior_hashes, prior_tombstone) = revision
            .map(|(revision, hashes, tombstone)| {
                (
                    revision,
                    serde_json::from_str::<BTreeMap<String, String>>(&hashes).unwrap_or_default(),
                    tombstone != 0,
                )
            })
            .unwrap_or_default();
        let policy = registry::policy(&row.entity_type)?;
        let action = match row.action.as_str() {
            "upsert" => OperationAction::Upsert,
            "tombstone" => OperationAction::Tombstone,
            other => return Err(SyncError::Protocol(format!("未知 dirty action: {other}"))),
        };
        let mut changed_fields = BTreeMap::new();
        let mut current_hashes = prior_hashes.clone();
        let mut atomic_group = None;
        if action == OperationAction::Upsert {
            let current = fetch_entity(&mut tx, policy, &row.entity_id)
                .await?
                .ok_or_else(|| {
                    SyncError::Integrity(format!(
                        "标记为 upsert 的实体不存在: {}/{}",
                        row.entity_type, row.entity_id
                    ))
                })?;
            current_hashes = hash_fields(&current);
            let changed_names = current_hashes
                .iter()
                .filter(|(field, hash)| prior_hashes.get(*field) != Some(*hash))
                .map(|(field, _)| field.clone())
                .collect::<BTreeSet<_>>();
            if changed_names.is_empty() && !prior_tombstone {
                sqlx::query(
                    "DELETE FROM device_sync_dirty_entities
                     WHERE entity_type=?1 AND entity_id=?2",
                )
                .bind(&row.entity_type)
                .bind(&row.entity_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                continue;
            }
            for field in &changed_names {
                if let Some(group) = registry::atomic_group_for_field(&row.entity_type, field) {
                    atomic_group = Some(group.to_string());
                    for grouped in registry::atomic_group_fields(&row.entity_type, group)? {
                        if let Some(value) = current.get(*grouped) {
                            changed_fields.insert((*grouped).to_string(), value.clone());
                        }
                    }
                } else if let Some(value) = current.get(field) {
                    changed_fields.insert(field.clone(), value.clone());
                }
            }
            // A new entity must be self-contained because the other device may
            // not have any row to patch.
            if base_revision == 0 {
                changed_fields = current.into_iter().collect();
            }
        }
        let base_field_hashes = changed_fields
            .keys()
            .filter_map(|field| {
                prior_hashes
                    .get(field)
                    .map(|hash| (field.clone(), hash.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let logical_time: i64 =
            sqlx::query_scalar("SELECT CAST(strftime('%s','now') AS INTEGER) * 1000")
                .fetch_one(&mut *tx)
                .await?;
        let capture_sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(capture_sequence),0)+1
             FROM device_sync_outbox
             WHERE group_id=?1",
        )
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;
        let operation = SyncOperation {
            operation_id: uuid::Uuid::new_v4().to_string(),
            entity_type: row.entity_type.clone(),
            entity_id: row.entity_id.clone(),
            case_id: row.case_id.clone(),
            action,
            base_revision,
            changed_fields,
            base_field_hashes,
            atomic_group,
            author_device_id: local_device_id.clone(),
            logical_time,
            capture_sequence,
            schema_version: 1,
        };
        sqlx::query(
            "INSERT INTO device_sync_outbox (
                 operation_id, group_id, entity_type, entity_id, case_id, action,
                 base_revision, changed_fields_json, base_field_hashes_json,
                 atomic_group, author_device_id, logical_time, capture_sequence,
                 schema_version
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,1)",
        )
        .bind(&operation.operation_id)
        .bind(group_id)
        .bind(&operation.entity_type)
        .bind(&operation.entity_id)
        .bind(&operation.case_id)
        .bind(match operation.action {
            OperationAction::Upsert => "upsert",
            OperationAction::Tombstone => "tombstone",
        })
        .bind(operation.base_revision)
        .bind(serde_json::to_string(&operation.changed_fields)?)
        .bind(serde_json::to_string(&operation.base_field_hashes)?)
        .bind(&operation.atomic_group)
        .bind(&operation.author_device_id)
        .bind(operation.logical_time)
        .bind(operation.capture_sequence)
        .execute(&mut *tx)
        .await?;
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
        .bind(&row.entity_type)
        .bind(&row.entity_id)
        .bind(base_revision + 1)
        .bind(serde_json::to_string(&current_hashes)?)
        .bind(if action == OperationAction::Tombstone {
            1
        } else {
            0
        })
        .bind(&local_device_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM device_sync_dirty_entities
             WHERE entity_type=?1 AND entity_id=?2",
        )
        .bind(&row.entity_type)
        .bind(&row.entity_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        captured += 1;
    }
    Ok(captured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_never_exposes_excluded_entity_types() {
        let names = registry::all_policies()
            .iter()
            .map(|policy| policy.entity_type)
            .collect::<BTreeSet<_>>();
        for excluded in [
            "document",
            "chat_message",
            "case_memory",
            "memory_candidate",
            "material_queue",
        ] {
            assert!(!names.contains(excluded));
        }
    }
}
