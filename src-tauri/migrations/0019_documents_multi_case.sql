-- 0019 · 2026-06-04 · 多案件共用材料:一个文件可同时属于多个案件。
--
-- 背景:多案件文件夹拆分后,「原告身份证 / 共同被告流水 / 聊天记录」等共用证据
-- 需要同时挂到 A 案、B 案。原本 source_path 是**全局**唯一索引,一个文件只能属一个案件,
-- 第二个案件 INSERT 会撞 UNIQUE。这里放宽成 **(case_id, source_path) 复合唯一** ——
-- 同一文件在不同案件各一行,案件内仍唯一(防重复扫描入库)。
--
-- 安全性:`idx_documents_source_path` 是**独立**索引(非表内联约束),DROP + 重建即可,
-- 无需重建表。迁移前 source_path 全局唯一 → (case_id, source_path) 必然也唯一,
-- 已有数据不会触发唯一冲突。所有按 case_id 过滤文档的查询不受影响。

DROP INDEX IF EXISTS idx_documents_source_path;
CREATE UNIQUE INDEX idx_documents_case_source ON documents(case_id, source_path);
