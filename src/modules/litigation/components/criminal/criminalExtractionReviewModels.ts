export type CriminalCandidateReviewStatus =
  | "pending"
  | "partially_confirmed"
  | "confirmed"
  | "rejected"
  | "superseded";

export type CriminalCandidateTechnicalStatus = "success" | "partial" | "failed";

export type CriminalCandidateFieldStatus =
  | "pending"
  | "accepted"
  | "rejected"
  | "protected";

export type CandidateDecision = "accept" | "reject" | "pending";

export interface CriminalExtractionCandidateFieldView {
  id: string;
  field_key: string;
  value_json: string;
  current_value_json?: string | null;
  source_document_id?: string | null;
  source_filename: string;
  evidence_excerpt?: string | null;
  confidence?: number | null;
  review_status: CriminalCandidateFieldStatus;
  decision_note?: string | null;
  is_user_protected?: boolean;
  protection_reason?: string | null;
  has_conflict?: boolean;
}

export interface CriminalExtractionCandidateBatchView {
  id: string;
  case_id: string;
  source_document_id?: string | null;
  source_filename: string;
  document_type?: string | null;
  technical_status: CriminalCandidateTechnicalStatus;
  review_status: CriminalCandidateReviewStatus;
  warning_json?: string | null;
  error_message?: string | null;
  created_at: string;
  updated_at: string;
  reviewed_at?: string | null;
  profile_revision: number;
  fields: CriminalExtractionCandidateFieldView[];
}

export const CRIMINAL_FIELD_LABELS: Record<string, string> = {
  stage: "阶段",
  charge: "涉嫌罪名",
  name: "姓名",
  confidence: "置信度",
  evidence: "证据摘录",
  event_type: "日期类型",
  date: "日期",
  current_stage: "当前阶段",
  procedure_type: "程序类型",
  suspected_charge: "涉嫌罪名",
  suspect_or_defendant_name: "犯罪嫌疑人/被告人",
  victim_name: "被害人",
  detention_center: "羁押场所",
  coercive_measure_type: "强制措施",
  guilty_plea_status: "认罪认罚",
  sentencing_recommendation: "量刑建议",
  sentence_term: "判决刑期",
  charge_history_json: "罪名变化",
  restitution_amount: "退赃退赔金额",
  restitution_status: "退赃退赔情况",
  victim_forgiveness: "被害人谅解",
  surrender_status: "自首情况",
  meritorious_service_status: "立功情况",
  co_defendants_json: "同案人员",
  detention_date: "拘留日期",
  arrest_request_date: "提请批准逮捕日期",
  arrest_review_received_date: "审查逮捕收案日期",
  arrest_decision_date: "逮捕决定日期",
  arrest_date: "逮捕日期",
  bail_start_date: "取保候审开始日期",
  residential_surveillance_start_date: "监视居住开始日期",
  transfer_for_prosecution_date: "移送审查起诉日期",
  prosecution_received_date: "审查起诉收案日期",
  first_instance_accepted_date: "一审受理日期",
  second_instance_accepted_date: "二审受理日期",
  judgment_received_date: "判决书收到日期",
  ruling_received_date: "裁定书收到日期",
  supplementary_investigation_1_date: "第一次退补日期",
  supplementary_investigation_2_date: "第二次退补日期",
  judgment_effective_date: "判决生效日期",
  death_penalty_review_start_date: "死刑复核开始日期",
};

export function criminalFieldLabel(fieldKey: string) {
  return CRIMINAL_FIELD_LABELS[fieldKey] ?? fieldKey.replace(/_/g, " ");
}

function parseValue(valueJson: string | null | undefined): unknown {
  if (valueJson == null || valueJson.trim() === "") return null;
  try {
    return JSON.parse(valueJson);
  } catch {
    return valueJson;
  }
}

