-- 2026-05-26 V0.1.13+ · 案件 AI 助手(case-aware chat)
--
-- 目的:在案件详情页右侧加聊天面板,围绕当前 case_id 问答 + 触发固定任务
--      (生成案件总览 / 证据目录 / 时间线 / 客户进展 / 查付款 / 待补)。
--
-- 设计:
--   - chat_messages:聊天记录主表,每条 user/assistant 一行
--     · 流式输出完成后整段落盘(中途不写,避免半句进 DB)
--     · 记录 token 用量 + 延迟,用于反馈 MD 性能埋点
--     · based_on 记录这次回答引用了哪些 document.id (JSON 数组,可空)
--     · content 内容**不进**反馈 MD(隐私铁律 #3),只走前端 listChatHistory
--
--   - documents 加 source 列:区分 'scan'(扫到的原始文件) /
--     'llm_extract'(LLM 全局抽出的 MD 报告) /
--     'chat'(chat 面板生成的 artifact)
--     回写 backfill:旧 is_ai_artifact=1 → 'llm_extract',否则 'scan'
--
-- 隐私:chat_messages.content 永远不进反馈 MD;feedback::tests 加 regression。

PRAGMA foreign_keys = ON;

-- ----------------------------------------------------------------------------
-- 案件聊天记录
-- ----------------------------------------------------------------------------
CREATE TABLE chat_messages (
    id                  TEXT PRIMARY KEY NOT NULL,
    case_id             TEXT NOT NULL,
    role                TEXT NOT NULL,           -- 'user' / 'assistant'
    content             TEXT NOT NULL,           -- 完整消息文本(流式拼完后整段写)
    task_type           TEXT,                    -- NULL=自由问,否则枚举:
                                                 --   generate_case_overview
                                                 --   generate_evidence_list
                                                 --   generate_timeline
                                                 --   generate_client_update
                                                 --   find_payment
                                                 --   list_missing
    model               TEXT,                    -- 'deepseek-v4-flash' / 'deepseek-v4-pro' / 本机模型名
    prompt_tokens       INTEGER,                 -- DeepSeek usage.prompt_tokens
    completion_tokens   INTEGER,                 -- DeepSeek usage.completion_tokens
    latency_ms          INTEGER,                 -- 从请求发出到流式结束的耗时
    based_on            TEXT,                    -- JSON 数组,引用的 document.id
                                                 -- 例:["doc-uuid-1","doc-uuid-2"]
    artifact_doc_id     TEXT,                    -- 若本条 assistant 输出落了 artifact
                                                 -- 这里指向 documents.id (FK)
    error_short         TEXT,                    -- 若 assistant 出错,这里填脱敏错误
                                                 -- (content 此时为部分输出或空串)
    -- 毫秒精度时间戳:chat 一秒内可能多条消息,需要毫秒级排序稳定
    -- (其他业务表用 datetime('now') 秒级即可,这里特殊处理)
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (artifact_doc_id) REFERENCES documents(id) ON DELETE SET NULL
);

CREATE INDEX idx_chat_messages_case_created ON chat_messages(case_id, created_at);

-- ----------------------------------------------------------------------------
-- documents 加 source 列
--
-- 三态:
--   'scan'        — 扫描原始文件夹时录入的源文件(默认)
--   'llm_extract' — LLM 全局抽产生的 MD 报告(案件画像 / 风险报告 / 深挖等)
--   'chat'        — 案件 AI 助手的 chat artifact(总览 / 证据目录 / 时间线等)
--
-- backfill 策略:旧 is_ai_artifact=1 → 'llm_extract',其余 → 'scan'。
-- 之后 'chat' 由 chat 模块在落 artifact 时显式写入。
-- ----------------------------------------------------------------------------
ALTER TABLE documents ADD COLUMN source TEXT NOT NULL DEFAULT 'scan';

UPDATE documents SET source = 'llm_extract' WHERE is_ai_artifact = 1;

CREATE INDEX idx_documents_source ON documents(case_id, source);
