-- 2026-05-24 k · 元典查被执行人 P1 落地
--
-- 字段:
--   risk_assessment_path  - 风险提示报告 MD 路径(详情页「🔍 查被执行人」按钮渲染)
--   risk_assessment_at    - 报告生成时间(ISO 8601)
--
-- 报告文件结构:~/Library/.../external/<case_id>/
--   yuandian_raw/         ← 元典原始 JSON 落盘
--     <subject>_<endpoint>.json
--   reports/
--     risk_<ts>.md         ← LLM 风险提示报告
--     dig_hints_<ts>.json  ← 深挖建议 JSON(P2 用)

ALTER TABLE cases ADD COLUMN risk_assessment_path TEXT;
ALTER TABLE cases ADD COLUMN risk_assessment_at TEXT;
