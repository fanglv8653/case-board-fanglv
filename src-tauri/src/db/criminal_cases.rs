//! 刑事案件后端最小模型(2026-07-04)
//!
//! 本模块仅提供刑事画像、阶段节点、期限节点、机关联系人四类最小 CRUD，
//! 作为后续 `CRIM-N2` UI 与 `CRIM-N3` 期限规则引擎的上游。
//! 不接首页、团队、聊天、AI、MCP，也不重复创建 `case_work_items`。

use std::collections::HashSet;

use chrono::{Months, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

const DEFAULT_DOMAIN: &str = "criminal";
const DEFAULT_STAGE_STATUS: &str = "pending";
const DEFAULT_RECORD_SOURCE: &str = "manual";
const DEFAULT_DEADLINE_PRIORITY: &str = "normal";
const DEFAULT_DEADLINE_STATUS: &str = "pending";
const DEFAULT_DEADLINE_SOURCE_TYPE: &str = "manual";
const AUTO_DEADLINE_SOURCE_TYPE: &str = "auto";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalCaseProfile {
    pub case_id: String,
    pub current_stage: Option<String>,
    pub procedure_type: Option<String>,
    pub case_subtype: Option<String>,
    pub defense_role: Option<String>,
    pub suspected_charge: Option<String>,
    pub suspect_or_defendant_name: Option<String>,
    pub victim_name: Option<String>,
    pub client_name: Option<String>,
    pub client_relationship: Option<String>,
    pub detention_center: Option<String>,
    pub coercive_measure_type: Option<String>,
    pub detention_date: Option<String>,
    pub arrest_request_date: Option<String>,
    pub arrest_review_received_date: Option<String>,
    pub arrest_decision_date: Option<String>,
    pub arrest_date: Option<String>,
    pub bail_start_date: Option<String>,
    pub residential_surveillance_start_date: Option<String>,
    pub transfer_for_prosecution_date: Option<String>,
    pub prosecution_received_date: Option<String>,
    pub first_instance_accepted_date: Option<String>,
    pub second_instance_accepted_date: Option<String>,
    pub judgment_received_date: Option<String>,
    pub ruling_received_date: Option<String>,
    pub stage_sort_mode: String,
    pub guilty_plea_status: Option<String>,
    pub sentencing_recommendation: Option<String>,
    pub sentence_term: Option<String>,
    pub charge_history_json: Option<String>,
    pub restitution_amount: Option<f64>,
    pub restitution_status: Option<String>,
    pub victim_forgiveness: Option<String>,
    pub surrender_status: Option<String>,
    pub meritorious_service_status: Option<String>,
    pub co_defendants_json: Option<String>,
    pub supplementary_investigation_1_date: Option<String>,
    pub supplementary_investigation_2_date: Option<String>,
    pub judgment_effective_date: Option<String>,
    pub death_penalty_review_start_date: Option<String>,
    pub extraction_meta_json: Option<String>,
    pub notes: Option<String>,
    pub user_overrides_json: Option<String>,
    pub profile_revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertCriminalCaseProfileInput {
    pub case_id: String,
    pub current_stage: Option<String>,
    pub procedure_type: Option<String>,
    pub case_subtype: Option<String>,
    pub defense_role: Option<String>,
    pub suspected_charge: Option<String>,
    pub suspect_or_defendant_name: Option<String>,
    pub victim_name: Option<String>,
    pub client_name: Option<String>,
    pub client_relationship: Option<String>,
    pub detention_center: Option<String>,
    pub coercive_measure_type: Option<String>,
    pub detention_date: Option<String>,
    pub arrest_request_date: Option<String>,
    pub arrest_review_received_date: Option<String>,
    pub arrest_decision_date: Option<String>,
    pub arrest_date: Option<String>,
    pub bail_start_date: Option<String>,
    pub residential_surveillance_start_date: Option<String>,
    pub transfer_for_prosecution_date: Option<String>,
    pub prosecution_received_date: Option<String>,
    pub first_instance_accepted_date: Option<String>,
    pub second_instance_accepted_date: Option<String>,
    pub judgment_received_date: Option<String>,
    pub ruling_received_date: Option<String>,
    pub stage_sort_mode: Option<String>,
    pub guilty_plea_status: Option<String>,
    pub sentencing_recommendation: Option<String>,
    pub sentence_term: Option<String>,
    pub charge_history_json: Option<String>,
    pub restitution_amount: Option<f64>,
    pub restitution_status: Option<String>,
    pub victim_forgiveness: Option<String>,
    pub surrender_status: Option<String>,
    pub meritorious_service_status: Option<String>,
    pub co_defendants_json: Option<String>,
    pub supplementary_investigation_1_date: Option<String>,
    pub supplementary_investigation_2_date: Option<String>,
    pub judgment_effective_date: Option<String>,
    pub death_penalty_review_start_date: Option<String>,
    pub extraction_meta_json: Option<String>,
    pub notes: Option<String>,
    pub user_overrides_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseStageItem {
    pub id: String,
    pub case_id: String,
    pub domain: String,
    pub major_stage: Option<String>,
    pub stage_label: String,
    pub status: String,
    pub started_at: Option<String>,
    pub due_at: Option<String>,
    pub completed_at: Option<String>,
    pub reminder_at: Option<String>,
    pub source: String,
    pub external_source: Option<String>,
    pub external_record_id: Option<String>,
    pub raw_payload_json: Option<String>,
    pub notes: Option<String>,
    pub sort_order: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertCaseStageItemInput {
    pub id: Option<String>,
    pub case_id: String,
    pub domain: Option<String>,
    pub major_stage: Option<String>,
    pub stage_label: String,
    pub status: Option<String>,
    pub started_at: Option<String>,
    pub due_at: Option<String>,
    pub completed_at: Option<String>,
    pub reminder_at: Option<String>,
    pub source: Option<String>,
    pub external_source: Option<String>,
    pub external_record_id: Option<String>,
    pub raw_payload_json: Option<String>,
    pub notes: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderCaseStageItemsInput {
    pub case_id: String,
    pub ordered_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalDeadlineItem {
    pub id: String,
    pub case_id: String,
    pub stage_item_id: Option<String>,
    pub rule_code: Option<String>,
    pub title: String,
    pub major_stage: Option<String>,
    pub minor_stage: Option<String>,
    pub trigger_date: Option<String>,
    pub trigger_time: Option<String>,
    pub default_due_at: Option<String>,
    pub manual_due_at: Option<String>,
    pub effective_due_at: Option<String>,
    pub reminder_at: Option<String>,
    pub priority: String,
    pub status: String,
    pub source_type: String,
    pub applicability_status: String,
    pub source_law: Option<String>,
    pub source_article: Option<String>,
    pub source_url: Option<String>,
    pub calculation_note: Option<String>,
    pub exception_type: Option<String>,
    pub exception_note: Option<String>,
    pub override_reason: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertCriminalDeadlineItemInput {
    pub id: Option<String>,
    pub case_id: String,
    pub stage_item_id: Option<String>,
    pub rule_code: Option<String>,
    pub title: String,
    pub major_stage: Option<String>,
    pub minor_stage: Option<String>,
    pub trigger_date: Option<String>,
    pub trigger_time: Option<String>,
    pub default_due_at: Option<String>,
    pub manual_due_at: Option<String>,
    pub effective_due_at: Option<String>,
    pub reminder_at: Option<String>,
    pub priority: Option<String>,
    pub status: Option<String>,
    pub source_type: Option<String>,
    pub applicability_status: Option<String>,
    pub source_law: Option<String>,
    pub source_article: Option<String>,
    pub source_url: Option<String>,
    pub calculation_note: Option<String>,
    pub exception_type: Option<String>,
    pub exception_note: Option<String>,
    pub override_reason: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriminalDeadlineRefreshReport {
    pub case_id: String,
    pub generated_count: usize,
    pub updated_count: usize,
    pub preserved_count: usize,
    pub needs_confirmation_count: usize,
    pub skipped_count: usize,
    pub items: Vec<CriminalDeadlineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseAgencyContact {
    pub id: String,
    pub case_id: String,
    pub stage_scope: Option<String>,
    pub agency_type: Option<String>,
    pub agency_name: Option<String>,
    pub contact_role: Option<String>,
    pub contact_name: Option<String>,
    pub phone: Option<String>,
    pub case_no: Option<String>,
    pub query_code: Option<String>,
    pub notes: Option<String>,
    pub source: String,
    pub external_record_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertCaseAgencyContactInput {
    pub id: Option<String>,
    pub case_id: String,
    pub stage_scope: Option<String>,
    pub agency_type: Option<String>,
    pub agency_name: Option<String>,
    pub contact_role: Option<String>,
    pub contact_name: Option<String>,
    pub phone: Option<String>,
    pub case_no: Option<String>,
    pub query_code: Option<String>,
    pub notes: Option<String>,
    pub source: Option<String>,
    pub external_record_id: Option<String>,
}

struct ComputedCriminalCaseProfileInput {
    case_id: String,
    current_stage: Option<String>,
    procedure_type: Option<String>,
    case_subtype: Option<String>,
    defense_role: Option<String>,
    suspected_charge: Option<String>,
    suspect_or_defendant_name: Option<String>,
    victim_name: Option<String>,
    client_name: Option<String>,
    client_relationship: Option<String>,
    detention_center: Option<String>,
    coercive_measure_type: Option<String>,
    detention_date: Option<String>,
    arrest_request_date: Option<String>,
    arrest_review_received_date: Option<String>,
    arrest_decision_date: Option<String>,
    arrest_date: Option<String>,
    bail_start_date: Option<String>,
    residential_surveillance_start_date: Option<String>,
    transfer_for_prosecution_date: Option<String>,
    prosecution_received_date: Option<String>,
    first_instance_accepted_date: Option<String>,
    second_instance_accepted_date: Option<String>,
    judgment_received_date: Option<String>,
    ruling_received_date: Option<String>,
    stage_sort_mode: String,
    guilty_plea_status: Option<String>,
    sentencing_recommendation: Option<String>,
    sentence_term: Option<String>,
    charge_history_json: Option<String>,
    restitution_amount: Option<f64>,
    restitution_status: Option<String>,
    victim_forgiveness: Option<String>,
    surrender_status: Option<String>,
    meritorious_service_status: Option<String>,
    co_defendants_json: Option<String>,
    supplementary_investigation_1_date: Option<String>,
    supplementary_investigation_2_date: Option<String>,
    judgment_effective_date: Option<String>,
    death_penalty_review_start_date: Option<String>,
    extraction_meta_json: Option<String>,
    notes: Option<String>,
    user_overrides_json: Option<String>,
}

struct ComputedCaseStageItemInput {
    id: String,
    case_id: String,
    domain: String,
    major_stage: Option<String>,
    stage_label: String,
    status: String,
    started_at: Option<String>,
    due_at: Option<String>,
    completed_at: Option<String>,
    reminder_at: Option<String>,
    source: String,
    external_source: Option<String>,
    external_record_id: Option<String>,
    raw_payload_json: Option<String>,
    notes: Option<String>,
    sort_order: Option<i64>,
}

struct ComputedCriminalDeadlineItemInput {
    id: String,
    case_id: String,
    stage_item_id: Option<String>,
    rule_code: Option<String>,
    title: String,
    major_stage: Option<String>,
    minor_stage: Option<String>,
    trigger_date: Option<String>,
    trigger_time: Option<String>,
    default_due_at: Option<String>,
    manual_due_at: Option<String>,
    effective_due_at: Option<String>,
    reminder_at: Option<String>,
    priority: String,
    status: String,
    source_type: String,
    applicability_status: String,
    source_law: Option<String>,
    source_article: Option<String>,
    source_url: Option<String>,
    calculation_note: Option<String>,
    exception_type: Option<String>,
    exception_note: Option<String>,
    override_reason: Option<String>,
    completed_at: Option<String>,
}

struct ComputedCaseAgencyContactInput {
    id: String,
    case_id: String,
    stage_scope: Option<String>,
    agency_type: Option<String>,
    agency_name: Option<String>,
    contact_role: Option<String>,
    contact_name: Option<String>,
    phone: Option<String>,
    case_no: Option<String>,
    query_code: Option<String>,
    notes: Option<String>,
    source: String,
    external_record_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum DeadlineOffset {
    Days(i64),
    Months(u32),
    MonthsAndDays(u32, i64),
}

#[derive(Debug, Clone, Copy)]
struct CriminalDeadlineRule {
    code: &'static str,
    title: &'static str,
    major_stage: &'static str,
    minor_stage: &'static str,
    trigger_field: &'static str,
    offset: DeadlineOffset,
    priority: &'static str,
    source_law: &'static str,
    source_article: &'static str,
    source_url: &'static str,
    calculation_note: &'static str,
    auto_apply: bool,
}

const CRIMINAL_PROCEDURE_LAW_URL: &str =
    "https://flk.npc.gov.cn/detail?fileId=&id=ff8080816f135f46016f1d1b81b01351&type=";

const CRIMINAL_DEADLINE_RULES: &[CriminalDeadlineRule] = &[
    CriminalDeadlineRule {
        code: "CRIM_DETENTION_INTERROGATION_24H",
        title: "拘留后讯问核查",
        major_stage: "侦查阶段",
        minor_stage: "刑事拘留",
        trigger_field: "detention_date",
        offset: DeadlineOffset::Days(1),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第86条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "现有画像字段为日期粒度，MVP 按拘留日期 +1 日生成 24 小时提醒；具体时点请人工核对。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_DETENTION_FAMILY_NOTICE_24H",
        title: "拘留后家属通知/例外核查",
        major_stage: "侦查阶段",
        minor_stage: "刑事拘留",
        trigger_field: "detention_date",
        offset: DeadlineOffset::Days(1),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第85条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "现有画像字段为日期粒度，MVP 按拘留日期 +1 日生成 24 小时提醒；有碍侦查等例外需人工标注。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_ARREST_REQUEST_3D",
        title: "一般提请批准逮捕期限",
        major_stage: "侦查阶段",
        minor_stage: "提请批准逮捕",
        trigger_field: "detention_date",
        offset: DeadlineOffset::Days(3),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第91条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按拘留日期 +3 日生成一般提请批准逮捕提醒；特殊延长、流窜/多次/结伙重大嫌疑不自动适用。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_ARREST_REVIEW_7D",
        title: "检察院审查逮捕决定期限",
        major_stage: "审查逮捕",
        minor_stage: "批准逮捕审查",
        trigger_field: "arrest_review_received_date",
        offset: DeadlineOffset::Days(7),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第91条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按检察院收到提请批准逮捕书日期 +7 日生成审查逮捕提醒。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_BAIL_12M",
        title: "取保候审届满提醒",
        major_stage: "强制措施",
        minor_stage: "取保候审",
        trigger_field: "bail_start_date",
        offset: DeadlineOffset::Months(12),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第79条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按取保候审开始日期 +12 个月生成届满提醒；跨阶段是否重新办理需人工判断。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_RESIDENTIAL_SURVEILLANCE_6M",
        title: "监视居住届满提醒",
        major_stage: "强制措施",
        minor_stage: "监视居住",
        trigger_field: "residential_surveillance_start_date",
        offset: DeadlineOffset::Months(6),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第79条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按监视居住开始日期 +6 个月生成届满提醒；指定居所等情形需人工核对。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_INVESTIGATION_CUSTODY_2M",
        title: "逮捕后侦查羁押届满提醒",
        major_stage: "侦查阶段",
        minor_stage: "侦查羁押",
        trigger_field: "arrest_date",
        offset: DeadlineOffset::Months(2),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第156条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按逮捕日期 +2 个月生成侦查羁押届满提醒；上级检察院、省级检察院等延长分支不自动适用。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_PROSECUTION_REVIEW_1M",
        title: "审查起诉期限提醒",
        major_stage: "审查起诉",
        minor_stage: "审查起诉",
        trigger_field: "prosecution_received_date",
        offset: DeadlineOffset::Months(1),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第172条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按检察院收到移送审查起诉案件日期 +1 个月生成提醒；重大复杂延长 15 日不自动适用。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_FIRST_INSTANCE_2M",
        title: "一审二个月审限提醒",
        major_stage: "一审",
        minor_stage: "公诉一审",
        trigger_field: "first_instance_accepted_date",
        offset: DeadlineOffset::Months(2),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第208条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按一审受理日期 +2 个月生成黄色提醒；死刑、附民、第158条情形等延长分支需人工标注。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_FIRST_INSTANCE_3M",
        title: "一审三个月至迟审限提醒",
        major_stage: "一审",
        minor_stage: "公诉一审",
        trigger_field: "first_instance_accepted_date",
        offset: DeadlineOffset::Months(3),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第208条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按一审受理日期 +3 个月生成红色提醒；法定延长分支不自动适用。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_APPEAL_JUDGMENT_10D",
        title: "判决上诉/抗诉期限提醒",
        major_stage: "上诉/抗诉",
        minor_stage: "判决",
        trigger_field: "judgment_received_date",
        offset: DeadlineOffset::Days(10),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第230条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按收到判决书日期 +10 日生成；法条口径为收到次日起算，请结合实际送达时间人工核对。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_APPEAL_RULING_5D",
        title: "裁定上诉/抗诉期限提醒",
        major_stage: "上诉/抗诉",
        minor_stage: "裁定",
        trigger_field: "ruling_received_date",
        offset: DeadlineOffset::Days(5),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第230条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按收到裁定书日期 +5 日生成；法条口径为收到次日起算，请结合实际送达时间人工核对。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_SECOND_INSTANCE_2M",
        title: "二审审限提醒",
        major_stage: "二审",
        minor_stage: "二审审限",
        trigger_field: "second_instance_accepted_date",
        offset: DeadlineOffset::Months(2),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第243条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按二审受理日期 +2 个月生成提醒；特殊延长分支不自动适用。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_DETENTION_EXTEND_7D",
        title: "拘留延长至七日（适用性待确认）",
        major_stage: "侦查阶段",
        minor_stage: "刑事拘留",
        trigger_field: "detention_date",
        offset: DeadlineOffset::Days(7),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第91条第1款",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按拘留日期 +7 日生成最晚提醒；是否属于特殊情况、实际延长几日须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_DETENTION_EXTEND_30D",
        title: "拘留延长至三十日（适用性待确认）",
        major_stage: "侦查阶段",
        minor_stage: "刑事拘留",
        trigger_field: "detention_date",
        offset: DeadlineOffset::Days(30),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第91条第2款",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按拘留日期 +30 日生成最晚提醒；仅适用于流窜、多次、结伙作案的重大嫌疑分子，须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_INVESTIGATION_CUSTODY_EXTEND_3M",
        title: "侦查羁押延长至三个月（适用性待确认）",
        major_stage: "侦查阶段",
        minor_stage: "侦查羁押",
        trigger_field: "arrest_date",
        offset: DeadlineOffset::Months(3),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第157条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按逮捕日期 +3 个月生成；案情复杂、期限届满不能终结并经上一级检察院批准的前提须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_INVESTIGATION_CUSTODY_EXTEND_5M",
        title: "侦查羁押延长至五个月（适用性待确认）",
        major_stage: "侦查阶段",
        minor_stage: "侦查羁押",
        trigger_field: "arrest_date",
        offset: DeadlineOffset::Months(5),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第158条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按逮捕日期 +5 个月生成；仅作重大复杂案件延长分支提示，法定情形及批准手续须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_INVESTIGATION_CUSTODY_EXTEND_7M",
        title: "侦查羁押延长至七个月（适用性待确认）",
        major_stage: "侦查阶段",
        minor_stage: "侦查羁押",
        trigger_field: "arrest_date",
        offset: DeadlineOffset::Months(7),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第159条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按逮捕日期 +7 个月生成；可能判处十年有期徒刑以上且依第158条仍不能终结等前提须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_PROSECUTION_REVIEW_EXTEND_45D",
        title: "审查起诉延长至一个半月（适用性待确认）",
        major_stage: "审查起诉",
        minor_stage: "审查起诉",
        trigger_field: "prosecution_received_date",
        offset: DeadlineOffset::MonthsAndDays(1, 15),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第172条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按检察院受理日期 +1 个月 +15 日生成；重大、复杂案件延长前提须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_SUPPLEMENTARY_INVESTIGATION_1M",
        title: "第一次退回补充侦查届满",
        major_stage: "审查起诉",
        minor_stage: "退回补充侦查",
        trigger_field: "supplementary_investigation_1_date",
        offset: DeadlineOffset::Months(1),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第175条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按第一次退回补充侦查日期 +1 个月生成。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_PROSECUTION_RESTART_AFTER_SUPP_1M",
        title: "第一次补充侦查后审查起诉重新计算",
        major_stage: "审查起诉",
        minor_stage: "重新计算",
        trigger_field: "supplementary_investigation_1_date",
        offset: DeadlineOffset::Months(2),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第175条、第172条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "在仅记录退补决定日期的情况下，以退补 +1 个月估算补侦完毕，再 +1 个月形成审查起诉提醒；实际重新移送日期应人工修正。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_SUPPLEMENTARY_INVESTIGATION_2M",
        title: "第二次退回补充侦查届满",
        major_stage: "审查起诉",
        minor_stage: "退回补充侦查",
        trigger_field: "supplementary_investigation_2_date",
        offset: DeadlineOffset::Months(1),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第175条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按第二次退回补充侦查日期 +1 个月生成；退回补充侦查以二次为限。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_PROSECUTION_RESTART_AFTER_SUPP_2M",
        title: "第二次补充侦查后审查起诉重新计算",
        major_stage: "审查起诉",
        minor_stage: "重新计算",
        trigger_field: "supplementary_investigation_2_date",
        offset: DeadlineOffset::Months(2),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第175条、第172条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "在仅记录退补决定日期的情况下，以退补 +1 个月估算补侦完毕，再 +1 个月形成审查起诉提醒；实际重新移送日期应人工修正。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_FIRST_INSTANCE_EXTEND_6M",
        title: "一审延长至六个月（适用性待确认）",
        major_stage: "一审",
        minor_stage: "公诉一审",
        trigger_field: "first_instance_accepted_date",
        offset: DeadlineOffset::Months(6),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第208条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按一审受理日期 +6 个月生成；死刑、附带民事诉讼或第158条情形及批准手续须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_SUMMARY_PROCEDURE_20D",
        title: "简易程序二十日审限",
        major_stage: "一审",
        minor_stage: "简易程序",
        trigger_field: "first_instance_accepted_date",
        offset: DeadlineOffset::Days(20),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第220条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "仅在程序类型为简易程序时，按受理日期 +20 日生成。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_SUMMARY_PROCEDURE_45D",
        title: "简易程序延长至一个半月（适用性待确认）",
        major_stage: "一审",
        minor_stage: "简易程序",
        trigger_field: "first_instance_accepted_date",
        offset: DeadlineOffset::MonthsAndDays(1, 15),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第220条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "仅在简易程序下生成；可能判处有期徒刑超过三年的适用前提须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_FAST_TRACK_10D",
        title: "速裁程序十日审限",
        major_stage: "一审",
        minor_stage: "速裁程序",
        trigger_field: "first_instance_accepted_date",
        offset: DeadlineOffset::Days(10),
        priority: "urgent",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第225条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "仅在程序类型为速裁程序时，按受理日期 +10 日生成。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_FAST_TRACK_15D",
        title: "速裁程序延长至十五日（适用性待确认）",
        major_stage: "一审",
        minor_stage: "速裁程序",
        trigger_field: "first_instance_accepted_date",
        offset: DeadlineOffset::Days(15),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第225条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "仅在速裁程序下生成；可能判处有期徒刑超过一年的适用前提须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_SECOND_INSTANCE_EXTEND_4M",
        title: "二审延长至四个月（适用性待确认）",
        major_stage: "二审",
        minor_stage: "二审审限",
        trigger_field: "second_instance_accepted_date",
        offset: DeadlineOffset::Months(4),
        priority: "high",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第243条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按二审受理日期 +4 个月生成；死刑、附带民事诉讼或第158条情形及批准手续须人工确认。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_EXECUTION_DELIVERY_10D",
        title: "判决生效后交付执行",
        major_stage: "执行阶段",
        minor_stage: "交付执行",
        trigger_field: "judgment_effective_date",
        offset: DeadlineOffset::Days(10),
        priority: "warning",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第264条",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "按明确记录的判决生效日期 +10 日生成；不得以判决送达日期推定生效。",
        auto_apply: true,
    },
    CriminalDeadlineRule {
        code: "CRIM_DEATH_PENALTY_REVIEW_3M",
        title: "死刑复核三个月关注节点（非审限）",
        major_stage: "死刑复核",
        minor_stage: "复核关注",
        trigger_field: "death_penalty_review_start_date",
        offset: DeadlineOffset::Months(3),
        priority: "normal",
        source_law: "中华人民共和国刑事诉讼法",
        source_article: "第三编第四章",
        source_url: CRIMINAL_PROCEDURE_LAW_URL,
        calculation_note: "死刑复核没有统一法定办结期限；本项仅为自定义三个月关注节点，不得表述为法定审限。",
        auto_apply: true,
    },
];

const PROFILE_SELECT: &str = r#"
SELECT
    case_id,
    current_stage,
    procedure_type,
    case_subtype,
    defense_role,
    suspected_charge,
    suspect_or_defendant_name,
    victim_name,
    client_name,
    client_relationship,
    detention_center,
    coercive_measure_type,
    detention_date,
    arrest_request_date,
    arrest_review_received_date,
    arrest_decision_date,
    arrest_date,
    bail_start_date,
    residential_surveillance_start_date,
    transfer_for_prosecution_date,
    prosecution_received_date,
    first_instance_accepted_date,
    second_instance_accepted_date,
    judgment_received_date,
    ruling_received_date,
    stage_sort_mode,
    guilty_plea_status,
    sentencing_recommendation,
    sentence_term,
    charge_history_json,
    restitution_amount,
    restitution_status,
    victim_forgiveness,
    surrender_status,
    meritorious_service_status,
    co_defendants_json,
    supplementary_investigation_1_date,
    supplementary_investigation_2_date,
    judgment_effective_date,
    death_penalty_review_start_date,
    extraction_meta_json,
    notes,
      user_overrides_json,
      profile_revision,
      created_at,
    updated_at
FROM criminal_case_profiles
"#;

const STAGE_SELECT: &str = r#"
SELECT
    id,
    case_id,
    domain,
    major_stage,
    stage_label,
    status,
    started_at,
    due_at,
    completed_at,
    reminder_at,
    source,
    external_source,
    external_record_id,
    raw_payload_json,
    notes,
    sort_order,
    created_at,
    updated_at,
    deleted_at
FROM case_stage_items
"#;

const DEADLINE_SELECT: &str = r#"
SELECT
    id,
    case_id,
    stage_item_id,
    rule_code,
    title,
    major_stage,
    minor_stage,
    trigger_date,
    trigger_time,
    default_due_at,
    manual_due_at,
    effective_due_at,
    reminder_at,
    priority,
    status,
    source_type,
    applicability_status,
    source_law,
    source_article,
    source_url,
    calculation_note,
    exception_type,
    exception_note,
    override_reason,
    completed_at,
    created_at,
    updated_at,
    deleted_at
FROM criminal_deadline_items
"#;

const CONTACT_SELECT: &str = r#"
SELECT
    id,
    case_id,
    stage_scope,
    agency_type,
    agency_name,
    contact_role,
    contact_name,
    phone,
    case_no,
    query_code,
    notes,
    source,
    external_record_id,
    created_at,
    updated_at,
    deleted_at
FROM case_agency_contacts
"#;

pub async fn get_criminal_case_profile(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Option<CriminalCaseProfile>, String> {
    let sql = format!("{PROFILE_SELECT} WHERE case_id = ?");
    sqlx::query_as::<_, CriminalCaseProfile>(&sql)
        .bind(case_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_criminal_case_profile(
    pool: &SqlitePool,
    input: UpsertCriminalCaseProfileInput,
) -> Result<CriminalCaseProfile, String> {
    upsert_criminal_case_profile_impl(pool, input, false).await
}

/// 刑事详情页人工保存入口：整行画像、人工保护集与 revision 在同一事务提交。
pub async fn upsert_criminal_case_profile_manual(
    pool: &SqlitePool,
    input: UpsertCriminalCaseProfileInput,
) -> Result<CriminalCaseProfile, String> {
    upsert_criminal_case_profile_impl(pool, input, true).await
}

async fn upsert_criminal_case_profile_impl(
    pool: &SqlitePool,
    input: UpsertCriminalCaseProfileInput,
    manual: bool,
) -> Result<CriminalCaseProfile, String> {
    let computed = compute_criminal_case_profile_input(input)?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let old = if manual {
        let sql = format!("{PROFILE_SELECT} WHERE case_id = ?");
        sqlx::query_as::<_, CriminalCaseProfile>(&sql)
            .bind(&computed.case_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
    } else {
        None
    };
    let mut overrides = if manual {
        match old.as_ref().and_then(|p| p.user_overrides_json.as_deref()) {
            Some(raw) => serde_json::from_str::<serde_json::Value>(raw)
                .map_err(|e| format!("人工覆盖记录损坏，已停止保存: {e}"))?,
            None => serde_json::json!({"fields": {}}),
        }
    } else {
        serde_json::Value::Null
    };
    sqlx::query(
        "INSERT INTO criminal_case_profiles (
            case_id, current_stage, procedure_type, case_subtype, defense_role,
            suspected_charge, suspect_or_defendant_name, victim_name, client_name,
            client_relationship, detention_center, coercive_measure_type,
            detention_date, arrest_request_date, arrest_review_received_date,
            arrest_decision_date, arrest_date, bail_start_date,
            residential_surveillance_start_date, transfer_for_prosecution_date,
            prosecution_received_date, first_instance_accepted_date,
            second_instance_accepted_date, judgment_received_date,
            ruling_received_date, stage_sort_mode, guilty_plea_status,
            sentencing_recommendation, sentence_term, charge_history_json,
            restitution_amount, restitution_status, victim_forgiveness,
            surrender_status, meritorious_service_status, co_defendants_json,
            supplementary_investigation_1_date, supplementary_investigation_2_date,
            judgment_effective_date, death_penalty_review_start_date,
            extraction_meta_json, notes, user_overrides_json
        ) VALUES (
            ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
            ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
        )
        ON CONFLICT(case_id) DO UPDATE SET
            current_stage = excluded.current_stage,
            procedure_type = excluded.procedure_type,
            case_subtype = excluded.case_subtype,
            defense_role = excluded.defense_role,
            suspected_charge = excluded.suspected_charge,
            suspect_or_defendant_name = excluded.suspect_or_defendant_name,
            victim_name = excluded.victim_name,
            client_name = excluded.client_name,
            client_relationship = excluded.client_relationship,
            detention_center = excluded.detention_center,
            coercive_measure_type = excluded.coercive_measure_type,
            detention_date = excluded.detention_date,
            arrest_request_date = excluded.arrest_request_date,
            arrest_review_received_date = excluded.arrest_review_received_date,
            arrest_decision_date = excluded.arrest_decision_date,
            arrest_date = excluded.arrest_date,
            bail_start_date = excluded.bail_start_date,
            residential_surveillance_start_date = excluded.residential_surveillance_start_date,
            transfer_for_prosecution_date = excluded.transfer_for_prosecution_date,
            prosecution_received_date = excluded.prosecution_received_date,
            first_instance_accepted_date = excluded.first_instance_accepted_date,
            second_instance_accepted_date = excluded.second_instance_accepted_date,
            judgment_received_date = excluded.judgment_received_date,
            ruling_received_date = excluded.ruling_received_date,
            stage_sort_mode = excluded.stage_sort_mode,
            guilty_plea_status = excluded.guilty_plea_status,
            sentencing_recommendation = excluded.sentencing_recommendation,
            sentence_term = excluded.sentence_term,
            charge_history_json = excluded.charge_history_json,
            restitution_amount = excluded.restitution_amount,
            restitution_status = excluded.restitution_status,
            victim_forgiveness = excluded.victim_forgiveness,
            surrender_status = excluded.surrender_status,
            meritorious_service_status = excluded.meritorious_service_status,
            co_defendants_json = excluded.co_defendants_json,
            supplementary_investigation_1_date = excluded.supplementary_investigation_1_date,
            supplementary_investigation_2_date = excluded.supplementary_investigation_2_date,
            judgment_effective_date = excluded.judgment_effective_date,
            death_penalty_review_start_date = excluded.death_penalty_review_start_date,
            extraction_meta_json = excluded.extraction_meta_json,
            notes = excluded.notes,
            user_overrides_json = excluded.user_overrides_json,
            updated_at = datetime('now')",
    )
    .bind(&computed.case_id)
    .bind(&computed.current_stage)
    .bind(&computed.procedure_type)
    .bind(&computed.case_subtype)
    .bind(&computed.defense_role)
    .bind(&computed.suspected_charge)
    .bind(&computed.suspect_or_defendant_name)
    .bind(&computed.victim_name)
    .bind(&computed.client_name)
    .bind(&computed.client_relationship)
    .bind(&computed.detention_center)
    .bind(&computed.coercive_measure_type)
    .bind(&computed.detention_date)
    .bind(&computed.arrest_request_date)
    .bind(&computed.arrest_review_received_date)
    .bind(&computed.arrest_decision_date)
    .bind(&computed.arrest_date)
    .bind(&computed.bail_start_date)
    .bind(&computed.residential_surveillance_start_date)
    .bind(&computed.transfer_for_prosecution_date)
    .bind(&computed.prosecution_received_date)
    .bind(&computed.first_instance_accepted_date)
    .bind(&computed.second_instance_accepted_date)
    .bind(&computed.judgment_received_date)
    .bind(&computed.ruling_received_date)
    .bind(&computed.stage_sort_mode)
    .bind(&computed.guilty_plea_status)
    .bind(&computed.sentencing_recommendation)
    .bind(&computed.sentence_term)
    .bind(&computed.charge_history_json)
    .bind(computed.restitution_amount)
    .bind(&computed.restitution_status)
    .bind(&computed.victim_forgiveness)
    .bind(&computed.surrender_status)
    .bind(&computed.meritorious_service_status)
    .bind(&computed.co_defendants_json)
    .bind(&computed.supplementary_investigation_1_date)
    .bind(&computed.supplementary_investigation_2_date)
    .bind(&computed.judgment_effective_date)
    .bind(&computed.death_penalty_review_start_date)
    .bind(&computed.extraction_meta_json)
    .bind(&computed.notes)
    .bind(&computed.user_overrides_json)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    let sql = format!("{PROFILE_SELECT} WHERE case_id = ?");
    let mut saved = sqlx::query_as::<_, CriminalCaseProfile>(&sql)
        .bind(&computed.case_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "刑事画像写入后读取失败".to_string())?;
    if manual {
        saved =
            apply_manual_profile_protection(&mut tx, old.as_ref(), saved, &mut overrides).await?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(saved)
}

async fn apply_manual_profile_protection(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    old: Option<&CriminalCaseProfile>,
    mut saved: CriminalCaseProfile,
    overrides: &mut serde_json::Value,
) -> Result<CriminalCaseProfile, String> {
    let object = overrides
        .as_object_mut()
        .ok_or_else(|| "人工覆盖记录必须是 JSON 对象，已停止保存".to_string())?;
    let fields = object
        .entry("fields")
        .or_insert_with(|| serde_json::json!({}));
    let fields = fields
        .as_object_mut()
        .ok_or_else(|| "人工覆盖记录 fields 必须是对象，已停止保存".to_string())?;

    let old_value = old
        .map(serde_json::to_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| serde_json::json!({}));
    let saved_value = serde_json::to_value(&saved).map_err(|e| e.to_string())?;
    const MANUAL_FIELDS: &[&str] = &[
        "current_stage",
        "procedure_type",
        "case_subtype",
        "defense_role",
        "suspected_charge",
        "suspect_or_defendant_name",
        "victim_name",
        "client_name",
        "client_relationship",
        "detention_center",
        "coercive_measure_type",
        "detention_date",
        "arrest_request_date",
        "arrest_review_received_date",
        "arrest_decision_date",
        "arrest_date",
        "bail_start_date",
        "residential_surveillance_start_date",
        "transfer_for_prosecution_date",
        "prosecution_received_date",
        "first_instance_accepted_date",
        "second_instance_accepted_date",
        "judgment_received_date",
        "ruling_received_date",
        "stage_sort_mode",
        "guilty_plea_status",
        "sentencing_recommendation",
        "sentence_term",
        "charge_history_json",
        "restitution_amount",
        "restitution_status",
        "victim_forgiveness",
        "surrender_status",
        "meritorious_service_status",
        "co_defendants_json",
        "supplementary_investigation_1_date",
        "supplementary_investigation_2_date",
        "judgment_effective_date",
        "death_penalty_review_start_date",
        "notes",
    ];
    let mut changed = false;
    for key in MANUAL_FIELDS {
        let before = old_value
            .get(*key)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let after = saved_value
            .get(*key)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if before != after {
            fields.insert(
                (*key).to_string(),
                serde_json::json!({
                    "value": after,
                    "source": "manual",
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                }),
            );
            changed = true;
        }
    }
    let next_revision = old.map_or(0, |p| p.profile_revision) + i64::from(changed);
    sqlx::query(
        "UPDATE criminal_case_profiles SET user_overrides_json=?, profile_revision=?, updated_at=datetime('now') WHERE case_id=?",
    )
    .bind(serde_json::to_string(overrides).map_err(|e| e.to_string())?)
    .bind(next_revision)
    .bind(&saved.case_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    saved.user_overrides_json = Some(serde_json::to_string(overrides).map_err(|e| e.to_string())?);
    saved.profile_revision = next_revision;
    Ok(saved)
}

pub async fn list_case_stage_items(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CaseStageItem>, String> {
    let sql = format!(
        "{STAGE_SELECT} WHERE case_id = ? AND deleted_at IS NULL ORDER BY sort_order IS NULL, sort_order ASC, started_at DESC, updated_at DESC"
    );
    sqlx::query_as::<_, CaseStageItem>(&sql)
        .bind(case_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_case_stage_item(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<CaseStageItem>, String> {
    let sql = format!("{STAGE_SELECT} WHERE id = ? AND deleted_at IS NULL");
    sqlx::query_as::<_, CaseStageItem>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_case_stage_item(
    pool: &SqlitePool,
    input: UpsertCaseStageItemInput,
) -> Result<CaseStageItem, String> {
    let computed = compute_case_stage_item_input(input)?;
    sqlx::query(
        "INSERT INTO case_stage_items (
            id, case_id, domain, major_stage, stage_label, status,
            started_at, due_at, completed_at, reminder_at, source,
            external_source, external_record_id, raw_payload_json, notes, sort_order
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            case_id = excluded.case_id,
            domain = excluded.domain,
            major_stage = excluded.major_stage,
            stage_label = excluded.stage_label,
            status = excluded.status,
            started_at = excluded.started_at,
            due_at = excluded.due_at,
            completed_at = excluded.completed_at,
            reminder_at = excluded.reminder_at,
            source = excluded.source,
            external_source = excluded.external_source,
            external_record_id = excluded.external_record_id,
            raw_payload_json = excluded.raw_payload_json,
            notes = excluded.notes,
            sort_order = excluded.sort_order,
            deleted_at = NULL,
            updated_at = datetime('now')",
    )
    .bind(&computed.id)
    .bind(&computed.case_id)
    .bind(&computed.domain)
    .bind(&computed.major_stage)
    .bind(&computed.stage_label)
    .bind(&computed.status)
    .bind(&computed.started_at)
    .bind(&computed.due_at)
    .bind(&computed.completed_at)
    .bind(&computed.reminder_at)
    .bind(&computed.source)
    .bind(&computed.external_source)
    .bind(&computed.external_record_id)
    .bind(&computed.raw_payload_json)
    .bind(&computed.notes)
    .bind(computed.sort_order)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_case_stage_item(pool, &computed.id)
        .await?
        .ok_or_else(|| "阶段节点写入后读取失败".to_string())
}

pub async fn delete_case_stage_item(pool: &SqlitePool, id: &str) -> Result<u64, String> {
    soft_delete(pool, "case_stage_items", id).await
}

pub async fn reorder_case_stage_items(
    pool: &SqlitePool,
    input: ReorderCaseStageItemsInput,
) -> Result<Vec<CaseStageItem>, String> {
    let case_id = required_text(input.case_id, "case_id")?;
    let ordered_ids: Vec<String> = input
        .ordered_ids
        .into_iter()
        .map(|id| required_text(id, "ordered_ids"))
        .collect::<Result<_, _>>()?;
    let unique: HashSet<&str> = ordered_ids.iter().map(String::as_str).collect();
    if unique.len() != ordered_ids.len() {
        return Err("ordered_ids 不能包含重复节点".to_string());
    }

    let active_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM case_stage_items WHERE case_id = ? AND deleted_at IS NULL",
    )
    .bind(&case_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let active: HashSet<&str> = active_ids.iter().map(String::as_str).collect();
    if ordered_ids.len() != active_ids.len() || unique != active {
        return Err("ordered_ids 必须完整包含该案件全部有效阶段节点".to_string());
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for (index, id) in ordered_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE case_stage_items SET sort_order = ?, updated_at = datetime('now')
             WHERE id = ? AND case_id = ? AND deleted_at IS NULL",
        )
        .bind(index as i64)
        .bind(id)
        .bind(&case_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    sqlx::query(
        "UPDATE criminal_case_profiles SET stage_sort_mode = 'manual', updated_at = datetime('now')
         WHERE case_id = ?",
    )
    .bind(&case_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    list_case_stage_items(pool, &case_id).await
}

pub async fn list_criminal_deadline_items(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CriminalDeadlineItem>, String> {
    let sql = format!(
        "{DEADLINE_SELECT} WHERE case_id = ? AND deleted_at IS NULL ORDER BY effective_due_at ASC, updated_at DESC"
    );
    sqlx::query_as::<_, CriminalDeadlineItem>(&sql)
        .bind(case_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_criminal_deadline_item(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<CriminalDeadlineItem>, String> {
    let sql = format!("{DEADLINE_SELECT} WHERE id = ? AND deleted_at IS NULL");
    sqlx::query_as::<_, CriminalDeadlineItem>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_criminal_deadline_item(
    pool: &SqlitePool,
    input: UpsertCriminalDeadlineItemInput,
) -> Result<CriminalDeadlineItem, String> {
    let computed = compute_criminal_deadline_item_input(input)?;
    sqlx::query(
        "INSERT INTO criminal_deadline_items (
            id, case_id, stage_item_id, rule_code, title, major_stage,
            minor_stage, trigger_date, trigger_time, default_due_at,
            manual_due_at, effective_due_at, reminder_at, priority, status,
            source_type, applicability_status, source_law, source_article, source_url,
            calculation_note, exception_type, exception_note, override_reason,
            completed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            case_id = excluded.case_id,
            stage_item_id = excluded.stage_item_id,
            rule_code = excluded.rule_code,
            title = excluded.title,
            major_stage = excluded.major_stage,
            minor_stage = excluded.minor_stage,
            trigger_date = excluded.trigger_date,
            trigger_time = excluded.trigger_time,
            default_due_at = excluded.default_due_at,
            manual_due_at = excluded.manual_due_at,
            effective_due_at = excluded.effective_due_at,
            reminder_at = excluded.reminder_at,
            priority = excluded.priority,
            status = excluded.status,
            source_type = excluded.source_type,
            applicability_status = excluded.applicability_status,
            source_law = excluded.source_law,
            source_article = excluded.source_article,
            source_url = excluded.source_url,
            calculation_note = excluded.calculation_note,
            exception_type = excluded.exception_type,
            exception_note = excluded.exception_note,
            override_reason = excluded.override_reason,
            completed_at = excluded.completed_at,
            deleted_at = NULL,
            updated_at = datetime('now')",
    )
    .bind(&computed.id)
    .bind(&computed.case_id)
    .bind(&computed.stage_item_id)
    .bind(&computed.rule_code)
    .bind(&computed.title)
    .bind(&computed.major_stage)
    .bind(&computed.minor_stage)
    .bind(&computed.trigger_date)
    .bind(&computed.trigger_time)
    .bind(&computed.default_due_at)
    .bind(&computed.manual_due_at)
    .bind(&computed.effective_due_at)
    .bind(&computed.reminder_at)
    .bind(&computed.priority)
    .bind(&computed.status)
    .bind(&computed.source_type)
    .bind(&computed.applicability_status)
    .bind(&computed.source_law)
    .bind(&computed.source_article)
    .bind(&computed.source_url)
    .bind(&computed.calculation_note)
    .bind(&computed.exception_type)
    .bind(&computed.exception_note)
    .bind(&computed.override_reason)
    .bind(&computed.completed_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_criminal_deadline_item(pool, &computed.id)
        .await?
        .ok_or_else(|| "期限节点写入后读取失败".to_string())
}

pub async fn refresh_criminal_deadlines(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<CriminalDeadlineRefreshReport, String> {
    let case_id = required_text(case_id.to_string(), "case_id")?;
    let profile = get_criminal_case_profile(pool, &case_id)
        .await?
        .ok_or_else(|| format!("未找到刑事画像: {case_id}"))?;

    let mut generated_count = 0;
    let mut updated_count = 0;
    let mut preserved_count = 0;
    let mut skipped_count = 0;

    for rule in CRIMINAL_DEADLINE_RULES
        .iter()
        .filter(|rule| rule.auto_apply)
    {
        let existing = get_latest_deadline_by_rule(pool, &case_id, rule.code).await?;
        if !rule_matches_procedure(&profile, rule.code) {
            if let Some(item) = existing
                .as_ref()
                .filter(|item| !should_preserve_deadline(item))
            {
                let result = sqlx::query(
                    "UPDATE criminal_deadline_items
                     SET applicability_status = 'not_applicable', updated_at = datetime('now')
                     WHERE id = ? AND deleted_at IS NULL",
                )
                .bind(&item.id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                if result.rows_affected() > 0 {
                    updated_count += 1;
                }
            }
            continue;
        }
        let Some(trigger_value) = profile_trigger_value(&profile, rule.trigger_field) else {
            continue;
        };
        let Some(trigger_date) = parse_date_value(trigger_value) else {
            skipped_count += 1;
            continue;
        };
        let Some(default_due_date) = apply_deadline_offset(trigger_date, rule.offset) else {
            skipped_count += 1;
            continue;
        };

        let trigger_date = format_date(trigger_date);
        let default_due_at = format_date(default_due_date);
        let applicability_status = rule_applicability_status(rule.code);
        let exception_type = (applicability_status == "needs_confirmation")
            .then(|| "statutory_condition".to_string());
        let exception_note = (applicability_status == "needs_confirmation")
            .then(|| "该节点存在法定适用前提，须由办案人员确认后依赖。".to_string());

        if let Some(item) = existing {
            if should_preserve_deadline(&item) {
                preserved_count += 1;
                continue;
            }

            let result = sqlx::query(
                "UPDATE criminal_deadline_items
                SET title = ?,
                    major_stage = ?,
                    minor_stage = ?,
                    trigger_date = ?,
                    trigger_time = NULL,
                    default_due_at = ?,
                    effective_due_at = ?,
                    priority = ?,
                    source_type = ?,
                    applicability_status = ?,
                    source_law = ?,
                    source_article = ?,
                    source_url = ?,
                    calculation_note = ?,
                    exception_type = ?,
                    exception_note = ?,
                    updated_at = datetime('now')
                WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(rule.title)
            .bind(rule.major_stage)
            .bind(rule.minor_stage)
            .bind(&trigger_date)
            .bind(&default_due_at)
            .bind(&default_due_at)
            .bind(rule.priority)
            .bind(AUTO_DEADLINE_SOURCE_TYPE)
            .bind(applicability_status)
            .bind(rule.source_law)
            .bind(rule.source_article)
            .bind(rule.source_url)
            .bind(rule.calculation_note)
            .bind(&exception_type)
            .bind(&exception_note)
            .bind(&item.id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            if result.rows_affected() > 0 {
                updated_count += 1;
            }
            continue;
        }

        let input = UpsertCriminalDeadlineItemInput {
            id: None,
            case_id: case_id.clone(),
            stage_item_id: None,
            rule_code: Some(rule.code.to_string()),
            title: rule.title.to_string(),
            major_stage: Some(rule.major_stage.to_string()),
            minor_stage: Some(rule.minor_stage.to_string()),
            trigger_date: Some(trigger_date),
            trigger_time: None,
            default_due_at: Some(default_due_at.clone()),
            manual_due_at: None,
            effective_due_at: Some(default_due_at),
            reminder_at: None,
            priority: Some(rule.priority.to_string()),
            status: Some(DEFAULT_DEADLINE_STATUS.to_string()),
            source_type: Some(AUTO_DEADLINE_SOURCE_TYPE.to_string()),
            applicability_status: Some(applicability_status.to_string()),
            source_law: Some(rule.source_law.to_string()),
            source_article: Some(rule.source_article.to_string()),
            source_url: Some(rule.source_url.to_string()),
            calculation_note: Some(rule.calculation_note.to_string()),
            exception_type,
            exception_note,
            override_reason: None,
            completed_at: None,
        };
        upsert_criminal_deadline_item(pool, input).await?;
        generated_count += 1;
    }

    let items = list_criminal_deadline_items(pool, &case_id).await?;
    let needs_confirmation_count = items
        .iter()
        .filter(|item| item.applicability_status == "needs_confirmation")
        .count();
    Ok(CriminalDeadlineRefreshReport {
        case_id,
        generated_count,
        updated_count,
        preserved_count,
        needs_confirmation_count,
        skipped_count,
        items,
    })
}

pub async fn delete_criminal_deadline_item(pool: &SqlitePool, id: &str) -> Result<u64, String> {
    soft_delete(pool, "criminal_deadline_items", id).await
}

pub async fn list_case_agency_contacts(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CaseAgencyContact>, String> {
    let sql = format!(
        "{CONTACT_SELECT} WHERE case_id = ? AND deleted_at IS NULL ORDER BY updated_at DESC"
    );
    sqlx::query_as::<_, CaseAgencyContact>(&sql)
        .bind(case_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_case_agency_contact(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<CaseAgencyContact>, String> {
    let sql = format!("{CONTACT_SELECT} WHERE id = ? AND deleted_at IS NULL");
    sqlx::query_as::<_, CaseAgencyContact>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_case_agency_contact(
    pool: &SqlitePool,
    input: UpsertCaseAgencyContactInput,
) -> Result<CaseAgencyContact, String> {
    let computed = compute_case_agency_contact_input(input)?;
    sqlx::query(
        "INSERT INTO case_agency_contacts (
            id, case_id, stage_scope, agency_type, agency_name, contact_role,
            contact_name, phone, case_no, query_code, notes, source, external_record_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            case_id = excluded.case_id,
            stage_scope = excluded.stage_scope,
            agency_type = excluded.agency_type,
            agency_name = excluded.agency_name,
            contact_role = excluded.contact_role,
            contact_name = excluded.contact_name,
            phone = excluded.phone,
            case_no = excluded.case_no,
            query_code = excluded.query_code,
            notes = excluded.notes,
            source = excluded.source,
            external_record_id = excluded.external_record_id,
            deleted_at = NULL,
            updated_at = datetime('now')",
    )
    .bind(&computed.id)
    .bind(&computed.case_id)
    .bind(&computed.stage_scope)
    .bind(&computed.agency_type)
    .bind(&computed.agency_name)
    .bind(&computed.contact_role)
    .bind(&computed.contact_name)
    .bind(&computed.phone)
    .bind(&computed.case_no)
    .bind(&computed.query_code)
    .bind(&computed.notes)
    .bind(&computed.source)
    .bind(&computed.external_record_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_case_agency_contact(pool, &computed.id)
        .await?
        .ok_or_else(|| "机关联系人写入后读取失败".to_string())
}

pub async fn delete_case_agency_contact(pool: &SqlitePool, id: &str) -> Result<u64, String> {
    soft_delete(pool, "case_agency_contacts", id).await
}

fn compute_criminal_case_profile_input(
    input: UpsertCriminalCaseProfileInput,
) -> Result<ComputedCriminalCaseProfileInput, String> {
    let stage_sort_mode =
        normalize_opt(input.stage_sort_mode).unwrap_or_else(|| "auto".to_string());
    if !matches!(stage_sort_mode.as_str(), "auto" | "manual") {
        return Err("stage_sort_mode 必须为 auto 或 manual".to_string());
    }
    if input.restitution_amount.is_some_and(|amount| amount < 0.0) {
        return Err("restitution_amount 不能为负数".to_string());
    }
    Ok(ComputedCriminalCaseProfileInput {
        case_id: required_text(input.case_id, "case_id")?,
        current_stage: normalize_opt(input.current_stage),
        procedure_type: normalize_opt(input.procedure_type),
        case_subtype: normalize_opt(input.case_subtype),
        defense_role: normalize_opt(input.defense_role),
        suspected_charge: normalize_opt(input.suspected_charge),
        suspect_or_defendant_name: normalize_opt(input.suspect_or_defendant_name),
        victim_name: normalize_opt(input.victim_name),
        client_name: normalize_opt(input.client_name),
        client_relationship: normalize_opt(input.client_relationship),
        detention_center: normalize_opt(input.detention_center),
        coercive_measure_type: normalize_opt(input.coercive_measure_type),
        detention_date: normalize_opt(input.detention_date),
        arrest_request_date: normalize_opt(input.arrest_request_date),
        arrest_review_received_date: normalize_opt(input.arrest_review_received_date),
        arrest_decision_date: normalize_opt(input.arrest_decision_date),
        arrest_date: normalize_opt(input.arrest_date),
        bail_start_date: normalize_opt(input.bail_start_date),
        residential_surveillance_start_date: normalize_opt(
            input.residential_surveillance_start_date,
        ),
        transfer_for_prosecution_date: normalize_opt(input.transfer_for_prosecution_date),
        prosecution_received_date: normalize_opt(input.prosecution_received_date),
        first_instance_accepted_date: normalize_opt(input.first_instance_accepted_date),
        second_instance_accepted_date: normalize_opt(input.second_instance_accepted_date),
        judgment_received_date: normalize_opt(input.judgment_received_date),
        ruling_received_date: normalize_opt(input.ruling_received_date),
        stage_sort_mode,
        guilty_plea_status: normalize_opt(input.guilty_plea_status),
        sentencing_recommendation: normalize_opt(input.sentencing_recommendation),
        sentence_term: normalize_opt(input.sentence_term),
        charge_history_json: normalize_json_opt(input.charge_history_json, "charge_history_json")?,
        restitution_amount: input.restitution_amount,
        restitution_status: normalize_opt(input.restitution_status),
        victim_forgiveness: normalize_opt(input.victim_forgiveness),
        surrender_status: normalize_opt(input.surrender_status),
        meritorious_service_status: normalize_opt(input.meritorious_service_status),
        co_defendants_json: normalize_json_opt(input.co_defendants_json, "co_defendants_json")?,
        supplementary_investigation_1_date: normalize_opt(input.supplementary_investigation_1_date),
        supplementary_investigation_2_date: normalize_opt(input.supplementary_investigation_2_date),
        judgment_effective_date: normalize_opt(input.judgment_effective_date),
        death_penalty_review_start_date: normalize_opt(input.death_penalty_review_start_date),
        extraction_meta_json: normalize_json_opt(
            input.extraction_meta_json,
            "extraction_meta_json",
        )?,
        notes: normalize_opt(input.notes),
        user_overrides_json: normalize_opt(input.user_overrides_json),
    })
}

fn compute_case_stage_item_input(
    input: UpsertCaseStageItemInput,
) -> Result<ComputedCaseStageItemInput, String> {
    if input.sort_order.is_some_and(|order| order < 0) {
        return Err("sort_order 不能为负数".to_string());
    }
    Ok(ComputedCaseStageItemInput {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        case_id: required_text(input.case_id, "case_id")?,
        domain: normalize_opt(input.domain).unwrap_or_else(|| DEFAULT_DOMAIN.to_string()),
        major_stage: normalize_opt(input.major_stage),
        stage_label: required_text(input.stage_label, "stage_label")?,
        status: normalize_opt(input.status).unwrap_or_else(|| DEFAULT_STAGE_STATUS.to_string()),
        started_at: normalize_opt(input.started_at),
        due_at: normalize_opt(input.due_at),
        completed_at: normalize_opt(input.completed_at),
        reminder_at: normalize_opt(input.reminder_at),
        source: normalize_opt(input.source).unwrap_or_else(|| DEFAULT_RECORD_SOURCE.to_string()),
        external_source: normalize_opt(input.external_source),
        external_record_id: normalize_opt(input.external_record_id),
        raw_payload_json: normalize_opt(input.raw_payload_json),
        notes: normalize_opt(input.notes),
        sort_order: input.sort_order,
    })
}

fn compute_criminal_deadline_item_input(
    input: UpsertCriminalDeadlineItemInput,
) -> Result<ComputedCriminalDeadlineItemInput, String> {
    let applicability_status =
        normalize_opt(input.applicability_status).unwrap_or_else(|| "confirmed".to_string());
    if !matches!(
        applicability_status.as_str(),
        "confirmed" | "needs_confirmation" | "not_applicable"
    ) {
        return Err(
            "applicability_status 必须为 confirmed、needs_confirmation 或 not_applicable"
                .to_string(),
        );
    }
    Ok(ComputedCriminalDeadlineItemInput {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        case_id: required_text(input.case_id, "case_id")?,
        stage_item_id: normalize_opt(input.stage_item_id),
        rule_code: normalize_opt(input.rule_code),
        title: required_text(input.title, "title")?,
        major_stage: normalize_opt(input.major_stage),
        minor_stage: normalize_opt(input.minor_stage),
        trigger_date: normalize_opt(input.trigger_date),
        trigger_time: normalize_opt(input.trigger_time),
        default_due_at: normalize_opt(input.default_due_at),
        manual_due_at: normalize_opt(input.manual_due_at),
        effective_due_at: normalize_opt(input.effective_due_at),
        reminder_at: normalize_opt(input.reminder_at),
        priority: normalize_opt(input.priority)
            .unwrap_or_else(|| DEFAULT_DEADLINE_PRIORITY.to_string()),
        status: normalize_opt(input.status).unwrap_or_else(|| DEFAULT_DEADLINE_STATUS.to_string()),
        source_type: normalize_opt(input.source_type)
            .unwrap_or_else(|| DEFAULT_DEADLINE_SOURCE_TYPE.to_string()),
        applicability_status,
        source_law: normalize_opt(input.source_law),
        source_article: normalize_opt(input.source_article),
        source_url: normalize_opt(input.source_url),
        calculation_note: normalize_opt(input.calculation_note),
        exception_type: normalize_opt(input.exception_type),
        exception_note: normalize_opt(input.exception_note),
        override_reason: normalize_opt(input.override_reason),
        completed_at: normalize_opt(input.completed_at),
    })
}

fn compute_case_agency_contact_input(
    input: UpsertCaseAgencyContactInput,
) -> Result<ComputedCaseAgencyContactInput, String> {
    Ok(ComputedCaseAgencyContactInput {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        case_id: required_text(input.case_id, "case_id")?,
        stage_scope: normalize_opt(input.stage_scope),
        agency_type: normalize_opt(input.agency_type),
        agency_name: normalize_opt(input.agency_name),
        contact_role: normalize_opt(input.contact_role),
        contact_name: normalize_opt(input.contact_name),
        phone: normalize_opt(input.phone),
        case_no: normalize_opt(input.case_no),
        query_code: normalize_opt(input.query_code),
        notes: normalize_opt(input.notes),
        source: normalize_opt(input.source).unwrap_or_else(|| DEFAULT_RECORD_SOURCE.to_string()),
        external_record_id: normalize_opt(input.external_record_id),
    })
}

async fn soft_delete(pool: &SqlitePool, table: &str, id: &str) -> Result<u64, String> {
    let sql = format!(
        "UPDATE {table} SET deleted_at = datetime('now'), updated_at = datetime('now') WHERE id = ? AND deleted_at IS NULL"
    );
    let result = sqlx::query(&sql)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected())
}

async fn get_latest_deadline_by_rule(
    pool: &SqlitePool,
    case_id: &str,
    rule_code: &str,
) -> Result<Option<CriminalDeadlineItem>, String> {
    let sql = format!(
        "{DEADLINE_SELECT} WHERE case_id = ? AND rule_code = ? ORDER BY updated_at DESC LIMIT 1"
    );
    sqlx::query_as::<_, CriminalDeadlineItem>(&sql)
        .bind(case_id)
        .bind(rule_code)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

fn should_preserve_deadline(item: &CriminalDeadlineItem) -> bool {
    item.deleted_at.is_some()
        || item.completed_at.is_some()
        || item
            .manual_due_at
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
        || item
            .override_reason
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
        || matches!(
            item.status.as_str(),
            "done" | "completed" | "ignored" | "overridden"
        )
}

fn profile_trigger_value<'a>(
    profile: &'a CriminalCaseProfile,
    trigger_field: &str,
) -> Option<&'a str> {
    let value = match trigger_field {
        "detention_date" => profile.detention_date.as_deref(),
        "arrest_review_received_date" => profile.arrest_review_received_date.as_deref(),
        "bail_start_date" => profile.bail_start_date.as_deref(),
        "residential_surveillance_start_date" => {
            profile.residential_surveillance_start_date.as_deref()
        }
        "arrest_date" => profile.arrest_date.as_deref(),
        "prosecution_received_date" => profile.prosecution_received_date.as_deref(),
        "first_instance_accepted_date" => profile.first_instance_accepted_date.as_deref(),
        "second_instance_accepted_date" => profile.second_instance_accepted_date.as_deref(),
        "judgment_received_date" => profile.judgment_received_date.as_deref(),
        "ruling_received_date" => profile.ruling_received_date.as_deref(),
        "supplementary_investigation_1_date" => {
            profile.supplementary_investigation_1_date.as_deref()
        }
        "supplementary_investigation_2_date" => {
            profile.supplementary_investigation_2_date.as_deref()
        }
        "judgment_effective_date" => profile.judgment_effective_date.as_deref(),
        "death_penalty_review_start_date" => profile.death_penalty_review_start_date.as_deref(),
        _ => None,
    }?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn parse_date_value(value: &str) -> Option<NaiveDate> {
    let trimmed = value.trim();
    if trimmed.len() >= 10 {
        if let Ok(date) = NaiveDate::parse_from_str(&trimmed[..10], "%Y-%m-%d") {
            return Some(date);
        }
    }
    NaiveDate::parse_from_str(trimmed, "%Y/%m/%d").ok()
}

fn apply_deadline_offset(date: NaiveDate, offset: DeadlineOffset) -> Option<NaiveDate> {
    match offset {
        DeadlineOffset::Days(days) => date.checked_add_signed(chrono::Duration::days(days)),
        DeadlineOffset::Months(months) => date.checked_add_months(Months::new(months)),
        DeadlineOffset::MonthsAndDays(months, days) => date
            .checked_add_months(Months::new(months))?
            .checked_add_signed(chrono::Duration::days(days)),
    }
}

fn rule_matches_procedure(profile: &CriminalCaseProfile, rule_code: &str) -> bool {
    let required = if rule_code.starts_with("CRIM_SUMMARY_PROCEDURE_") {
        Some("简易程序")
    } else if rule_code.starts_with("CRIM_FAST_TRACK_") {
        Some("速裁程序")
    } else {
        None
    };
    required.is_none_or(|expected| profile.procedure_type.as_deref() == Some(expected))
}

fn rule_applicability_status(rule_code: &str) -> &'static str {
    if matches!(
        rule_code,
        "CRIM_DETENTION_EXTEND_7D"
            | "CRIM_DETENTION_EXTEND_30D"
            | "CRIM_INVESTIGATION_CUSTODY_EXTEND_3M"
            | "CRIM_INVESTIGATION_CUSTODY_EXTEND_5M"
            | "CRIM_INVESTIGATION_CUSTODY_EXTEND_7M"
            | "CRIM_PROSECUTION_REVIEW_EXTEND_45D"
            | "CRIM_PROSECUTION_RESTART_AFTER_SUPP_1M"
            | "CRIM_PROSECUTION_RESTART_AFTER_SUPP_2M"
            | "CRIM_FIRST_INSTANCE_EXTEND_6M"
            | "CRIM_SUMMARY_PROCEDURE_45D"
            | "CRIM_FAST_TRACK_15D"
            | "CRIM_SECOND_INSTANCE_EXTEND_4M"
            | "CRIM_DEATH_PENALTY_REVIEW_3M"
    ) {
        "needs_confirmation"
    } else {
        "confirmed"
    }
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn normalize_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_json_opt(value: Option<String>, field: &str) -> Result<Option<String>, String> {
    let value = normalize_opt(value);
    if let Some(raw) = value.as_deref() {
        serde_json::from_str::<serde_json::Value>(raw)
            .map_err(|e| format!("{field} 不是有效 JSON: {e}"))?;
    }
    Ok(value)
}

fn required_text(value: String, field: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} 不能为空"))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_case(pool: &SqlitePool) -> String {
        crate::db::cases::create_case(
            pool,
            crate::db::cases::NewCase {
                name: "v0.6.2 刑事期限测试".into(),
                case_type: "criminal".into(),
                source_folder: format!("D:/test/criminal-v062/{}", Uuid::new_v4()),
            },
        )
        .await
        .expect("create criminal test case")
        .id
    }

    fn full_profile(case_id: String) -> UpsertCriminalCaseProfileInput {
        UpsertCriminalCaseProfileInput {
            case_id,
            procedure_type: Some("简易程序".into()),
            stage_sort_mode: Some("auto".into()),
            detention_date: Some("2026-01-01".into()),
            arrest_review_received_date: Some("2026-01-04".into()),
            arrest_date: Some("2026-01-11".into()),
            bail_start_date: Some("2026-01-01".into()),
            residential_surveillance_start_date: Some("2026-01-01".into()),
            prosecution_received_date: Some("2026-03-11".into()),
            supplementary_investigation_1_date: Some("2026-04-11".into()),
            supplementary_investigation_2_date: Some("2026-06-11".into()),
            first_instance_accepted_date: Some("2026-08-11".into()),
            second_instance_accepted_date: Some("2026-12-11".into()),
            judgment_received_date: Some("2026-11-01".into()),
            ruling_received_date: Some("2026-11-01".into()),
            judgment_effective_date: Some("2026-11-12".into()),
            death_penalty_review_start_date: Some("2026-12-20".into()),
            guilty_plea_status: Some("认罪认罚".into()),
            sentencing_recommendation: Some("有期徒刑三年至四年".into()),
            sentence_term: Some("有期徒刑三年六个月".into()),
            charge_history_json: Some(r#"[{"stage":"起诉","charge":"诈骗罪"}]"#.into()),
            restitution_amount: Some(120_000.0),
            restitution_status: Some("部分退赔".into()),
            victim_forgiveness: Some("已谅解".into()),
            surrender_status: Some("待确认".into()),
            meritorious_service_status: Some("无".into()),
            co_defendants_json: Some(r#"[{"name":"测试同案犯"}]"#.into()),
            extraction_meta_json: Some(r#"{"source":"manual-test"}"#.into()),
            ..Default::default()
        }
    }

    #[test]
    fn v062_rule_catalog_has_thirty_one_rules_and_current_source() {
        assert_eq!(CRIMINAL_DEADLINE_RULES.len(), 31);
        assert!(CRIMINAL_DEADLINE_RULES
            .iter()
            .all(|rule| rule.source_url.contains("flk.npc.gov.cn")));
        assert_eq!(
            apply_deadline_offset(
                NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
                DeadlineOffset::Months(1)
            ),
            NaiveDate::from_ymd_opt(2024, 2, 29)
        );
        assert_eq!(
            apply_deadline_offset(
                NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(),
                DeadlineOffset::MonthsAndDays(1, 15)
            ),
            NaiveDate::from_ymd_opt(2026, 4, 26)
        );
    }

    #[tokio::test]
    async fn migration_profile_and_refresh_preserve_v062_fields() {
        let pool = crate::db::init_pool(":memory:")
            .await
            .expect("migrate database");
        let case_id = test_case(&pool).await;
        let saved = upsert_criminal_case_profile(&pool, full_profile(case_id.clone()))
            .await
            .expect("save v062 profile");
        assert_eq!(saved.stage_sort_mode, "auto");
        assert_eq!(saved.restitution_amount, Some(120_000.0));
        assert!(saved.charge_history_json.is_some());

        let report = refresh_criminal_deadlines(&pool, &case_id)
            .await
            .expect("refresh deadlines");
        assert_eq!(report.generated_count, 29);
        assert_eq!(report.items.len(), 29);
        assert_eq!(report.needs_confirmation_count, 12);
        assert!(report.items.iter().any(|item| {
            item.rule_code.as_deref() == Some("CRIM_SUMMARY_PROCEDURE_20D")
                && item.applicability_status == "confirmed"
        }));
        assert!(!report
            .items
            .iter()
            .any(|item| item.rule_code.as_deref() == Some("CRIM_FAST_TRACK_10D")));
    }

    #[tokio::test]
    async fn manual_profile_save_tracks_changes_and_explicit_null_in_one_revision() {
        let pool = crate::db::init_pool(":memory:").await.expect("migrate database");
        let case_id = test_case(&pool).await;
        let original_overrides = r#"{"fields":{"legacy_key":{"value":"keep"}},"unknown":"keep"}"#;
        upsert_criminal_case_profile(&pool, UpsertCriminalCaseProfileInput {
            case_id: case_id.clone(), current_stage: Some("侦查".into()), suspected_charge: Some("盗窃罪".into()),
            user_overrides_json: Some(original_overrides.into()), ..Default::default()
        }).await.unwrap();
        let saved = upsert_criminal_case_profile_manual(&pool, UpsertCriminalCaseProfileInput {
            case_id, current_stage: None, suspected_charge: Some("诈骗罪".into()),
            user_overrides_json: Some(original_overrides.into()), ..Default::default()
        }).await.unwrap();
        assert_eq!(saved.profile_revision, 1);
        assert!(saved.current_stage.is_none());
        let overrides: serde_json::Value = serde_json::from_str(saved.user_overrides_json.as_deref().unwrap()).unwrap();
        assert_eq!(overrides["unknown"], "keep");
        assert!(overrides["fields"].get("legacy_key").is_some());
        assert_eq!(overrides["fields"]["current_stage"]["value"], serde_json::Value::Null);
        assert_eq!(overrides["fields"]["suspected_charge"]["value"], "诈骗罪");
    }

    #[tokio::test]
    async fn procedure_switch_marks_old_rules_not_applicable() {
        let pool = crate::db::init_pool(":memory:")
            .await
            .expect("migrate database");
        let case_id = test_case(&pool).await;
        upsert_criminal_case_profile(&pool, full_profile(case_id.clone()))
            .await
            .unwrap();
        refresh_criminal_deadlines(&pool, &case_id).await.unwrap();
        sqlx::query(
            "UPDATE criminal_case_profiles SET procedure_type = '速裁程序' WHERE case_id = ?",
        )
        .bind(&case_id)
        .execute(&pool)
        .await
        .unwrap();

        let report = refresh_criminal_deadlines(&pool, &case_id).await.unwrap();
        assert!(report.items.iter().any(|item| {
            item.rule_code.as_deref() == Some("CRIM_SUMMARY_PROCEDURE_20D")
                && item.applicability_status == "not_applicable"
        }));
        assert!(report.items.iter().any(|item| {
            item.rule_code.as_deref() == Some("CRIM_FAST_TRACK_10D")
                && item.applicability_status == "confirmed"
        }));
        assert!(report.items.iter().any(|item| {
            item.rule_code.as_deref() == Some("CRIM_FAST_TRACK_15D")
                && item.applicability_status == "needs_confirmation"
        }));
    }

    #[tokio::test]
    async fn refresh_keeps_manual_override_and_does_not_resurrect_soft_delete() {
        let pool = crate::db::init_pool(":memory:")
            .await
            .expect("migrate database");
        let case_id = test_case(&pool).await;
        upsert_criminal_case_profile(&pool, full_profile(case_id.clone()))
            .await
            .unwrap();
        let first = refresh_criminal_deadlines(&pool, &case_id).await.unwrap();
        let protected = first
            .items
            .iter()
            .find(|item| item.rule_code.as_deref() == Some("CRIM_APPEAL_JUDGMENT_10D"))
            .unwrap();
        let deleted = first
            .items
            .iter()
            .find(|item| item.rule_code.as_deref() == Some("CRIM_APPEAL_RULING_5D"))
            .unwrap();
        sqlx::query(
            "UPDATE criminal_deadline_items SET manual_due_at = '2026-11-20', effective_due_at = '2026-11-20', override_reason = '人工核对送达' WHERE id = ?",
        )
        .bind(&protected.id)
        .execute(&pool)
        .await
        .unwrap();
        delete_criminal_deadline_item(&pool, &deleted.id)
            .await
            .unwrap();

        let second = refresh_criminal_deadlines(&pool, &case_id).await.unwrap();
        assert!(second.preserved_count >= 2);
        let kept = get_latest_deadline_by_rule(&pool, &case_id, "CRIM_APPEAL_JUDGMENT_10D")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kept.manual_due_at.as_deref(), Some("2026-11-20"));
        let soft_deleted = get_latest_deadline_by_rule(&pool, &case_id, "CRIM_APPEAL_RULING_5D")
            .await
            .unwrap()
            .unwrap();
        assert!(soft_deleted.deleted_at.is_some());
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM criminal_deadline_items WHERE case_id = ? AND rule_code = 'CRIM_APPEAL_RULING_5D'",
        )
        .bind(&case_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn reorder_requires_complete_same_case_node_set() {
        let pool = crate::db::init_pool(":memory:")
            .await
            .expect("migrate database");
        let case_id = test_case(&pool).await;
        upsert_criminal_case_profile(&pool, full_profile(case_id.clone()))
            .await
            .unwrap();
        let first = upsert_case_stage_item(
            &pool,
            UpsertCaseStageItemInput {
                case_id: case_id.clone(),
                stage_label: "侦查".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let second = upsert_case_stage_item(
            &pool,
            UpsertCaseStageItemInput {
                case_id: case_id.clone(),
                stage_label: "审查起诉".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(reorder_case_stage_items(
            &pool,
            ReorderCaseStageItemsInput {
                case_id: case_id.clone(),
                ordered_ids: vec![first.id.clone()],
            },
        )
        .await
        .is_err());
        let ordered = reorder_case_stage_items(
            &pool,
            ReorderCaseStageItemsInput {
                case_id: case_id.clone(),
                ordered_ids: vec![second.id.clone(), first.id.clone()],
            },
        )
        .await
        .unwrap();
        assert_eq!(ordered[0].id, second.id);
        assert_eq!(ordered[0].sort_order, Some(0));
        assert_eq!(ordered[1].id, first.id);
        assert_eq!(ordered[1].sort_order, Some(1));
        assert_eq!(
            get_criminal_case_profile(&pool, &case_id)
                .await
                .unwrap()
                .unwrap()
                .stage_sort_mode,
            "manual"
        );
    }
}
