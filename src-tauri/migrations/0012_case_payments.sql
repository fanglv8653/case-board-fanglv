-- 2026-05-25 · 还款记录手工录入(V0.2 路线图)
--
-- 律师在执行案件里手工录入对方实际还款,App 自动计算剩余执行款 + 跟「利息执行款」
-- 工具联动。
--
-- 字段:
--   id          - UUID 主键
--   case_id     - 关联案件
--   amount      - 本次还款金额(元)
--   paid_at     - 实际付款日期(YYYY-MM-DD)
--   note        - 备注(转账银行 / 担保人代付 / 强制执行等)
--   created_at  - 录入时间

CREATE TABLE IF NOT EXISTS case_payments (
    id          TEXT PRIMARY KEY NOT NULL,
    case_id     TEXT NOT NULL,
    amount      REAL NOT NULL,
    paid_at     TEXT NOT NULL,
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX idx_case_payments_case ON case_payments(case_id, paid_at);
