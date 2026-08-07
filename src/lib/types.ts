/**
 * 前后端共享的数据结构定义。
 *
 * 对应 Rust 端 `src-tauri/src/ingest/scanner.rs::ScannedDoc`。
 * 字段命名跟 Rust 端保持一致(snake_case),不做 camelCase 转换。
 */

export interface ScannedDoc {
  /** 原文件绝对路径(只读引用,工具不复制原文件) */
  source_path: string;
  /** 文件名(不含路径) */
  filename: string;
  /** 阶段:立案 / 一审 / 二审 / 再审 / 执行 / 证据 / 身份信息 / null */
  stage: string | null;
  /** 类别:起诉状 / 判决书 / 笔录 / ... / null */
  category: string | null;
  /** 是否是 AI 跑出来的中间产物(总览/调查/精要等) */
  is_ai_artifact: boolean;
  /** 文件大小(字节) */
  size_bytes: number;
}

/**
 * 阶段显示顺序(立案 → 一审 → 二审 → 再审 → 执行 → 证据 → 身份)。
 * `null` stage 会显示成"其他",排在最后。AI 产物单独成组排在最前。
 */
export const STAGE_ORDER = [
  "立案",
  "一审",
  "二审",
  "再审",
  "执行",
  "证据",
  "身份信息",
] as const;

/* ------------------------------------------------------------------ */
/* 数据库类型(对应 src-tauri/src/db/)                                 */
/* ------------------------------------------------------------------ */

export type LegalDomain = "criminal" | "civil" | "other" | "unknown";
export type LegalDomainSource = "manual" | "inferred" | "legacy";

/** 对应 Rust `db::cases::Case` */
export interface Case {
  id: string;
  name: string;
  case_type: string; // 诉讼 / 非诉
  legal_domain: LegalDomain;
  domain_source: LegalDomainSource;
  display_name_override: string | null;
  cause: string | null;
  case_no: string | null;
  court: string | null;
  judge_id: string | null;
  stage: string | null;
  source_folder: string;
  ai_summary_md: string | null;
  created_at: string;
  updated_at: string;
  last_scanned_at: string | null;

  // ===== 2026-05-23 加(migration 0002)=====
  /** 案件级聚合字段(由 aggregator 从 documents.extracted_fields 算出) */
  agg_case_no: string | null;
  agg_court: string | null;
  agg_cause: string | null;
  /** JSON array(用 parseJsonArray 安全解析) */
  agg_plaintiffs: string | null;
  agg_defendants: string | null;
  agg_third_parties: string | null;
  agg_judges: string | null;
  agg_claim_amount: number | null;
  agg_filed_at: string | null;
  agg_computed_at: string | null;

  /** 下一关键节点(驱动首页"办案节点 30 天" widget,V0.2 用) */
  next_milestone_type: string | null;
  next_milestone_at: string | null;
  next_milestone_status: string | null;
  next_milestone_note: string | null;

  /** 案件总状态:进行中 / 已结案 / 已归档 */
  case_status: string;

  /** 执行款追踪聚合 */
  execution_total: number | null;
  execution_total_breakdown: string | null; // JSON
  execution_started_at: string | null;
  execution_received: number | null;
  execution_remaining: number | null;

  /** ====== 2026-05-24 e 加(migration 0006)======
   * 看板卡片右上角的工作流状态(8 档枚举)。
   * null = 走前端自动推断(基于 documents.category + key_dates);
   * 非 null = 用户在卡片右上角下拉手工选过,优先取用户值。
   * 见 src/modules/litigation/lib/inferStatus.ts
   */
  workflow_status: string | null;

  /** ====== 2026-05-24 h 加(migration 0008 · LLM 全局抽方案)======
   * LLM 全局抽出来的扩展字段。替代旧 aggregator 规则方案。
   */
  /** 一句话案件概括(50 字内) */
  case_summary: string | null;
  /** 完整案件分析报告 MD 路径(详情页「📖 案件报告」按钮渲染) */
  case_report_path: string | null;
  case_report_generated_at: string | null;
  /** 调解 / 判决 / 执行结果(自由文本,200 字内) */
  agg_resolution: string | null;
  /** LLM 推断的状态文字(跟 workflow_status 8 档不同,自由描述) */
  agg_status_text: string | null;
  /** JSON: [{name,role,id_no,address,phone,is_our_side}] */
  agg_party_contacts: string | null;
  /** JSON: [{name,role,phone}] */
  agg_court_contacts: string | null;
  /** JSON: [{date,event,note}] */
  agg_key_dates: string | null;
  /** JSON: [{item,amount,note}] */
  agg_fees: string | null;

  /** ====== 2026-05-24 k 加(migration 0010 · 元典查被执行人 P1)====== */
  /** 风险提示报告 MD 路径(详情页「🔍 查被执行人」按钮触发后落盘) */
  risk_assessment_path: string | null;
  risk_assessment_at: string | null;

  /** P2 深挖报告 MD 路径(2026-05-24 k-9 · migration 0011) */
  deep_dive_report_path: string | null;
  deep_dive_at: string | null;

  /** 2026-05-25 V0.1.7 完整报告 MD 路径(migration 0013):合并风险报告 + 深挖报告 → DeepSeek 出第三份 */
  full_report_path: string | null;
  full_report_at: string | null;

  /**
   * 2026-05-26 V0.1.13 用户手改 overlay JSON 字符串(migration 0016)。
   *
   * 结构定义见 `lib/userOverrides.ts`(UserOverrides interface)。
   * LLM 全局抽永不写这列;前端"编辑模式"调 `update_case_overrides` Tauri command 写;
   * 渲染时 `applyOverrides()` 把它叠加在 agg_* 之上。
   */
  user_overrides_json: string | null;

  /**
   * 2026-06-11 审级模型(migration 0022):当前承办机关类型('法院'/'仲裁委'/'其他')。
   * 驱动前端 label(承办法院 vs 仲裁委);agg_court/agg_case_no 语义=「当前审级」快照,
   * 全部审级明细走 listCaseInstances()。
   */
  agg_court_type: string | null;

  /**
   * 2026-06-13(migration 0023):我方代理立场('原告方'/'被告方'/'第三人'/'反诉混合'/null)。
   * LLM 从 is_our_side=true 当事人推断;用户改值走 user_overrides_json(fields.agg_our_side)。
   * 驱动报告侧重、AI 助手立场、各 chip 不再"猜我方"。
   */
  agg_our_side: string | null;

  /**
   * 2026-06-13(migration 0025):工作流状态锁。1=用户手动选过状态,
   * 全局抽不再用 LLM 值覆盖(修「结案/手设状态被重新分析刷新掉」)。
   */
  workflow_status_locked: number;
  /** 跨领域管理状态，与民事 workflow_status / 刑事程序阶段分离。 */
  management_status: "negotiating" | "active" | "closed" | "unknown" | string;
  management_status_source: "manual" | "feishu" | "legacy" | string;
}

/**
 * 审级实例(case_instances 表一行)。一个案件 = N 个审级:[仲裁]→一审→二审→[再审]。
 * seq 最大者 is_current=true;handlers/party_roles 是 JSON 字符串。
 */
export interface CaseInstance {
  id: string;
  case_id: string;
  level: string; // 仲裁 / 一审 / 二审 / 再审
  seq: number;
  case_no: string | null;
  authority: string | null;
  authority_type: string | null; // 法院 / 仲裁委 / 其他
  handlers: string | null; // JSON [{name,role,phone}]
  party_roles: string | null; // JSON [{name,role,is_our_side,note}]
  filed_at: string | null;
  result: string | null;
  note: string | null;
  is_current: boolean;
  source: string; // llm / user
  created_at: string;
  updated_at: string;
}

export type IncomeSourceType = "personal" | "collaboration";

export type IncomeArchiveHoldbackStatus = "holding" | "returned" | "not_returned";

export type IncomeInvoiceStatus = "all" | "invoiced" | "not_invoiced";
export type IncomeRecordStatus = "draft" | "confirmed";

export interface IncomeRecord {
  id: string;
  case_id: string | null;
  /** 解析后的显示案件名:优先 manual_case_name,否则取关联案件 name。 */
  case_name: string | null;
  manual_case_name: string | null;
  lawyer_fee_total: number;
  source_type: IncomeSourceType;
  collaborator_name: string | null;
  share_ratio: number;
  firm_deduction_rate: number;
  archive_holdback_rate: number;
  personal_share_amount: number;
  firm_deduction_amount: number;
  archive_holdback_amount: number;
  archive_holdback_status: IncomeArchiveHoldbackStatus;
  archive_returned_at: string | null;
  archive_returned_amount: number;
  invoice_date: string | null;
  invoice_no: string | null;
  record_status: IncomeRecordStatus;
  invoice_total: number | null;
  invoice_buyer: string | null;
  invoice_seller: string | null;
  invoice_type: string | null;
  auto_source_document_id: string | null;
  auto_source_filename: string | null;
  auto_fields_json: string;
  manual_fields_json: string;
  recognized_month: string;
  actual_income_amount: number;
  actual_income_overridden: number;
  actual_income_override_note: string | null;
  note: string | null;
  created_at: string;
  updated_at: string;
}

