-- 2026-05-24 i · 修今日消费 = null bug:加 day_start_balance 字段
--
-- 现状 bug:`today_consumed` 用「昨天 snapshot - 今天 fetch」算,但用户今天才开始用,
-- DB 里没"昨天" snapshot,所以一直 None。
--
-- 修法:加 `day_start_balance` 字段,每天**第一次** fetch 时记今日初始余额,
-- 后续 fetch 用 UPSERT 但**不更新** day_start_balance,只更新 total_balance + fetched_at。
-- 算式:today_consumed = day_start_balance - current_total。
--
-- 兼容性:旧 snapshot 没填 day_start_balance,会 NULL,逻辑里用 IFNULL 兜底到 total_balance
--          (相当于"那天没有消费记录"),不影响新数据。

ALTER TABLE deepseek_balance_snapshots ADD COLUMN day_start_balance REAL;

-- 把现有 snapshot 的 day_start_balance 回填成 total_balance(等同于"消费=0"作为兜底)
UPDATE deepseek_balance_snapshots SET day_start_balance = total_balance WHERE day_start_balance IS NULL;
