-- v0.8.1 NAS mounted-folder encrypted device sync core.
-- Business rows remain in their source tables. These tables only keep protocol
-- metadata, operations, revisions, conflicts, receipts, snapshots and audits.

CREATE TABLE device_sync_groups (
    id TEXT PRIMARY KEY NOT NULL,
    connector_type TEXT NOT NULL DEFAULT 'mounted_folder'
        CHECK(connector_type = 'mounted_folder'),
    connector_root TEXT NOT NULL,
    local_device_id TEXT NOT NULL,
    protocol_version INTEGER NOT NULL DEFAULT 1,
    key_epoch INTEGER NOT NULL DEFAULT 1,
    next_sequence INTEGER NOT NULL DEFAULT 1,
    paused INTEGER NOT NULL DEFAULT 0 CHECK(paused IN (0,1)),
    last_manifest_hash TEXT,
    last_synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now'))
);

CREATE TABLE device_sync_members (
    group_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    signing_public_key TEXT NOT NULL,
    exchange_public_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    key_epoch INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'trusted'
        CHECK(status IN ('pending','trusted','revoked')),
    last_seen_sequence INTEGER NOT NULL DEFAULT 0,
    last_manifest_hash TEXT,
    revoked_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    PRIMARY KEY(group_id, device_id),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);

CREATE TABLE device_sync_invites (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT NOT NULL,
    inviter_device_id TEXT NOT NULL,
    code_hash TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active','consumed','expired','revoked')),
    consumed_by_device_id TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);

CREATE TABLE device_sync_join_requests (
    id TEXT PRIMARY KEY NOT NULL,
    invite_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    signing_public_key TEXT NOT NULL,
    exchange_public_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    proof_hash TEXT NOT NULL,
    request_signature TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','approved','completed','rejected','expired')),
    expires_at TEXT NOT NULL,
    approved_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE,
    UNIQUE(invite_id, device_id)
);

CREATE TABLE device_sync_outbox (
    operation_id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN (
        'case','party','contact','work_item','stage_item','agency_contact',
        'criminal_deadline','criminal_workflow','criminal_task','case_todo',
        'calendar_event','income_record','case_payment','feishu_link',
        'feishu_snapshot','feishu_conflict','feishu_inbox',
        'feishu_binding_audit','legal_skill_package','legal_skill_binding',
        'legal_skill_binding_suppression'
    )),
    entity_id TEXT NOT NULL,
    case_id TEXT,
    action TEXT NOT NULL CHECK(action IN ('upsert','tombstone')),
    base_revision INTEGER NOT NULL,
    changed_fields_json TEXT NOT NULL,
    base_field_hashes_json TEXT NOT NULL DEFAULT '{}',
    atomic_group TEXT,
    author_device_id TEXT NOT NULL,
    logical_time INTEGER NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK(state IN ('pending','exported','acknowledged','quarantined')),
    exported_sequence INTEGER,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);
CREATE INDEX idx_device_sync_outbox_pending
ON device_sync_outbox(group_id, state, logical_time);

-- Business transactions only mark stable logical entities dirty. Projection,
-- encryption and NAS I/O happen asynchronously after commit, so NAS outages
-- never roll back local work. Remote apply removes its own dirty mark in the
-- same transaction to prevent echo.
CREATE TABLE device_sync_dirty_entities (
    entity_type TEXT NOT NULL CHECK(entity_type IN (
        'case','party','contact','work_item','stage_item','agency_contact',
        'criminal_deadline','criminal_workflow','criminal_task','case_todo',
        'calendar_event','income_record','case_payment','feishu_link',
        'feishu_snapshot','feishu_conflict','feishu_inbox',
        'feishu_binding_audit','legal_skill_package','legal_skill_binding',
        'legal_skill_binding_suppression'
    )),
    entity_id TEXT NOT NULL,
    case_id TEXT,
    action TEXT NOT NULL CHECK(action IN ('upsert','tombstone')),
    changed_at TEXT NOT NULL DEFAULT(datetime('now')),
    PRIMARY KEY(entity_type, entity_id)
);