export interface IncomeRecordFilter {
  month_from?: string | null;
  month_to?: string | null;
  source_type?: IncomeSourceType | null;
  archive_holdback_status?: IncomeArchiveHoldbackStatus | null;
  invoice_status?: IncomeInvoiceStatus | null;
  query?: string | null;
}

export interface IncomeRecordUpsertInput {
  id?: string | null;
  case_id?: string | null;
  manual_case_name?: string | null;
  lawyer_fee_total: number;
  source_type?: IncomeSourceType | null;
  collaborator_name?: string | null;
  share_ratio?: number | null;
  firm_deduction_rate?: number | null;
  archive_holdback_rate?: number | null;
  archive_holdback_status?: IncomeArchiveHoldbackStatus | null;
  archive_returned_at?: string | null;
  archive_returned_amount?: number | null;
  invoice_date?: string | null;
  invoice_no?: string | null;
  recognized_month?: string | null;
  actual_income_amount?: number | null;
  actual_income_overridden?: number | null;
  actual_income_override_note?: string | null;
  note?: string | null;
  record_status?: IncomeRecordStatus | null;
}

export interface InvoiceDraftInput {
  case_id?: string | null;
  source_document_id: string;
  source_filename: string;
  invoice_date?: string | null;
  invoice_no: string;
  invoice_total?: number | null;
  invoice_buyer?: string | null;
  invoice_seller?: string | null;
  invoice_type?: string | null;
}

export interface IncomeSummary {
  record_count: number;
  lawyer_fee_total_sum: number;
  personal_share_sum: number;
  firm_deduction_sum: number;
  archive_holdback_sum: number;
  actual_income_sum: number;
  holding_amount_sum: number;
  returned_holdback_sum: number;
  invoiced_fee_sum: number;
  overridden_count: number;
}

