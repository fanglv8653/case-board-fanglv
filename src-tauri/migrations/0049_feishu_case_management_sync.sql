-- 飞书案件管理四表同步：只保存映射、快照、冲突、待绑定和运行审计。
-- 凭据与 access token 不进入 SQLite；案卷、OCR、分析和文书不在同步范围内。

ALTER TABLE cases ADD COLUMN management_status TEXT NOT NULL DEFAULT 'unknown'
    CHECK(management_status IN ('negotiating','active','closed','unknown'));
ALTER TABLE cases ADD COLUMN management_status_source TEXT NOT NULL DEFAULT 'legacy'
    CHECK(management_status_source IN ('manual','feishu','legacy'));

CREATE TABLE feishu_sync_links (
    id TEXT PRIMARY KEY NOT NULL,
    entity_type TEXT NOT NULL
        CHECK(entity_type IN ('case','work_item','stage','contact')),
    local_entity_id TEXT NOT NULL,
    app_token TEXT NOT NULL,
    table_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    slot_key TEXT NOT NULL DEFAULT '',
    link_source TEXT NOT NULL DEFAULT 'manual'
        CHECK(link_source IN ('manual','exact_case_no','exact_display_name','created_from_local')),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('pending','active','archived')),
    confirmed_at TEXT,
    last_local_updated_at TEXT,
    last_feishu_modified_at TEXT,
    last_synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    UNIQUE(app_token, table_id, record_id, slot_key),
    UNIQUE(entity_type, local_entity_id, table_id, slot_key)
);
CREATE INDEX idx_feishu_sync_links_local
ON feishu_sync_links(entity_type, local_entity_id, status);

CREATE TABLE feishu_sync_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    link_id TEXT NOT NULL,
    local_updated_at TEXT,
    feishu_modified_at TEXT,
    payload_hash TEXT NOT NULL,
    mapped_payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(link_id) REFERENCES feishu_sync_links(id) ON DELETE CASCADE,
    UNIQUE(link_id, payload_hash)
);
CREATE INDEX idx_feishu_sync_snapshots_link
ON feishu_sync_snapshots(link_id, created_at DESC);

CREATE TABLE feishu_sync_conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    link_id TEXT NOT NULL,
    field_key TEXT NOT NULL,
    base_value_json TEXT,
    local_value_json TEXT,
    feishu_value_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','resolved_local','resolved_feishu','resolved_manual','dismissed')),
    resolution_value_json TEXT,
    resolved_by TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(link_id) REFERENCES feishu_sync_links(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX uq_feishu_sync_conflict_pending
ON feishu_sync_conflicts(link_id, field_key)
WHERE status = 'pending';

CREATE TABLE feishu_sync_inbox (
    id TEXT PRIMARY KEY NOT NULL,
    app_token TEXT NOT NULL,
    table_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    legal_type TEXT,
    case_no TEXT,
    remote_modified_at TEXT,
    mapped_payload_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending_binding'
        CHECK(status IN ('pending_binding','bound','ignored','archived')),
    bound_case_id TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(bound_case_id) REFERENCES cases(id) ON DELETE SET NULL,
    UNIQUE(app_token, table_id, record_id)
);
CREATE INDEX idx_feishu_sync_inbox_status
ON feishu_sync_inbox(status, updated_at DESC);

CREATE TABLE feishu_sync_runs (
    id TEXT PRIMARY KEY NOT NULL,
    mode TEXT NOT NULL
        CHECK(mode IN ('readonly_preflight','pull','push','bidirectional')),
    status TEXT NOT NULL
        CHECK(status IN ('running','succeeded','partial','failed','cancelled')),
    active_case_filter TEXT NOT NULL DEFAULT '在办',
    started_at TEXT NOT NULL DEFAULT(datetime('now')),
    completed_at TEXT,
    counts_json TEXT NOT NULL DEFAULT '{}',
    error_code TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now'))
);
CREATE INDEX idx_feishu_sync_runs_started
ON feishu_sync_runs(started_at DESC);
