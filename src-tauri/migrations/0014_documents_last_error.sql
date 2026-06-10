-- 2026-05-25 · 失败原因落库 + 三轮动态降级重试支持
--
-- 字段:
--   last_error  — 最近一次抽取失败的错误信息(OCR / LLM / 网络等);
--                 成功 / skipped 时会被 UPDATE 成 NULL。
--                 用于事后复盘:之前失败只更新 status='failed' 不存原因,两眼一抹黑。
--
-- 配合 ingest/pipeline.rs 的三轮动态降级(8 路 → 4 路 → 1 路),
-- 还失败的才落 last_error 标 failed,前端可读 last_error 显示给用户。

ALTER TABLE documents ADD COLUMN last_error TEXT;
