import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Clock3,
  EyeOff,
  Link2,
  Loader2,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Unlink,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Chip } from "@/components/ui/chip";
import { toast } from "@/components/ui/toast";
import {
  bindFeishuSyncCase,
  getFeishuConnectionStatus,
  getFeishuSyncPreview,
  ignoreFeishuSyncCase,
  pullFeishuSyncPreview,
  resolveFeishuSyncField,
  resolveFeishuSyncEntity,
  restoreFeishuSyncCase,
  unbindFeishuSyncCase,
} from "@/lib/api";
import type { FeishuConnectionStatus, FeishuSyncPreview as PreviewData } from "@/lib/types";
import { cn } from "@/lib/utils";

type Section = "bound" | "pending" | "ignored" | "changes" | "entities" | "conflicts" | "runs";

const SECTIONS: Array<{ id: Section; label: string }> = [
  { id: "bound", label: "已绑定案件" },
  { id: "pending", label: "待绑定案件" },
  { id: "ignored", label: "已忽略案件" },
  { id: "changes", label: "拟更新字段" },
  { id: "entities", label: "明细复核" },
  { id: "conflicts", label: "冲突字段" },
  { id: "runs", label: "最近同步状态" },
];

function showValue(value: string | null): string {
  if (!value) return "—";
  try {
    const parsed = JSON.parse(value) as unknown;
    if (parsed == null || parsed === "") return "—";
    if (Array.isArray(parsed)) return parsed.join("、") || "—";
    if (typeof parsed === "object") return JSON.stringify(parsed);
    return String(parsed);
  } catch {
    return value;
  }
}

function showTime(value: string | null | undefined): string {
  if (!value) return "—";
  const date = /^\d{13}$/.test(value)
    ? new Date(Number(value))
    : new Date(value.includes("T") ? value : `${value.replace(" ", "T")}Z`);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString("zh-CN", { hour12: false });
}

function runLabel(status: string): string {
  return ({ succeeded: "成功", partial: "部分成功", failed: "失败", running: "进行中", cancelled: "已取消" } as Record<string, string>)[status] ?? status;
}

function pullErrorMessage(error: unknown): string {
  const message = String(error).toUpperCase();
  if (message.includes("FEISHU_CONFIG_INVALID")) {
    return "请先在“日历设置—案件管理多维表格”填写 App Token 和案件总表 Table ID。";
  }
  if (
    message.includes("FEISHU_AUTH_REQUIRED")
    || message.includes("FEISHU_OAUTH_REAUTHORIZATION_REQUIRED")
    || message.includes("FEISHU_OAUTH_TOKEN_REJECTED")
  ) {
    return "飞书连接未建立或已失效，请重新连接后再试。";
  }
  if (
    message.includes("FEISHU_PERMISSION_DENIED")
    || message.includes("FEISHU_OAUTH_MISSING_READONLY_SCOPE")
    || message.includes("FEISHU_OAUTH_MISSING_READWRITE_SCOPE")
  ) {
    return "当前连接缺少多维表格读写权限，请补充授权后重试。";
  }
  if (
    message.includes("FEISHU_TABLE_SCHEMA_MISMATCH")
    || message.includes("FEISHU_SCHEMA_CHANGED")
    || message.includes("FEISHU_FILTER_MISMATCH")
  ) {
    return "当前 Table ID 不是案件总表，或案件表字段结构发生变化；请选择含“案件名称”和“☑状态”的案件总表。";
  }
  if (message.includes("FEISHU_TABLE_NOT_FOUND")) {
    return "找不到飞书案件总表，请检查 App Token、Table ID 和应用访问权限。";
  }
  if (message.includes("FEISHU_NETWORK_TIMEOUT") || message.includes("FEISHU_NETWORK_ERROR")) {
    return "读取飞书超时，请检查网络后重试。";
  }
  if (message.includes("FEISHU_RESPONSE_INVALID")) {
    return "飞书返回的数据格式异常，本次未更新预演结果，请稍后重试。";
  }
  if (message.includes("FEISHU_DB_PREVIEW_WRITE_FAILED")) {
    return "已读取飞书，但本地预演结果保存失败；案件业务数据未被修改。";
  }
  if (message.includes("FEISHU_PULL_IN_PROGRESS")) {
    return "已有一次飞书预演正在进行，请等待完成后再试。";
  }
  return "本次读取未完成，请稍后重试。";
}

