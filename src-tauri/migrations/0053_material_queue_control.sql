-- v0.8.0 B1+B2: material inclusion decisions and persistent processing queue.
-- Source files remain read-only; this migration stores only paths, states and
-- redacted operational summaries.

CREATE TABLE material_source_decisions (
    case_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    disposition TEXT NOT NULL
        CHECK(disposition IN ('recognize','index_only','excluded')),
    document_id TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    PRIMARY KEY(case_id, source_path),
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE SET NULL
);

-- Every document that existed before the three-state selector was introduced
-- keeps its historical behaviour: it remains eligible for recognition.
INSERT INTO material_source_decisions(case_id, source_path, disposition, document_id)
SELECT case_id, source_path, 'recognize', id
FROM documents
WHERE 1
ON CONFLICT(case_id, source_path) DO NOTHING;

CREATE INDEX idx_material_source_decisions_case_disposition
ON material_source_decisions(case_id, disposition);

CREATE TRIGGER trg_material_decision_document_case_insert
BEFORE INSERT ON material_source_decisions
WHEN NEW.document_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM documents
    WHERE id=NEW.document_id AND case_id=NEW.case_id
 )
BEGIN
    SELECT RAISE(ABORT, 'material decision document must belong to case');
END;

CREATE TRIGGER trg_material_decision_document_case_update
BEFORE UPDATE OF case_id, document_id ON material_source_decisions
WHEN NEW.document_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM documents
    WHERE id=NEW.document_id AND case_id=NEW.case_id
 )
BEGIN
    SELECT RAISE(ABORT, 'material decision document must belong to case');
END;

CREATE TABLE material_processing_batches (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    status TEXT NOT NULL
        CHECK(status IN (
            'queued','running','paused','cancelled','completed','failed',
            'recovery_required'
        )),
    error_category TEXT,
    error_summary TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    started_at TEXT,
    finished_at TEXT,
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX idx_material_processing_batches_status
ON material_processing_batches(status, created_at);
CREATE INDEX idx_material_processing_batches_case
ON material_processing_batches(case_id, created_at DESC);

CREATE TABLE material_processing_items (
    id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT NOT NULL,
    case_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    document_id TEXT,
    ordinal INTEGER NOT NULL,
    status TEXT NOT NULL
        CHECK(status IN (
            'queued','running','paused','cancelled','completed','failed',
            'recovery_required'
        )),
    claim_token TEXT,
    claimed_at TEXT,
    completed_at TEXT,
    error_category TEXT,
    error_summary TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(batch_id) REFERENCES material_processing_batches(id) ON DELETE CASCADE,
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE SET NULL,
    UNIQUE(batch_id, source_path),
    UNIQUE(claim_token)
);

CREATE INDEX idx_material_processing_items_claim
ON material_processing_items(batch_id, status, ordinal);
CREATE INDEX idx_material_processing_items_document
ON material_processing_items(document_id, status);

CREATE TRIGGER trg_material_item_scope_insert
BEFORE INSERT ON material_processing_items
WHEN NOT EXISTS (
        SELECT 1 FROM material_processing_batches
        WHERE id=NEW.batch_id AND case_id=NEW.case_id
     )
  OR NOT EXISTS (
        SELECT 1 FROM material_source_decisions
        WHERE case_id=NEW.case_id
          AND source_path=NEW.source_path
          AND disposition='recognize'
     )
  OR (
        NEW.document_id IS NOT NULL
        AND NOT EXISTS (
            SELECT 1 FROM documents
            WHERE id=NEW.document_id AND case_id=NEW.case_id
        )
     )
BEGIN
    SELECT RAISE(ABORT, 'material item scope mismatch');
END;

CREATE TRIGGER trg_material_item_scope_update
BEFORE UPDATE OF batch_id, case_id, source_path, document_id
ON material_processing_items
WHEN NOT EXISTS (
        SELECT 1 FROM material_processing_batches
        WHERE id=NEW.batch_id AND case_id=NEW.case_id
     )
  OR NOT EXISTS (
        SELECT 1 FROM material_source_decisions
        WHERE case_id=NEW.case_id
          AND source_path=NEW.source_path
          AND disposition='recognize'
     )
  OR (
        NEW.document_id IS NOT NULL
        AND NOT EXISTS (
            SELECT 1 FROM documents
            WHERE id=NEW.document_id AND case_id=NEW.case_id
        )
     )
BEGIN
    SELECT RAISE(ABORT, 'material item scope mismatch');
END;

CREATE TABLE material_processing_events (
    id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT NOT NULL,
    item_id TEXT,
    event_type TEXT NOT NULL,
    from_status TEXT,
    to_status TEXT,
    actor TEXT NOT NULL,
    error_category TEXT,
    error_summary TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(batch_id) REFERENCES material_processing_batches(id) ON DELETE CASCADE,
    FOREIGN KEY(item_id) REFERENCES material_processing_items(id) ON DELETE CASCADE
);

CREATE INDEX idx_material_processing_events_batch
ON material_processing_events(batch_id, created_at, id);
CREATE INDEX idx_material_processing_events_item
ON material_processing_events(item_id, created_at, id);

CREATE TRIGGER trg_material_event_item_batch_insert
BEFORE INSERT ON material_processing_events
WHEN NEW.item_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM material_processing_items
    WHERE id=NEW.item_id AND batch_id=NEW.batch_id
 )
BEGIN
    SELECT RAISE(ABORT, 'material event item must belong to batch');
END;

CREATE TRIGGER trg_material_event_item_batch_update
BEFORE UPDATE OF batch_id, item_id ON material_processing_events
WHEN NEW.item_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1 FROM material_processing_items
    WHERE id=NEW.item_id AND batch_id=NEW.batch_id
 )
BEGIN
    SELECT RAISE(ABORT, 'material event item must belong to batch');
END;
