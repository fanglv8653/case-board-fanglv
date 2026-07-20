-- 飞书只读预演的字段级差异。每条记录归属一次运行，不代表已应用。
CREATE TABLE feishu_sync_field_previews (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    link_id TEXT NOT NULL,
    field_key TEXT NOT NULL,
    field_label TEXT NOT NULL DEFAULT '',
    local_value_json TEXT,
    feishu_value_json TEXT,
    classification TEXT NOT NULL,
    proposed_action TEXT NOT NULL DEFAULT 'none'
        CHECK(proposed_action IN ('none','pull_to_local','review')),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(run_id) REFERENCES feishu_sync_runs(id) ON DELETE CASCADE,
    FOREIGN KEY(link_id) REFERENCES feishu_sync_links(id) ON DELETE CASCADE,
    UNIQUE(run_id, link_id, field_key)
);

CREATE INDEX idx_feishu_sync_field_previews_run
ON feishu_sync_field_previews(run_id, proposed_action, created_at DESC);
