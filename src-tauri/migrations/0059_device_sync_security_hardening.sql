-- v0.8.1 device-sync security hardening.
-- Give legal skill binding suppressions a stable logical ID so that a user's
-- explicit "no default skill" decision participates in encrypted sync.
ALTER TABLE legal_skill_binding_suppressions
RENAME TO legal_skill_binding_suppressions_legacy;

CREATE TABLE legal_skill_binding_suppressions (
    id TEXT PRIMARY KEY NOT NULL,
    legal_domain TEXT NOT NULL,
    task_type TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT 'user_unbound',
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    UNIQUE(legal_domain, task_type)
);

INSERT INTO legal_skill_binding_suppressions (
    id, legal_domain, task_type, reason, created_at, updated_at
)
SELECT lower(hex(legal_domain || char(31) || task_type)),
       legal_domain, task_type, reason, created_at, updated_at
FROM legal_skill_binding_suppressions_legacy;

DROP TABLE legal_skill_binding_suppressions_legacy;

CREATE TRIGGER device_sync_skill_binding_suppressions_insert
AFTER INSERT ON legal_skill_binding_suppressions BEGIN
  INSERT INTO device_sync_dirty_entities
  VALUES('legal_skill_binding_suppression',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET
    action='upsert',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_skill_binding_suppressions_update
AFTER UPDATE ON legal_skill_binding_suppressions BEGIN
  INSERT INTO device_sync_dirty_entities
  VALUES('legal_skill_binding_suppression',NEW.id,NULL,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET
    action='upsert',changed_at=excluded.changed_at;
END;

CREATE TRIGGER device_sync_skill_binding_suppressions_delete
AFTER DELETE ON legal_skill_binding_suppressions BEGIN
  INSERT INTO device_sync_dirty_entities
  VALUES('legal_skill_binding_suppression',OLD.id,NULL,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET
    action='tombstone',changed_at=excluded.changed_at;
END;