function compactObject(value: Record<string, unknown>) {
  return Object.entries(value)
    .filter(([, item]) => item !== null && item !== undefined && item !== "")
    .map(([key, item]) => `${criminalFieldLabel(key)}：${formatCandidateValue(item)}`)
    .join("；");
}

export function formatCandidateValue(value: unknown): string {
  if (value === null || value === undefined || value === "") return "未填写";
  if (typeof value === "boolean") return value ? "是" : "否";
  if (typeof value === "number") return Number.isFinite(value) ? String(value) : "未填写";
  if (typeof value === "string") {
    const parsed = parseValue(value);
    return parsed === value ? value : formatCandidateValue(parsed);
  }
  if (Array.isArray(value)) {
    if (value.length === 0) return "未填写";
    return value.map((item) => formatCandidateValue(item)).join("；");
  }
  if (typeof value === "object") return compactObject(value as Record<string, unknown>) || "未填写";
  return String(value);
}

export function formatValueJson(valueJson: string | null | undefined) {
  return formatCandidateValue(parseValue(valueJson));
}

export function formatCandidateFieldValue(
  fieldKey: string,
  valueJson: string | null | undefined,
) {
  const value = parseValue(valueJson);
  if (fieldKey === "restitution_amount" && typeof value === "number") {
    return new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency: "CNY",
      maximumFractionDigits: 2,
    }).format(value);
  }
  return formatCandidateValue(value);
}

export function valuesAreEqual(
  currentValueJson: string | null | undefined,
  candidateValueJson: string | null | undefined,
) {
  return JSON.stringify(parseValue(currentValueJson)) === JSON.stringify(parseValue(candidateValueJson));
}

export function isEmptyValueJson(valueJson: string | null | undefined) {
  const value = parseValue(valueJson);
  return value == null || value === "" || (Array.isArray(value) && value.length === 0);
}

export function confidenceLabel(confidence: number | null | undefined) {
  if (confidence == null || !Number.isFinite(confidence)) return "置信度未知";
  const level = confidence >= 0.8 ? "高" : confidence >= 0.5 ? "中" : "低";
  return `${level} ${Math.round(confidence * 100)}%`;
}

export function shouldDefaultAccept(field: CriminalExtractionCandidateFieldView) {
  if (field.review_status !== "pending") return false;
  if (field.is_user_protected || field.has_conflict) return false;
  if (!isEmptyValueJson(field.current_value_json)) return false;
  return (field.confidence ?? 0) >= 0.5;
}

export function candidateBatchStatusLabel(
  reviewStatus: CriminalCandidateReviewStatus,
  technicalStatus: CriminalCandidateTechnicalStatus,
) {
  if (technicalStatus === "failed") return "识别失败";
  if (technicalStatus === "partial") return "部分识别失败";
  const labels: Record<CriminalCandidateReviewStatus, string> = {
    pending: "待确认",
    partially_confirmed: "部分已确认",
    confirmed: "已确认",
    rejected: "已拒绝",
    superseded: "已被新结果替代",
  };
  return labels[reviewStatus];
}

export function parseTechnicalWarnings(warningJson: string | null | undefined) {
  const value = parseValue(warningJson);
  if (value == null) return [];
  if (Array.isArray(value)) return value.map(formatCandidateValue).filter(Boolean);
  return [formatCandidateValue(value)];
}

export function parseProtectedFieldKeys(userOverridesJson: string | null | undefined) {
  if (!userOverridesJson) return { keys: new Set<string>(), corrupt: false };
  try {
    const parsed = JSON.parse(userOverridesJson) as { fields?: unknown };
    if (!parsed || typeof parsed !== "object" || !parsed.fields || typeof parsed.fields !== "object" || Array.isArray(parsed.fields)) {
      return { keys: new Set<string>(), corrupt: true };
    }
    return {
      keys: new Set(Object.keys(parsed.fields as Record<string, unknown>)),
      corrupt: false,
    };
  } catch {
    return { keys: new Set<string>(), corrupt: true };
  }
}
