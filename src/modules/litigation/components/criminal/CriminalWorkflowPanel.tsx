import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { CheckCircle2, Loader2, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import {
  applyCriminalTaskAction,
  getCriminalWorkflow,
  listCriminalWorkflowTasks,
  refreshCriminalWorkflow,
} from "@/lib/api";
import type {
  CaseWorkItem,
  CriminalDeadlineItem,
  CriminalTaskAction,
  CriminalWorkflow,
  CriminalWorkflowTask,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  allowedCriminalTaskActions,
  bucketCriminalTasks,
  groupCriminalTasksByStage,
  stageLabel,
  statusLabel,
} from "./criminalWorkflowViewModel";

type WorkflowTab = "now" | "flow" | "deadlines" | "work";

const TABS: Array<{ key: WorkflowTab; label: string }> = [
  { key: "now", label: "现在要做" },
  { key: "flow", label: "全流程" },
  { key: "deadlines", label: "法定期限" },
  { key: "work", label: "工作记录" },
];

const ACTION_LABELS: Record<CriminalTaskAction, string> = {
  confirm_applicable: "确认适用",
  not_applicable: "不适用",
  schedule: "排期",
  start: "开始",
  defer: "延期",
  complete: "完成",
  ignore: "忽略",
  reopen: "重开",
};

interface ActionDraft {
  task: CriminalWorkflowTask;
  action: CriminalTaskAction;
  plannedAt: string;
  result: string;
  nextAction: string;
  durationMinutes: string;
  reason: string;
  clientFeedbackRecorded: boolean;
}

export function CriminalWorkflowPanel({
  caseId,
  deadlines,
  workItems,
  onChanged,
}: {
  caseId: string;
  deadlines: CriminalDeadlineItem[];
  workItems: CaseWorkItem[];
  onChanged?: () => void | Promise<void>;
}) {
  const [tab, setTab] = useState<WorkflowTab>("now");
  const [workflow, setWorkflow] = useState<CriminalWorkflow | null>(null);
  const [tasks, setTasks] = useState<CriminalWorkflowTask[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyTaskId, setBusyTaskId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<ActionDraft | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      let workflowRow = await getCriminalWorkflow(caseId);
      if (!workflowRow) {
        await refreshCriminalWorkflow({
          case_id: caseId,
          event_code: "case_created",
          event_id: `case_created:${caseId}`,
          confirmed_by: "system",
          source_type: "manual_confirmed",
        });
        workflowRow = await getCriminalWorkflow(caseId);
      }
      const taskRows = await listCriminalWorkflowTasks({ case_id: caseId });
      setWorkflow(workflowRow);
      setTasks(taskRows);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, [caseId]);

  useEffect(() => {
    void load();
  }, [load]);

  const buckets = useMemo(() => bucketCriminalTasks(tasks), [tasks]);
  const stages = useMemo(() => groupCriminalTasksByStage(tasks), [tasks]);

  const openAction = (task: CriminalWorkflowTask, action: CriminalTaskAction) => {
    if (action === "start" || action === "reopen") {
      void submitAction({
        task,
        action,
        plannedAt: "",
        result: "",
        nextAction: "",
        durationMinutes: "",
        reason: "",
        clientFeedbackRecorded: false,
      });
      return;
    }
    setDraft({
      task,
      action,
      plannedAt: toLocalDateTime(task.planned_at),
      result: "",
      nextAction: "",
      durationMinutes: task.duration_minutes == null ? "" : String(task.duration_minutes),
      reason: "",
      clientFeedbackRecorded: task.client_feedback_recorded,
    });
  };

  const submitAction = async (value: ActionDraft) => {
    if (["schedule", "defer"].includes(value.action) && !value.plannedAt) {
      toast("请填写计划时间", "error");
      return;
    }
    if (["not_applicable", "defer", "ignore"].includes(value.action) && !value.reason.trim()) {
      toast("请填写原因", "error");
      return;
    }
    if (value.action === "complete" && !value.result.trim()) {
      toast("完成任务必须填写简要办理结果", "error");
      return;
    }
    const duration = value.durationMinutes.trim() === "" ? null : Number(value.durationMinutes);
    if (duration != null && (!Number.isInteger(duration) || duration < 0)) {
      toast("办理时长须为非负整数分钟", "error");
      return;
    }

    setBusyTaskId(value.task.id);
    try {
      await applyCriminalTaskAction({
        task_id: value.task.id,
        action: value.action,
        actor: "local_user",
        planned_at: value.plannedAt || null,
        result: value.result.trim() || null,
        next_action: value.nextAction.trim() || null,
        duration_minutes: duration,
        reason: value.reason.trim() || null,
        client_feedback_recorded: value.clientFeedbackRecorded,
      });
      toast(`任务已${ACTION_LABELS[value.action]}`, "success");
      setDraft(null);
      await load();
      await onChanged?.();
    } catch (cause) {
      toast(`任务操作失败：${cause}`, "error");
    } finally {
      setBusyTaskId(null);
    }
  };

  return (
    <section className="rounded-xl border border-border bg-card">
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border px-4 py-4">
        <div>
          <h3 className="font-semibold text-foreground">刑事辩护办案流程</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            SOP 工作任务与法定期限分别管理；条件任务须经律师确认后办理。
          </p>
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={() => void load()} disabled={loading}>
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
          刷新流程
        </Button>
      </div>

      <div className="flex overflow-x-auto border-b border-border px-2" role="tablist" aria-label="刑事办案流程视图">
        {TABS.map((item) => (
          <button
            key={item.key}
            type="button"
            role="tab"
            aria-selected={tab === item.key}
            onClick={() => setTab(item.key)}
            className={cn(
              "shrink-0 border-b-2 px-4 py-3 text-sm transition-colors",
              tab === item.key
                ? "border-foreground font-medium text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>

      <div className="p-4">
        {loading ? (
          <EmptyState><Loader2 className="size-4 animate-spin" />正在加载办案流程</EmptyState>
        ) : error ? (
          <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            办案流程加载失败：{error}
          </div>
        ) : tab === "now" ? (
          <NowView buckets={buckets} busyTaskId={busyTaskId} onAction={openAction} />
        ) : tab === "flow" ? (
          <FlowView workflow={workflow} stages={stages} busyTaskId={busyTaskId} onAction={openAction} />
        ) : tab === "deadlines" ? (
          <DeadlinesView deadlines={deadlines} />
        ) : (
          <WorkItemsView workItems={workItems} />
        )}

        {draft && (
          <ActionEditor
            draft={draft}
            busy={busyTaskId === draft.task.id}
            onChange={setDraft}
            onCancel={() => setDraft(null)}
            onSubmit={() => void submitAction(draft)}
          />
        )}
      </div>
    </section>
  );
}

function NowView({
  buckets,
  busyTaskId,
  onAction,
}: {
  buckets: ReturnType<typeof bucketCriminalTasks>;
  busyTaskId: string | null;
  onAction: (task: CriminalWorkflowTask, action: CriminalTaskAction) => void;
}) {
  const visible = buckets.filter((bucket) => bucket.tasks.length > 0);
  if (visible.length === 0) return <EmptyState><CheckCircle2 className="size-4" />当前没有待处理流程任务</EmptyState>;
  return (
    <div className="space-y-4">
      {visible.map((bucket) => (
        <div key={bucket.key}>
          <div className="mb-2 flex items-center gap-2">
            <h4 className="text-sm font-semibold text-foreground">{bucket.label}</h4>
            <span className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">{bucket.tasks.length}</span>
          </div>
          <div className="space-y-2">
            {bucket.tasks.map((task) => <TaskCard key={task.id} task={task} busy={busyTaskId === task.id} onAction={onAction} />)}
          </div>
        </div>
      ))}
    </div>
  );
}

function FlowView({
  workflow,
  stages,
  busyTaskId,
  onAction,
}: {
  workflow: CriminalWorkflow | null;
  stages: ReturnType<typeof groupCriminalTasksByStage>;
  busyTaskId: string | null;
  onAction: (task: CriminalWorkflowTask, action: CriminalTaskAction) => void;
}) {
  if (!workflow || stages.length === 0) {
    return <EmptyState>尚未生成刑事辩护流程。请在确认案件程序事件后刷新。</EmptyState>;
  }
  return (
    <div className="space-y-4">
      <p className="text-xs text-muted-foreground">
        流程状态：{workflow.status === "active" ? "进行中" : "已关闭"}
        {workflow.current_stage_code ? ` · 当前阶段 ${stageLabel(workflow.current_stage_code)}` : ""}
      </p>
      {stages.map((stage) => (
        <div key={stage.stageCode} className="rounded-lg border border-border bg-background p-3">
          <div className="mb-3 flex items-center justify-between gap-2">
            <h4 className="text-sm font-semibold">{stage.stageLabel}</h4>
            <span className="text-xs text-muted-foreground">
              {stage.tasks.filter((task) => task.status === "completed").length}/{stage.tasks.length} 已完成
            </span>
          </div>
          <div className="space-y-2">
            {stage.tasks.map((task) => <TaskCard key={task.id} task={task} busy={busyTaskId === task.id} onAction={onAction} compact />)}
          </div>
        </div>
      ))}
    </div>
  );
}

function TaskCard({
  task,
  busy,
  onAction,
  compact = false,
}: {
  task: CriminalWorkflowTask;
  busy: boolean;
  onAction: (task: CriminalWorkflowTask, action: CriminalTaskAction) => void;
  compact?: boolean;
}) {
  const actions = allowedCriminalTaskActions(task);
  return (
    <div className={cn("rounded-lg border border-border bg-background", compact ? "p-2.5" : "p-3")}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <p className="font-medium text-foreground">{task.title}{task.occurrence_no > 1 ? `（第 ${task.occurrence_no} 次）` : ""}</p>
            <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">{statusLabel(task.status)}</span>
            {task.time_nature === "internal_service_target" && (
              <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-800 dark:text-amber-200">内部服务目标</span>
            )}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {stageLabel(task.stage_code)}
            {task.planned_at ? ` · 计划 ${formatDateTime(task.planned_at)}` : " · 尚未排期"}
            {task.deadline_item_id ? " · 已关联法定期限" : ""}
          </p>
          {task.result && <p className="mt-1 text-xs text-muted-foreground">结果：{task.result}</p>}
        </div>
        {actions.length > 0 && (
          <div className="flex flex-wrap justify-end gap-1">
            {actions.map((action) => (
              <Button key={action} type="button" variant="ghost" size="sm" disabled={busy} onClick={() => onAction(task, action)}>
                {busy && <Loader2 className="size-3.5 animate-spin" />}{ACTION_LABELS[action]}
              </Button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function DeadlinesView({ deadlines }: { deadlines: CriminalDeadlineItem[] }) {
  if (deadlines.length === 0) return <EmptyState>暂无法定期限。期限规则须由案件事实触发并人工核对适用性。</EmptyState>;
  return (
    <div className="space-y-2">
      <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-900 dark:text-amber-100">
        法定期限与内部 SOP 任务分别管理；人工修正不会被流程刷新覆盖。
      </div>
      {deadlines.map((item) => (
        <div key={item.id} className="rounded-lg border border-border bg-background p-3">
          <div className="flex flex-wrap items-start justify-between gap-2">
            <p className="font-medium text-foreground">{item.title}</p>
            <span className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">{deadlineApplicabilityLabel(item.applicability_status)}</span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {item.effective_due_at ? `到期 ${formatDateTime(item.effective_due_at)}` : "尚无有效到期时间"}
            {item.manual_due_at ? " · 已人工修正" : ""}
            {item.source_law ? ` · ${item.source_law}${item.source_article ? ` ${item.source_article}` : ""}` : ""}
          </p>
          {item.calculation_note && <p className="mt-1 text-xs text-muted-foreground">计算说明：{item.calculation_note}</p>}
          {item.override_reason && <p className="mt-1 text-xs text-muted-foreground">修正原因：{item.override_reason}</p>}
        </div>
      ))}
    </div>
  );
}

function WorkItemsView({ workItems }: { workItems: CaseWorkItem[] }) {
  if (workItems.length === 0) return <EmptyState>暂无工作记录。完成要求留痕的 SOP 任务后将自动生成记录。</EmptyState>;
  return (
    <div className="space-y-2">
      {workItems.map((item) => (
        <div key={item.id} className="rounded-lg border border-border bg-background p-3">
          <div className="flex flex-wrap items-start justify-between gap-2">
            <p className="font-medium text-foreground">{item.title}</p>
            <span className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
              {item.confirmation_status === "pending" ? "待确认（不计工时）" : "已确认"}
            </span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {formatDateTime(item.occurred_at)} · {item.work_type}
            {item.duration_minutes ? ` · ${item.duration_minutes} 分钟` : ""}
            {item.external_source === "criminal_sop" ? " · SOP 自动留痕" : ""}
          </p>
          {item.content && <p className="mt-1 text-xs text-muted-foreground">{item.content}</p>}
        </div>
      ))}
    </div>
  );
}

function ActionEditor({
  draft,
  busy,
  onChange,
  onCancel,
  onSubmit,
}: {
  draft: ActionDraft;
  busy: boolean;
  onChange: (draft: ActionDraft) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  const needsDate = ["confirm_applicable", "schedule", "defer"].includes(draft.action);
  const needsReason = ["not_applicable", "defer", "ignore"].includes(draft.action);
  return (
    <div className="mt-4 rounded-lg border border-primary/30 bg-muted/30 p-3">
      <p className="text-sm font-semibold">{ACTION_LABELS[draft.action]}：{draft.task.title}</p>
      <div className="mt-3 grid gap-3 md:grid-cols-2">
        {needsDate && (
          <Field label={draft.action === "confirm_applicable" ? "计划时间（可选）" : "计划时间"}>
            <input type="datetime-local" value={draft.plannedAt} onChange={(event) => onChange({ ...draft, plannedAt: event.currentTarget.value })} className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm" />
          </Field>
        )}
        {needsReason && (
          <Field label="原因">
            <input value={draft.reason} onChange={(event) => onChange({ ...draft, reason: event.currentTarget.value })} className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm" />
          </Field>
        )}
        {draft.action === "complete" && (
          <>
            <Field label="简要办理结果" className="md:col-span-2">
              <textarea rows={3} value={draft.result} onChange={(event) => onChange({ ...draft, result: event.currentTarget.value })} className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm" />
            </Field>
            <Field label="下一步安排">
              <input value={draft.nextAction} onChange={(event) => onChange({ ...draft, nextAction: event.currentTarget.value })} className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm" />
            </Field>
            <Field label="办理时长（分钟）">
              <input inputMode="numeric" value={draft.durationMinutes} onChange={(event) => onChange({ ...draft, durationMinutes: event.currentTarget.value })} className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm" />
            </Field>
            <label className="flex items-center gap-2 text-sm md:col-span-2">
              <input type="checkbox" checked={draft.clientFeedbackRecorded} onChange={(event) => onChange({ ...draft, clientFeedbackRecorded: event.currentTarget.checked })} />
              已向委托人或家属反馈并完成留痕
            </label>
          </>
        )}
      </div>
      <div className="mt-3 flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={busy}>取消</Button>
        <Button type="button" onClick={onSubmit} disabled={busy}>{busy && <Loader2 className="size-3.5 animate-spin" />}确认{ACTION_LABELS[draft.action]}</Button>
      </div>
    </div>
  );
}

function EmptyState({ children }: { children: ReactNode }) {
  return <div className="flex min-h-20 items-center justify-center gap-2 rounded-lg border border-dashed border-border px-4 py-6 text-sm text-muted-foreground">{children}</div>;
}

function Field({ label, className, children }: { label: string; className?: string; children: ReactNode }) {
  return <label className={cn("block space-y-1 text-xs text-muted-foreground", className)}><span>{label}</span>{children}</label>;
}

function toLocalDateTime(value: string | null): string {
  if (!value) return "";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value.slice(0, 16);
  const offset = parsed.getTimezoneOffset() * 60_000;
  return new Date(parsed.getTime() - offset).toISOString().slice(0, 16);
}

function formatDateTime(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleString("zh-CN", { hour12: false });
}

function deadlineApplicabilityLabel(value: CriminalDeadlineItem["applicability_status"]): string {
  if (value === "confirmed") return "已确认适用";
  if (value === "not_applicable") return "不适用";
  return "待确认适用性";
}
