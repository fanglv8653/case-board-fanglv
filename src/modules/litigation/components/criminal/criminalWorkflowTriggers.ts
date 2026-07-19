import type {
  CriminalWorkflowConfirmedEvent,
} from "@/lib/types";
import type { CriminalExtractionCandidateBatchView } from "./criminalExtractionReviewModels";

export interface CriminalWorkflowTriggerFields {
  detention_date?: string | null;
  arrest_review_received_date?: string | null;
  arrest_date?: string | null;
  transfer_for_prosecution_date?: string | null;
  prosecution_received_date?: string | null;
  first_instance_accepted_date?: string | null;
  judgment_received_date?: string | null;
  second_instance_accepted_date?: string | null;
  guilty_plea_status?: string | null;
}

export interface CriminalWorkflowTriggerEvent {
  eventCode: CriminalWorkflowConfirmedEvent;
  eventId: string;
  fieldKeys: string[];
  normalizedValue: string;
}

const AFFIRMATIVE_PLEA_VALUES = new Set([
  "是",
  "已确认",
  "已认罪认罚",
  "认罪认罚",
  "同意认罪认罚",
  "已签署具结书",
  "已签署认罪认罚具结书",
]);

function normalizeDate(value: string | null | undefined): string | null {
  const normalized = value?.trim();
  if (!normalized) return null;
  const datePrefix = normalized.match(/^(\d{4})[-/]?(\d{2})[-/]?(\d{2})/)?.slice(1);
  if (!datePrefix) return null;
  const [year, month, day] = datePrefix;
  const date = new Date(`${year}-${month}-${day}T00:00:00Z`);
  if (
    Number.isNaN(date.getTime())
    || date.getUTCFullYear() !== Number(year)
    || date.getUTCMonth() + 1 !== Number(month)
    || date.getUTCDate() !== Number(day)
  ) return null;
  return `${year}-${month}-${day}`;
}

function normalizePlea(value: string | null | undefined): string | null {
  const normalized = value?.trim().replace(/\s+/g, "");
  return normalized && AFFIRMATIVE_PLEA_VALUES.has(normalized) ? normalized : null;
}

export function stableCriminalWorkflowEventId(
  caseId: string,
  eventCode: CriminalWorkflowConfirmedEvent,
  normalizedValue: string,
): string {
  return `${caseId}:${eventCode}:${encodeURIComponent(normalizedValue)}`;
}

export function buildCriminalWorkflowTriggerEvents(
  caseId: string,
  fields: CriminalWorkflowTriggerFields,
  onlyFields?: ReadonlySet<string>,
): CriminalWorkflowTriggerEvent[] {
  const events: CriminalWorkflowTriggerEvent[] = [];
  const enabled = (...keys: string[]) => !onlyFields || keys.some((key) => onlyFields.has(key));
  const push = (
    eventCode: CriminalWorkflowConfirmedEvent,
    normalizedValue: string | null,
    fieldKeys: string[],
  ) => {
    if (!normalizedValue || !enabled(...fieldKeys)) return;
    events.push({
      eventCode,
      eventId: stableCriminalWorkflowEventId(caseId, eventCode, normalizedValue),
      fieldKeys,
      normalizedValue,
    });
  };

  push("detention_confirmed", normalizeDate(fields.detention_date), ["detention_date"]);
  push(
    "arrest_review_request_confirmed",
    normalizeDate(fields.arrest_review_received_date),
    ["arrest_review_received_date"],
  );
  push("arrest_confirmed", normalizeDate(fields.arrest_date), ["arrest_date"]);

  const transferDate = normalizeDate(fields.transfer_for_prosecution_date);
  const receivedDate = normalizeDate(fields.prosecution_received_date);
  const prosecutionField = onlyFields?.has("transfer_for_prosecution_date")
    ? "transfer_for_prosecution_date"
    : onlyFields?.has("prosecution_received_date")
      ? "prosecution_received_date"
      : transferDate
        ? "transfer_for_prosecution_date"
        : "prosecution_received_date";
  push(
    "prosecution_transfer_confirmed",
    prosecutionField === "transfer_for_prosecution_date" ? transferDate : receivedDate,
    [prosecutionField],
  );

  push(
    "court_acceptance_confirmed",
    normalizeDate(fields.first_instance_accepted_date),
    ["first_instance_accepted_date"],
  );
  push(
    "first_instance_judgment_received",
    normalizeDate(fields.judgment_received_date),
    ["judgment_received_date"],
  );
  push(
    "second_instance_procedure_confirmed",
    normalizeDate(fields.second_instance_accepted_date),
    ["second_instance_accepted_date"],
  );
  push(
    "plea_process_confirmed",
    normalizePlea(fields.guilty_plea_status),
    ["guilty_plea_status"],
  );
  return events;
}

function parseCandidateValue(valueJson: string): string | null {
  try {
    const value: unknown = JSON.parse(valueJson);
    return typeof value === "string" ? value : value == null ? null : String(value);
  } catch {
    return valueJson;
  }
}

export function appliedCandidateTriggerFields(
  batch: Pick<CriminalExtractionCandidateBatchView, "fields"> | null | undefined,
  appliedFields: readonly string[],
): CriminalWorkflowTriggerFields {
  if (!batch || appliedFields.length === 0) return {};
  const applied = new Set(appliedFields);
  const values: Record<string, string | null> = {};
  for (const field of batch.fields) {
    if (!applied.has(field.field_key)) continue;
    values[field.field_key] = parseCandidateValue(field.value_json);
  }
  return values as CriminalWorkflowTriggerFields;
}
