CREATE TABLE criminal_sentencing_estimates (
    id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    profile_case_id TEXT NOT NULL REFERENCES criminal_case_profiles(case_id) ON DELETE CASCADE,
    profile_revision INTEGER NOT NULL CHECK (profile_revision >= 0),
    input_snapshot_json TEXT NOT NULL,
    output_min_months REAL NOT NULL CHECK (output_min_months >= 0),
    output_max_months REAL CHECK (
        output_max_months IS NULL OR output_max_months >= output_min_months
    ),
    output_snapshot_json TEXT NOT NULL,
    process_snapshot_json TEXT NOT NULL,
    basis_snapshot_json TEXT NOT NULL,
    created_source TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (case_id = profile_case_id)
);

CREATE INDEX idx_criminal_sentencing_estimates_case_created
ON criminal_sentencing_estimates(case_id, created_at DESC, id DESC);
