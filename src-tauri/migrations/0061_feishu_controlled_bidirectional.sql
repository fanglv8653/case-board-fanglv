-- v0.8.2 飞书受控双向同步：每个字段必须由用户逐项确认。
-- 不增加后台自动写入；远端删除仍不自动映射为本地删除。

ALTER TABLE feishu_sync_field_previews ADD COLUMN review_status TEXT NOT NULL DEFAULT 'pending'
    CHECK(review_status IN ('pending','applied_feishu','applied_local','dismissed','superseded'));
ALTER TABLE feishu_sync_field_previews ADD COLUMN resolution_value_json TEXT;
ALTER TABLE feishu_sync_field_previews ADD COLUMN resolved_at TEXT;

CREATE TABLE feishu_sync_operation_audits (
    id TEXT PRIMARY KEY NOT NULL,
    preview_id TEXT,
    link_id TEXT NOT NULL,
    entity_type TEXT NOT NULL
        CHECK(entity_type IN ('case','work_item','stage','contact')),
    field_key TEXT NOT NULL,
    direction TEXT NOT NULL
        CHECK(direction IN ('feishu_to_local','local_to_feishu','dismiss')),
    status TEXT NOT NULL
        CHECK(status IN ('succeeded','failed')),
    before_value_json TEXT,
    after_value_json TEXT,
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(preview_id) REFERENCES feishu_sync_field_previews(id) ON DELETE SET NULL,
    FOREIGN KEY(link_id) REFERENCES feishu_sync_links(id) ON DELETE CASCADE
);

CREATE INDEX idx_feishu_sync_operation_audits_preview
ON feishu_sync_operation_audits(preview_id, created_at DESC);

CREATE INDEX idx_feishu_sync_field_previews_review
ON feishu_sync_field_previews(review_status, created_at DESC);
