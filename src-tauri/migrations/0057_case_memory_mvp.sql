-- v0.8.1 · 案件隔离记忆 MVP
--
-- 案件记忆、用户通用偏好、系统规则物理分层。案件记忆在表结构、
-- 复合外键和查询 API 三层强制绑定 case_id；AI/工具只能提交候选，
-- 只有人工确认后的 active revision 才能进入逐轮注入预览。

CREATE UNIQUE INDEX IF NOT EXISTS ux_documents_case_id_id
ON documents(case_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS ux_chat_messages_case_id_id
ON chat_messages(case_id, id);

CREATE TABLE case_memory_items (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    memory_type TEXT NOT NULL
        CHECK(memory_type IN ('fact','procedure','strategy','client_instruction','risk_warning')),
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK(status IN ('draft','active','disabled','deleted')),
    verification_status TEXT NOT NULL DEFAULT 'unverified'
        CHECK(verification_status IN ('unverified','verified','disputed','stale')),
    injection_mode TEXT NOT NULL DEFAULT 'archive_only'
        CHECK(injection_mode IN ('archive_only','manual_each_turn')),
    current_revision_no INTEGER NOT NULL DEFAULT 1 CHECK(current_revision_no >= 1),
    active_revision_no INTEGER,
    created_by_type TEXT NOT NULL
        CHECK(created_by_type IN ('user','assistant','tool')),
    created_by_id TEXT,
    confirmed_by TEXT,
    confirmed_at TEXT,
    disabled_at TEXT,
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE,
    CHECK(
        (
            status = 'active'
            AND confirmed_by IS NOT NULL
            AND confirmed_at IS NOT NULL
            AND active_revision_no IS NOT NULL
        )
        OR status != 'active'
    ),
    CHECK(active_revision_no IS NULL OR active_revision_no BETWEEN 1 AND current_revision_no)
);

CREATE UNIQUE INDEX ux_case_memory_items_case_id_id
ON case_memory_items(case_id, id);

CREATE INDEX ix_case_memory_items_list
ON case_memory_items(case_id, status, verification_status, updated_at DESC);

CREATE TABLE case_memory_revisions (
    memory_id TEXT NOT NULL,
    case_id TEXT NOT NULL,
    revision_no INTEGER NOT NULL CHECK(revision_no >= 1),
    title TEXT NOT NULL CHECK(length(trim(title)) BETWEEN 1 AND 120),
    content TEXT NOT NULL CHECK(length(trim(content)) BETWEEN 1 AND 4000),
    change_reason TEXT NOT NULL,
    verification_status TEXT NOT NULL
        CHECK(verification_status IN ('unverified','verified','disputed','stale')),
    authored_by TEXT NOT NULL,
    authored_at TEXT NOT NULL DEFAULT(datetime('now')),
    confirmed_by TEXT,
    confirmed_at TEXT,
    content_sha256 TEXT NOT NULL,
    PRIMARY KEY(memory_id, revision_no),
    FOREIGN KEY(case_id, memory_id)
        REFERENCES case_memory_items(case_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX ux_case_memory_revisions_case
ON case_memory_revisions(case_id, memory_id, revision_no);

CREATE TABLE case_memory_sources (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    memory_id TEXT NOT NULL,
    revision_no INTEGER NOT NULL,
    source_type TEXT NOT NULL
        CHECK(source_type IN (
            'manual_assertion','document','chat_user','chat_assistant',
            'tool_result','case_field'
        )),
    document_id TEXT,
    chat_message_id TEXT,
    locator TEXT,
    excerpt TEXT,
    external_ref TEXT,
    source_sha256 TEXT,
    verification_status TEXT NOT NULL DEFAULT 'unverified'
        CHECK(verification_status IN ('unverified','verified','disputed','stale')),
    verified_by TEXT,
    verified_at TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(case_id, memory_id, revision_no)
        REFERENCES case_memory_revisions(case_id, memory_id, revision_no)
        ON DELETE CASCADE,
    FOREIGN KEY(case_id, document_id)
        REFERENCES documents(case_id, id) ON DELETE RESTRICT,
    FOREIGN KEY(case_id, chat_message_id)
        REFERENCES chat_messages(case_id, id) ON DELETE RESTRICT,
    CHECK(
        (source_type = 'document' AND document_id IS NOT NULL AND chat_message_id IS NULL)
        OR (
            source_type IN ('chat_user','chat_assistant')
            AND chat_message_id IS NOT NULL
            AND document_id IS NULL
        )
        OR (
            source_type IN ('manual_assertion','tool_result','case_field')
            AND document_id IS NULL
            AND chat_message_id IS NULL
        )
    )
);

CREATE INDEX ix_case_memory_sources_revision
ON case_memory_sources(case_id, memory_id, revision_no);

CREATE TRIGGER trg_case_memory_active_insert_guard
BEFORE INSERT ON case_memory_items
WHEN NEW.status = 'active'
BEGIN
    SELECT RAISE(ABORT, 'case memory cannot be inserted as active');
END;

CREATE TRIGGER trg_case_memory_active_revision_guard
BEFORE UPDATE OF status, active_revision_no ON case_memory_items
WHEN NEW.status = 'active'
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM case_memory_revisions r
        WHERE r.case_id = NEW.case_id
          AND r.memory_id = NEW.id
          AND r.revision_no = NEW.active_revision_no
          AND r.confirmed_by IS NOT NULL
          AND r.confirmed_at IS NOT NULL
    ) THEN RAISE(ABORT, 'active memory revision must exist and be confirmed') END;
END;

CREATE TABLE case_memory_candidates (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    proposed_type TEXT NOT NULL
        CHECK(proposed_type IN ('fact','procedure','strategy','client_instruction','risk_warning')),
    proposed_title TEXT NOT NULL CHECK(length(trim(proposed_title)) BETWEEN 1 AND 120),
    proposed_content TEXT NOT NULL CHECK(length(trim(proposed_content)) BETWEEN 1 AND 4000),
    proposed_by_type TEXT NOT NULL
        CHECK(proposed_by_type IN ('user','assistant','tool')),
    source_message_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','accepted','rejected','expired')),
    decided_by TEXT,
    decided_at TEXT,
    decision_reason TEXT,
    accepted_memory_id TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY(case_id, source_message_id)
        REFERENCES chat_messages(case_id, id) ON DELETE RESTRICT,
    FOREIGN KEY(case_id, accepted_memory_id)
        REFERENCES case_memory_items(case_id, id) ON DELETE RESTRICT,
    CHECK(
        (status = 'pending' AND decided_by IS NULL AND accepted_memory_id IS NULL)
        OR (status = 'accepted' AND decided_by IS NOT NULL AND accepted_memory_id IS NOT NULL)
        OR (
            status IN ('rejected','expired')
            AND decided_by IS NOT NULL
            AND accepted_memory_id IS NULL
        )
    )
);

CREATE INDEX ix_case_memory_candidates_list
ON case_memory_candidates(case_id, status, created_at DESC);

CREATE TABLE user_memory_preferences (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL CHECK(length(trim(title)) BETWEEN 1 AND 120),
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK(status IN ('draft','active','disabled','deleted')),
    injection_mode TEXT NOT NULL DEFAULT 'archive_only'
        CHECK(injection_mode IN ('archive_only','manual_each_turn')),
    current_revision_no INTEGER NOT NULL DEFAULT 1 CHECK(current_revision_no >= 1),
    confirmed_by TEXT,
    confirmed_at TEXT,
    disabled_at TEXT,
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    CHECK(
        (status = 'active' AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL)
        OR status != 'active'
    )
);

CREATE TABLE user_memory_preference_revisions (
    preference_id TEXT NOT NULL,
    revision_no INTEGER NOT NULL CHECK(revision_no >= 1),
    title TEXT NOT NULL CHECK(length(trim(title)) BETWEEN 1 AND 120),
    content TEXT NOT NULL CHECK(length(trim(content)) BETWEEN 1 AND 2000),
    change_reason TEXT NOT NULL,
    authored_by TEXT NOT NULL,
    authored_at TEXT NOT NULL DEFAULT(datetime('now')),
    confirmed_by TEXT,
    confirmed_at TEXT,
    content_sha256 TEXT NOT NULL,
    PRIMARY KEY(preference_id, revision_no),
    FOREIGN KEY(preference_id) REFERENCES user_memory_preferences(id) ON DELETE CASCADE
);

CREATE TRIGGER trg_user_memory_preference_active_insert_guard
BEFORE INSERT ON user_memory_preferences
WHEN NEW.status = 'active'
BEGIN
    SELECT RAISE(ABORT, 'user memory preference cannot be inserted as active');
END;

CREATE TRIGGER trg_user_memory_preference_active_revision_guard
BEFORE UPDATE OF status, current_revision_no ON user_memory_preferences
WHEN NEW.status = 'active'
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM user_memory_preference_revisions r
        WHERE r.preference_id = NEW.id
          AND r.revision_no = NEW.current_revision_no
          AND r.confirmed_by IS NOT NULL
          AND r.confirmed_at IS NOT NULL
    ) THEN RAISE(ABORT, 'active user preference revision must exist and be confirmed') END;
END;

CREATE TABLE memory_audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT,
    entity_type TEXT NOT NULL
        CHECK(entity_type IN ('case_memory','candidate','user_preference','injection')),
    entity_id TEXT NOT NULL,
    event_type TEXT NOT NULL
        CHECK(event_type IN (
            'created','candidate_accepted','candidate_rejected','confirmed',
            'revised','verified','marked_disputed','marked_stale',
            'disabled','restored','deleted','previewed','injected','cancelled'
        )),
    actor_type TEXT NOT NULL
        CHECK(actor_type IN ('user','assistant','tool','system')),
    actor_id TEXT,
    revision_no INTEGER,
    reason TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE SET NULL
);

