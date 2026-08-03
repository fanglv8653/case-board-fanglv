-- v0.8.2 飞书案件明细仅生成候选变化，禁止拉取过程直接改写业务表。
CREATE TABLE feishu_sync_entity_previews (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    link_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK(entity_type IN ('work_item','stage','contact')),
    local_entity_id TEXT,
    app_token TEXT NOT NULL,
    table_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    slot_key TEXT NOT NULL DEFAULT '',
    case_id TEXT NOT NULL,
    case_name TEXT NOT NULL DEFAULT '',
    change_kind TEXT NOT NULL CHECK(change_kind IN ('create','update','restore','archive')),
    local_value_json TEXT,
    feishu_value_json TEXT,
    mapped_value_json TEXT,
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK(review_status IN ('pending','applied_feishu','applied_local','dismissed','superseded')),
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(run_id) REFERENCES feishu_sync_runs(id) ON DELETE CASCADE,
    FOREIGN KEY(link_id) REFERENCES feishu_sync_links(id) ON DELETE CASCADE,
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE,
    UNIQUE(run_id, entity_type, record_id, slot_key)
);

CREATE INDEX idx_feishu_sync_entity_previews_pending
ON feishu_sync_entity_previews(review_status, created_at DESC);
