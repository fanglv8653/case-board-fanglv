-- 2026-05-24 e · 案件工作流状态(看板卡片右上角的"接案/立案中/...")
--
-- 作者 2026-05-24 拍板 8 档状态:
--   intake / filing / awaiting_hearing / trial / appeal_window / appeal / execution / closed
--
-- NULL = 走前端自动推断(基于 documents.category + key_dates);
-- 非 NULL = 用户在 UI 上手工选过,优先取用户值
--
-- "closed"(已结案):卡片仍显示,dim 灰色 + 排到末尾
ALTER TABLE cases ADD COLUMN workflow_status TEXT;
