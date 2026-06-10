-- 2026-05-27 V0.2 D2 · 案件 AI 助手 V2 数据模型扩展
--
-- 详 docs/V0.2-法律AI工作台-实施计划.md § 3.1。本 migration 落:
--   1. documents.pinned_at —— 引用弹窗"📎 引用文件"里的置顶能力
--   2. chat_messages 扩 3 列 —— attached_doc_ids / citations_json / task_id
--   3. 新表 chat_tasks —— 长任务持久化(规划/执行/校验/合成多步状态机)
--   4. 新表 yuandian_credits_monthly —— 月度积分账,避免每次扫 chat_tasks
--
-- 隐私:本 migration 不引入新隐私字段,chat_tasks.error_short 入库前
--      仍要走 feedback::sanitize_paths(由调用方负责)。
--
-- 注意 CLAUDE.md 坑 #3:sqlx 启动 SHA384 校验,一发布就不能再改本文件 —
-- 后续 schema 变化必须新增 0019/0020/...

PRAGMA foreign_keys = ON;

-- ----------------------------------------------------------------------------
-- 1. documents.pinned_at — 引用弹窗排序用
-- ----------------------------------------------------------------------------
ALTER TABLE documents ADD COLUMN pinned_at TEXT;

CREATE INDEX idx_documents_pinned ON documents(case_id, pinned_at DESC) WHERE pinned_at IS NOT NULL;

-- ----------------------------------------------------------------------------
-- 2. chat_tasks — 长任务持久化(必须先建,chat_messages.task_id FK 它)
-- ----------------------------------------------------------------------------
CREATE TABLE chat_tasks (
    id                      TEXT PRIMARY KEY NOT NULL,
    case_id                 TEXT NOT NULL,
    message_id              TEXT NOT NULL,
    task_type               TEXT NOT NULL,                -- 'compile_legal_basis' | 'find_similar_cases' | 'verify_my_draft' | ...
    status                  TEXT NOT NULL,                -- 'planning' | 'executing' | 'synthesizing' | 'verifying' | 'done' | 'failed' | 'cancelled'
    attached_doc_ids        TEXT,                         -- JSON 数组
    plan_json               TEXT,                         -- 主 agent 规划出的子任务清单
    subtask_results_json    TEXT,                         -- 并行子任务结果汇总
    tool_calls_json         TEXT,                         -- 工具调用 trace
    citations_json          TEXT,                         -- <CITATIONS> 解析后落库
    verification_passes     INTEGER,                      -- hall_detect 跑了几轮
    yuandian_credits_used   INTEGER NOT NULL DEFAULT 0,
    kb_hits                 INTEGER NOT NULL DEFAULT 0,
    yuandian_calls          INTEGER NOT NULL DEFAULT 0,
    model_used              TEXT,
    prompt_tokens           INTEGER,
    completion_tokens       INTEGER,
    cache_hit_tokens        INTEGER,                      -- DeepSeek prefix cache 命中
    artifact_doc_id         TEXT,
    started_at              TEXT NOT NULL,
    finished_at             TEXT,
    error_short             TEXT,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (artifact_doc_id) REFERENCES documents(id) ON DELETE SET NULL
);

CREATE INDEX idx_chat_tasks_case ON chat_tasks(case_id, started_at DESC);
CREATE INDEX idx_chat_tasks_active ON chat_tasks(status, started_at DESC)
    WHERE status IN ('planning','executing','synthesizing','verifying');

-- ----------------------------------------------------------------------------
-- 3. chat_messages 扩 3 列
-- ----------------------------------------------------------------------------
ALTER TABLE chat_messages ADD COLUMN attached_doc_ids TEXT;  -- JSON 数组,本轮引用的 doc.id
ALTER TABLE chat_messages ADD COLUMN citations_json TEXT;    -- <CITATIONS> 解析后落库
ALTER TABLE chat_messages ADD COLUMN task_id TEXT REFERENCES chat_tasks(id) ON DELETE SET NULL;

CREATE INDEX idx_chat_messages_task ON chat_messages(task_id) WHERE task_id IS NOT NULL;

-- ----------------------------------------------------------------------------
-- 4. yuandian_credits_monthly — 月度积分账
-- ----------------------------------------------------------------------------
CREATE TABLE yuandian_credits_monthly (
    year_month              TEXT PRIMARY KEY NOT NULL,    -- 'YYYY-MM'
    credits_used            INTEGER NOT NULL DEFAULT 0,
    api_calls               INTEGER NOT NULL DEFAULT 0,
    kb_hits                 INTEGER NOT NULL DEFAULT 0,
    updated_at              TEXT NOT NULL
);
