-- ============================================================================
-- CaseBoard V0.1 初始 schema
--
-- 设计说明:
--   - 所有 ID 用 UUID (TEXT, v4),为团队版预留(以后多端合并不撞车)
--   - 所有时间字段用 ISO 8601 TEXT(SQLite 推荐做法,避免时区坑)
--   - JSON 字段用 TEXT 存(SQLite 3.38+ 有 JSON1,够用)
--   - 外键开 ON DELETE CASCADE,删案件时关联数据全清
--
-- 完整设计见项目根 CLAUDE.md "数据模型补一版(诉讼版)" 章节。
-- ============================================================================

PRAGMA foreign_keys = ON;

-- ----------------------------------------------------------------------------
-- 案件主表
-- ----------------------------------------------------------------------------
CREATE TABLE cases (
    id                TEXT PRIMARY KEY NOT NULL,
    name              TEXT NOT NULL,           -- 例如:"起诉张三 股权转让纠纷"
    case_type         TEXT NOT NULL DEFAULT '诉讼',  -- 诉讼 / 非诉
    cause             TEXT,                    -- 案由:股权转让纠纷
    case_no           TEXT,                    -- 案号
    court             TEXT,                    -- 受理法院
    judge_id          TEXT,                    -- 承办法官(→ contacts.id)
    stage             TEXT,                    -- 立案 / 一审 / 二审 / 再审 / 执行 / 已结
    source_folder     TEXT NOT NULL,           -- 原始文件夹绝对路径(只读引用)
    ai_summary_md     TEXT,                    -- 案件总览.md 的全文(AI 中间产物)
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
    last_scanned_at   TEXT,
    FOREIGN KEY (judge_id) REFERENCES contacts(id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED
);
CREATE UNIQUE INDEX idx_cases_source_folder ON cases(source_folder);
CREATE INDEX idx_cases_stage ON cases(stage);

-- ----------------------------------------------------------------------------
-- 当事人(原告/被告/第三人/被执行人)
-- ----------------------------------------------------------------------------
CREATE TABLE parties (
    id            TEXT PRIMARY KEY NOT NULL,
    case_id       TEXT NOT NULL,
    role          TEXT NOT NULL,    -- 原告/被告/第三人/被执行人
    name          TEXT NOT NULL,
    party_type    TEXT,             -- 自然人 / 公司
    id_no         TEXT,             -- 身份证号 / 统一社会信用代码
    id_doc_path   TEXT,             -- 身份证图片路径(只读引用)
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_parties_case_id ON parties(case_id);

-- ----------------------------------------------------------------------------
-- 文档(只存路径 + 元数据 + 抽取结果)
-- ----------------------------------------------------------------------------
CREATE TABLE documents (
    id                  TEXT PRIMARY KEY NOT NULL,
    case_id             TEXT NOT NULL,
    source_path         TEXT NOT NULL,    -- 原文件绝对路径(只读引用)
    filename            TEXT NOT NULL,
    stage               TEXT,             -- 立案 / 一审 / 二审 / 执行 / 证据 / 身份信息
    category            TEXT,             -- 起诉状 / 判决书 / 笔录 / ...
    is_ai_artifact      INTEGER NOT NULL DEFAULT 0,  -- bool(0/1)
    mime_type           TEXT,
    size_bytes          INTEGER NOT NULL DEFAULT 0,
    modified_at         TEXT,             -- 原文件 mtime
    extracted_fields    TEXT,             -- JSON: LLM 抽出的字段(案号/日期/金额...)
    extraction_status   TEXT NOT NULL DEFAULT 'pending',  -- pending/processing/done/failed
    missing             INTEGER NOT NULL DEFAULT 0,  -- 原文件是否失联
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_documents_case_id ON documents(case_id);
CREATE INDEX idx_documents_stage ON documents(case_id, stage);
CREATE UNIQUE INDEX idx_documents_source_path ON documents(source_path);

-- ----------------------------------------------------------------------------
-- 时间线节点
-- ----------------------------------------------------------------------------
CREATE TABLE events (
    id              TEXT PRIMARY KEY NOT NULL,
    case_id         TEXT NOT NULL,
    occurred_at     TEXT NOT NULL,     -- ISO 8601 日期/时间
    type            TEXT,              -- 立案/开庭/举证期/判决送达/上诉/执行立案/查控/...
    title           TEXT NOT NULL,
    notes           TEXT,
    related_doc_id  TEXT,              -- → documents.id (可选)
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (related_doc_id) REFERENCES documents(id) ON DELETE SET NULL
);
CREATE INDEX idx_events_case_id_occurred ON events(case_id, occurred_at);

-- ----------------------------------------------------------------------------
-- 联系人(法官/书记员/对方律师/当事人)
-- ----------------------------------------------------------------------------
CREATE TABLE contacts (
    id            TEXT PRIMARY KEY NOT NULL,
    case_id       TEXT,                -- 可为空(跨案件的法官)
    role          TEXT NOT NULL,       -- 法官/书记员/对方律师/当事人/对方当事人
    name          TEXT NOT NULL,
    phone_office  TEXT,                -- 座机
    mobile        TEXT,
    wechat        TEXT,
    email         TEXT,
    notes         TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE SET NULL
);
CREATE INDEX idx_contacts_case_id ON contacts(case_id);
CREATE INDEX idx_contacts_role ON contacts(role);

-- ----------------------------------------------------------------------------
-- 邮寄凭证(EMS/顺丰单号,这些是法律凭证,要存)
-- ----------------------------------------------------------------------------
CREATE TABLE mail_records (
    id              TEXT PRIMARY KEY NOT NULL,
    case_id         TEXT NOT NULL,
    direction       TEXT NOT NULL,     -- 寄出 / 收件
    carrier         TEXT,              -- EMS/顺丰/...
    tracking_no     TEXT,
    to_whom         TEXT,
    subject         TEXT,              -- 寄了什么
    sent_at         TEXT,              -- ISO 8601 日期
    received_at     TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_mail_records_case_id ON mail_records(case_id);
CREATE INDEX idx_mail_records_tracking_no ON mail_records(tracking_no);

-- ----------------------------------------------------------------------------
-- 执行阶段:被执行人状态追踪(V0.1 的杀手锏)
-- ----------------------------------------------------------------------------
CREATE TABLE execution_targets (
    id                    TEXT PRIMARY KEY NOT NULL,
    case_id               TEXT NOT NULL,
    party_id              TEXT NOT NULL,   -- → parties.id (必须是"被执行人"角色)
    has_xianxiao          INTEGER,         -- 限制消费(NULL = 未查询过)
    has_shixin            INTEGER,         -- 失信
    active_cases_count    INTEGER,         -- 在案执行案件数
    property_clues        TEXT,            -- 财产线索摘要(多行文本)
    last_queried_at       TEXT,            -- 上次查询时间
    next_due_at           TEXT,            -- 下次该查询的日期
    raw_snapshot          TEXT,            -- 每次查询的原始 JSON 快照(AI 通过 MCP 写入)
    created_at            TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at            TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (party_id) REFERENCES parties(id) ON DELETE CASCADE
);
CREATE INDEX idx_execution_targets_case_id ON execution_targets(case_id);
CREATE INDEX idx_execution_targets_due ON execution_targets(next_due_at);

-- ----------------------------------------------------------------------------
-- 已授权的 AI 客户端(V0.2 MCP server 安全)
-- ----------------------------------------------------------------------------
CREATE TABLE mcp_clients (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,        -- "Claude Desktop" / "Claude Code" / "Codex"
    authorized_at   TEXT NOT NULL DEFAULT (datetime('now')),
    scope           TEXT NOT NULL DEFAULT 'read',  -- read / read_write
    last_used_at    TEXT,
    revoked_at      TEXT                  -- 撤销时间,NULL 表示有效
);
CREATE INDEX idx_mcp_clients_name ON mcp_clients(name);
