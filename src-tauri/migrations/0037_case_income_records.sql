-- 2026-06-29 · 收入台账后端最小实现(REV-B1)
--
-- 独立的个人收入台账,不复用 cases.agg_fees / case_payments,也不进入工作台/团队/AI。
-- `case_id` 可空:允许记录未导入案件;若关联案件被删,保留台账并把 case_id 置空。
CREATE TABLE IF NOT EXISTS case_income_records (
    id                          TEXT PRIMARY KEY NOT NULL,
    case_id                     TEXT,
    manual_case_name            TEXT,
    lawyer_fee_total            REAL NOT NULL,
    source_type                 TEXT NOT NULL DEFAULT 'personal',
    collaborator_name           TEXT,
    share_ratio                 REAL NOT NULL DEFAULT 1.0,
    firm_deduction_rate         REAL NOT NULL DEFAULT 0.15,
    archive_holdback_rate       REAL NOT NULL DEFAULT 0.05,
    personal_share_amount       REAL NOT NULL,
    firm_deduction_amount       REAL NOT NULL,
    archive_holdback_amount     REAL NOT NULL,
    archive_holdback_status     TEXT NOT NULL DEFAULT 'holding',
    archive_returned_at         TEXT,
    archive_returned_amount     REAL NOT NULL DEFAULT 0,
    invoice_date                TEXT,
    invoice_no                  TEXT,
    recognized_month            TEXT NOT NULL,
    actual_income_amount        REAL NOT NULL,
    actual_income_overridden    INTEGER NOT NULL DEFAULT 0,
    actual_income_override_note TEXT,
    note                        TEXT,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_case_income_records_case
ON case_income_records(case_id);

CREATE INDEX IF NOT EXISTS idx_case_income_records_recognized_month
ON case_income_records(recognized_month);

CREATE INDEX IF NOT EXISTS idx_case_income_records_source_month
ON case_income_records(source_type, recognized_month);

CREATE INDEX IF NOT EXISTS idx_case_income_records_holdback_status
ON case_income_records(archive_holdback_status, recognized_month);
