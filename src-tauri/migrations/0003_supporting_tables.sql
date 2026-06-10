-- ============================================================================
-- 0003: 6 张支撑表
--   case_stages         程序阶段(每阶段独立期限)
--   case_fees           律师费/受理费收款
--   case_logs           办案日志(过程留痕)
--   personal_tasks      普通任务(脱离案件)
--   execution_payments  执行款回款(作者 2026-05-23 需求:手工录入)
--   case_preservations  保全记录(LLM 抽 + 手工补)
--
-- 对应 docs/architecture-v0.2.md 第 5.4 节 + reports/04-supplementary-design.md
-- ============================================================================

-- ---------------------------------------------------------------------------
-- 程序阶段(每个案件多个阶段,每阶段独立"起算日 + 期限")
-- 支持审限计算 + 即将届满提醒
-- ---------------------------------------------------------------------------
CREATE TABLE case_stages (
    id                  TEXT PRIMARY KEY NOT NULL,
    case_id             TEXT NOT NULL,
    stage_name          TEXT NOT NULL,      -- 审查起诉/一审/二审/再审/执行
    started_at          TEXT,               -- ISO 8601 DATE (起算日)
    duration_days       INTEGER,            -- 法定期限天数
    expected_end_at     TEXT,               -- 自动算 = started_at + duration_days
    status              TEXT,               -- 进行中/已完成/未开始/待开始
    court               TEXT,               -- 该阶段承办机关
    notes               TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_case_stages_case_id ON case_stages(case_id);
CREATE INDEX idx_case_stages_end_at  ON case_stages(expected_end_at);

-- ---------------------------------------------------------------------------
-- 律师费/受理费/材料费 等收费记录
-- ---------------------------------------------------------------------------
CREATE TABLE case_fees (
    id              TEXT PRIMARY KEY NOT NULL,
    case_id         TEXT NOT NULL,
    item_name       TEXT NOT NULL,           -- 案件受理费/律师代理费/材料费/财产保全费
    amount          REAL NOT NULL,
    charged_at      TEXT,                    -- ISO 8601 DATE
    receipt_no      TEXT,                    -- 收据号 / 发票号
    notes           TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_case_fees_case_id ON case_fees(case_id);

-- ---------------------------------------------------------------------------
-- 办案日志(过程留痕,自动从 AI 报告抽 + 手工补)
-- ---------------------------------------------------------------------------
CREATE TABLE case_logs (
    id              TEXT PRIMARY KEY NOT NULL,
    case_id         TEXT NOT NULL,
    occurred_at     TEXT NOT NULL,           -- ISO 8601 DATETIME
    content         TEXT NOT NULL,
    source          TEXT,                    -- auto(从 AI 报告/调解书抽)/ manual
    source_doc_id   TEXT,                    -- 来源文档(如果是 auto)
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (source_doc_id) REFERENCES documents(id) ON DELETE SET NULL
);
CREATE INDEX idx_case_logs_case_id_occurred ON case_logs(case_id, occurred_at DESC);

-- ---------------------------------------------------------------------------
-- 普通任务(跟案件无关的小事项,手工录入)
-- ---------------------------------------------------------------------------
CREATE TABLE personal_tasks (
    id          TEXT PRIMARY KEY NOT NULL,
    title       TEXT NOT NULL,
    due_at      TEXT,                       -- ISO 8601 DATETIME
    status      TEXT NOT NULL DEFAULT '未开始',  -- 未开始/进行中/已完成
    notes       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_personal_tasks_due ON personal_tasks(due_at);
CREATE INDEX idx_personal_tasks_status ON personal_tasks(status);

-- ---------------------------------------------------------------------------
-- 执行回款(作者 2026-05-23 实务需求:执行款断续到账,手工录入)
-- 关联 lawtools.top/interest.html 算"剩余 + 利息 + 迟延履行金"
-- ---------------------------------------------------------------------------
CREATE TABLE execution_payments (
    id              TEXT PRIMARY KEY NOT NULL,
    case_id         TEXT NOT NULL,
    paid_at         TEXT NOT NULL,           -- ISO 8601 DATE
    amount          REAL NOT NULL,           -- 元
    payer           TEXT,                    -- 主债务人/担保人/拍卖款/抵债/其他
    payment_method  TEXT,                    -- 银行转账/现金/支付宝/拍卖款/抵债
    receipt_no      TEXT,
    notes           TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);
CREATE INDEX idx_execution_payments_case_id ON execution_payments(case_id);
CREATE INDEX idx_execution_payments_paid_at ON execution_payments(paid_at);

-- ---------------------------------------------------------------------------
-- 保全记录(作者 2026-05-23 实务需求:保全到期提醒)
-- LLM 从财产保全裁定书自动抽 + 用户手工补
-- ---------------------------------------------------------------------------
CREATE TABLE case_preservations (
    id                  TEXT PRIMARY KEY NOT NULL,
    case_id             TEXT NOT NULL,
    target_type         TEXT NOT NULL,       -- 账户冻结/股权冻结/房产查封/车辆查封/其他
    target_detail       TEXT,                -- "无锡汇尔盛农业 800 万股权"
    preserved_amount    REAL,                -- 保全金额或标的价值
    court               TEXT,                -- 出具裁定的法院
    doc_no              TEXT,                -- 裁定书案号
    started_at          TEXT NOT NULL,       -- ISO 8601 DATE
    duration_years      INTEGER NOT NULL,    -- 期限年数(2/3)
    expires_at          TEXT NOT NULL,       -- 到期日(= started_at + duration_years 年)
    status              TEXT NOT NULL DEFAULT 'active',  -- active/expired/renewed/lifted
    renewed_to_id       TEXT,                -- 续封后的新记录 id
    notes               TEXT,
    source_doc_id       TEXT,                -- 来源文档(裁定书)
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (source_doc_id) REFERENCES documents(id) ON DELETE SET NULL,
    FOREIGN KEY (renewed_to_id) REFERENCES case_preservations(id) ON DELETE SET NULL
);
CREATE INDEX idx_case_preservations_case_id ON case_preservations(case_id);
CREATE INDEX idx_case_preservations_expires ON case_preservations(expires_at);
CREATE INDEX idx_case_preservations_status  ON case_preservations(status);