export function FeishuSyncPreview({
  connectionStatus,
  onOpenConnection,
  onConnectionStatusChange,
}: {
  connectionStatus: FeishuConnectionStatus | null;
  onOpenConnection: () => void;
  onConnectionStatusChange: (status: FeishuConnectionStatus) => void;
}) {
  const [data, setData] = useState<PreviewData | null>(null);
  const [active, setActive] = useState<Section>("bound");
  const [loading, setLoading] = useState(true);
  const [localError, setLocalError] = useState<string | null>(null);
  const [pulling, setPulling] = useState(false);
  const [pullError, setPullError] = useState<string | null>(null);
  const [liveMessage, setLiveMessage] = useState("");
  const [selectedCases, setSelectedCases] = useState<Record<string, string>>({});
  const [actingId, setActingId] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setLocalError(null);
    try {
      setData(await getFeishuSyncPreview());
    } catch {
      setLocalError("无法读取本地预演结果，请稍后重试。");
    } finally {
      setLoading(false);
    }
  }, []);

  const pullLatest = async () => {
    if (connectionStatus?.connected !== true || connectionStatus.reauthorization_required) {
      onOpenConnection();
      return;
    }
    setPulling(true);
    setPullError(null);
    setLiveMessage("正在从飞书读取最新在办案件");
    try {
      const result = await pullFeishuSyncPreview();
      const next = await getFeishuSyncPreview();
      setData(next);
      const message = `飞书预演完成：读取 ${result.remote_count} 件在办案件；生成进展候选 ${result.work_item_count} 条、阶段候选 ${result.stage_count} 条、联系人候选 ${result.contact_count} 条。待绑定 ${result.pending_count} 件；尚未写入任何一端。`;
      setLiveMessage(message);
      toast(message, "info");
      void getFeishuConnectionStatus().then(onConnectionStatusChange).catch(() => {});
    } catch (error) {
      setPullError(`${pullErrorMessage(error)} 已保留上次预演结果。`);
      setLiveMessage("本次飞书读取失败，已保留上次预演结果");
    } finally {
      setPulling(false);
    }
  };

  useEffect(() => void reload(), [reload]);

  const latestRun = data?.recent_runs[0] ?? null;
  const counts = useMemo<Record<Section, number>>(() => ({
    bound: data?.bound_cases.length ?? 0,
    pending: data?.pending_cases.length ?? 0,
    ignored: data?.ignored_cases.length ?? 0,
    changes: data?.proposed_changes.length ?? 0,
    entities: data?.entity_changes.length ?? 0,
    conflicts: data?.conflicts.length ?? 0,
    runs: data?.recent_runs.length ?? 0,
  }), [data]);

  const runBindingAction = async (id: string, action: () => Promise<void>, success: string) => {
    setActingId(id);
    try {
      await action();
      await reload();
      setLiveMessage(success);
      toast(success, "info");
    } catch (error) {
      const code = String(error).toUpperCase();
      const message = code.includes("FEISHU_BINDING_CONFLICT")
        ? "该本地案件或飞书记录已经绑定，请刷新后重新选择。"
        : code.includes("FEISHU_BINDING_STATE_INVALID")
          ? "绑定状态已变化，请刷新后重试。"
          : code.includes("FEISHU_BINDING_CASE_NOT_FOUND")
            ? "所选本地案件已不存在，请刷新后重新选择。"
            : "本地绑定操作失败；飞书数据和案件业务字段均未修改。";
      toast(message, "error");
      setLiveMessage(message);
    } finally {
      setActingId(null);
    }
  };

  const runFieldAction = async (
    id: string,
    resolution: "local" | "feishu" | "dismiss",
  ) => {
    setActingId(id);
    try {
      await resolveFeishuSyncField(id, resolution);
      await reload();
      const message = resolution === "local"
        ? "已保留本地值并写入飞书。"
        : resolution === "feishu"
          ? "已采用飞书值并更新本地案件。"
          : "已暂不处理该字段；下次预演若仍有差异会再次提示。";
      toast(message, "info");
      setLiveMessage(message);
    } catch (error) {
      const code = String(error).toUpperCase();
      const message = code.includes("MISSING_READWRITE_SCOPE")
        ? "当前仍是旧只读授权，请断开后重新连接飞书。"
        : code.includes("FEISHU_REVIEW_STALE")
          ? "本地或飞书值在预演后已经变化，请先获取最新预演再复核。"
        : code.includes("FEISHU_REVIEW_ALREADY_RESOLVED")
          ? "该字段已由另一操作处理，请刷新查看。"
          : code.includes("FEISHU_REVIEW_UNSUPPORTED")
            ? "该字段须在对应的阶段或通讯录中处理，不能从案件总表直接写入。"
            : "字段处理失败，未执行静默覆盖；请检查权限或刷新后重试。";
      toast(message, "error");
      setLiveMessage(message);
    } finally {
      setActingId(null);
    }
  };

  const runEntityAction = async (id: string, resolution: "local" | "feishu" | "dismiss") => {
    setActingId(id);
    try {
      await resolveFeishuSyncEntity(id, resolution);
      await reload();
      const message = resolution === "local"
        ? "已保留本地明细并写入飞书。"
        : resolution === "feishu"
          ? "已采用飞书明细并更新本地。"
          : "已暂不处理该明细。";
      toast(message, "info");
      setLiveMessage(message);
    } catch (error) {
      const code = String(error).toUpperCase();
      const message = code.includes("MISSING_READWRITE_SCOPE")
        ? "当前授权不能写入飞书，请重新连接并授予多维表格读写权限。"
        : code.includes("FEISHU_REVIEW_STALE")
          ? "本地或飞书明细在预演后已经变化，请先获取最新预演再复核。"
        : code.includes("FEISHU_REVIEW_UNSUPPORTED")
          ? "该候选目前没有可写回的本地明细，请采用飞书或暂不处理。"
          : "明细处理失败，未执行静默覆盖。请刷新后重试。";
      toast(message, "error");
      setLiveMessage(message);
    } finally {
      setActingId(null);
    }
  };

  if (loading && !data) {
    return <div className="flex min-h-64 items-center justify-center gap-2 text-sm text-muted-foreground"><Loader2 className="size-4 animate-spin" />正在读取本地预演结果…</div>;
  }

  const needsReauthorization = Boolean(connectionStatus?.app_id) && connectionStatus?.reauthorization_required === true;
  const connected = connectionStatus?.connected === true && !needsReauthorization;
  const lastSuccessfulRun = data?.recent_runs.find((run) => run.status === "succeeded" || run.status === "partial") ?? null;

  return (
    <div className="space-y-5" aria-busy={pulling}>
      <p className="sr-only" role="status" aria-live="polite">{liveMessage}</p>
      <div className="flex flex-col gap-3 rounded-xl border border-sky-200 bg-sky-50/80 p-4 dark:border-sky-900 dark:bg-sky-950/25 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-start gap-3">
          <ShieldCheck className="mt-0.5 size-5 shrink-0 text-sky-700 dark:text-sky-300" />
          <div>
            <p className="text-sm font-semibold text-sky-950 dark:text-sky-100">飞书受控双向同步 · 仅“在办”案件</p>
        <p className="mt-1 text-xs leading-relaxed text-sky-800 dark:text-sky-200">后台刷新仍只生成预演，不会自动写入任何一端。只有点击字段后的确认按钮，才会执行一次明确方向的更新。</p>
            <button type="button" onClick={onOpenConnection} className="mt-2 text-xs font-medium text-sky-900 underline-offset-2 hover:underline dark:text-sky-100">
              {connected ? (connectionStatus?.write_enabled ? "已连接 · 读写权限" : "已连接 · 旧只读权限，需重新授权") : needsReauthorization ? "授权已失效，前往重新连接" : "未连接，前往连接飞书"}
            </button>
          </div>
        </div>
        <Button variant="outline" size="sm" onClick={pullLatest} disabled={pulling || !connected} aria-label="从飞书获取最新只读预演">
          {pulling ? <Loader2 className="animate-spin" /> : <RefreshCw />}{pulling ? "正在从飞书读取…" : "从飞书获取最新预演"}
        </Button>
      </div>

      {localError && <div role="alert" className="flex items-start justify-between gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive"><span className="flex items-start gap-2"><AlertTriangle className="mt-0.5 size-4 shrink-0" />{localError}</span><button type="button" onClick={reload} className="shrink-0 font-medium underline-offset-2 hover:underline">重试</button></div>}
      {pullError && <div role="alert" className="flex items-start justify-between gap-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-100"><span className="flex items-start gap-2"><AlertTriangle className="mt-0.5 size-4 shrink-0" />{pullError}{lastSuccessfulRun && ` 上次成功：${showTime(lastSuccessfulRun.completed_at ?? lastSuccessfulRun.started_at)}。`}</span><button type="button" onClick={pullLatest} disabled={pulling || !connected} className="shrink-0 font-medium underline-offset-2 hover:underline disabled:opacity-50">重试</button></div>}

      <section className="rounded-xl border border-border bg-card p-4" aria-labelledby="latest-run-title">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p id="latest-run-title" className="text-xs font-medium text-muted-foreground">最近同步状态</p>
            <div className="mt-1 flex items-center gap-2">
              {latestRun?.status === "succeeded" ? <CheckCircle2 className="size-4 text-emerald-600" /> : latestRun?.status === "failed" || latestRun?.status === "partial" ? <AlertTriangle className="size-4 text-amber-600" /> : <Clock3 className="size-4 text-muted-foreground" />}
              <span className="text-sm font-semibold text-foreground">{latestRun ? runLabel(latestRun.status) : "尚无预演记录"}</span>
              {latestRun && <Chip size="sm" variant="muted">筛选：状态={latestRun.active_case_filter}</Chip>}
            </div>
          </div>
          <p className="text-xs text-muted-foreground">{latestRun ? `完成于 ${showTime(latestRun.completed_at ?? latestRun.started_at)}` : "运行只读预演后将在此显示"}</p>
        </div>
      </section>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        {SECTIONS.slice(0, 5).map((section, index) => {
          const Icon = [Link2, Unlink, EyeOff, ArrowRight, AlertTriangle][index];
          return <button key={section.id} type="button" onClick={() => setActive(section.id)} className={cn("rounded-xl border bg-card p-4 text-left transition-colors hover:border-foreground/25 hover:bg-accent/30", active === section.id && "border-foreground/30 ring-2 ring-foreground/5")}>
            <div className="flex items-center justify-between"><Icon className="size-4 text-muted-foreground" /><span className="text-2xl font-semibold tabular-nums text-foreground">{counts[section.id]}</span></div>
            <p className="mt-3 text-sm font-medium text-foreground">{section.label}</p>
          </button>;
        })}
      </div>

      <div className="overflow-hidden rounded-xl border border-border bg-card">
        <div role="tablist" aria-label="飞书同步预览分区" className="flex overflow-x-auto border-b border-border bg-muted/25 px-2">
          {SECTIONS.map((section) => <button key={section.id} id={`feishu-tab-${section.id}`} role="tab" aria-selected={active === section.id} aria-controls={`feishu-panel-${section.id}`} onClick={() => setActive(section.id)} className={cn("whitespace-nowrap border-b-2 border-transparent px-3 py-3 text-sm text-muted-foreground", active === section.id && "border-foreground font-medium text-foreground")}>{section.label}<span className="ml-1.5 text-xs tabular-nums">{counts[section.id]}</span></button>)}
        </div>
        <div id={`feishu-panel-${active}`} role="tabpanel" aria-labelledby={`feishu-tab-${active}`} className="min-h-56 overflow-x-auto p-4">
          {active === "bound" && ((data?.bound_cases.length ?? 0) === 0
            ? <EmptyState text="尚无已绑定的在办案件。" />
            : <table className="w-full min-w-[720px] border-collapse text-left text-sm"><thead><tr className="border-b border-border">{["本地案件", "匹配方式", "最近同步", "操作"].map((header) => <th key={header} className="px-3 py-2.5 text-xs font-medium text-muted-foreground">{header}</th>)}</tr></thead><tbody>{data?.bound_cases.map((item) => <tr key={item.id} className="border-b border-border/70 last:border-0"><td className="px-3 py-3 font-medium text-foreground">{item.local_case_name}</td><td className="px-3 py-3 text-muted-foreground">{item.link_source === "manual" ? "人工确认" : "唯一精确案号"}</td><td className="px-3 py-3 text-muted-foreground">{showTime(item.last_synced_at)}</td><td className="px-3 py-3"><Button variant="outline" size="sm" disabled={actingId === item.id} onClick={() => {
              if (!window.confirm(`确认解除“${item.local_case_name}”的飞书绑定？解除后仍可重新绑定。`)) return;
              void runBindingAction(item.id, () => unbindFeishuSyncCase(item.id), "已解除本地绑定，案件恢复为待绑定状态。");
            }}>{actingId === item.id ? <Loader2 className="animate-spin" /> : <Unlink />}解除绑定</Button></td></tr>)}</tbody></table>)}
          {active === "pending" && ((data?.pending_cases.length ?? 0) === 0
            ? <EmptyState text="没有待绑定的在办案件。" />
            : <table className="w-full min-w-[900px] border-collapse text-left text-sm"><thead><tr className="border-b border-border">{["飞书案件", "类型 / 案号", "选择本地案件", "操作"].map((header) => <th key={header} className="px-3 py-2.5 text-xs font-medium text-muted-foreground">{header}</th>)}</tr></thead><tbody>{data?.pending_cases.map((item) => {
              const selected = selectedCases[item.id] ?? item.recommended_case_id ?? "";
              return <tr key={item.id} className="border-b border-border/70 last:border-0"><td className="max-w-64 px-3 py-3 align-top"><p className="font-medium text-foreground">{item.display_name || "未命名案件"}</p><p className="mt-1 text-xs text-muted-foreground">远端更新：{showTime(item.remote_modified_at)}</p></td><td className="px-3 py-3 align-top text-muted-foreground">{[item.legal_type, item.case_no].filter(Boolean).join(" · ") || "—"}</td><td className="min-w-80 px-3 py-3 align-top"><select aria-label={`为${item.display_name || "未命名案件"}选择本地案件`} value={selected} onChange={(event) => setSelectedCases((current) => ({ ...current, [item.id]: event.target.value }))} className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground"><option value="">请选择本地案件</option>{data?.available_local_cases.map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.display_name}{candidate.id === item.recommended_case_id ? "（推荐）" : ""}</option>)}</select>{item.recommendation_reason && <p className="mt-1.5 text-xs text-sky-700 dark:text-sky-300">{item.recommendation_reason}</p>}</td><td className="px-3 py-3 align-top"><div className="flex flex-wrap gap-2"><Button size="sm" disabled={!selected || actingId === item.id} onClick={() => void runBindingAction(item.id, () => bindFeishuSyncCase(item.id, selected), "已确认本地绑定；飞书和案件业务字段均未修改。")}>{actingId === item.id ? <Loader2 className="animate-spin" /> : <Link2 />}确认绑定</Button><Button variant="outline" size="sm" disabled={actingId === item.id} onClick={() => void runBindingAction(item.id, () => ignoreFeishuSyncCase(item.id), "已忽略该记录，可在“已忽略案件”中恢复。")}>{actingId === item.id ? <Loader2 className="animate-spin" /> : <EyeOff />}忽略</Button></div></td></tr>;
            })}</tbody></table>)}
          {active === "ignored" && ((data?.ignored_cases.length ?? 0) === 0
            ? <EmptyState text="没有已忽略案件。" />
            : <table className="w-full min-w-[720px] border-collapse text-left text-sm"><thead><tr className="border-b border-border">{["飞书案件", "类型 / 案号", "操作"].map((header) => <th key={header} className="px-3 py-2.5 text-xs font-medium text-muted-foreground">{header}</th>)}</tr></thead><tbody>{data?.ignored_cases.map((item) => <tr key={item.id} className="border-b border-border/70 last:border-0"><td className="px-3 py-3 font-medium text-foreground">{item.display_name || "未命名案件"}</td><td className="px-3 py-3 text-muted-foreground">{[item.legal_type, item.case_no].filter(Boolean).join(" · ") || "—"}</td><td className="px-3 py-3"><Button variant="outline" size="sm" disabled={actingId === item.id} onClick={() => void runBindingAction(item.id, () => restoreFeishuSyncCase(item.id), "已恢复为待绑定案件。")}>{actingId === item.id ? <Loader2 className="animate-spin" /> : <RotateCcw />}恢复</Button></td></tr>)}</tbody></table>)}
          {active === "changes" && ((data?.proposed_changes.length ?? 0) === 0
            ? <EmptyState text="最近一次预演没有可建议更新的字段。" />
            : <table className="w-full min-w-[980px] border-collapse text-left text-sm"><thead><tr className="border-b border-border">{["案件", "字段", "本地值", "飞书值", "处理"].map((header) => <th key={header} className="px-3 py-2.5 text-xs font-medium text-muted-foreground">{header}</th>)}</tr></thead><tbody>{data?.proposed_changes.map((item) => <tr key={item.id} className="border-b border-border/70 last:border-0"><td className="px-3 py-3 font-medium text-foreground">{item.case_name}</td><td className="px-3 py-3 text-muted-foreground">{item.field_label || item.field_key}</td><td className="max-w-56 px-3 py-3 text-foreground"><span className="line-clamp-3 break-words">{showValue(item.local_value_json)}</span></td><td className="max-w-56 px-3 py-3 text-foreground"><span className="line-clamp-3 break-words">{showValue(item.feishu_value_json)}</span></td><td className="px-3 py-3"><div className="flex min-w-72 flex-wrap gap-2"><Button size="sm" disabled={actingId === item.id} onClick={() => {
              if (!window.confirm(`确认采用飞书值并更新本地“${item.case_name}”的“${item.field_label || item.field_key}”？`)) return;
              void runFieldAction(item.id, "feishu");
            }}>{actingId === item.id ? <Loader2 className="animate-spin" /> : <ArrowRight />}采用飞书</Button><Button variant="outline" size="sm" disabled={actingId === item.id || connectionStatus?.write_enabled !== true} title={connectionStatus?.write_enabled ? "将本地值写入飞书" : "需重新授权飞书读写权限"} onClick={() => {
              if (!window.confirm(`确认保留本地值，并写入飞书“${item.field_label || item.field_key}”？`)) return;
              void runFieldAction(item.id, "local");
            }}>保留本地并写飞书</Button><Button variant="ghost" size="sm" disabled={actingId === item.id} onClick={() => void runFieldAction(item.id, "dismiss")}>暂不处理</Button></div></td></tr>)}</tbody></table>)}
          {active === "entities" && ((data?.entity_changes.length ?? 0) === 0
            ? <EmptyState text="最近一次预演没有待复核的进展、阶段或联系人。" />
            : <table className="w-full min-w-[980px] border-collapse text-left text-sm"><thead><tr className="border-b border-border">{["案件", "明细类型", "变化", "本地值", "飞书值", "处理"].map((header) => <th key={header} className="px-3 py-2.5 text-xs font-medium text-muted-foreground">{header}</th>)}</tr></thead><tbody>{data?.entity_changes.map((item) => <tr key={item.id} className="border-b border-border/70 last:border-0"><td className="px-3 py-3 font-medium text-foreground">{item.case_name}</td><td className="px-3 py-3 text-muted-foreground">{({ work_item: "进展", stage: "阶段", contact: "联系人" } as Record<string, string>)[item.entity_type] ?? item.entity_type}</td><td className="px-3 py-3 text-muted-foreground">{({ create: "新增", update: "更新", restore: "恢复", archive: "归档" } as Record<string, string>)[item.change_kind] ?? item.change_kind}</td><td className="max-w-56 px-3 py-3"><span className="line-clamp-3 break-words">{showValue(item.local_value_json)}</span></td><td className="max-w-56 px-3 py-3"><span className="line-clamp-3 break-words">{showValue(item.feishu_value_json)}</span></td><td className="px-3 py-3"><div className="flex min-w-72 flex-wrap gap-2"><Button size="sm" disabled={actingId === item.id} onClick={() => { if (window.confirm(`确认采用飞书中的${item.entity_type === "work_item" ? "进展" : item.entity_type === "stage" ? "阶段" : "联系人"}并更新本地？`)) void runEntityAction(item.id, "feishu"); }}>{actingId === item.id ? <Loader2 className="animate-spin" /> : <ArrowRight />}采用飞书</Button><Button variant="outline" size="sm" disabled={actingId === item.id || connectionStatus?.write_enabled !== true || !item.local_value_json} onClick={() => { if (window.confirm("确认保留本地明细并写入飞书？")) void runEntityAction(item.id, "local"); }}>保留本地并写飞书</Button><Button variant="ghost" size="sm" disabled={actingId === item.id} onClick={() => void runEntityAction(item.id, "dismiss")}>暂不处理</Button></div></td></tr>)}</tbody></table>)}
          {active === "conflicts" && <SimpleTable headers={["案件", "字段", "本地值", "飞书值", "状态"]} rows={(data?.conflicts ?? []).map((item) => [item.case_name, item.field_key, showValue(item.local_value_json), showValue(item.feishu_value_json), "待人工处理"])} empty="没有待处理的字段冲突。" />}
          {active === "runs" && <SimpleTable headers={["状态", "模式", "案件范围", "开始时间", "完成时间"]} rows={(data?.recent_runs ?? []).map((item) => [runLabel(item.status), item.mode === "readonly_preflight" ? "只读预演" : item.mode, `状态=${item.active_case_filter}`, showTime(item.started_at), showTime(item.completed_at)])} empty="尚无同步预演记录。" />}
        </div>
      </div>
    </div>
  );
}

function SimpleTable({ headers, rows, empty }: { headers: string[]; rows: string[][]; empty: string }) {
  if (rows.length === 0) return <EmptyState text={empty} />;
  return <table className="w-full min-w-[720px] border-collapse text-left text-sm"><thead><tr className="border-b border-border">{headers.map((header, index) => <th key={`${header}-${index}`} className="px-3 py-2.5 text-xs font-medium text-muted-foreground">{header}</th>)}</tr></thead><tbody>{rows.map((row, rowIndex) => <tr key={rowIndex} className="border-b border-border/70 last:border-0">{row.map((cell, cellIndex) => <td key={cellIndex} className={cn("max-w-72 px-3 py-3 align-top text-foreground", cellIndex > 0 && "text-muted-foreground")}><span className="line-clamp-3 break-words" title={cell}>{cell}</span></td>)}</tr>)}</tbody></table>;
}

function EmptyState({ text }: { text: string }) {
  return <div className="flex min-h-48 items-center justify-center text-sm text-muted-foreground">{text}</div>;
}
