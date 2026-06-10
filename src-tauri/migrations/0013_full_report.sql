-- 2026-05-25 · 完整报告(合并风险报告 + 深挖报告 → DeepSeek 总结出第三份)
--
-- 字段:
--   full_report_path  — 完整报告 MD 落盘路径(reports/<case_id>_full.md)
--   full_report_at    — 生成时间(ISO 8601)
--
-- 触发:用户在执行详情页点「查看完整报告」时,若未生成则调 merge_full_report 命令。

ALTER TABLE cases ADD COLUMN full_report_path TEXT;
ALTER TABLE cases ADD COLUMN full_report_at TEXT;
