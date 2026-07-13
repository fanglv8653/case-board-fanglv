//! 刑事案件后端最小模型(2026-07-04)
//!
//! 本模块仅提供刑事画像、阶段节点、期限节点、机关联系人四类最小 CRUD，
//! 作为后续 `CRIM-N2` UI 与 `CRIM-N3` 期限规则引擎的上游。
//! 不接首页、团队、聊天、AI、MCP，也不重复创建 `case_work_items`。

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
    pub notes: Option<String>,
    pub user_overrides_json: Option<String>,
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
    "https://www.spp.gov.cn/zdgz/201810/t20181027_396818.shtml";

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
    notes,
    user_overrides_json,
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
    let computed = compute_criminal_case_profile_input(input)?;
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
            ruling_received_date, notes, user_overrides_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    .bind(&computed.notes)
    .bind(&computed.user_overrides_json)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_criminal_case_profile(pool, &computed.case_id)
        .await?
        .ok_or_else(|| "刑事画像写入后读取失败".to_string())
}

pub async fn list_case_stage_items(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CaseStageItem>, String> {
    let sql = format!(
        "{STAGE_SELECT} WHERE case_id = ? AND deleted_at IS NULL ORDER BY started_at DESC, updated_at DESC"
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
            external_source, external_record_id, raw_payload_json, notes
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            source_type, source_law, source_article, source_url,
            calculation_note, exception_type, exception_note, override_reason,
            completed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        let existing = get_latest_deadline_by_rule(pool, &case_id, rule.code).await?;

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
                    source_law = ?,
                    source_article = ?,
                    source_url = ?,
                    calculation_note = ?,
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
            .bind(rule.source_law)
            .bind(rule.source_article)
            .bind(rule.source_url)
            .bind(rule.calculation_note)
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
            source_law: Some(rule.source_law.to_string()),
            source_article: Some(rule.source_article.to_string()),
            source_url: Some(rule.source_url.to_string()),
            calculation_note: Some(rule.calculation_note.to_string()),
            exception_type: None,
            exception_note: None,
            override_reason: None,
            completed_at: None,
        };
        upsert_criminal_deadline_item(pool, input).await?;
        generated_count += 1;
    }

    let items = list_criminal_deadline_items(pool, &case_id).await?;
    Ok(CriminalDeadlineRefreshReport {
        case_id,
        generated_count,
        updated_count,
        preserved_count,
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
        notes: normalize_opt(input.notes),
        user_overrides_json: normalize_opt(input.user_overrides_json),
    })
}

fn compute_case_stage_item_input(
    input: UpsertCaseStageItemInput,
) -> Result<ComputedCaseStageItemInput, String> {
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
    })
}

fn compute_criminal_deadline_item_input(
    input: UpsertCriminalDeadlineItemInput,
) -> Result<ComputedCriminalDeadlineItemInput, String> {
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

fn required_text(value: String, field: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} 不能为空"))
    } else {
        Ok(trimmed.to_string())
    }
}
