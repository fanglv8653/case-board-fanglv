import type {
  CriminalTaskAction,
  CriminalWorkflowTask,
  CriminalWorkflowTaskStatus,
} from "@/lib/types";

export type CriminalTaskBucket =
  | "overdue"
  | "today"
  | "next_seven_days"
  | "pending_confirmation"
  | "unscheduled"
  | "pending_feedback"
  | "later";

export interface CriminalTaskBucketGroup {
  key: Exclude<CriminalTaskBucket, "later">;
  label: string;
  tasks: CriminalWorkflowTask[];
}

const HIDDEN_FROM_NOW_STATUSES: CriminalWorkflowTaskStatus[] = [
  "completed",
  "ignored",
  "not_applicable",
];

const STAGE_LABELS: Record<string, string> = {
  engagement_intake: "收案委托",
  detention_arrest_review: "刑拘及审查逮捕",
  post_arrest_investigation: "逮捕后侦查",
  prosecution_review: "审查起诉",
  first_instance: "一审",
  appeal_second_instance: "上诉及二审",
  current: "全案通用",
};

export const TASK_BUCKET_LABELS: Record<Exclude<CriminalTaskBucket, "later">, string> = {
  overdue: "逾期",
  today: "今日",
  next_seven_days: "7 日内",
  pending_confirmation: "待确认",
  unscheduled: "待排期",
  pending_feedback: "待反馈",
};

function localDayStart(value: Date): Date {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}

export function isFeedbackTask(task: Pick<CriminalWorkflowTask, "node_code" | "task_type" | "title">): boolean {
  return task.node_code === "common_client_feedback"
    || task.task_type === "client_feedback"
    || task.title.includes("反馈");
}

export function classifyCriminalTask(
  task: CriminalWorkflowTask,
  now: Date = new Date(),
): CriminalTaskBucket {
  if (HIDDEN_FROM_NOW_STATUSES.includes(task.status)) return "later";
  if (task.status === "pending_confirmation" || task.applicability_status === "pending_confirmation") {
    return "pending_confirmation";
  }
  if (!task.planned_at) {
    return isFeedbackTask(task) ? "pending_feedback" : "unscheduled";
  }

  const planned = new Date(task.planned_at);
  if (Number.isNaN(planned.getTime())) return "unscheduled";
  const today = localDayStart(now);
  const plannedDay = localDayStart(planned);
  const days = Math.round((plannedDay.getTime() - today.getTime()) / 86_400_000);
  if (days < 0) return "overdue";
  if (days === 0) return "today";
  if (days <= 7) return "next_seven_days";
  return isFeedbackTask(task) ? "pending_feedback" : "later";
}

export function bucketCriminalTasks(
  tasks: CriminalWorkflowTask[],
  now: Date = new Date(),
): CriminalTaskBucketGroup[] {
  const order: CriminalTaskBucketGroup["key"][] = [
    "overdue",
    "today",
    "next_seven_days",
    "pending_confirmation",
    "unscheduled",
    "pending_feedback",
  ];
  const buckets = new Map(order.map((key) => [key, [] as CriminalWorkflowTask[]]));
  for (const task of tasks) {
    const bucket = classifyCriminalTask(task, now);
    if (bucket !== "later") buckets.get(bucket)?.push(task);
  }
  return order.map((key) => ({ key, label: TASK_BUCKET_LABELS[key], tasks: buckets.get(key) ?? [] }));
}

export interface CriminalTaskStageGroup {
  stageCode: string;
  stageLabel: string;
  stageSort: number;
  tasks: CriminalWorkflowTask[];
}

export function groupCriminalTasksByStage(tasks: CriminalWorkflowTask[]): CriminalTaskStageGroup[] {
  const grouped = new Map<string, CriminalWorkflowTask[]>();
  for (const task of tasks) {
    const rows = grouped.get(task.stage_code) ?? [];
    rows.push(task);
    grouped.set(task.stage_code, rows);
  }
  return [...grouped.entries()]
    .map(([stageCode, rows]) => ({
      stageCode,
      stageLabel: STAGE_LABELS[stageCode] ?? stageCode,
      stageSort: Math.min(...rows.map((task) => task.stage_sort)),
      tasks: [...rows].sort((left, right) => left.node_sort - right.node_sort || left.occurrence_no - right.occurrence_no),
    }))
    .sort((left, right) => left.stageSort - right.stageSort);
}

export function allowedCriminalTaskActions(task: CriminalWorkflowTask): CriminalTaskAction[] {
  if (task.status === "pending_confirmation") return ["confirm_applicable", "not_applicable"];
  if (task.status === "completed" || task.status === "ignored") return ["reopen"];
  if (task.status === "not_applicable") return [];

  const actions: CriminalTaskAction[] = [];
  if (["unscheduled", "pending", "in_progress", "deferred", "reopened"].includes(task.status)) {
    actions.push("schedule");
  }
  if (["unscheduled", "pending", "deferred", "reopened"].includes(task.status)) actions.push("start");
  if (["unscheduled", "pending", "in_progress", "deferred", "reopened"].includes(task.status)) {
    actions.push("defer", "complete", "ignore");
  }
  return actions;
}

export function statusLabel(status: CriminalWorkflowTaskStatus): string {
  return {
    pending_confirmation: "待确认",
    unscheduled: "待排期",
    pending: "待办理",
    in_progress: "办理中",
    completed: "已完成",
    deferred: "已延期",
    ignored: "已忽略",
    reopened: "已重开",
    not_applicable: "不适用",
  }[status];
}

export function stageLabel(stageCode: string): string {
  return STAGE_LABELS[stageCode] ?? stageCode;
}
