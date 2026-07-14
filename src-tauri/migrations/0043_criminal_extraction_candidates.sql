ALTER TABLE criminal_case_profiles
ADD COLUMN profile_revision INTEGER NOT NULL DEFAULT 0;

CREATE TABLE criminal_extraction_candidate_batches (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    source_document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    source_filename TEXT NOT NULL,
    document_type TEXT,
    model_name TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    source_fingerprint TEXT NOT NULL,
    result_fingerprint TEXT NOT NULL,
    technical_status TEXT NOT NULL CHECK (technical_status IN ('success','partial','failed')),
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (review_status IN ('pending','partially_confirmed','confirmed','rejected','superseded')),
    warning_json TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT
);

CREATE UNIQUE INDEX uq_criminal_candidate_result
ON criminal_extraction_candidate_batches(source_document_id, source_fingerprint, result_fingerprint, schema_version);

CREATE INDEX idx_criminal_candidate_case_review
ON criminal_extraction_candidate_batches(case_id, review_status, created_at DESC);

CREATE TABLE criminal_extraction_candidate_fields (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL REFERENCES criminal_extraction_candidate_batches(id) ON DELETE CASCADE,
    field_key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    source_document_id TEXT,
    source_filename TEXT NOT NULL,
    evidence_excerpt TEXT,
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (review_status IN ('pending','accepted','rejected','protected')),
    decision_note TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(batch_id, field_key)
);

CREATE INDEX idx_criminal_candidate_fields_batch
ON criminal_extraction_candidate_fields(batch_id, review_status, field_key);
