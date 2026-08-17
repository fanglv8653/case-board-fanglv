-- v0.8.4：飞书“收件箱”待办同步独立账本。技术元数据不写入用户的飞书表。

CREATE TABLE todo_feishu_sync_links (
    id TEXT PRIMARY KEY NOT NULL,
    item_id TEXT,
    app_token TEXT NOT NULL,
    table_id TEXT NOT NULL,
    view_id TEXT,
    record_id TEXT NOT NULL,
    remote_business_key TEXT NOT NULL,
    remote_case_text TEXT,
    mapped_case_id TEXT,
    base_payload_hash TEXT,
    last_local_hash TEXT,
    last_remote_hash TEXT,
    remote_modified_at TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active','conflict','remote_missing','archived')),
    last_synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(item_id) REFERENCES case_todos(id) ON DELETE SET NULL,
    FOREIGN KEY(mapped_case_id) REFERENCES cases(id) ON DELETE SET NULL,
    UNIQUE(app_token, table_id, remote_business_key),
    UNIQUE(app_token, table_id, record_id)
);

CREATE TABLE todo_feishu_sync_runs (
    id TEXT PRIMARY KEY NOT NULL,
    app_token TEXT NOT NULL,
    table_id TEXT NOT NULL,
    view_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed')),
    remote_count INTEGER NOT NULL DEFAULT 0,
    preview_count INTEGER NOT NULL DEFAULT 0,
    conflict_count INTEGER NOT NULL DEFAULT 0,
    error_code TEXT,
    started_at TEXT NOT NULL DEFAULT(datetime('now')),
    completed_at TEXT
);

CREATE TABLE todo_feishu_sync_previews (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    link_id TEXT,
    item_id TEXT,
    record_id TEXT,
    remote_business_key TEXT,
    change_kind TEXT NOT NULL CHECK(change_kind IN (
        'noop','create_local','create_remote','pull_to_local','push_to_remote',
        'soft_delete_local','remote_missing','metadata_invalid','duplicate_id','conflict'
    )),
    base_payload_json TEXT,
    local_payload_json TEXT,
    remote_payload_json TEXT,
    base_hash TEXT,
    local_hash TEXT,
    remote_hash TEXT,
    remote_modified_at TEXT,
    case_hint TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','applied_local','applied_remote','dismissed','superseded','failed','write_uncertain')),
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    resolved_at TEXT,
    FOREIGN KEY(run_id) REFERENCES todo_feishu_sync_runs(id) ON DELETE CASCADE,
    FOREIGN KEY(link_id) REFERENCES todo_feishu_sync_links(id) ON DELETE SET NULL,
    FOREIGN KEY(item_id) REFERENCES case_todos(id) ON DELETE SET NULL
);

CREATE INDEX idx_todo_feishu_preview_pending_item
ON todo_feishu_sync_previews(item_id, remote_business_key)
WHERE status='pending';

CREATE TABLE todo_feishu_sync_conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    preview_id TEXT NOT NULL,
    item_id TEXT,
    conflict_type TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','resolved','dismissed')),
    resolution TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    resolved_at TEXT,
    FOREIGN KEY(preview_id) REFERENCES todo_feishu_sync_previews(id) ON DELETE CASCADE,
    FOREIGN KEY(item_id) REFERENCES case_todos(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_todo_feishu_conflict_pending_item
ON todo_feishu_sync_conflicts(item_id)
WHERE status='pending' AND item_id IS NOT NULL;

CREATE TABLE todo_feishu_sync_operation_audits (
    action_id TEXT PRIMARY KEY NOT NULL,
    preview_id TEXT NOT NULL,
    direction TEXT NOT NULL CHECK(direction IN ('local','remote','dismiss')),
    status TEXT NOT NULL CHECK(status IN ('started','succeeded','failed','write_uncertain')),
    error_code TEXT,
    before_hash TEXT,
    after_hash TEXT,
    started_at TEXT NOT NULL DEFAULT(datetime('now')),
    completed_at TEXT,
    FOREIGN KEY(preview_id) REFERENCES todo_feishu_sync_previews(id) ON DELETE CASCADE
);

CREATE INDEX idx_todo_feishu_links_item ON todo_feishu_sync_links(item_id, status);
CREATE INDEX idx_todo_feishu_previews_run ON todo_feishu_sync_previews(run_id, status, change_kind);
CREATE INDEX idx_todo_feishu_runs_started ON todo_feishu_sync_runs(started_at DESC);
