-- Invoice recognition creates reviewable drafts only; existing manual records stay confirmed.
ALTER TABLE case_income_records ADD COLUMN record_status TEXT NOT NULL DEFAULT 'confirmed'
CHECK (record_status IN ('draft', 'confirmed'));
ALTER TABLE case_income_records ADD COLUMN invoice_total REAL;
ALTER TABLE case_income_records ADD COLUMN invoice_buyer TEXT;
ALTER TABLE case_income_records ADD COLUMN invoice_seller TEXT;
ALTER TABLE case_income_records ADD COLUMN invoice_type TEXT;
ALTER TABLE case_income_records ADD COLUMN auto_source_document_id TEXT;
ALTER TABLE case_income_records ADD COLUMN auto_source_filename TEXT;
ALTER TABLE case_income_records ADD COLUMN auto_fields_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE case_income_records ADD COLUMN manual_fields_json TEXT NOT NULL DEFAULT '[]';
CREATE UNIQUE INDEX IF NOT EXISTS idx_income_invoice_no_nonempty
ON case_income_records(invoice_no) WHERE invoice_no IS NOT NULL AND trim(invoice_no) <> '';