CREATE INDEX ix_memory_audit_case_time
ON memory_audit_events(case_id, created_at DESC);

CREATE TABLE memory_injection_runs (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    task_type TEXT,
    target_message_id TEXT,
    system_rules_version TEXT NOT NULL,
    case_budget_chars INTEGER NOT NULL CHECK(case_budget_chars BETWEEN 0 AND 4500),
    preference_budget_chars INTEGER NOT NULL CHECK(preference_budget_chars BETWEEN 0 AND 1500),
    case_used_chars INTEGER NOT NULL CHECK(case_used_chars >= 0),
    preference_used_chars INTEGER NOT NULL CHECK(preference_used_chars >= 0),
    preview_sha256 TEXT NOT NULL,
    status TEXT NOT NULL
        CHECK(status IN ('preview','confirmed','injected','cancelled','expired')),
    confirmed_by TEXT,
    confirmed_at TEXT,
    injected_at TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY(case_id, target_message_id)
        REFERENCES chat_messages(case_id, id) ON DELETE RESTRICT,
    CHECK(case_used_chars <= case_budget_chars),
    CHECK(preference_used_chars <= preference_budget_chars)
);

CREATE UNIQUE INDEX ux_memory_injection_runs_case_id_id
ON memory_injection_runs(case_id, id);