export interface CaseWorkItem {
  id: string;
  case_id: string | null;
  occurred_at: string;
  work_type: string;
  title: string;
  content: string;
  result: string | null;
  next_action: string | null;
  duration_minutes: number | null;
  source: string;
  external_source: string | null;
  external_record_id: string | null;
  external_updated_at: string | null;
  raw_payload_json: string | null;
  confirmation_status: "pending" | "confirmed";
  source_document_id: string | null;
  source_filename: string | null;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CaseWorkItemFilter {
  case_id?: string | null;
  occurred_from?: string | null;
  occurred_to?: string | null;
  work_type?: string | null;
  source?: string | null;
  query?: string | null;
}

export interface CaseWorkItemUpsertInput {
  id?: string | null;
  case_id?: string | null;
  occurred_at: string;
  work_type?: string | null;
  title: string;
  content: string;
  result?: string | null;
  next_action?: string | null;
  duration_minutes?: number | null;
  source?: string | null;
  external_source?: string | null;
  external_record_id?: string | null;
  external_updated_at?: string | null;
  raw_payload_json?: string | null;
  confirmation_status?: "pending" | "confirmed" | null;
  source_document_id?: string | null;
  source_filename?: string | null;
}

export interface CriminalCaseProfile {
  case_id: string;
  current_stage: string | null;
  procedure_type: string | null;
  case_subtype: string | null;
  defense_role: string | null;
  suspected_charge: string | null;
  suspect_or_defendant_name: string | null;
  victim_name: string | null;
  client_name: string | null;
  client_relationship: string | null;
  detention_center: string | null;
  coercive_measure_type: string | null;
  detention_date: string | null;
  arrest_request_date: string | null;
  arrest_review_received_date: string | null;
  arrest_decision_date: string | null;
  arrest_date: string | null;
  bail_start_date: string | null;
  residential_surveillance_start_date: string | null;
  transfer_for_prosecution_date: string | null;
  prosecution_received_date: string | null;
  first_instance_accepted_date: string | null;
  second_instance_accepted_date: string | null;
  judgment_received_date: string | null;
  ruling_received_date: string | null;
  stage_sort_mode: "auto" | "manual";
  guilty_plea_status: string | null;
  sentencing_recommendation: string | null;
  sentence_term: string | null;
  charge_history_json: string | null;
  restitution_amount: number | null;
  restitution_status: string | null;
  victim_forgiveness: string | null;
  surrender_status: string | null;
  meritorious_service_status: string | null;
  co_defendants_json: string | null;
  supplementary_investigation_1_date: string | null;
  supplementary_investigation_2_date: string | null;
  judgment_effective_date: string | null;
  death_penalty_review_start_date: string | null;
  extraction_meta_json: string | null;
  notes: string | null;
  user_overrides_json: string | null;
  profile_revision: number;
  created_at: string;
  updated_at: string;
}

export interface CriminalCaseProfileUpsertInput {
  case_id: string;
  current_stage?: string | null;
  procedure_type?: string | null;
  case_subtype?: string | null;
  defense_role?: string | null;
  suspected_charge?: string | null;
  suspect_or_defendant_name?: string | null;
  victim_name?: string | null;
  client_name?: string | null;
  client_relationship?: string | null;
  detention_center?: string | null;
  coercive_measure_type?: string | null;
  detention_date?: string | null;
  arrest_request_date?: string | null;
  arrest_review_received_date?: string | null;
  arrest_decision_date?: string | null;
  arrest_date?: string | null;
  bail_start_date?: string | null;
  residential_surveillance_start_date?: string | null;
  transfer_for_prosecution_date?: string | null;
  prosecution_received_date?: string | null;
  first_instance_accepted_date?: string | null;
  second_instance_accepted_date?: string | null;
  judgment_received_date?: string | null;
  ruling_received_date?: string | null;
  stage_sort_mode?: "auto" | "manual" | null;
  guilty_plea_status?: string | null;
  sentencing_recommendation?: string | null;
  sentence_term?: string | null;
  charge_history_json?: string | null;
  restitution_amount?: number | null;
  restitution_status?: string | null;
  victim_forgiveness?: string | null;
  surrender_status?: string | null;
  meritorious_service_status?: string | null;
  co_defendants_json?: string | null;
  supplementary_investigation_1_date?: string | null;
  supplementary_investigation_2_date?: string | null;
  judgment_effective_date?: string | null;
  death_penalty_review_start_date?: string | null;
  extraction_meta_json?: string | null;
  notes?: string | null;
  user_overrides_json?: string | null;
}

export interface CriminalSentencingEstimateSaveInput {
  case_id: string;
  expected_profile_revision: number;
  input_snapshot: unknown;
  output_min_months: number;
  output_max_months: number | null;
  output_snapshot: unknown;
  process_snapshot: unknown;
  basis_snapshot: unknown;
  created_source: string;
}

export interface CriminalSentencingEstimate {
  id: string;
  case_id: string;
  profile_case_id: string;
  profile_revision: number;
  input_snapshot: unknown;
  output_min_months: number;
  output_max_months: number | null;
  output_snapshot: unknown;
  process_snapshot: unknown;
  basis_snapshot: unknown;
  created_source: string;
  created_at: string;
}

export interface CaseStageItem {
  id: string;
  case_id: string;
  domain: string;
  major_stage: string | null;
  stage_label: string;
  status: string;
  started_at: string | null;
  due_at: string | null;
  completed_at: string | null;
  reminder_at: string | null;
  source: string;
  external_source: string | null;
  external_record_id: string | null;
  raw_payload_json: string | null;
  notes: string | null;
  sort_order: number | null;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CaseStageItemUpsertInput {
  id?: string | null;
  case_id: string;
  domain?: string | null;
  major_stage?: string | null;
  stage_label: string;
  status?: string | null;
  started_at?: string | null;
  due_at?: string | null;
  completed_at?: string | null;
  reminder_at?: string | null;
  source?: string | null;
  external_source?: string | null;
  external_record_id?: string | null;
  raw_payload_json?: string | null;
  notes?: string | null;
  sort_order?: number | null;
}

export interface CriminalExtractionCandidateBatch {
  id: string;
  case_id: string;
  source_document_id: string | null;
  source_filename: string;
  document_type: string | null;
  model_name: string;
  schema_version: string;
  source_fingerprint: string;
  result_fingerprint: string;
  technical_status: "success" | "partial" | "failed";
  review_status:
    | "pending"
    | "partially_confirmed"
    | "confirmed"
    | "rejected"
    | "superseded";
  warning_json: string | null;
  error_message: string | null;
  created_at: string;
  updated_at: string;
  reviewed_at: string | null;
}

export interface CriminalExtractionCandidateField {
  id: string;
  batch_id: string;
  field_key: string;
  value_json: string;
  source_document_id: string | null;
  source_filename: string;
  evidence_excerpt: string | null;
  confidence: number | null;
  review_status: "pending" | "accepted" | "rejected" | "protected";
  decision_note: string | null;
  created_at: string;
  updated_at: string;
}

export interface CriminalExtractionCandidateDetail {
  batch: CriminalExtractionCandidateBatch;
  fields: CriminalExtractionCandidateField[];
}

export interface CriminalExtractionCandidateDecision {
  field_key: string;
  decision: "accept" | "reject";
  note?: string | null;
}

export interface ConfirmCriminalExtractionCandidateBatchInput {
  batch_id: string;
  expected_profile_revision: number;
  decisions: CriminalExtractionCandidateDecision[];
}

export interface CriminalExtractionCandidateReviewResult {
  batch: CriminalExtractionCandidateBatch;
  profile_revision: number;
  applied_fields: string[];
  protected_fields: string[];
}

export interface CriminalCaseReextractReport {
  cached_count: number;
  scheduled_ocr_count: number;
  failed_count: number;
  errors: string[];
}

export interface ReorderCaseStageItemsInput {
  case_id: string;
  ordered_ids: string[];
}

export interface CriminalDeadlineItem {
  id: string;
  case_id: string;
  stage_item_id: string | null;
  rule_code: string | null;
  title: string;
  major_stage: string | null;
  minor_stage: string | null;
  trigger_date: string | null;
  trigger_time: string | null;
  default_due_at: string | null;
  manual_due_at: string | null;
  effective_due_at: string | null;
  reminder_at: string | null;
  priority: string;
  status: string;
  source_type: string;
  applicability_status: "confirmed" | "needs_confirmation" | "not_applicable";
  source_law: string | null;
  source_article: string | null;
  source_url: string | null;
  calculation_note: string | null;
  exception_type: string | null;
  exception_note: string | null;
  override_reason: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CriminalDeadlineItemUpsertInput {
  id?: string | null;
  case_id: string;
  stage_item_id?: string | null;
  rule_code?: string | null;
  title: string;
  major_stage?: string | null;
  minor_stage?: string | null;
  trigger_date?: string | null;
  trigger_time?: string | null;
  default_due_at?: string | null;
  manual_due_at?: string | null;
  effective_due_at?: string | null;
  reminder_at?: string | null;
  priority?: string | null;
  status?: string | null;
  source_type?: string | null;
  applicability_status?: "confirmed" | "needs_confirmation" | "not_applicable" | null;
  source_law?: string | null;
  source_article?: string | null;
  source_url?: string | null;
  calculation_note?: string | null;
  exception_type?: string | null;
  exception_note?: string | null;
  override_reason?: string | null;
  completed_at?: string | null;
}

export interface CriminalDeadlineRefreshReport {
  case_id: string;
  generated_count: number;
  updated_count: number;
  preserved_count: number;
  needs_confirmation_count: number;
  skipped_count: number;
  items: CriminalDeadlineItem[];
}

export type CriminalWorkflowTaskStatus =
  | "pending_confirmation"
  | "unscheduled"
  | "pending"
  | "in_progress"
  | "completed"
  | "deferred"
  | "ignored"
  | "reopened"
  | "not_applicable";

export type CriminalTaskAction =
  | "confirm_applicable"
  | "not_applicable"
  | "schedule"
  | "start"
  | "defer"
  | "complete"
  | "ignore"
  | "reopen";

export interface CriminalWorkflow {
  id: string;
  case_id: string;
  template_version_id: string;
  status: "active" | "closed";
  current_stage_code: string | null;
  started_at: string;
  closed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface CriminalWorkflowTask {
  id: string;
  workflow_id: string;
  case_id: string;
  template_node_id: string;
  node_code: string;
  title: string;
  stage_code: string;
  stage_sort: number;
  node_sort: number;
  task_type: string;
  applicability_status: "applicable" | "pending_confirmation" | "not_applicable";
  status: CriminalWorkflowTaskStatus;
  occurrence_key: string;
  occurrence_no: number;
  trigger_event: string;
  trigger_event_id: string;
  trigger_source_type: string;
  trigger_source_ref_id: string | null;
  planned_at: string | null;
  original_planned_at: string | null;
  started_at: string | null;
  completed_at: string | null;
  deferred_at: string | null;
  ignored_at: string | null;
  reopened_at: string | null;
  result: string | null;
  next_action: string | null;
  duration_minutes: number | null;
  disposition_reason: string | null;
  client_feedback_recorded: boolean;
  time_nature: "statutory_deadline_link" | "internal_service_target" | "unscheduled";
  deadline_item_id: string | null;
  work_item_id: string | null;
  assigned_to: string | null;
  created_at: string;
  updated_at: string;
}

export interface CriminalTaskEvent {
  id: string;
  task_id: string;
  case_id: string;
  event_type: string;
  actor: string;
  event_id: string | null;
  source_type: string | null;
  source_ref_id: string | null;
  from_status: string | null;
  to_status: string | null;
  reason: string | null;
  payload_json: string;
  created_at: string;
}

export type CriminalWorkflowConfirmedEvent =
  | "case_created"
  | "engagement_decision_confirmed"
  | "detention_confirmed"
  | "arrest_review_request_confirmed"
  | "non_arrest_confirmed"
  | "arrest_confirmed"
  | "prosecution_transfer_confirmed"
  | "plea_process_confirmed"
  | "public_prosecution_confirmed"
  | "court_acceptance_confirmed"
  | "hearing_scheduled"
  | "hearing_completed"
  | "first_instance_judgment_received"
  | "appeal_intention_confirmed"
  | "appeal_confirmed"
  | "second_instance_procedure_confirmed"
  | "second_instance_decision_received"
  | "second_instance_closed";

interface RefreshCriminalWorkflowInputBase {
  case_id: string;
  event_code: CriminalWorkflowConfirmedEvent;
  event_id: string;
  confirmed_by: string;
}

export type RefreshCriminalWorkflowInput = RefreshCriminalWorkflowInputBase &
  (
    | {
        source_type: "manual_confirmed";
        source_ref_id?: string | null;
      }
    | {
        source_type: "accepted_extraction_candidate";
        source_ref_id: string;
      }
    | {
        source_type: "workflow_confirmed";
        source_ref_id: string;
      }
  );

export interface RefreshCriminalWorkflowResult {
  workflow: CriminalWorkflow;
  generated_count: number;
  preserved_count: number;
  tasks: CriminalWorkflowTask[];
}

export interface CriminalTaskActionInput {
  task_id: string;
  action: CriminalTaskAction;
  actor: string;
  planned_at?: string | null;
  result?: string | null;
  next_action?: string | null;
  duration_minutes?: number | null;
  reason?: string | null;
  client_feedback_recorded?: boolean | null;
}

export interface CreateCriminalTaskOccurrenceInput {
  case_id: string;
  node_code: string;
  actor: string;
  occurrence_key?: string | null;
  planned_at?: string | null;
}

export interface CriminalTaskFilter {
  case_id?: string | null;
  statuses?: CriminalWorkflowTaskStatus[] | null;
  planned_from?: string | null;
  planned_to?: string | null;
}

export interface CriminalTaskSummaryRow {
  case_id: string;
  case_name: string;
  task_id: string;
  title: string;
  stage_code: string;
  task_type: string;
  status: CriminalWorkflowTaskStatus;
  applicability_status: "applicable" | "pending_confirmation" | "not_applicable";
  planned_at: string | null;
  client_feedback_required: boolean;
}

export interface CriminalDeadlineCalendarRow {
  deadline_id: string;
  case_id: string;
  case_name: string;
  title: string;
  rule_code: string | null;
  deadline_at: string;
  status: string;
  applicability_status: "confirmed";
}

export interface CriminalReminderDelivery {
  id: string;
  task_id: string;
  case_id: string;
  reminder_key: string;
  channel: string;
  scheduled_for: string;
  status: "candidate" | "claimed" | "sent" | "failed";
  claimed_at: string | null;
  sent_at: string | null;
  failed_at: string | null;
  error_message: string | null;
  attempt_count: number;
  created_at: string;
  updated_at: string;
}

export interface ClaimCriminalRemindersInput {
  now: string;
  channel?: string | null;
  limit?: number | null;
}

export interface MarkCriminalReminderInput {
  delivery_id: string;
  sent: boolean;
  error_message?: string | null;
}

export interface CaseAgencyContact {
  id: string;
  case_id: string;
  stage_scope: string | null;
  agency_type: string | null;
  agency_name: string | null;
  contact_role: string | null;
  contact_name: string | null;
  phone: string | null;
  case_no: string | null;
  query_code: string | null;
  notes: string | null;
  source: string;
  external_record_id: string | null;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CaseAgencyContactUpsertInput {
  id?: string | null;
  case_id: string;
  stage_scope?: string | null;
  agency_type?: string | null;
  agency_name?: string | null;
  contact_role?: string | null;
  contact_name?: string | null;
  phone?: string | null;
  case_no?: string | null;
  query_code?: string | null;
  notes?: string | null;
  source?: string | null;
  external_record_id?: string | null;
}

/** 新建/更新审级的输入(add/updateCaseInstance 共用)。 */
export interface NewCaseInstance {
  level: string;
  seq: number;
  case_no: string | null;
  authority: string | null;
  authority_type: string | null;
  handlers: string | null;
  party_roles: string | null;
  filed_at: string | null;
  result: string | null;
  note: string | null;
}

/**
 * 把 Case 里 JSON 字符串字段(agg_plaintiffs 等)安全 parse 成数组。
 * 解析失败/null 时返回 []。
 */
export function parseJsonArray(s: string | null): string[] {
  if (!s) return [];
  try {
    const parsed = JSON.parse(s);
    return Array.isArray(parsed) ? parsed.filter((x) => typeof x === "string") : [];
  } catch {
    return [];
  }
}

/** 对应 Rust `db::documents::Document` */
export interface Document {
  id: string;
  case_id: string;
  source_path: string;
  filename: string;
  stage: string | null;
  category: string | null;
  is_ai_artifact: boolean;
  /**
   * 文档来源(后端 documents.source)。判别 AI 写的可编辑材料的精确依据:
   * `'chat_artifact'` = save_artifact 起草的正式文书;`'chat'` = AI 助手任务产物(类案检索/法律依据等)。
   * 其它(scan/llm_extract)= 扫描原件 / 全局抽报告。编辑按钮只给前两者(app 自有,不动用户原文件)。
   */
  source: string;
  mime_type: string | null;
  size_bytes: number;
  modified_at: string | null;
  extracted_fields: string | null;
  extraction_status: string;
  missing: boolean;
  created_at: string;
  /** 2026-05-23 晚十 加:软删时间戳(看板已过滤,正常不会拿到非 null) */
  deleted_at: string | null;
  /** 抽出来的 .md 文件落盘路径(extracts/<case_id>/<doc_id>.md) */
  extracted_text_path: string | null;
  /** 缓存键 = "<mtime>:<size>" */
  cache_key: string | null;
  /**
   * V0.2 D2(migration 0018)· 引用弹窗排序用。
   * `null` = 未置顶;有值时是 ISO 时间戳,越新越靠前。AttachmentPicker 据此分组。
   */
  pinned_at: string | null;
  /**
   * 2026-06-13(migration 0026)· 文档级 OCR 后端覆盖。
   * 'ppocrv6' = 用户点了「去水印重新识别」→ 强制 PP-OCRv6+去水印;null = 常规 OCR 策略。
   */
  ocr_backend_override: string | null;
  /**
   * 2026-06-20(migration 0034)· 板内显示名(干净、带类型前缀的中文名,替代杂乱原文件名)。
   * null = 回退原始 filename。纯元数据,不碰磁盘原件。
   */
  display_name: string | null;
  /** 显示名来源:'user'(人工右键改名,永不被 AI 覆盖)/ 'ai_suggest'(AI 自动整理建议)。 */
  display_name_source: string | null;
}

export interface CachedExtractionRetryReport {
  used_cached_text: boolean;
  status: "done" | "partial" | "failed" | "pending";
  error: string | null;
}

export type MaterialDisposition = "recognize" | "index_only" | "excluded";

export interface MaterialPreflightItem {
  sourcePath: string;
  relativePath: string;
  filename: string;
  sizeBytes: number;
  stage: string | null;
  category: string | null;
  isExisting: boolean;
  defaultDisposition: MaterialDisposition;
}

export interface MaterialPreflight {
  mode: "import" | "refresh";
  caseId: string | null;
  rootPath: string;
  legalDomain: "criminal" | "civil";
  totalFiles: number;
  totalSizeBytes: number;
  largeCriminalBatch: boolean;
  items: MaterialPreflightItem[];
}

export interface MaterialDecisionInput {
  sourcePath: string;
  disposition: MaterialDisposition;
  documentId?: string | null;
}

export interface MaterialProcessingBatch {
  id: string;
  caseId: string;
  status: string;
  errorCategory: string | null;
  errorSummary: string | null;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
  updatedAt: string;
}

export interface MaterialProcessingItem {
  id: string;
  batchId: string;
  caseId: string;
  sourcePath: string;
  documentId: string | null;
  ordinal: number;
  status: string;
  claimToken: string | null;
  claimedAt: string | null;
  completedAt: string | null;
  errorCategory: string | null;
  errorSummary: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface MaterialProcessingEvent {
  id: string;
  batchId: string;
  itemId: string | null;
  eventType: string;
  fromStatus: string | null;
  toStatus: string | null;
  actor: string;
  errorCategory: string | null;
  errorSummary: string | null;
  createdAt: string;
}

export interface MaterialBatchDetail {
  batch: MaterialProcessingBatch;
  items: MaterialProcessingItem[];
  events: MaterialProcessingEvent[];
}

export interface CommitMaterialPreflightResult {
  case: Case;
  documents: Document[];
  sync: {
    added: number;
    updated: number;
    unchanged: number;
    deleted: number;
  };
  batch: MaterialBatchDetail | null;
  isExisting: boolean;
}

/** 文档标记(源文件看板 Phase 3)。对应 Rust `db::document_tags::DocumentTag`。 */
export interface DocumentTag {
  id: string;
  document_id: string;
  /** 'importance' | 'party_side' */
  namespace: string;
  /** importance: 重要|忽略 ; party_side: 原告|被告|第三人 */
  value: string;
  /** 'user' | 'ai_suggest' */
  source: string;
  created_at: string;
  updated_at: string;
}

/** 文档内搜索命中(2026-06-20)。对应 Rust `doc_search::SearchHit`。 */
export interface SearchHit {
  /** 命中页码(1-based);null = 文本无页码标记,无法定位(旧文档,重抽后支持) */
  page: number | null;
  snippet: string;
  count: number;
}

/** PDF 页码书签(2026-06-20)。对应 Rust `db::bookmarks::Bookmark`。 */
export interface Bookmark {
  id: string;
  document_id: string;
  /** 1-based 页码 */
  page: number;
  label: string | null;
  created_at: string;
}

/** 对应 Rust 端 `CaseWithDocs`,get_case_with_docs 命令的返回 */
export interface CaseWithDocs {
  case: Case;
  documents: Document[];
}

/* ------------------------------------------------------------------ */
/* V0.2 D6 · 案件 AI 助手 V2 · chat 工具调用 + 引用协议                 */
/* ------------------------------------------------------------------ */

/**
 * 单次工具调用 trace。对应 Rust `chat::agent_loop::ToolCallRecord`。
 * 给 `ToolCallTrace` 组件渲染 🟢/🌐/🟡/⚠️ 状态行。
 */
export interface ToolCallRecord {
  /** 工具名,例如 `search_laws` / `enterprise_aggregation_summary` */
  tool: string;
  /** 工具调用入参,任意 JSON 对象 */
  args: unknown;
  /** 是否本地 KB 缓存命中(true → 🟢, false → 🌐 在线) */
  kb_hit: boolean;
  /** 本次消耗的元典积分(本地工具/缓存命中 = 0) */
  credits_used: number;
  /** 工具调用是否成功 */
  success: boolean;
  /** 失败时的脱敏短错(成功为 null) */
  error_short: string | null;
  /** epoch 毫秒,开始时间 */
  started_at_ms: number;
  /** epoch 毫秒,结束时间 */
  finished_at_ms: number;
}

/**
 * `<CITATIONS>` 协议的单条引用。对应 Rust `chat::citations::Citation`。
 * 给 `CitationsCard` 组件按 type 分组渲染。
 */
export interface Citation {
  /** 正文里 `[ref:N]` 标记的 N(从 1 开始) */
  ref: number;
  /** "law" | "case" | "doc" | "kb_local" */
  type: string;
  /** 引用源:法条全名 / 案号 / 文件名 / KB 路径 */
  source: string;
  /** 原文摘抄(可选,但强烈推荐) */
  quote?: string | null;
  /** type=case 时的法院名(可选) */
  court?: string | null;
  /**
   * 后端校验结果:`type=doc` 时校验 quote 是否在文档里;其他 type 默认 true。
   * false → CitationsCard 标 ⚠️
   */
  verified: boolean;
  /** 产生该引用的工具调用 ID(可选,前端可据此回到对应 ToolCallTrace 行) */
  tool_call_id?: string | null;
}

/* ------------------------------------------------------------------ */
/* 用户设置                                                            */
/* ------------------------------------------------------------------ */

/** 对应 Rust `settings::Settings`。所有字段都可空(用户没填时为 null)。 */
export interface CredentialStatus {
  locator: string;
  configured: boolean;
  backend: "windows_credential_manager" | string;
  error_code: string | null;
}

export interface Settings {
  /** WebView never receives credential values; it receives status only. */
  credential_statuses?: CredentialStatus[];
  credential_migration_version?: number | null;
  /** 用户的显示称呼(例:"刘律师"),首页问候用。 */
  user_display_name: string | null;
  /** 合同审查修订批注版的默认作者；为空时回退到 user_display_name。 */
  contract_review_comment_author: string | null;
  /** 2026-05-23 加:用户是否完成 onboarding。默认 false,首次启动会强制弹 wizard。 */
  setup_completed: boolean;

