ALTER TABLE case_work_items
ADD COLUMN confirmation_status TEXT NOT NULL DEFAULT 'confirmed'
CHECK (confirmation_status IN ('pending', 'confirmed'));

ALTER TABLE case_work_items
ADD COLUMN source_document_id TEXT;

ALTER TABLE case_work_items
ADD COLUMN source_filename TEXT;

CREATE INDEX idx_case_work_items_confirmation
ON case_work_items(case_id, confirmation_status, deleted_at);
