CREATE TABLE criminal_workflow_template_versions (
    id TEXT PRIMARY KEY NOT NULL,
    template_code TEXT NOT NULL,
    version INTEGER NOT NULL,
    name TEXT NOT NULL,
    scope_note TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'published' CHECK (status IN ('draft','published','retired')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    published_at TEXT,
    UNIQUE(template_code, version)
);

CREATE TABLE criminal_workflow_template_nodes (
    id TEXT PRIMARY KEY NOT NULL,
    template_version_id TEXT NOT NULL,
    node_code TEXT NOT NULL,
    title TEXT NOT NULL,
    stage_code TEXT NOT NULL,
    stage_sort INTEGER NOT NULL,
    node_sort INTEGER NOT NULL,
    trigger_event TEXT NOT NULL,
    prerequisite_codes_json TEXT NOT NULL DEFAULT '[]',
    task_type TEXT NOT NULL,
    default_applicability TEXT NOT NULL CHECK (default_applicability IN ('applicable','pending_confirmation')),
    repeatable INTEGER NOT NULL DEFAULT 0 CHECK (repeatable IN (0,1)),
    client_feedback_required INTEGER NOT NULL DEFAULT 0 CHECK (client_feedback_required IN (0,1)),
    time_nature TEXT NOT NULL CHECK (time_nature IN ('statutory_deadline_link','internal_service_target','unscheduled')),
    deadline_rule_codes_json TEXT NOT NULL DEFAULT '[]',
    work_record_required INTEGER NOT NULL DEFAULT 1 CHECK (work_record_required IN (0,1)),
    guidance_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
    FOREIGN KEY (template_version_id) REFERENCES criminal_workflow_template_versions(id) ON DELETE CASCADE,
    UNIQUE(template_version_id, node_code)
);

CREATE TABLE criminal_case_workflows (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    template_version_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','closed')),
    current_stage_code TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    closed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (template_version_id) REFERENCES criminal_workflow_template_versions(id),
    -- V1 每案仅允许一个刑事辩护标准主流程；申诉、死刑复核等并列专项模板不在本迁移范围。
    UNIQUE(case_id)
);

CREATE TABLE criminal_case_tasks (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    case_id TEXT NOT NULL,
    template_node_id TEXT NOT NULL,
    node_code TEXT NOT NULL,
    title TEXT NOT NULL,
    stage_code TEXT NOT NULL,
    stage_sort INTEGER NOT NULL,
    node_sort INTEGER NOT NULL,
    task_type TEXT NOT NULL,
    applicability_status TEXT NOT NULL CHECK (applicability_status IN ('applicable','pending_confirmation','not_applicable')),
    status TEXT NOT NULL CHECK (status IN ('pending_confirmation','unscheduled','pending','in_progress','completed','deferred','ignored','reopened','not_applicable')),
    occurrence_key TEXT NOT NULL,
    occurrence_no INTEGER NOT NULL DEFAULT 1,
    trigger_event TEXT NOT NULL,
    trigger_event_id TEXT NOT NULL,
    trigger_source_type TEXT NOT NULL,
    trigger_source_ref_id TEXT,
    planned_at TEXT,
    original_planned_at TEXT,
    started_at TEXT,
    completed_at TEXT,
    deferred_at TEXT,
    ignored_at TEXT,
    reopened_at TEXT,
    result TEXT,
    next_action TEXT,
    duration_minutes INTEGER,
    disposition_reason TEXT,
    client_feedback_recorded INTEGER NOT NULL DEFAULT 0 CHECK (client_feedback_recorded IN (0,1)),
    time_nature TEXT NOT NULL,
    deadline_item_id TEXT,
    work_item_id TEXT,
    assigned_to TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (workflow_id) REFERENCES criminal_case_workflows(id) ON DELETE CASCADE,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    FOREIGN KEY (template_node_id) REFERENCES criminal_workflow_template_nodes(id),
    FOREIGN KEY (deadline_item_id) REFERENCES criminal_deadline_items(id) ON DELETE SET NULL,
    FOREIGN KEY (work_item_id) REFERENCES case_work_items(id) ON DELETE SET NULL,
    UNIQUE(workflow_id, node_code, occurrence_key)
);

CREATE INDEX idx_criminal_case_tasks_case_status_due ON criminal_case_tasks(case_id, status, planned_at);
CREATE INDEX idx_criminal_case_tasks_calendar ON criminal_case_tasks(planned_at, case_id) WHERE planned_at IS NOT NULL;

CREATE TABLE criminal_task_events (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    case_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    event_id TEXT,
    source_type TEXT,
    source_ref_id TEXT,
    from_status TEXT,
    to_status TEXT,
    reason TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (task_id) REFERENCES criminal_case_tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE
);

CREATE INDEX idx_criminal_task_events_task_created ON criminal_task_events(task_id, created_at DESC);

CREATE TABLE criminal_reminder_deliveries (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    case_id TEXT NOT NULL,
    reminder_key TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'windows',
    scheduled_for TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'candidate' CHECK (status IN ('candidate','claimed','sent','failed')),
    claimed_at TEXT,
    sent_at TEXT,
    failed_at TEXT,
    error_message TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (task_id) REFERENCES criminal_case_tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (case_id) REFERENCES cases(id) ON DELETE CASCADE,
    UNIQUE(task_id, reminder_key, channel)
);

CREATE INDEX idx_criminal_reminders_scan ON criminal_reminder_deliveries(status, scheduled_for);

INSERT INTO criminal_workflow_template_versions
    (id, template_code, version, name, scope_note, status, published_at)
VALUES
    ('criminal_defense_standard_v1:1', 'criminal_defense_standard_v1', 1,
     '刑事辩护标准流程 V1', '犯罪嫌疑人、被告人一侧刑事辩护；不覆盖被害人代理、自诉、申诉、死刑复核',
     'published', datetime('now'));

-- V1 不设置内部目标天数；所有无明确法定期限的任务均由律师人工排期。
INSERT INTO criminal_workflow_template_nodes
    (id,template_version_id,node_code,title,stage_code,stage_sort,node_sort,trigger_event,prerequisite_codes_json,task_type,default_applicability,repeatable,client_feedback_required,time_nature,deadline_rule_codes_json,work_record_required)
VALUES
('sop1:intake_consultation','criminal_defense_standard_v1:1','intake_consultation','洽谈委托：了解情况、确认委托、确定方案','engagement_intake',10,10,'case_created','[]','intake_consultation','applicable',1,1,'unscheduled','[]',1),
('sop1:engagement_contract_signing','criminal_defense_standard_v1:1','engagement_contract_signing','签订委托合同、家属对接、核验授权意愿','engagement_intake',10,20,'engagement_decision_confirmed','["intake_consultation"]','engagement_contract','applicable',0,1,'unscheduled','[]',1),
('sop1:detention_first_meeting','criminal_defense_standard_v1:1','detention_first_meeting','首次会见：了解案情、量刑情节、法律心理辅导及初步辩护策略沟通','detention_arrest_review',20,10,'detention_confirmed','["engagement_contract_signing"]','meeting','applicable',1,1,'unscheduled','["CRIM_DETENTION_INTERROGATION_24H","CRIM_DETENTION_FAMILY_NOTICE_24H"]',1),
('sop1:detention_investigation','criminal_defense_standard_v1:1','detention_investigation','调查取证：调取言词证据并提示执业风险','detention_arrest_review',20,20,'detention_confirmed','["detention_first_meeting"]','investigation','pending_confirmation',1,0,'unscheduled','[]',1),
('sop1:detention_bail_application','criminal_defense_standard_v1:1','detention_bail_application','提交取保候审/变更强制措施申请','detention_arrest_review',20,30,'detention_confirmed','["detention_first_meeting"]','application','pending_confirmation',1,1,'statutory_deadline_link','["CRIM_ARREST_REQUEST_3D","CRIM_DETENTION_EXTEND_7D","CRIM_DETENTION_EXTEND_30D"]',1),
('sop1:pre_arrest_review_meeting','criminal_defense_standard_v1:1','pre_arrest_review_meeting','呈捕/审查逮捕阶段会见指导','detention_arrest_review',20,40,'arrest_review_request_confirmed','["detention_first_meeting"]','meeting','applicable',1,1,'statutory_deadline_link','["CRIM_ARREST_REVIEW_7D"]',1),
('sop1:non_arrest_legal_opinion','criminal_defense_standard_v1:1','non_arrest_legal_opinion','提交不予批准逮捕法律意见并沟通','detention_arrest_review',20,50,'arrest_review_request_confirmed','["pre_arrest_review_meeting"]','legal_opinion','pending_confirmation',1,1,'statutory_deadline_link','["CRIM_ARREST_REVIEW_7D"]',1),
('sop1:arrest_review_agency_communication','criminal_defense_standard_v1:1','arrest_review_agency_communication','与检察机关进行审查逮捕程序沟通','detention_arrest_review',20,60,'arrest_review_request_confirmed','["pre_arrest_review_meeting"]','agency_communication','applicable',1,1,'statutory_deadline_link','["CRIM_ARREST_REVIEW_7D"]',1),
('sop1:post_arrest_meeting','criminal_defense_standard_v1:1','post_arrest_meeting','逮捕后会见：捕后法律心理辅导、了解检方关注点','post_arrest_investigation',30,10,'arrest_confirmed','[]','meeting','applicable',1,1,'unscheduled','["CRIM_INVESTIGATION_CUSTODY_2M"]',1),
('sop1:post_arrest_investigation','criminal_defense_standard_v1:1','post_arrest_investigation','补充调查取证，形成有利证据','post_arrest_investigation',30,20,'arrest_confirmed','["post_arrest_meeting"]','investigation','pending_confirmation',1,0,'unscheduled','[]',1),
('sop1:custody_necessity_review_application','criminal_defense_standard_v1:1','custody_necessity_review_application','申请羁押必要性审查/变更强制措施','post_arrest_investigation',30,30,'arrest_confirmed','["post_arrest_meeting"]','application','pending_confirmation',1,1,'statutory_deadline_link','["CRIM_INVESTIGATION_CUSTODY_2M","CRIM_INVESTIGATION_CUSTODY_EXTEND_3M","CRIM_INVESTIGATION_CUSTODY_EXTEND_5M","CRIM_INVESTIGATION_CUSTODY_EXTEND_7M"]',1),
('sop1:investigation_agency_communication','criminal_defense_standard_v1:1','investigation_agency_communication','与侦查机关进行程序性沟通','post_arrest_investigation',30,40,'arrest_confirmed','["post_arrest_meeting"]','agency_communication','applicable',1,1,'unscheduled','[]',1),
('sop1:prosecution_file_review','criminal_defense_standard_v1:1','prosecution_file_review','阅卷并制作阅卷笔录、证据展示和案情疑点','prosecution_review',40,10,'prosecution_transfer_confirmed','[]','file_review','applicable',1,1,'statutory_deadline_link','["CRIM_PROSECUTION_REVIEW_1M","CRIM_PROSECUTION_REVIEW_EXTEND_45D"]',1),
('sop1:prosecution_meeting','criminal_defense_standard_v1:1','prosecution_meeting','审查起诉阶段会见：核对证据、案情并确定辩护方向','prosecution_review',40,20,'prosecution_transfer_confirmed','["prosecution_file_review"]','meeting','applicable',1,1,'unscheduled','[]',1),
('sop1:prosecution_legal_opinion','criminal_defense_standard_v1:1','prosecution_legal_opinion','提出审查起诉阶段法律意见','prosecution_review',40,30,'prosecution_transfer_confirmed','["prosecution_file_review","prosecution_meeting"]','legal_opinion','applicable',1,1,'statutory_deadline_link','["CRIM_PROSECUTION_REVIEW_1M","CRIM_PROSECUTION_REVIEW_EXTEND_45D"]',1),
('sop1:prosecution_bail_or_custody_review','criminal_defense_standard_v1:1','prosecution_bail_or_custody_review','申请取保候审/羁押必要性审查','prosecution_review',40,40,'prosecution_transfer_confirmed','["prosecution_file_review"]','application','pending_confirmation',1,1,'statutory_deadline_link','["CRIM_PROSECUTION_REVIEW_1M","CRIM_PROSECUTION_REVIEW_EXTEND_45D"]',1),
('sop1:prosecutor_communication','criminal_defense_standard_v1:1','prosecutor_communication','与检察官当面沟通案件：证据疑点、是否退侦、定罪与量刑建议','prosecution_review',40,50,'prosecution_transfer_confirmed','["prosecution_file_review"]','agency_communication','applicable',1,1,'statutory_deadline_link','["CRIM_PROSECUTION_REVIEW_1M","CRIM_PROSECUTION_RESTART_AFTER_SUPP_1M","CRIM_PROSECUTION_RESTART_AFTER_SUPP_2M"]',1),
('sop1:plea_witness_preparation','criminal_defense_standard_v1:1','plea_witness_preparation','认罪认罚具结见证及双认指导','prosecution_review',40,60,'plea_process_confirmed','["prosecution_file_review","prosecution_meeting"]','plea_witness','pending_confirmation',1,1,'unscheduled','[]',1),
('sop1:court_file_review','criminal_defense_standard_v1:1','court_file_review','法院阅卷：起诉书、量刑建议书、检察院补充侦查卷等','first_instance',50,10,'court_acceptance_confirmed','[]','file_review','applicable',1,1,'statutory_deadline_link','["CRIM_FIRST_INSTANCE_2M","CRIM_FIRST_INSTANCE_3M","CRIM_FIRST_INSTANCE_EXTEND_6M"]',1),
('sop1:pretrial_meeting','criminal_defense_standard_v1:1','pretrial_meeting','庭前会见与辅导','first_instance',50,20,'court_acceptance_confirmed','["court_file_review"]','meeting','applicable',1,1,'unscheduled','[]',1),
('sop1:pretrial_defense_opinion','criminal_defense_standard_v1:1','pretrial_defense_opinion','庭前提交辩护意见','first_instance',50,30,'court_acceptance_confirmed','["court_file_review","pretrial_meeting"]','legal_opinion','applicable',1,1,'statutory_deadline_link','["CRIM_FIRST_INSTANCE_2M","CRIM_FIRST_INSTANCE_3M","CRIM_SUMMARY_PROCEDURE_20D","CRIM_SUMMARY_PROCEDURE_45D","CRIM_FAST_TRACK_10D","CRIM_FAST_TRACK_15D"]',1),
('sop1:trial_preparation','criminal_defense_standard_v1:1','trial_preparation','庭前准备：发言提纲、质证意见、辩护意见','first_instance',50,40,'hearing_scheduled','["court_file_review","pretrial_meeting"]','trial_preparation','applicable',1,0,'internal_service_target','[]',1),
('sop1:trial_defense','criminal_defense_standard_v1:1','trial_defense','庭审辩护','first_instance',50,50,'hearing_scheduled','["trial_preparation"]','hearing','applicable',1,1,'unscheduled','[]',1),
('sop1:post_trial_meeting','criminal_defense_standard_v1:1','post_trial_meeting','庭后会见：庭审情况复盘、补充沟通、判决结果与上诉意愿预判','first_instance',50,60,'hearing_completed','["trial_defense"]','meeting','applicable',1,1,'unscheduled','[]',1),
('sop1:first_judgment_review','criminal_defense_standard_v1:1','first_judgment_review','一审判决复盘与判后答疑','first_instance',50,70,'first_instance_judgment_received','["trial_defense"]','judgment_review','applicable',0,1,'statutory_deadline_link','["CRIM_APPEAL_JUDGMENT_10D","CRIM_APPEAL_RULING_5D"]',1),
('sop1:appeal_intention_meeting','criminal_defense_standard_v1:1','appeal_intention_meeting','判后会见：答疑、确定二审委托、沟通上诉方案并提前准备签署','appeal_second_instance',60,10,'first_instance_judgment_received','["first_judgment_review"]','meeting','applicable',1,1,'statutory_deadline_link','["CRIM_APPEAL_JUDGMENT_10D","CRIM_APPEAL_RULING_5D"]',1),
('sop1:appeal_drafting','criminal_defense_standard_v1:1','appeal_drafting','拟写上诉状','appeal_second_instance',60,20,'appeal_intention_confirmed','["appeal_intention_meeting"]','appeal_document','applicable',1,1,'statutory_deadline_link','["CRIM_APPEAL_JUDGMENT_10D","CRIM_APPEAL_RULING_5D"]',1),
('sop1:appeal_submission','criminal_defense_standard_v1:1','appeal_submission','签署并提交上诉状','appeal_second_instance',60,30,'appeal_intention_confirmed','["appeal_drafting"]','appeal_document','applicable',0,1,'statutory_deadline_link','["CRIM_APPEAL_JUDGMENT_10D","CRIM_APPEAL_RULING_5D"]',1),
('sop1:second_instance_file_review','criminal_defense_standard_v1:1','second_instance_file_review','二审阅卷和补充阅卷','appeal_second_instance',60,40,'appeal_confirmed','["appeal_submission"]','file_review','applicable',1,1,'statutory_deadline_link','["CRIM_SECOND_INSTANCE_2M","CRIM_SECOND_INSTANCE_EXTEND_4M"]',1),
('sop1:second_instance_meeting','criminal_defense_standard_v1:1','second_instance_meeting','二审阶段会见','appeal_second_instance',60,50,'appeal_confirmed','["appeal_submission"]','meeting','applicable',1,1,'unscheduled','[]',1),
('sop1:second_instance_defense_opinion','criminal_defense_standard_v1:1','second_instance_defense_opinion','提交二审辩护意见','appeal_second_instance',60,60,'appeal_confirmed','["second_instance_file_review","second_instance_meeting"]','legal_opinion','applicable',1,1,'statutory_deadline_link','["CRIM_SECOND_INSTANCE_2M","CRIM_SECOND_INSTANCE_EXTEND_4M"]',1),
('sop1:second_instance_hearing_or_review','criminal_defense_standard_v1:1','second_instance_hearing_or_review','二审开庭或书面审理辩护','appeal_second_instance',60,70,'second_instance_procedure_confirmed','["second_instance_defense_opinion"]','hearing','pending_confirmation',1,1,'statutory_deadline_link','["CRIM_SECOND_INSTANCE_2M","CRIM_SECOND_INSTANCE_EXTEND_4M"]',1),
('sop1:second_instance_result_review','criminal_defense_standard_v1:1','second_instance_result_review','二审裁判复盘、委托人反馈及后续路径提示','appeal_second_instance',60,80,'second_instance_decision_received','["second_instance_hearing_or_review"]','judgment_review','applicable',0,1,'unscheduled','[]',1),
('sop1:common_active_follow_up','criminal_defense_standard_v1:1','common_active_follow_up','主动跟进办案机关和程序进展','current',90,10,'manual_occurrence','[]','active_follow_up','applicable',1,1,'internal_service_target','[]',1),
('sop1:common_client_feedback','criminal_defense_standard_v1:1','common_client_feedback','向委托人/家属反馈程序变化或工作进展','current',90,20,'manual_occurrence','[]','client_feedback','applicable',1,0,'internal_service_target','[]',1);