  /** 2026-05-23 晚六:OCR 后端单独选 (local / cloud) */
  ocr_provider: ProviderChoice | null;
  /** 2026-05-23 晚六:LLM 后端单独选 (local / cloud) */
  llm_provider: ProviderChoice | null;

  /** 本机模型目录(留空就用智能默认:LM Studio / ~/.cache/caseboard/models) */
  local_model_dir: string | null;
  /** 是否允许 App 自动拉起 llama-server(默认 true) */
  local_server_auto_start: boolean | null;

  /** [DEPRECATED] 老的全局云端开关,保留向后兼容,以 ocr/llm_provider 优先 */
  cloud_enabled: boolean;
  mineru_endpoint: string | null;
  /** 2026-06-12:PaddleOCR VL-1.6(AI Studio)访问令牌,免费 2 万页/天。 */
  /** PaddleOCR key 验证通过时间(ISO 8601)。非 null = 绿勾。 */
  paddle_vl_verified_at: string | null;
  /** 云端 OCR 主力:"mineru"(默认)/ "paddle-vl"。另一家自动成为备用。 */
  ocr_cloud_primary: string | null;
  ollama_endpoint: string | null;
  ollama_model: string | null;
  cloud_llm_endpoint: string | null;
  cloud_llm_model: string | null;
  /** 云端 LLM 后端:"deepseek"(默认/null)/ "minimax" / "glm" / "mimo" / "custom"。
   *  minimax 读 minimax_*;glm/mimo/custom 读各自独立配置;其余读 cloud_llm_*。 */
  cloud_llm_backend: string | null;
  minimax_endpoint: string | null;
  /** MiniMax 模型名(可编辑文本,默认 MiniMax-M2)。型号以 MiniMax 控制台为准。 */
  minimax_model: string | null;
  minimax_verified_at: string | null;
  /** 2026-06-16:旧版通用 OpenAI 兼容字段。保留作升级兜底,新 UI 写入下面的独立字段。 */
  compat_llm_endpoint: string | null;
  compat_llm_model: string | null;
  compat_llm_verified_at: string | null;
  /** 2026-06-17:智谱 / MiMo / 自定义模型各自独立保存,切换服务商不互相覆盖。 */
  glm_llm_endpoint: string | null;
  glm_llm_model: string | null;
  glm_llm_verified_at: string | null;
  mimo_llm_endpoint: string | null;
  mimo_llm_model: string | null;
  mimo_llm_verified_at: string | null;
  custom_llm_endpoint: string | null;
  custom_llm_model: string | null;
  custom_llm_verified_at: string | null;
  /** 2026-05-24 k:元典法律开放平台 API key(执行案件查被执行人 / 财产线索)*/
  /** 2026-06-01 V0.3:快递100 实时查询 customer + key(快递查询工具用)*/
  /** 2026-06-01 V0.3.3:Embedding 云端模型(案件文档语义检索)。填了 api_key 才启用,否则回退关键词。 */
  embedding_endpoint: string | null;
  embedding_model: string | null;
  embedding_verified_at: string | null;
  /** 本地知识库语义索引「自动维护」开关。null/true=开(默认),false=关。 */
  kb_semantic_auto_index: boolean | null;

