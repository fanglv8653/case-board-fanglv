-- v0.8.5: include manual criminal fee records in encrypted device sync.
-- Rebuild only sync tables whose CHECK constraints enumerate entity types.

DROP INDEX idx_device_sync_outbox_pending;
DROP INDEX idx_device_sync_outbox_capture_sequence;
DROP INDEX idx_device_sync_outbox_pending_capture;
CREATE TABLE device_sync_outbox_v085 (
    operation_id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN (
        'case','party','contact','work_item','stage_item','agency_contact','criminal_deadline',
        'criminal_workflow','criminal_task','case_todo','calendar_event','income_record',
        'case_payment','case_fee','feishu_link','feishu_snapshot','feishu_conflict','feishu_inbox',
        'feishu_binding_audit','legal_skill_package','legal_skill_binding','legal_skill_binding_suppression'
    )),
    entity_id TEXT NOT NULL, case_id TEXT,
    action TEXT NOT NULL CHECK(action IN ('upsert','tombstone')), base_revision INTEGER NOT NULL,
    changed_fields_json TEXT NOT NULL, base_field_hashes_json TEXT NOT NULL DEFAULT '{}',
    atomic_group TEXT, author_device_id TEXT NOT NULL, logical_time INTEGER NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','exported','acknowledged','quarantined')),
    exported_sequence INTEGER, created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    capture_sequence INTEGER NOT NULL DEFAULT 0 CHECK(capture_sequence >= 0),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);
INSERT INTO device_sync_outbox_v085 SELECT * FROM device_sync_outbox;
DROP TABLE device_sync_outbox;
ALTER TABLE device_sync_outbox_v085 RENAME TO device_sync_outbox;
CREATE INDEX idx_device_sync_outbox_pending ON device_sync_outbox(group_id,state,logical_time);
CREATE UNIQUE INDEX idx_device_sync_outbox_capture_sequence ON device_sync_outbox(group_id,capture_sequence);
CREATE INDEX idx_device_sync_outbox_pending_capture ON device_sync_outbox(group_id,state,capture_sequence);

-- Keep the original dirty table untouched so every historical business trigger
-- remains valid. Fees use a companion queue and are merged by the capture layer.
CREATE TABLE device_sync_case_fee_dirty_entities (
    entity_id TEXT PRIMARY KEY NOT NULL, case_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('upsert','tombstone')),
    changed_at TEXT NOT NULL DEFAULT(datetime('now'))
);

CREATE TABLE device_sync_entity_revisions_v085 (
    group_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN (
        'case','party','contact','work_item','stage_item','agency_contact','criminal_deadline',
        'criminal_workflow','criminal_task','case_todo','calendar_event','income_record',
        'case_payment','case_fee','feishu_link','feishu_snapshot','feishu_conflict','feishu_inbox',
        'feishu_binding_audit','legal_skill_package','legal_skill_binding','legal_skill_binding_suppression'
    )),
    entity_id TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0,
    field_hashes_json TEXT NOT NULL DEFAULT '{}', tombstoned INTEGER NOT NULL DEFAULT 0 CHECK(tombstoned IN (0,1)),
    updated_by_device_id TEXT NOT NULL, updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    PRIMARY KEY(group_id,entity_type,entity_id),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);
INSERT INTO device_sync_entity_revisions_v085 SELECT * FROM device_sync_entity_revisions;
DROP TABLE device_sync_entity_revisions;
ALTER TABLE device_sync_entity_revisions_v085 RENAME TO device_sync_entity_revisions;

DROP INDEX idx_device_sync_conflicts_pending;
CREATE TABLE device_sync_conflicts_v085 (
    id TEXT PRIMARY KEY NOT NULL, group_id TEXT NOT NULL, operation_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN (
        'case','party','contact','work_item','stage_item','agency_contact','criminal_deadline',
        'criminal_workflow','criminal_task','case_todo','calendar_event','income_record',
        'case_payment','case_fee','feishu_link','feishu_snapshot','feishu_conflict','feishu_inbox',
        'feishu_binding_audit','legal_skill_package','legal_skill_binding','legal_skill_binding_suppression'
    )),
    entity_id TEXT NOT NULL, case_id TEXT, field_key TEXT NOT NULL, atomic_group TEXT,
    base_value_hash TEXT, local_value_json TEXT, remote_value_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','resolved_local','resolved_remote','resolved_manual')),
    resolution_value_json TEXT, resolved_at TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE,
    UNIQUE(operation_id,field_key)
);
INSERT INTO device_sync_conflicts_v085 SELECT * FROM device_sync_conflicts;
DROP TABLE device_sync_conflicts;
ALTER TABLE device_sync_conflicts_v085 RENAME TO device_sync_conflicts;
CREATE INDEX idx_device_sync_conflicts_pending ON device_sync_conflicts(group_id,status,case_id,entity_type);

CREATE TRIGGER device_sync_case_fees_insert AFTER INSERT ON case_fees BEGIN
  INSERT INTO device_sync_case_fee_dirty_entities VALUES(NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_case_fees_update AFTER UPDATE ON case_fees BEGIN
  INSERT INTO device_sync_case_fee_dirty_entities VALUES(NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_case_fees_delete AFTER DELETE ON case_fees BEGIN
  INSERT INTO device_sync_case_fee_dirty_entities VALUES(OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_id) DO UPDATE SET case_id=excluded.case_id,action='tombstone',changed_at=excluded.changed_at;
END;
