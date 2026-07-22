-- 飞书 -> 案件看板单向入站实体状态。
-- 只扩展本地来源、幂等键和失效标记；不增加任何飞书写入能力。

ALTER TABLE case_work_items ADD COLUMN external_status TEXT NOT NULL DEFAULT 'active'
    CHECK(external_status IN ('active','archived'));
ALTER TABLE case_work_items ADD COLUMN external_last_seen_at TEXT;

ALTER TABLE case_stage_items ADD COLUMN external_status TEXT NOT NULL DEFAULT 'active'
    CHECK(external_status IN ('active','archived'));
ALTER TABLE case_stage_items ADD COLUMN external_updated_at TEXT;
ALTER TABLE case_stage_items ADD COLUMN external_last_seen_at TEXT;

ALTER TABLE case_agency_contacts ADD COLUMN external_source TEXT;
ALTER TABLE case_agency_contacts ADD COLUMN external_slot_key TEXT NOT NULL DEFAULT '';
ALTER TABLE case_agency_contacts ADD COLUMN external_updated_at TEXT;
ALTER TABLE case_agency_contacts ADD COLUMN external_last_seen_at TEXT;
ALTER TABLE case_agency_contacts ADD COLUMN external_status TEXT NOT NULL DEFAULT 'active'
    CHECK(external_status IN ('active','archived'));
ALTER TABLE case_agency_contacts ADD COLUMN raw_payload_json TEXT;

UPDATE case_agency_contacts
SET external_source = 'feishu'
WHERE source = 'feishu'
  AND external_record_id IS NOT NULL
  AND trim(external_record_id) <> ''
  AND external_source IS NULL;

CREATE UNIQUE INDEX uq_case_agency_contacts_external_slot
ON case_agency_contacts(external_source, external_record_id, external_slot_key)
WHERE external_source IS NOT NULL
  AND external_record_id IS NOT NULL;

CREATE INDEX idx_case_work_items_external_status
ON case_work_items(case_id, external_source, external_status, external_last_seen_at);

CREATE INDEX idx_case_stage_items_external_status
ON case_stage_items(case_id, external_source, external_status, external_last_seen_at);

CREATE INDEX idx_case_agency_contacts_external_status
ON case_agency_contacts(case_id, external_source, external_status, external_last_seen_at);

CREATE TABLE feishu_sync_entity_audits (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN ('work_item','stage','contact')),
    local_entity_id TEXT NOT NULL,
    remote_record_id TEXT NOT NULL,
    slot_key TEXT NOT NULL DEFAULT '',
    action TEXT NOT NULL CHECK(action IN ('insert','update','unchanged','archive','restore')),
    payload_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(run_id) REFERENCES feishu_sync_runs(id) ON DELETE CASCADE
);

CREATE INDEX idx_feishu_sync_entity_audits_run
ON feishu_sync_entity_audits(run_id, entity_type, action);