  /** 2026-05-25 V0.1.6:MinerU key 验证通过时间(ISO 8601)。非 null = 绿勾。 */
  mineru_verified_at: string | null;
  /** DeepSeek key 验证通过时间。 */
  deepseek_verified_at: string | null;
  /** 2026-05-25 V0.1.8:元典 key 验证通过时间。 */
  yuandian_verified_at: string | null;

  /** 2026-05-26 V0.1.13:首页"在办案件"卡片用户拖动后的顺序。
   *  null = 没排过,用 listCases 默认顺序;非空 = 数组里的 case_id 按这个顺序,
   *  没在数组里的新案件自动追加在末尾;已删的 case_id 留着也无害(前端 filter)。
   */
  home_case_order: string[] | null;

  /** 2026-06-14:首页"日程日历"功能开关(默认 false / 关闭) */
  home_calendar_enabled: boolean;

  // ===== 2026-06-17 飞书日历(整合外部贡献 PR #9) =====
  /** 飞书日历总开关。null/false = 关。开+配好后首页显示飞书月历(替代本地日程日历卡)。 */
  feishu_enabled: boolean | null;
  /** lark-cli 可执行文件路径。null/空 = 按平台自动找(mac 走 Homebrew,Win/Linux 靠 PATH)。 */
  feishu_lark_cli_path: string | null;
  /** (可选)飞书"案件池"多维表格 App Token;配了才能点日历事件反查并导入本地案件目录。 */
  feishu_app_token: string | null;
  /** (可选)飞书"案件池"多维表格 Table ID。 */
  feishu_cases_table_id: string | null;
  /** 飞书自建应用 App ID。App Secret 与 OAuth token 不进入 Settings。 */
  feishu_oauth_app_id: string | null;

  // ===== 2026-06-17 辅助在线立案(整合外部贡献 PR #8) =====
  /** 立案 CLI 包根目录。null = 用应用内置 standalone/court_filing_cli。 */
  court_filing_cli_path: string | null;
  /** Python 解释器路径。null = 用 "python3"(Windows 需填 "python" 或 venv 全路径)。 */
  court_filing_python: string | null;
  /** 全国法院一张网账号(手机号)。只存本机。 */
  court_filing_account: string | null;
  /** 全国法院一张网密码。只存本机。 */
  court_filing_password: string | null;
  /** 一张网登录态 cookie 缓存目录。null = 默认应用数据目录。 */
  court_filing_cookie_dir: string | null;

  // ===== V0.2 D2 新增 · 本地知识库 + chat V2 budget (对应 settings.rs 同名字段) =====
  /** 本地法律知识库根目录(支持 ~/);null = 不启用。 */
  local_kb_root: string | null;
  /** 本地 KB 总开关。false = 即使 root 有值也不启用。 */
  local_kb_enabled: boolean | null;
  /** 元典积分月度上限(普通 1 / 聚合 5);null = 不限制。 */
  yuandian_monthly_credit_limit: number | null;
  /** chat 总上下文 char 预算(默认 300_000)。 */
  chat_context_budget_total: number | null;
  chat_context_budget_system: number | null;
  chat_context_budget_attached: number | null;
  chat_context_budget_history: number | null;
  /** chat agent loop 最大迭代轮数(默认 8)。 */
  chat_loop_max_iters: number | null;
  /** chat 单条消息最多引用文档数(默认 5)。 */
  chat_max_attached: number | null;
  /** 2026-06-21 方律场景路由总开关。false = 完全关闭,聊天保持原主链。 */
  enable_fanglv_router: boolean;

