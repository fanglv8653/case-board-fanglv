-- 飞书案件人工绑定状态与本地审计。
-- 只记录本地关联决策；不修改案件业务字段，也不产生任何飞书写入。

ALTER TABLE feishu_sync_inbox ADD COLUMN auto_bind_suppressed INTEGER NOT NULL DEFAULT 0
    CHECK(auto_bind_suppressed IN (0, 1));

CREATE TABLE feishu_sync_binding_audits (
    id TEXT PRIMARY KEY NOT NULL,
    inbox_id TEXT NOT NULL,
    action TEXT NOT NULL
        CHECK(action IN ('auto_bind','manual_bind','unbind','ignore','restore')),
    previous_status TEXT,
    next_status TEXT NOT NULL,
    previous_case_id TEXT,
    next_case_id TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(inbox_id) REFERENCES feishu_sync_inbox(id) ON DELETE CASCADE,
    FOREIGN KEY(previous_case_id) REFERENCES cases(id) ON DELETE SET NULL,
    FOREIGN KEY(next_case_id) REFERENCES cases(id) ON DELETE SET NULL
);

CREATE INDEX idx_feishu_sync_binding_audits_inbox
ON feishu_sync_binding_audits(inbox_id, created_at DESC);
