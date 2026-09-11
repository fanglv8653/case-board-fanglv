-- v0.8.5：把手工收费记录与 LLM 聚合 JSON 分离，手工记录不可被重新识别覆盖。
ALTER TABLE case_fees ADD COLUMN source TEXT NOT NULL DEFAULT 'legacy'
    CHECK(source IN ('manual','recognized','legacy'));
ALTER TABLE case_fees ADD COLUMN updated_at TEXT NOT NULL DEFAULT '1970-01-01 00:00:00';
ALTER TABLE case_fees ADD COLUMN deleted_at TEXT;

UPDATE case_fees SET updated_at=created_at WHERE updated_at='1970-01-01 00:00:00';
CREATE INDEX idx_case_fees_active ON case_fees(case_id, deleted_at, charged_at DESC);
