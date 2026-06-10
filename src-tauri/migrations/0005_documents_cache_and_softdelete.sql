-- migration 0005: documents 表加缓存键 + 软删除 + extracts 文件路径
--
-- 2026-05-23 晚十 作者拍板 Q1-Q3:
--   Q1: extracts/<doc_id>.md 写盘 → 加 extracted_text_path 字段
--   Q2: mtime + size 做缓存键 → modified_at 已有,加 cache_key 冗余字段方便比对
--   Q3: 软删 → 加 deleted_at,看板按 deleted_at IS NULL 过滤
--
-- 目的:重扫不重抽 — 同一份文件(source_path + mtime + size 不变)→ 跳过

-- 软删除标记:用户从源文件夹删了文件,但 DB 留痕便于追溯
ALTER TABLE documents ADD COLUMN deleted_at TEXT;

-- 抽取出的纯文本 MD 文件路径(extracts/<case_id>/<doc_id>.md)
-- pipeline 写盘后填这里;后续可以从这个文件读做全文搜索 / 用户预览
ALTER TABLE documents ADD COLUMN extracted_text_path TEXT;

-- 缓存键 = mtime + size,用于判断"文件是否变过"。
-- 重扫时:DB 里现有缓存键 != 当前扫到的 → 重新抽取。
ALTER TABLE documents ADD COLUMN cache_key TEXT;

-- 软删后过滤用的索引
CREATE INDEX idx_documents_active ON documents(case_id, deleted_at);
