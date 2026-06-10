-- 2026-05-24 k-9 · P2 深挖
--
-- 字段:
--   deep_dive_report_path  - P2 深查报告 MD 路径
--   deep_dive_at           - 报告生成时间(ISO 8601)
--
-- 报告位置:~/Library/.../external/<case_id>/reports/deepdive_<ts>.md
-- 原始数据:~/Library/.../external/<case_id>/yuandian_deepdive/<target>_<endpoint>.json

ALTER TABLE cases ADD COLUMN deep_dive_report_path TEXT;
ALTER TABLE cases ADD COLUMN deep_dive_at TEXT;
