-- ============================================================================
-- 0002: cases 表加聚合字段 + 下一关键节点 + 执行款追踪聚合
-- events/parties/contacts 三张已有表加业务字段
--
-- 对应 docs/architecture-v0.2.md 第 5.4 节
-- ============================================================================

-- ---------------------------------------------------------------------------
-- cases 表:案件级聚合字段(由 aggregator 从 documents.extracted_fields 算出)
-- ---------------------------------------------------------------------------
ALTER TABLE cases ADD COLUMN agg_case_no       TEXT;
ALTER TABLE cases ADD COLUMN agg_court         TEXT;
ALTER TABLE cases ADD COLUMN agg_cause         TEXT;
ALTER TABLE cases ADD COLUMN agg_plaintiffs    TEXT;   -- JSON array
ALTER TABLE cases ADD COLUMN agg_defendants    TEXT;   -- JSON array
ALTER TABLE cases ADD COLUMN agg_third_parties TEXT;   -- JSON array
ALTER TABLE cases ADD COLUMN agg_judges        TEXT;   -- JSON array
ALTER TABLE cases ADD COLUMN agg_claim_amount  REAL;
ALTER TABLE cases ADD COLUMN agg_filed_at      TEXT;   -- ISO 8601 DATE
ALTER TABLE cases ADD COLUMN agg_computed_at   TEXT;   -- ISO 8601 DATETIME

-- ---------------------------------------------------------------------------
-- cases 表:下一关键节点(驱动首页"办案节点 30 天" widget)
-- ---------------------------------------------------------------------------
ALTER TABLE cases ADD COLUMN next_milestone_type   TEXT;  -- 开庭/上诉期/举证期/保全到期/续封/限消查询/审限届满/缴费/送达/回款记录/其他
ALTER TABLE cases ADD COLUMN next_milestone_at     TEXT;  -- ISO 8601 DATETIME
ALTER TABLE cases ADD COLUMN next_milestone_status TEXT;  -- 进行中/即将到期(7天内)/已完成
ALTER TABLE cases ADD COLUMN next_milestone_note   TEXT;

-- ---------------------------------------------------------------------------
-- cases 表:案件总状态(独立于 stage)
-- ---------------------------------------------------------------------------
ALTER TABLE cases ADD COLUMN case_status TEXT NOT NULL DEFAULT '进行中';  -- 进行中/已结案/已归档

-- ---------------------------------------------------------------------------
-- cases 表:执行款追踪聚合(细节在 execution_payments 表)
-- ---------------------------------------------------------------------------
ALTER TABLE cases ADD COLUMN execution_total              REAL;  -- 判决总执行金额
ALTER TABLE cases ADD COLUMN execution_total_breakdown    TEXT;  -- JSON {本金/违约金/律师费/受理费}
ALTER TABLE cases ADD COLUMN execution_started_at         TEXT;  -- 执行立案日(利息起算)
ALTER TABLE cases ADD COLUMN execution_received           REAL;  -- 实收总额(execution_payments 累加)
ALTER TABLE cases ADD COLUMN execution_remaining          REAL;  -- 未收余额(execution_total - execution_received)

CREATE INDEX IF NOT EXISTS idx_cases_milestone_at ON cases(next_milestone_at);
CREATE INDEX IF NOT EXISTS idx_cases_status      ON cases(case_status);

-- ---------------------------------------------------------------------------
-- events 表扩字段(事件管理 + 提醒)
-- ---------------------------------------------------------------------------
ALTER TABLE events ADD COLUMN event_type     TEXT;        -- 开庭/上诉期/举证期/保全到期/续封/限消查询/审限届满/缴费/送达/其他
ALTER TABLE events ADD COLUMN court_room     TEXT;        -- 法庭(如"第三审判庭")
ALTER TABLE events ADD COLUMN reminder_days  INTEGER;     -- 提前几天提醒(默认 7)
ALTER TABLE events ADD COLUMN is_done        INTEGER NOT NULL DEFAULT 0;
ALTER TABLE events ADD COLUMN done_at        TEXT;

CREATE INDEX IF NOT EXISTS idx_events_type    ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_is_done ON events(case_id, is_done);

-- ---------------------------------------------------------------------------
-- parties 表加联系字段(LLM 从起诉状/委托合同里抽出当事人手机号/邮箱)
-- ---------------------------------------------------------------------------
ALTER TABLE parties ADD COLUMN contact_phone TEXT;
ALTER TABLE parties ADD COLUMN contact_email TEXT;

-- ---------------------------------------------------------------------------
-- contacts 表细化角色(主办法官/书记员/法官助理/检察官)
-- ---------------------------------------------------------------------------
ALTER TABLE contacts ADD COLUMN role_in_court TEXT;