  /** 2026-06-04 V0.3.6 · 外部 MCP server 白名单(CaseBoard 当客户端消费其工具)。
   *  默认 [] = 桥接关闭、零行为变化。详 docs/adr/0008。 */
  mcp_servers: McpServerConfig[];
  /** 团队版:本机团队身份;null/缺省 = 未加入团队。后端 team_* 命令直接写,设置表单不碰它。 */
  team?: TeamIdentity | null;
}

/** 飞书日历事件(对应 Rust feishu::FeishuCalendarEvent)。 */
export interface FeishuCalendarEvent {
  event_id: string;
  summary: string;
  start_date: string;
  end_date: string | null;
  is_all_day: boolean;
  description: string | null;
  location: string | null;
  app_link: string | null;
}

export interface FeishuSyncLinkPreview {
  id: string;
  local_case_id: string;
  local_case_name: string;
  record_id: string;
  link_source: string;
  status: string;
  last_synced_at: string | null;
  is_orphaned: boolean;
  error_code: string | null;
}

export interface FeishuSyncInboxPreview {
  id: string;
  record_id: string;
  display_name: string;
  legal_type: string | null;
  case_no: string | null;
  remote_modified_at: string | null;
  status: string;
  recommended_case_id: string | null;
  recommendation_reason: string | null;
}

export interface FeishuLocalCaseOption {
  id: string;
  display_name: string;
  legal_domain: string;
  case_no: string | null;
  cause: string | null;
  party: string | null;
}

export interface FeishuSyncChangePreview {
  id: string;
  case_name: string;
  field_key: string;
  field_label: string;
  local_value_json: string | null;
  feishu_value_json: string | null;
  classification: string;
  proposed_action: "pull_to_local" | "review" | "none" | string;
  review_status: "pending" | "applied_feishu" | "applied_local" | "dismissed" | string;
}

export interface FeishuSyncEntityPreview {
  id: string;
  case_name: string;
  entity_type: "work_item" | "stage" | "contact" | string;
  change_kind: "create" | "update" | "restore" | "archive" | string;
  local_value_json: string | null;
  feishu_value_json: string | null;
  review_status: "pending" | "applied_feishu" | "applied_local" | "dismissed" | string;
}

export interface FeishuSyncConflictPreview {
  id: string;
  case_name: string;
  field_key: string;
  local_value_json: string | null;
  feishu_value_json: string | null;
  status: string;
  created_at: string;
}

export interface FeishuSyncRunPreview {
  id: string;
  mode: string;
  status: string;
  active_case_filter: string;
  started_at: string;
  completed_at: string | null;
  counts_json: string;
  error_code: string | null;
  error_message: string | null;
}

export interface FeishuSyncPreview {
  bound_cases: FeishuSyncLinkPreview[];
  pending_cases: FeishuSyncInboxPreview[];
  ignored_cases: FeishuSyncInboxPreview[];
  available_local_cases: FeishuLocalCaseOption[];
  proposed_changes: FeishuSyncChangePreview[];
  entity_changes: FeishuSyncEntityPreview[];
  conflicts: FeishuSyncConflictPreview[];
  recent_runs: FeishuSyncRunPreview[];
}

export interface FeishuConnectionStatus {
  connected: boolean;
  app_id: string;
  scopes: string[];
  access_expires_at: number | null;
  refresh_expires_at: number | null;
  reauthorization_required: boolean;
  write_enabled: boolean;
}

export interface FeishuConnectionInput {
  app_id: string;
  app_secret: string;
}

export interface FeishuPullResult {
  run_id: string;
  status: "succeeded" | "partial" | string;
  error_code: string | null;
  remote_count: number;
  bound_count: number;
  pending_count: number;
  proposed_change_count: number;
  work_item_count: number;
  stage_count: number;
  contact_count: number;
  archived_entity_count: number;
  orphan_count: number;
}

// ===== 法院一张网在线立案(整合外部贡献 PR #8) =====

export interface CourtFilingJob {
  id: string;
  case_id: string;
  filing_type: "civil" | "execution";
  court_name: string;
  cookie_account: string | null;
  status: "pending" | "running" | "waiting_captcha" | "completed" | "failed" | "cancelled";
  output_dir: string | null;
  preview_url: string | null;
  progress_json: string | null;
  captcha_active: number;
  error: string | null;
  timing_json: string | null;
  created_at: string;
  updated_at: string;
}

export interface CourtFilingProgress {
  job_id: string;
  case_id: string;
  phase: "system" | "login" | "http" | "playwright" | "captcha";
  stage: string;
  level: "info" | "warning" | "error";
  message: string;
  detail?: string;
  round?: number;
  task_id?: string;
  image_base64?: string;
  timing?: Record<string, number>;
}

export interface CourtFilingCaptcha {
  job_id: string;
  case_id: string;
  task_id: string;
  round: number;
  image_base64: string;
  timeout_sec: number;
}

/** 在线立案运行环境单组件体检结果。 */
export interface CourtFilingEnvComponent {
  name: string;
  id: string;
  version: string;
  ok: boolean;
}

/** 在线立案运行环境整体体检报告。 */
export interface CourtFilingEnvReport {
  ok: boolean;
  components: CourtFilingEnvComponent[];
  missing: string[];
  python_found: boolean;
  error?: string | null;
}

/** 一键安装的流式进度事件(court-filing-env-progress)。 */
export interface CourtFilingEnvProgress {
  step: string; // python / venv / deps / chromium / verify
  label: string;
  status: "running" | "done" | "error";
  detail?: string;
  log?: string;
}

export interface LawyerProfile {
  id: string;
  name: string;
  bar_number: string | null;
  law_firm: string | null;
  id_number: string | null;
  phone: string | null;
  address: string | null;
  is_default: boolean; // Rust 端 Option<bool>(整合 PR #17),跟契约对齐
  created_at: string;
  updated_at: string;
}

/** WebView 可见的去密 MCP 投影；敏感值只存在系统凭据库。 */
export interface McpServerConfig {
  server_id: string;
  name: string;
  transport: McpStoredTransport;
  enabled: boolean;
  complete: McpSecretReference;
}

export interface McpSecretReference {
  locator: string;
  configured: boolean;
}

export type McpStoredValue =
  | { kind: "plain"; value: string }
  | { kind: "secret"; credential: McpSecretReference };

export type McpStoredArgument =
  | { kind: "plain"; value: string }
  | { kind: "secret"; prefix: string; credential: McpSecretReference };

export type McpStoredTransport =
  | {
      type: "stdio";
      command: string;
      args: McpStoredArgument[];
      env: Record<string, McpSecretReference>;
    }
  | {
      type: "http";
      url: McpStoredValue;
      headers: Record<string, McpSecretReference>;
    };

/** 导入完成后返回的安全投影。 */
export interface ParsedMcpPaste {
  servers: McpServerConfig[];
  /** 人读警告(占位符令牌等),原样展示。 */
  warnings: string[];
}

/** MCP 连接测试结果(对应 Rust lib.rs McpTestReport)。 */
export interface McpTestReport {
  tool_count: number;
  /** 前若干个工具名(确认接对了用,不全量)。 */
  tool_names: string[];
}

/* ------------------------------------------------------------------ */
/* 团队版 Phase 1(LAN 接力同步,对应 Rust team 模块)                  */
/* ------------------------------------------------------------------ */

/** 可返回给 WebView 的非敏感团队身份。 */
export interface TeamIdentity {
  team_id: string;
  team_name: string;
  member_id: string;
  my_name: string;
  role: "leader" | "member" | string;
}

export interface RecognitionUsageQuery {
  granularity: "day" | "month";
  from?: string | null;
  to?: string | null;
}

export interface RecognitionUsageBucket {
  bucket: string;
  stage: string;
  providerModel: string;
  taskCount: number;
  successCount: number;
  failureCount: number;
  skippedCount: number;
  averageElapsedMs: number | null;
  rateLimit429Count: number;
  fallbackCount: number | null;
  pageCount: number | null;
}

export interface RecognitionUsageOverview {
  dataSource: string;
  isVendorReported: boolean;
  generatedAt: string;
  granularity: "day" | "month";
  from: string | null;
  to: string | null;
  buckets: RecognitionUsageBucket[];
  capabilities: {
    fallbackCountAvailable: boolean;
    fallbackCountReason: string;
    pageCountAvailable: boolean;
    pageCountReason: string;
    rateLimitSource: string;
  };
}

export interface YuandianLocalUsageOverview {
  dataSource: string;
  isOfficialBalance: boolean;
  officialBalance: number | null;
  estimateBasis: string;
  current: {
    year_month: string;
    credits_used: number;
    api_calls: number;
    kb_hits: number;
    updated_at: string;
  };
  previousRecordedMonth: {
    year_month: string;
    credits_used: number;
    api_calls: number;
    kb_hits: number;
    updated_at: string;
  } | null;
  totalEstimatedCredits: number;
  totalRecordedApiCalls: number;
  totalRecordedKbHits: number;
  hasAnyRecord: boolean;
  lastRecordedAt: string | null;
  refreshedAt: string;
}

export interface LocalKbRelocationBackendResult {
  operation: string;
  old_root: string;
  new_root: string;
  backup_path: string;
  backup_available: boolean;
  copied: boolean;
  index_rebuild_required: boolean;
}

/** 成员 + 权限(roster 条目)。view: null=全队可见;edit: 可编辑哪些成员的登记字段。 */
export interface RosterMember {
  member_id: string;
  name: string;
  role: string;
  view: string[] | null;
  edit: string[];
}

export interface TeamRoster {
  team_id: string;
  team_name: string;
  seq: number;
  members: RosterMember[];
  updated_at: string;
}

export interface TeamStatus {
  in_team: boolean;
  /** 被踢出的团队名(一次性提示,返回即已自动清理本机配置)。 */
  kicked_from: string | null;
  identity: TeamIdentity | null;
  roster: TeamRoster | null;
}

/** 创建团队的一次性响应；配对码不会出现在后续 teamStatus 中。 */
export interface TeamCreateResult {
  status: TeamStatus;
  pairing_code: string;
}

/** 团队看板里的单个案件(登记表粒度快照)。 */
export interface TeamSnapshotCase {
  /** 案件在所有人本机的 id(编辑请求定位用;老快照可能为空串)。 */
  id: string;
  name: string;
  case_no: string | null;
  parties: string | null;
  case_type: string | null;
  stage: string | null;
  status_detail: string | null;
  claim_amount: number | null;
  key_dates: { date: string; event: string }[];
  last_activity: string | null;
  summary: string | null;
  /** v2(0.3.11):时间轴里已发生的最新一件事(案件卡"最新进展");老快照可能缺。 */
  latest_event?: { date: string; event: string } | null;
  court?: string | null;
  cause?: string | null;
  filed_at?: string | null;
  plaintiffs?: string[];
  defendants?: string[];
  third_parties?: string[];
  execution_total?: number | null;
  execution_received?: number | null;
  execution_remaining?: number | null;
}

export interface TeamMemberView {
  member_id: string;
  name: string;
  role: string;
  is_me: boolean;
  can_edit: boolean;
  /** null = 还没收到过这个成员的快照。 */
  updated_at: string | null;
  cases: TeamSnapshotCase[];
}

export interface TeamView {
  team_name: string;
  my_member_id: string;
  my_role: string;
  members: TeamMemberView[];
  /** 编辑请求/改动记录(备注展示、待生效标记、撤销列表共用)。 */
  edits: TeamEdit[];
}

/** 跨成员编辑请求(对应 Rust team::TeamEdit)。 */
export interface TeamEdit {
  id: string;
  team_id: string;
  editor_id: string;
  editor_name: string;
  target_member_id: string;
  case_id: string;
  case_name: string;
  field: "workflow_status" | "note" | string;
  value: string;
  prev_value: string | null;
  status: "pending" | "applied" | "rejected" | "reverted" | string;
  created_at: string;
  applied_at: string | null;
}

export interface DiscoveredTeam {
  team_id: string;
  team_name: string;
  leader_online: boolean;
  online_members: number;
}

export interface TeamSyncReport {
  peers_found: number;
  peers_synced: number;
  snapshots_merged: number;
  errors: string[];
}

/** 验证 API key 的返回(对应 Rust verify::VerifyResult) */
export interface VerifyResult {
  ok: boolean;
  message: string;
}

/** 2026-05-25 V0.1.8 · 版本检测结果(对应 Rust update::UpdateInfo) */
export interface UpdateInfo {
  current: string;
  latest: string | null;
  has_update: boolean;
  released_at: string | null;
  notes: string | null;
  download_url: string | null;
  error: string | null;
}

/** OCR / LLM 后端的选项 */
export type ProviderChoice = "local" | "cloud";

/** 本机模型 / llama-server 状态(对应 Rust LocalReadiness) */
export interface LocalReadiness {
  model_dir: string | null;
  has_main_model: boolean;
  has_mmproj: boolean;
  llama_cpp_installed: boolean;
  server_running: boolean;
  server_endpoint: string;
}

/* ------------------------------------------------------------------ */
/* LLM 抽取的字段                                                      */
/* ------------------------------------------------------------------ */

/** 对应 Rust `llm::ExtractedFields`。LLM 从诉讼文书里抽出的结构化数据。
 *  2026-05-23 晚十三 大扩字段(参考"信息集中管理"图)。 */
export interface ExtractedFields {
  // 案件基本
  case_no: string | null;
  case_type: string | null;
  court: string | null;
  cause: string | null;
  case_stage: string | null;
  case_status: string | null;
  filed_at: string | null;
  expected_close_at: string | null;
  case_note: string | null;
  // 当事人
  plaintiffs: string[];
  defendants: string[];
  third_parties: string[];
  party_contacts: PartyContact[];
  // 金额 / 收费
  claim_amount: number | null;
  fees: FeeRecord[];
  // 法院人员
  judges: string[];
  court_contacts: CourtContact[];
  // 时间线 / 保全
  key_dates: KeyDate[];
  preservations: Preservation[];
}

export interface CourtContact {
  /** 2026-05-26 V0.1.12:改 null 兼容 — 合议庭只知职务无名时 LLM 返回 null */
  name: string | null;
  role: string | null;
  phone: string | null;
}

export interface PartyContact {
  party: string;
  /** 2026-05-26 V0.1.12:改 null 兼容 — 合同里只有机构名无联系人时 LLM 返回 null */
  name: string | null;
  role: string | null;
  phone: string | null;
  email: string | null;
  /** 2026-05-23 晚十五:是否为我方当事人(委托方),null=未知 */
  is_our_side: boolean | null;
  /** 2026-05-26 V0.1.12:同人跨文档其它身份("文档类型:角色"),
   *  如 ["委托合同:委托人", "执行申请:申请人"]。主身份(role)取最权威诉讼地位。 */
  aliases?: string[];
}

export interface FeeRecord {
  item: string;
  amount: number | null;
  charged_at: string | null;
  receipt_no: string | null;
  note: string | null;
}

export interface KeyDate {
  event_type: string;
  date: string | null;
  note: string | null;
  /** 2026-05-24 k-9:保全 / 续封 / 上诉期 / 还款期等"有到期"事件的失效日期 */
  expires_at?: string | null;
}

export interface Preservation {
  target: string;
  amount: number | null;
  started_at: string | null;
  duration_years: number | null;
  expires_at: string | null;
}

/* ------------------------------------------------------------------ */
/* 后台字段抽取进度(对应 Rust pipeline::ProgressEvent)                */
/* ------------------------------------------------------------------ */

export type DocOutcome =
  | { kind: "extracted" }
  | { kind: "skipped"; reason: string }
  | { kind: "failed"; error: string };

export type ProgressEvent =
  | {
      stage: "started";
      case_id: string;
      total: number;
      ocr_provider: "local" | "cloud";
      llm_provider: "local" | "cloud";
      llm_model: string;
    }
  | {
      stage: "doc_started";
      case_id: string;
      doc_id: string;
      filename: string;
      index: number;
      total: number;
      ocr_provider: "local" | "cloud";
      llm_provider: "local" | "cloud";
    }
  | {
      /** 2026-06-14:单文档云端 OCR 轮询中的实时状态(治大图扫描件"看着卡死")。
       *  不进主进度线;前端作为附加子状态单独渲染(不动百分比),每 ~3 秒来一拍。 */
      stage: "doc_ocr_status";
      case_id: string;
      doc_id: string;
      filename: string;
      index: number;
      total: number;
      /** queued(排队)/ processing(识别中)/ converting(转换中) */
      phase: "queued" | "processing" | "converting";
      elapsed_secs: number;
      pages_done: number | null;
      pages_total: number | null;
    }
  | {
      stage: "doc_finished";
      case_id: string;
      doc_id: string;
      filename: string;
      index: number;
      total: number;
      /** 完成计数(单调递增,从 1 开始)。2026-05-24 i 加,用于并发场景下计算进度条 — 不要用 index 算 percent(顺序乱)。 */
      completed_count: number;
      outcome: DocOutcome;
    }
  | {
      /** 2026-06-11:逐文档完成后、全案 LLM 分析中(耗时几十秒~几分钟,浮层显示别让用户以为卡死) */
      stage: "analyzing";
      case_id: string;
    }
  | {
      stage: "completed";
      case_id: string;
      total: number;
      extracted: number;
      skipped: number;
      failed: number;
      elapsed_ms: number;
      /** 2026-06-11:全案 LLM 分析是否成功;false 时 agg 字段与详情页没有更新 */
      analysis_ok: boolean;
      analysis_error: string | null;
    }
  | { stage: "error"; case_id: string; error: string };

/** 单文档云端 OCR 轮询子状态(从 ProgressEvent 抽出,App.tsx 单独存一份 state 用)。 */
export type DocOcrStatusEvent = Extract<
  ProgressEvent,
  { stage: "doc_ocr_status" }
>;

/* ------------------------------------------------------------------ */
/* V0.8.1 记忆：默认归档、逐轮人工选择与确认                            */
/* ------------------------------------------------------------------ */

export type MemoryType =
  | "fact"
  | "procedure"
  | "strategy"
  | "client_instruction"
  | "risk_warning";
export type MemoryVerificationStatus =
  | "unverified"
  | "verified"
  | "disputed"
  | "stale";
export type MemoryInjectionMode = "archive_only" | "manual_each_turn";
export type MemoryStatus = "draft" | "active" | "disabled" | "deleted";

export interface MemorySourceInput {
  source_type:
    | "manual_assertion"
    | "document"
    | "chat_user"
    | "chat_assistant"
    | "tool_result"
    | "case_field";
  document_id?: string | null;
  chat_message_id?: string | null;
  locator?: string | null;
  excerpt?: string | null;
  external_ref?: string | null;
  verification_status?: MemoryVerificationStatus | null;
}

export interface CaseMemory {
  id: string;
  case_id: string;
  memory_type: MemoryType;
  status: MemoryStatus;
  verification_status: MemoryVerificationStatus;
  injection_mode: MemoryInjectionMode;
  current_revision_no: number;
  active_revision_no: number | null;
  title: string;
  content: string;
  revision_no: number;
  source_count: number;
  confirmed_by: string | null;
  confirmed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateMemoryInput {
  memory_type: MemoryType;
  title: string;
  content: string;
  verification_status?: MemoryVerificationStatus;
  injection_mode?: MemoryInjectionMode;
  change_reason?: string;
  source?: MemorySourceInput;
}

export interface ReviseMemoryInput {
  expected_revision: number;
  title: string;
  content: string;
  change_reason: string;
  source?: MemorySourceInput;
}

export interface MemoryCandidate {
  id: string;
  case_id: string;
  proposed_type: MemoryType;
  proposed_title: string;
  proposed_content: string;
  proposed_by_type: string;
  source_message_id: string | null;
  status: "pending" | "accepted" | "rejected" | "expired";
  decided_by: string | null;
  decided_at: string | null;
  decision_reason: string | null;
  accepted_memory_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface AcceptCandidateInput {
  title: string;
  content: string;
  memory_type: MemoryType;
  verification_status?: MemoryVerificationStatus;
  source?: MemorySourceInput;
}

export interface UserMemoryPreference {
  id: string;
  title: string;
  content: string;
  status: MemoryStatus;
  injection_mode: MemoryInjectionMode;
  current_revision_no: number;
  confirmed_by: string | null;
  confirmed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreatePreferenceInput {
  title: string;
  content: string;
  injection_mode?: MemoryInjectionMode;
}

export interface InjectionPreviewEntry {
  scope: string;
  id: string;
  revision_no: number;
  title: string;
  content: string;
  verification_status: MemoryVerificationStatus | null;
  char_count: number;
  selected: boolean;
  omitted_reason: string | null;
}

export interface MemoryInjectionPreview {
  id: string;
  case_id: string;
  task_type: string | null;
  entries: InjectionPreviewEntry[];
  case_used_chars: number;
  preference_used_chars: number;
  preview_sha256: string;
  prompt_markdown: string;
  status: string;
}

/* ------------------------------------------------------------------ */
/* V0.8.1 我的设备同步（NAS 挂载目录、端到端加密、双向同步）             */
/* ------------------------------------------------------------------ */

export interface DeviceSyncStatus {
  group_id: string;
  connector_root: string;
  local_device_id: string;
  key_epoch: number;
  paused: boolean;
  auto_paused: boolean;
  pause_reason_code: string | null;
  last_attempt_at: string | null;
  last_success_at: string | null;
  pending_upload: number;
  conflicts: number;
  quarantined: number;
  manual_review: number;
}

export interface DeviceSyncManualReview {
  id: string;
  group_id: string;
  reason_code: string;
  first_seen_at: string;
  last_seen_at: string;
  retry_count: number;
}

export interface DeviceSyncRunResult {
  exported_operations: number;
  imported_operations: number;
  conflicts_created: number;
  duplicate_operations: number;
  quarantined_packages: number;
}

export interface DeviceSyncMember {
  device_id: string;
  display_name: string;
  signing_public_key: string;
  exchange_public_key: string;
  fingerprint: string;
  key_epoch: number;
  status: string;
}

export interface DeviceSyncInvite {
  group_id: string;
  invite_id: string;
  pairing_code: string;
  expires_at: string;
}

export interface DeviceSyncJoinRequest {
  request_id: string;
  invite_id: string;
  group_id: string;
  device_id: string;
  display_name: string;
  signing_public_key: string;
  exchange_public_key: string;
  fingerprint: string;
  expires_at: string;
  proof_hash: string;
  request_signature: string;
}

export interface DeviceSyncJoinCompletion {
  group_id: string;
  device_id: string;
  key_epoch: number;
  trusted_member_count: number;
}

export interface DeviceSyncConflict {
  id: string;
  operation_id: string;
  group_id: string;
  entity_type: string;
  entity_id: string;
  case_id: string | null;
  field_key: string;
  local_value_json: string | null;
  remote_value_json: string | null;
  status: string;
  created_at: string;
}

export interface DeviceSyncSnapshot {
  snapshot_id: string;
  encrypted_path: string;
  manifest_hash: string;
  entity_counts: Record<string, number>;
}

export interface DeviceSyncRestorePreview {
  snapshot_id: string;
  entity_counts: Record<string, number>;
  new_entities: Record<string, number>;
  existing_entities: Record<string, number>;
  plaintext_sha256: string;
  formal_database_unchanged: boolean;
}

export interface DeviceSyncRecoveryPreview {
  group_id: string;
  latest_key_epoch: number;
  historical_key_epochs: number[];
  trusted_members: DeviceSyncMember[];
  formal_database_unchanged: boolean;
}

export interface DeviceSyncCreatedGroup {
  identity: {
    group_id: string;
    device_id: string;
    display_name: string;
    signing_public_key: string;
    exchange_public_key: string;
    fingerprint: string;
    key_epoch: number;
  };
  recovery: {
    path: string;
    group_id: string;
    key_epochs: number[];
  };
}

export interface YuandianBalanceView {
  point_balance: number;
  count_balance: number;
  fetched_at: string;
  cached: boolean;
  previous_point_balance: number | null;
  previous_fetched_at: string | null;
  official_spent_since_previous: number | null;
  local_recorded_since_previous: number | null;
  local_api_calls_since_previous: number | null;
  difference: number | null;
  balance_increased_since_previous: number | null;
  comparison_status: string;
  refresh_error_code: string | null;
  refresh_error: string | null;
}

export interface LegalSkillFile {
  relative_path: string;
  content: string;
}

export interface LegalSkillPackageRecord {
  id: string;
  slug: string;
  title: string;
  version: string;
  description: string;
  origin: "builtin" | "imported" | string;
  status: "enabled" | "disabled" | "quarantined" | string;
  manifest_json: string;
  package_content_json: string;
  content_hash: string;
  created_at: string;
  updated_at: string;
}

export interface LegalSkillRegistration {
  package: LegalSkillPackageRecord;
  created: boolean;
}

export interface LegalSkillRevisionRecord {
  id: string;
  skill_id: string;
  slug: string;
  version: string;
  content_hash: string;
  manifest_json: string;
  package_content_json: string;
  revision_action: string;
  created_at: string;
}

export interface LegalSkillVersionHistory {
  packages: LegalSkillPackageRecord[];
  revisions: LegalSkillRevisionRecord[];
}

export interface LegalSkillFileDiff {
  path: string;
  change: "added" | "removed" | "modified" | string;
  before: string | null;
  after: string | null;
}

export interface LegalSkillDiffPreview {
  slug: string;
  from_skill_id: string;
  from_version: string;
  from_hash: string;
  to_skill_id: string;
  to_version: string;
  to_hash: string;
  files: LegalSkillFileDiff[];
}

export interface LegalSkillArchiveExport {
  file_name: string;
  bytes: number[];
}

export interface LocalKbGuide {
  schema_version: number;
  mode: "read_only";
  configured_root: string | null;
  root_available: boolean;
  keyword_search: {
    scope: string;
    extensions: string[];
    max_file_bytes: number;
    excluded_root_prefixes: string[];
    excluded_segments: string[];
    sorting: string[];
  };
  semantic_search: {
    scope: string;
    requires_embedding_credential: boolean;
    requires_prebuilt_matching_index: boolean;
    query_builds_or_updates_index: boolean;
    mismatch_behavior: string;
  };
  file_read: {
    path_kind: string;
    canonical_root_boundary: boolean;
    max_file_bytes: number;
    rejects_binary_nul: boolean;
    default_max_chars: number;
  };
  maintenance_boundaries: string[];
  internal_ai_tools: string[];
}
