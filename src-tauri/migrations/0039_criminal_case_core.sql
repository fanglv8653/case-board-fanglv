CREATE TABLE criminal_case_profiles (
    case_id TEXT PRIMARY KEY NOT NULL,
    current_stage TEXT,
    procedure_type TEXT,
    case_subtype TEXT,
    defense_role TEXT,
    suspected_charge TEXT,
    suspect_or_defendant_name TEXT,
    victim_name TEXT,
    client_name TEXT,
    client_relationship TEXT,
    detention_center TEXT,
    coercive_measure_type TEXT,
    detention_date TEXT,
    arrest_request_date TEXT,
    arrest_review_received_date TEXT,
    arrest_decision_date TEXT,
    arrest_date TEXT,
    bail_start_date TEXT,
    residential_surveillance_start_date TEXT,
    transfer_for_prosecution_date TEXT,
    prosecution_received_date TEXT,
    first_instance_accepted_date TEXT,
    second_instance_accepted_date TEXT,
    judgment_received_date TEXT,
    ruling_received_date TEXT,
    notes TEXT,
    user_overrides_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE TABLE case_stage_items (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    domain TEXT NOT NULL DEFAULT 'criminal',
    major_stage TEXT,
    stage_label TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TEXT,
    due_at TEXT,
    completed_at TEXT,
    reminder_at TEXT,
    source TEXT NOT NULL DEFAULT 'manual',
    external_source TEXT,
    external_record_id TEXT,
    raw_payload_json TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX idx_case_stage_items_case_started
ON case_stage_items(case_id, started_at DESC);

CREATE UNIQUE INDEX idx_case_stage_items_external_record
ON case_stage_items(external_source, external_record_id)
WHERE external_source IS NOT NULL
  AND external_record_id IS NOT NULL
  AND deleted_at IS NULL;

CREATE TABLE criminal_deadline_items (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    stage_item_id TEXT,
    rule_code TEXT,
    title TEXT NOT NULL,
    major_stage TEXT,
    minor_stage TEXT,
    trigger_date TEXT,
    trigger_time TEXT,
    default_due_at TEXT,
    manual_due_at TEXT,
    effective_due_at TEXT,
    reminder_at TEXT,
    priority TEXT NOT NULL DEFAULT 'normal',
    status TEXT NOT NULL DEFAULT 'pending',
    source_type TEXT NOT NULL DEFAULT 'manual',
    source_law TEXT,
    source_article TEXT,
    source_url TEXT,
    calculation_note TEXT,
    exception_type TEXT,
    exception_note TEXT,
    override_reason TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (stage_item_id) REFERENCES case_stage_items(id) ON DELETE SET NULL
);

CREATE INDEX idx_criminal_deadline_items_case_due
ON criminal_deadline_items(case_id, effective_due_at ASC);

CREATE TABLE case_agency_contacts (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    stage_scope TEXT,
    agency_type TEXT,
    agency_name TEXT,
    contact_role TEXT,
    contact_name TEXT,
    phone TEXT,
    case_no TEXT,
    query_code TEXT,
    notes TEXT,
    source TEXT NOT NULL DEFAULT 'manual',
    external_record_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX idx_case_agency_contacts_case_stage
ON case_agency_contacts(case_id, stage_scope);
