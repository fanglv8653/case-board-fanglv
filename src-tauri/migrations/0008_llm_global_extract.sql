-- 2026-05-24 h · LLM 全局抽方案上线(替代旧 aggregator 规则)
--
-- 新流程:所有文档 OCR 后,把每份 MD 拼起来 → DeepSeek 1M 上下文 → 两次调用:
--   call A (JSON):填表(下面这些 agg_* 字段)
--   call B (MD):生成完整案件分析报告,落盘到 reports/<case_id>.md
--
-- 字段说明:
--   case_summary             - LLM 给的一句话案件概括(详情页 hero / 列表卡片用)
--   case_report_path         - 完整案件分析报告 MD 路径(详情页「📖 案件报告」按钮渲染)
--   case_report_generated_at - 报告生成时间(增量更新判断 / 老报告检测)
--   agg_resolution           - 调解 / 判决 / 执行结果(自由文本)
--   agg_status_text          - LLM 推断的状态文字描述(跟 workflow_status 8 档不同,自由文本)
--   agg_party_contacts       - 当事人详细联系方式 JSON [{name,role,id_no,address,phone,is_our_side}]
--   agg_court_contacts       - 法院联系人 JSON [{name,role,phone}] -- 替代旧 agg_judges 的扩展版
--   agg_key_dates            - 关键日期 JSON [{date,event,note}]
--   agg_fees                 - 收费记录 JSON [{item,amount,note}]

ALTER TABLE cases ADD COLUMN case_summary TEXT;
ALTER TABLE cases ADD COLUMN case_report_path TEXT;
ALTER TABLE cases ADD COLUMN case_report_generated_at TEXT;
ALTER TABLE cases ADD COLUMN agg_resolution TEXT;
ALTER TABLE cases ADD COLUMN agg_status_text TEXT;
ALTER TABLE cases ADD COLUMN agg_party_contacts TEXT;
ALTER TABLE cases ADD COLUMN agg_court_contacts TEXT;
ALTER TABLE cases ADD COLUMN agg_key_dates TEXT;
ALTER TABLE cases ADD COLUMN agg_fees TEXT;
