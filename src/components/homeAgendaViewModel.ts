import type { CriminalDeadlineCalendarRow, CriminalTaskSummaryRow } from "@/lib/types";

export type HomeAgendaKind =
  | "hearing"
  | "deadline"
  | "sop"
  | "todo"
  | "manual"
  | "feishu";

export type CriminalHomeBucket =
  | "overdue"
  | "today"
  | "next_seven_days"
  | "pending_confirmation"
  | "unscheduled"
  | "pending_feedback"
  | "later";

export type CriminalHomeStats = Record<Exclude<CriminalHomeBucket, "later">, number>;

export interface HomeAgendaEvent {
  kind: Exclude<HomeAgendaKind, "feishu">;
  date: string;
  daysFromNow: number;
  type: string;
  note?: string | null;
  caseName: string;
  caseId: string;
  court?: string | null;
  id?: string;
}

export const AGENDA_SOURCE_META: Record<HomeAgendaKind, { label: string; dotClass: string; iconClass: string }> = {
  hearing: { label: "庭审", dotClass: "bg-rose-500", iconClass: "text-rose-600" },
  deadline: { label: "法定期限", dotClass: "bg-orange-500", iconClass: "text-orange-600" },
  sop: { label: "SOP 任务", dotClass: "bg-cyan-500", iconClass: "text-cyan-600" },
  todo: { label: "普通待办", dotClass: "bg-violet-500", iconClass: "text-violet-600" },
  manual: { label: "手工日程", dotClass: "bg-slate-400", iconClass: "text-slate-500" },
  feishu: { label: "飞书事件", dotClass: "bg-blue-500", iconClass: "text-blue-600" },
};

function localDayStart(value: Date): Date {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}

function isFeedbackSummary(row: CriminalTaskSummaryRow): boolean {
  return row.client_feedback_required
    || row.task_type === "client_feedback"
    || row.title.includes("反馈");
}

export function classifyCriminalSummaryTask(
  row: CriminalTaskSummaryRow,
  now: Date = new Date(),
): CriminalHomeBucket {
  if (["completed", "ignored", "not_applicable"].includes(row.status)) return "later";
  if (row.status === "pending_confirmation" || row.applicability_status === "pending_confirmation") {
    return "pending_confirmation";
  }
  if (!row.planned_at) return isFeedbackSummary(row) ? "pending_feedback" : "unscheduled";

  const planned = new Date(row.planned_at);
  if (Number.isNaN(planned.getTime())) return "unscheduled";
  const days = Math.round(
    (localDayStart(planned).getTime() - localDayStart(now).getTime()) / 86_400_000,
  );
  if (days < 0) return "overdue";
  if (days === 0) return "today";
  if (days <= 7) return "next_seven_days";
  return isFeedbackSummary(row) ? "pending_feedback" : "later";
}

export function summarizeCriminalTaskRows(
  rows: CriminalTaskSummaryRow[],
  now: Date = new Date(),
): CriminalHomeStats {
  const result: CriminalHomeStats = {
    overdue: 0,
    today: 0,
    next_seven_days: 0,
    pending_confirmation: 0,
    unscheduled: 0,
    pending_feedback: 0,
  };
  for (const row of rows) {
    const bucket = classifyCriminalSummaryTask(row, now);
    if (bucket !== "later") result[bucket] += 1;
  }
  return result;
}

export function dateKeyFromTimestamp(value: string): string | null {
  const direct = /^(\d{4}-\d{2}-\d{2})/.exec(value)?.[1];
  if (direct) {
    const [year, month, day] = direct.split("-").map(Number);
    const validated = new Date(year, month - 1, day);
    if (
      validated.getFullYear() === year
      && validated.getMonth() === month - 1
      && validated.getDate() === day
    ) return direct;
    return null;
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return null;
  const year = parsed.getFullYear();
  const month = String(parsed.getMonth() + 1).padStart(2, "0");
  const day = String(parsed.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function criminalSummaryRowsToAgenda(
  rows: CriminalTaskSummaryRow[],
  now: Date = new Date(),
): HomeAgendaEvent[] {
  const today = localDayStart(now);
  return rows.flatMap((row) => {
    if (!row.planned_at || ["completed", "ignored", "not_applicable"].includes(row.status)) return [];
    const date = dateKeyFromTimestamp(row.planned_at);
    if (!date) return [];
    const parsed = new Date(`${date}T00:00:00`);
    return [{
      kind: "sop" as const,
      date,
      daysFromNow: Math.round((parsed.getTime() - today.getTime()) / 86_400_000),
      type: row.title,
      note: row.stage_code,
      caseName: row.case_name,
      caseId: row.case_id,
      id: row.task_id,
    }];
  }).sort((left, right) => left.date.localeCompare(right.date));
}

export function criminalDeadlineRowsToAgenda(
  rows: CriminalDeadlineCalendarRow[],
  now: Date = new Date(),
): HomeAgendaEvent[] {
  const today = localDayStart(now);
  return rows.flatMap((row) => {
    const date = dateKeyFromTimestamp(row.deadline_at);
    if (!date) return [];
    const parsed = new Date(`${date}T00:00:00`);
    return [{
      kind: "deadline" as const,
      date,
      daysFromNow: Math.round((parsed.getTime() - today.getTime()) / 86_400_000),
      type: row.title,
      note: row.rule_code,
      caseName: row.case_name,
      caseId: row.case_id,
      id: row.deadline_id,
    }];
  }).sort((left, right) => left.date.localeCompare(right.date));
}

export function agendaDotClass(kind: HomeAgendaKind): string {
  return AGENDA_SOURCE_META[kind].dotClass;
}

export function dedupeAgendaEvents<T extends HomeAgendaEvent>(events: T[]): T[] {
  const output: T[] = [];
  const identifiedDeadlineIndexById = new Map<string, number>();
  const identifiedDeadlineIndexByKey = new Map<string, number>();
  const legacyDeadlineIndexByKey = new Map<string, number>();
  for (const event of events) {
    if (event.kind !== "deadline") {
      output.push(event);
      continue;
    }
    const key = [event.kind, event.caseId, event.date, event.type].join("\u0000");
    if (event.id) {
      if (identifiedDeadlineIndexById.has(event.id)) continue;
      const legacyIndex = legacyDeadlineIndexByKey.get(key);
      if (legacyIndex !== undefined) {
        output[legacyIndex] = event;
        legacyDeadlineIndexByKey.delete(key);
        identifiedDeadlineIndexById.set(event.id, legacyIndex);
        identifiedDeadlineIndexByKey.set(key, legacyIndex);
        continue;
      }
      identifiedDeadlineIndexById.set(event.id, output.length);
      identifiedDeadlineIndexByKey.set(key, output.length);
      output.push(event);
      continue;
    }
    if (identifiedDeadlineIndexByKey.has(key)) continue;
    if (legacyDeadlineIndexByKey.has(key)) continue;
    legacyDeadlineIndexByKey.set(key, output.length);
    output.push(event);
  }
  return output;
}
