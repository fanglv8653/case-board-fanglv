-- v0.8.4：把案件内待办演进为可独立存在的全局事项，并保留 0.8.3 兼容投影。
-- SQLx 默认在单事务中执行本迁移；任一步失败都不得留下半张表。

CREATE TABLE case_todos_v084 (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT,
    title TEXT NOT NULL CHECK(length(trim(title)) > 0),
    content TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT 'todo'
        CHECK(kind IN ('idea','todo','reminder','reference','memo')),
    priority TEXT NOT NULL DEFAULT 'unjudged'
        CHECK(priority IN ('high','medium','low','unjudged')),
    tags_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(tags_json)),
    next_action TEXT,
    status TEXT NOT NULL DEFAULT 'inbox'
        CHECK(status IN ('inbox','in_progress','waiting','completed','delete_pending','deleted')),
    done INTEGER NOT NULL DEFAULT 0 CHECK(done IN (0,1)),
    done_at TEXT,
    due_at TEXT,
    remind_at TEXT,
    due_date TEXT,
    source TEXT NOT NULL DEFAULT 'caseboard'
        CHECK(source IN ('caseboard','feishu','hermes')),
    source_message_id TEXT,
    source_at TEXT,
    delete_requested_at TEXT,
    delete_reason TEXT,
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE SET NULL
);

INSERT INTO case_todos_v084 (
    id, case_id, title, content, kind, priority, tags_json, next_action,
    status, done, done_at, due_at, remind_at, due_date, source,
    source_message_id, source_at, delete_requested_at, delete_reason, deleted_at,
    created_at, updated_at
)
SELECT
    id, case_id, title, '', 'todo', 'unjudged', '[]', NULL,
    CASE WHEN done = 1 THEN 'completed' ELSE 'inbox' END,
    done, done_at,
    CASE WHEN due_date IS NULL OR trim(due_date) = '' THEN NULL
         ELSE due_date || 'T00:00:00+08:00' END,
    NULL, due_date, 'caseboard',
    NULL, NULL, NULL, NULL, NULL,
    created_at, updated_at
FROM case_todos;

DROP TABLE case_todos;
ALTER TABLE case_todos_v084 RENAME TO case_todos;

CREATE INDEX idx_case_todos_case_done
ON case_todos(case_id, done)
WHERE deleted_at IS NULL;

CREATE INDEX idx_case_todos_status_due
ON case_todos(status, due_at, updated_at DESC)
WHERE deleted_at IS NULL;

CREATE INDEX idx_case_todos_due
ON case_todos(due_date)
WHERE due_date IS NOT NULL AND deleted_at IS NULL;

CREATE INDEX idx_case_todos_remind
ON case_todos(remind_at)
WHERE remind_at IS NOT NULL AND deleted_at IS NULL;

CREATE TRIGGER case_todos_source_immutable
BEFORE UPDATE OF source ON case_todos
WHEN NEW.source <> OLD.source
BEGIN
  SELECT RAISE(ABORT, 'TODO_SOURCE_IMMUTABLE');
END;

CREATE TRIGGER device_sync_todos_insert AFTER INSERT ON case_todos BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_todo',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_todos_update AFTER UPDATE ON case_todos BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_todo',NEW.id,NEW.case_id,'upsert',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='upsert',changed_at=excluded.changed_at;
END;
CREATE TRIGGER device_sync_todos_delete AFTER DELETE ON case_todos BEGIN
  INSERT INTO device_sync_dirty_entities VALUES('case_todo',OLD.id,OLD.case_id,'tombstone',datetime('now'))
  ON CONFLICT(entity_type,entity_id) DO UPDATE SET case_id=excluded.case_id,action='tombstone',changed_at=excluded.changed_at;
END;
