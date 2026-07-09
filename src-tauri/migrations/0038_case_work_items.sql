CREATE TABLE case_work_items (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT,
    occurred_at TEXT NOT NULL,
    work_type TEXT NOT NULL DEFAULT 'other',
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    result TEXT,
    next_action TEXT,
    duration_minutes INTEGER,
    source TEXT NOT NULL DEFAULT 'manual',
    external_source TEXT,
    external_record_id TEXT,
    external_updated_at TEXT,
    raw_payload_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE SET NULL
);

CREATE INDEX idx_case_work_items_case_occurred
ON case_work_items(case_id, occurred_at DESC);

CREATE UNIQUE INDEX idx_case_work_items_external_record
ON case_work_items(external_source, external_record_id)
WHERE external_source IS NOT NULL AND external_record_id IS NOT NULL;
