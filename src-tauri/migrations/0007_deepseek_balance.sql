-- 2026-05-24 e · DeepSeek 余额快照(给"今日消费"计算用)
--
-- 思路参考一个 Swift 版余额监控工具的 DeepSeekService:
-- DeepSeek 官方 API 只提供 GET /user/balance(当前余额),没有"今日消费"端点。
-- 我们靠"昨天快照 vs 今天 fetch 的余额 delta"算今日消费 — 每天保存一次快照即可。
--
-- 字段:
--   date         YYYY-MM-DD 当地日期(主键,每天一条)
--   total_balance / granted_balance / topped_up_balance:CNY 元,DeepSeek API 原值
--   fetched_at   ISO8601,该天最后一次 fetch 时刻
CREATE TABLE IF NOT EXISTS deepseek_balance_snapshots (
    date              TEXT PRIMARY KEY NOT NULL,
    total_balance     REAL NOT NULL,
    granted_balance   REAL NOT NULL DEFAULT 0,
    topped_up_balance REAL NOT NULL DEFAULT 0,
    fetched_at        TEXT NOT NULL DEFAULT (datetime('now'))
);