CREATE TRIGGER device_sync_cases_insert AFTER INSERT ON cases BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case',NEW.id,NEW.id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_cases_update AFTER UPDATE ON cases BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case',NEW.id,NEW.id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_cases_delete AFTER DELETE ON cases BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case',OLD.id,OLD.id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_parties_insert AFTER INSERT ON parties BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('party',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_parties_update AFTER UPDATE ON parties BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('party',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_parties_delete AFTER DELETE ON parties BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('party',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_contacts_insert AFTER INSERT ON contacts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('contact',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_contacts_update AFTER UPDATE ON contacts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('contact',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_contacts_delete AFTER DELETE ON contacts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('contact',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_work_items_insert AFTER INSERT ON case_work_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('work_item',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_work_items_update AFTER UPDATE ON case_work_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('work_item',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_work_items_delete AFTER DELETE ON case_work_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('work_item',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_stage_items_insert AFTER INSERT ON case_stage_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('stage_item',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_stage_items_update AFTER UPDATE ON case_stage_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('stage_item',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_stage_items_delete AFTER DELETE ON case_stage_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('stage_item',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_agency_contacts_insert AFTER INSERT ON case_agency_contacts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('agency_contact',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_agency_contacts_update AFTER UPDATE ON case_agency_contacts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('agency_contact',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_agency_contacts_delete AFTER DELETE ON case_agency_contacts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('agency_contact',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_deadlines_insert AFTER INSERT ON criminal_deadline_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_deadline',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_deadlines_update AFTER UPDATE ON criminal_deadline_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_deadline',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_deadlines_delete AFTER DELETE ON criminal_deadline_items BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_deadline',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_workflows_insert AFTER INSERT ON criminal_case_workflows BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_workflow',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_workflows_update AFTER UPDATE ON criminal_case_workflows BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_workflow',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_workflows_delete AFTER DELETE ON criminal_case_workflows BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_workflow',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_tasks_insert AFTER INSERT ON criminal_case_tasks BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_task',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_tasks_update AFTER UPDATE ON criminal_case_tasks BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_task',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_tasks_delete AFTER DELETE ON criminal_case_tasks BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('criminal_task',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_todos_insert AFTER INSERT ON case_todos BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_todo',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_todos_update AFTER UPDATE ON case_todos BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_todo',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_todos_delete AFTER DELETE ON case_todos BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_todo',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_calendar_insert AFTER INSERT ON calendar_events BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('calendar_event',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_calendar_update AFTER UPDATE ON calendar_events BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('calendar_event',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_calendar_delete AFTER DELETE ON calendar_events BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('calendar_event',OLD.id,NULL,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_income_insert AFTER INSERT ON case_income_records BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('income_record',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_income_update AFTER UPDATE ON case_income_records BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('income_record',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_income_delete AFTER DELETE ON case_income_records BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('income_record',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_payments_insert AFTER INSERT ON case_payments BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_payment',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_payments_update AFTER UPDATE ON case_payments BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_payment',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_payments_delete AFTER DELETE ON case_payments BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_payment',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_feishu_links_insert AFTER INSERT ON feishu_sync_links BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_link',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_links_update AFTER UPDATE ON feishu_sync_links BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_link',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_links_delete AFTER DELETE ON feishu_sync_links BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_link',OLD.id,NULL,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_feishu_snapshots_insert AFTER INSERT ON feishu_sync_snapshots BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_snapshot',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_snapshots_update AFTER UPDATE ON feishu_sync_snapshots BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_snapshot',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_snapshots_delete AFTER DELETE ON feishu_sync_snapshots BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_snapshot',OLD.id,NULL,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_feishu_conflicts_insert AFTER INSERT ON feishu_sync_conflicts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_conflict',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_conflicts_update AFTER UPDATE ON feishu_sync_conflicts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_conflict',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_conflicts_delete AFTER DELETE ON feishu_sync_conflicts BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_conflict',OLD.id,NULL,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_feishu_inbox_insert AFTER INSERT ON feishu_sync_inbox BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_inbox',NEW.id,NEW.bound_case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_inbox_update AFTER UPDATE ON feishu_sync_inbox BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_inbox',NEW.id,NEW.bound_case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_feishu_inbox_delete AFTER DELETE ON feishu_sync_inbox BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_inbox',OLD.id,OLD.bound_case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_feishu_binding_audits_insert AFTER INSERT ON feishu_sync_binding_audits BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('feishu_binding_audit',NEW.id,NEW.next_case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_skill_packages_insert AFTER INSERT ON legal_skill_packages BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('legal_skill_package',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_skill_packages_update AFTER UPDATE ON legal_skill_packages BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('legal_skill_package',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_skill_packages_delete AFTER DELETE ON legal_skill_packages BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('legal_skill_package',OLD.id,NULL,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_skill_bindings_insert AFTER INSERT ON legal_skill_bindings BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('legal_skill_binding',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_skill_bindings_update AFTER UPDATE ON legal_skill_bindings BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('legal_skill_binding',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_skill_bindings_delete AFTER DELETE ON legal_skill_bindings BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('legal_skill_binding',OLD.id,NULL,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET action='tombstone',changed_at=excluded.changed_at;
END;

CREATE TABLE device_sync_applied_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT NOT NULL,
    source_device_id TEXT NOT NULL,
    source_sequence INTEGER NOT NULL,
    payload_hash TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);
CREATE INDEX idx_device_sync_applied_source_sequence
ON device_sync_applied_operations(group_id, source_device_id, source_sequence);

CREATE TABLE device_sync_entity_revisions (
    group_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN (
        'case','party','contact','work_item','stage_item','agency_contact',
        'criminal_deadline','criminal_workflow','criminal_task','case_todo',
        'calendar_event','income_record','case_payment','feishu_link',
        'feishu_snapshot','feishu_conflict','feishu_inbox',
        'feishu_binding_audit','legal_skill_package','legal_skill_binding',
        'legal_skill_binding_suppression'
    )),
    entity_id TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    field_hashes_json TEXT NOT NULL DEFAULT '{}',
    tombstoned INTEGER NOT NULL DEFAULT 0 CHECK(tombstoned IN (0,1)),
    updated_by_device_id TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    PRIMARY KEY(group_id, entity_type, entity_id),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);

CREATE TABLE device_sync_conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN (
        'case','party','contact','work_item','stage_item','agency_contact',
        'criminal_deadline','criminal_workflow','criminal_task','case_todo',
        'calendar_event','income_record','case_payment','feishu_link',
        'feishu_snapshot','feishu_conflict','feishu_inbox',
        'feishu_binding_audit','legal_skill_package','legal_skill_binding',
        'legal_skill_binding_suppression'
    )),
    entity_id TEXT NOT NULL,
    case_id TEXT,
    field_key TEXT NOT NULL,
    atomic_group TEXT,
    base_value_hash TEXT,
    local_value_json TEXT,
    remote_value_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','resolved_local','resolved_remote','resolved_manual')),
    resolution_value_json TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE,
    UNIQUE(operation_id, field_key)
);
CREATE INDEX idx_device_sync_conflicts_pending
ON device_sync_conflicts(group_id, status, case_id, entity_type);

CREATE TABLE device_sync_receipts (
    group_id TEXT NOT NULL,
    acknowledging_device_id TEXT NOT NULL,
    source_device_id TEXT NOT NULL,
    acknowledged_sequence INTEGER NOT NULL,
    receipt_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    PRIMARY KEY(group_id, acknowledging_device_id, source_device_id),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);

CREATE TABLE device_sync_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT NOT NULL,
    key_epoch INTEGER NOT NULL,
    manifest_hash TEXT NOT NULL,
    encrypted_file_name TEXT NOT NULL,
    entity_counts_json TEXT NOT NULL DEFAULT '{}',
    logical_time INTEGER NOT NULL,
    snapshot_kind TEXT NOT NULL DEFAULT 'daily'
        CHECK(snapshot_kind IN ('daily','monthly','manual')),
    state TEXT NOT NULL DEFAULT 'created'
        CHECK(state IN ('created','verified','quarantined')),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
);

CREATE TABLE device_sync_quarantine (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT,
    source_path TEXT,
    reason_code TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE SET NULL
);

CREATE TABLE device_sync_audits (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT,
    device_id TEXT,
    action TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK(outcome IN ('succeeded','rejected','failed','paused')),
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE SET NULL
);
CREATE INDEX idx_device_sync_audits_group
ON device_sync_audits(group_id, created_at DESC);
