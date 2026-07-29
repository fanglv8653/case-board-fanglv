-- 方律全局法律 Skills（方法包）。
-- 方法包只保存文本/JSON 方法上下文，不授予工具权限，也不保存具体案件事实。

CREATE TABLE legal_skill_packages (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    version TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    origin TEXT NOT NULL CHECK(origin IN ('builtin','imported')),
    status TEXT NOT NULL DEFAULT 'disabled'
        CHECK(status IN ('enabled','disabled','quarantined','deleted')),
    manifest_json TEXT NOT NULL,
    package_content_json TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    UNIQUE(slug, version)
);
CREATE INDEX idx_legal_skill_packages_status
ON legal_skill_packages(status, origin, slug);

CREATE TABLE legal_skill_bindings (
    id TEXT PRIMARY KEY NOT NULL,
    skill_id TEXT NOT NULL,
    legal_domain TEXT NOT NULL,
    task_type TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0 CHECK(is_default IN (0,1)),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(skill_id) REFERENCES legal_skill_packages(id) ON DELETE CASCADE,
    UNIQUE(skill_id, legal_domain, task_type)
);
CREATE UNIQUE INDEX uq_legal_skill_default_binding
ON legal_skill_bindings(legal_domain, task_type)
WHERE is_default = 1;
CREATE INDEX idx_legal_skill_bindings_skill
ON legal_skill_bindings(skill_id, legal_domain, task_type);

-- 用户明确删除当前默认导入包后，保留“无附加方法”选择，避免自动回落到内置包。
CREATE TABLE legal_skill_binding_suppressions (
    legal_domain TEXT NOT NULL,
    task_type TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT 'user_unbound',
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    PRIMARY KEY(legal_domain, task_type)
);

CREATE TABLE legal_skill_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    skill_id TEXT NOT NULL,
    slug TEXT NOT NULL,
    version TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    package_content_json TEXT NOT NULL,
    revision_action TEXT NOT NULL
        CHECK(revision_action IN ('registered','upgraded','rolled_back','quarantined')),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(skill_id) REFERENCES legal_skill_packages(id) ON DELETE RESTRICT,
    UNIQUE(skill_id, content_hash, revision_action)
);
CREATE INDEX idx_legal_skill_revisions_skill
ON legal_skill_revisions(skill_id, created_at DESC);

CREATE TABLE legal_skill_run_audits (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    skill_id TEXT,
    slug TEXT,
    version TEXT,
    content_hash TEXT,
    selection_source TEXT NOT NULL
        CHECK(selection_source IN ('automatic','user','none')),
    truncated INTEGER NOT NULL DEFAULT 0 CHECK(truncated IN (0,1)),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(skill_id) REFERENCES legal_skill_packages(id) ON DELETE SET NULL,
    UNIQUE(run_id)
);
CREATE INDEX idx_legal_skill_run_audits_created
ON legal_skill_run_audits(created_at DESC);

CREATE TABLE legal_skill_import_audits (
    id TEXT PRIMARY KEY NOT NULL,
    skill_id TEXT,
    slug TEXT,
    version TEXT,
    content_hash TEXT,
    action TEXT NOT NULL
        CHECK(action IN ('register','enable','disable','upgrade','rollback','delete','quarantine','bind','unbind')),
    outcome TEXT NOT NULL CHECK(outcome IN ('succeeded','rejected','failed')),
    error_code TEXT,
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(skill_id) REFERENCES legal_skill_packages(id) ON DELETE SET NULL
);
CREATE INDEX idx_legal_skill_import_audits_created
ON legal_skill_import_audits(created_at DESC);
