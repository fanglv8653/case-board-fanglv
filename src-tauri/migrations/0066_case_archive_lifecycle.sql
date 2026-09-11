-- v0.8.5：案件办结与归档生命周期。归档不删除任何业务数据，也不触碰飞书绑定。
ALTER TABLE cases ADD COLUMN closed_at TEXT;
ALTER TABLE cases ADD COLUMN archived_at TEXT;
ALTER TABLE cases ADD COLUMN archive_note TEXT;

CREATE INDEX idx_cases_archived_at ON cases(archived_at, updated_at DESC);

CREATE TABLE case_archive_audits (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('close','reopen','archive','restore')),
    occurred_at TEXT NOT NULL,
    note TEXT,
    previous_management_status TEXT NOT NULL,
    new_management_status TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX idx_case_archive_audits_case
ON case_archive_audits(case_id, created_at DESC);