CREATE TABLE memory_injection_case_entries (
    run_id TEXT NOT NULL,
    case_id TEXT NOT NULL,
    memory_id TEXT NOT NULL,
    revision_no INTEGER NOT NULL,
    display_order INTEGER NOT NULL,
    selected INTEGER NOT NULL CHECK(selected IN (0,1)),
    char_count INTEGER NOT NULL CHECK(char_count >= 0),
    omitted_reason TEXT,
    PRIMARY KEY(run_id, memory_id),
    FOREIGN KEY(case_id, run_id)
        REFERENCES memory_injection_runs(case_id, id) ON DELETE CASCADE,
    FOREIGN KEY(case_id, memory_id, revision_no)
        REFERENCES case_memory_revisions(case_id, memory_id, revision_no)
        ON DELETE RESTRICT
);

CREATE TABLE memory_injection_preference_entries (
    run_id TEXT NOT NULL,
    preference_id TEXT NOT NULL,
    revision_no INTEGER NOT NULL,
    display_order INTEGER NOT NULL,
    selected INTEGER NOT NULL CHECK(selected IN (0,1)),
    char_count INTEGER NOT NULL CHECK(char_count >= 0),
    omitted_reason TEXT,
    PRIMARY KEY(run_id, preference_id),
    FOREIGN KEY(run_id) REFERENCES memory_injection_runs(id) ON DELETE CASCADE,
    FOREIGN KEY(preference_id, revision_no)
        REFERENCES user_memory_preference_revisions(preference_id, revision_no)
        ON DELETE RESTRICT
);
