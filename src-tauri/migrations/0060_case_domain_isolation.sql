-- v0.8.2：民事/刑事状态与阶段领域隔离。
-- 保留刑事案件曾写入的民事 workflow_status 作为可审计历史，再取消其展示/锁定效力。

CREATE TABLE case_domain_status_migration_audits (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    legal_domain TEXT NOT NULL,
    legacy_workflow_status TEXT,
    legacy_workflow_status_locked INTEGER NOT NULL DEFAULT 0,
    action TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX idx_case_domain_status_migration_audits_case
ON case_domain_status_migration_audits(case_id, created_at DESC);

INSERT INTO case_domain_status_migration_audits (
    id, case_id, legal_domain, legacy_workflow_status,
    legacy_workflow_status_locked, action
)
SELECT
    lower(hex(randomblob(16))), id, legal_domain, workflow_status,
    workflow_status_locked, 'detach_civil_workflow_from_criminal'
FROM cases
WHERE legal_domain = 'criminal'
  AND (workflow_status IS NOT NULL OR workflow_status_locked <> 0);

UPDATE cases
SET workflow_status = NULL,
    workflow_status_locked = 0,
    updated_at = datetime('now')
WHERE legal_domain = 'criminal'
  AND (workflow_status IS NOT NULL OR workflow_status_locked <> 0);

-- v0.7.6 飞书入站阶段曾统一写为 other；只有已绑定案件领域明确时才纠正。
UPDATE case_stage_items
SET domain = (
        SELECT c.legal_domain
        FROM cases c
        WHERE c.id = case_stage_items.case_id
    ),
    updated_at = datetime('now')
WHERE external_source = 'feishu'
  AND domain = 'other'
  AND EXISTS (
      SELECT 1 FROM cases c
      WHERE c.id = case_stage_items.case_id
        AND c.legal_domain IN ('civil', 'criminal')
  );

CREATE TRIGGER case_stage_items_domain_guard_insert
BEFORE INSERT ON case_stage_items
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM cases c
    WHERE c.id = NEW.case_id
      AND c.legal_domain = NEW.domain
)
BEGIN
    SELECT RAISE(ABORT, 'CASE_STAGE_DOMAIN_MISMATCH');
END;
CREATE TRIGGER case_stage_items_domain_guard_update
BEFORE UPDATE OF case_id, domain ON case_stage_items
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM cases c
    WHERE c.id = NEW.case_id
      AND c.legal_domain = NEW.domain
)
BEGIN
    SELECT RAISE(ABORT, 'CASE_STAGE_DOMAIN_MISMATCH');
END;
