ALTER TABLE criminal_case_profiles ADD COLUMN stage_sort_mode TEXT NOT NULL DEFAULT 'auto';
ALTER TABLE criminal_case_profiles ADD COLUMN guilty_plea_status TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN sentencing_recommendation TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN sentence_term TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN charge_history_json TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN restitution_amount REAL;
ALTER TABLE criminal_case_profiles ADD COLUMN restitution_status TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN victim_forgiveness TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN surrender_status TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN meritorious_service_status TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN co_defendants_json TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN supplementary_investigation_1_date TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN supplementary_investigation_2_date TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN judgment_effective_date TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN death_penalty_review_start_date TEXT;
ALTER TABLE criminal_case_profiles ADD COLUMN extraction_meta_json TEXT;

ALTER TABLE case_stage_items ADD COLUMN sort_order INTEGER;

CREATE INDEX idx_case_stage_items_case_sort
ON case_stage_items(case_id, sort_order ASC)
WHERE deleted_at IS NULL;

ALTER TABLE criminal_deadline_items
ADD COLUMN applicability_status TEXT NOT NULL DEFAULT 'confirmed';

CREATE INDEX idx_criminal_deadline_items_case_applicability
ON criminal_deadline_items(case_id, applicability_status, effective_due_at)
WHERE deleted_at IS NULL;
