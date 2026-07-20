import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Clock3,
  Link2,
  Loader2,
  RefreshCw,
  ShieldCheck,
  Unlink,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Chip } from "@/components/ui/chip";
import { getFeishuSyncPreview } from "@/lib/api";
import type { FeishuSyncPreview as PreviewData } from "@/lib/types";
import { cn } from "@/lib/utils";

type Section = "bound" | "pending" | "changes" | "conflicts" | "runs";

const SECTIONS: Array<{ id: Section; label: string }> = [
  { id: "bound", label: "已绑定案件" },
  { id: "pending", label: "待绑定案件" },
  { id: "changes", label: "拟更新字段" },
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

export function FeishuSyncPreview() {
  const [data, setData] = useState<PreviewData | null>(null);
  const [active, setActive] = useState<Section>("bound");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setData(await getFeishuSyncPreview());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => void reload(), [reload]);

  const latestRun = data?.recent_runs[0] ?? null;
  const counts = useMemo<Record<Section, number>>(() => ({
    bound: data?.bound_cases.length ?? 0,
    pending: data?.pending_cases.length ?? 0,
    changes: data?.proposed_changes.length ?? 0,
    conflicts: data?.conflicts.length ?? 0,
    runs: data?.recent_runs.length ?? 0,
  }), [data]);

  if (loading && !data) {
    return <div className="flex min-h-64 items-center justify-center gap-2 text-sm text-muted-foreground"><Loader2 className="size-4 animate-spin" />正在读取本地预演结果…</div>;
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-col gap-3 rounded-xl border border-sky-200 bg-sky-50/80 p-4 dark:border-sky-900 dark:bg-sky-950/25 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-start gap-3">
          <ShieldCheck className="mt-0.5 size-5 shrink-0 text-sky-700 dark:text-sky-300" />
          <div>
            <p className="text-sm font-semibold text-sky-950 dark:text-sky-100">只读预演 · 仅“在办”案件</p>
            <p className="mt-1 text-xs leading-relaxed text-sky-800 dark:text-sky-200">本页只展示最近的隔离预演结果，不会写入飞书，也不会修改本地案件。</p>
          </div>
        </div>
        <Button variant="outline" size="sm" onClick={reload} disabled={loading} aria-label="重读本地同步预览">
          {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}刷新预览
        </Button>
      </div>

      {error && <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive"><AlertTriangle className="mt-0.5 size-4 shrink-0" /><span>{error}</span></div>}

      <section className="rounded-xl border border-border bg-card p-4" aria-labelledby="latest-run-title">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p id="latest-run-title" className="text-xs font-medium text-muted-foreground">最近同步状态</p>
            <div className="mt-1 flex items-center gap-2">
              {latestRun?.status === "succeeded" ? <CheckCircle2 className="size-4 text-emerald-600" /> : <Clock3 className="size-4 text-muted-foreground" />}
              <span className="text-sm font-semibold text-foreground">{latestRun ? runLabel(latestRun.status) : "尚无预演记录"}</span>
              {latestRun && <Chip size="sm" variant="muted">筛选：状态={latestRun.active_case_filter}</Chip>}
            </div>
          </div>
          <p className="text-xs text-muted-foreground">{latestRun ? `完成于 ${showTime(latestRun.completed_at ?? latestRun.started_at)}` : "运行只读预演后将在此显示"}</p>
        </div>
      </section>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {SECTIONS.slice(0, 4).map((section, index) => {
          const Icon = [Link2, Unlink, ArrowRight, AlertTriangle][index];
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
          {active === "bound" && <SimpleTable headers={["本地案件", "匹配方式", "最近同步"]} rows={(data?.bound_cases ?? []).map((item) => [item.local_case_name, item.link_source === "manual" ? "人工确认" : "精确匹配", showTime(item.last_synced_at)])} empty="尚无已绑定的在办案件。" />}
          {active === "pending" && <SimpleTable headers={["飞书案件", "类型 / 案号", "远端修改时间"]} rows={(data?.pending_cases ?? []).map((item) => [item.display_name || "未命名案件", [item.legal_type, item.case_no].filter(Boolean).join(" · ") || "—", showTime(item.remote_modified_at)])} empty="没有待绑定的在办案件。" />}
          {active === "changes" && <SimpleTable headers={["案件", "字段", "本地值", "", "飞书值", "处理"]} rows={(data?.proposed_changes ?? []).map((item) => [item.case_name, item.field_label || item.field_key, showValue(item.local_value_json), "→", showValue(item.feishu_value_json), item.proposed_action === "review" ? "需人工复核" : "拟填充本地空值"])} empty="最近一次预演没有可建议更新的字段。" />}
          {active === "conflicts" && <SimpleTable headers={["案件", "字段", "本地值", "飞书值", "状态"]} rows={(data?.conflicts ?? []).map((item) => [item.case_name, item.field_key, showValue(item.local_value_json), showValue(item.feishu_value_json), "待人工处理"])} empty="没有待处理的字段冲突。" />}
          {active === "runs" && <SimpleTable headers={["状态", "模式", "案件范围", "开始时间", "完成时间"]} rows={(data?.recent_runs ?? []).map((item) => [runLabel(item.status), item.mode === "readonly_preflight" ? "只读预演" : item.mode, `状态=${item.active_case_filter}`, showTime(item.started_at), showTime(item.completed_at)])} empty="尚无同步预演记录。" />}
        </div>
      </div>
    </div>
  );
}

function SimpleTable({ headers, rows, empty }: { headers: string[]; rows: string[][]; empty: string }) {
  if (rows.length === 0) return <div className="flex min-h-48 items-center justify-center text-sm text-muted-foreground">{empty}</div>;
  return <table className="w-full min-w-[720px] border-collapse text-left text-sm"><thead><tr className="border-b border-border">{headers.map((header, index) => <th key={`${header}-${index}`} className="px-3 py-2.5 text-xs font-medium text-muted-foreground">{header}</th>)}</tr></thead><tbody>{rows.map((row, rowIndex) => <tr key={rowIndex} className="border-b border-border/70 last:border-0">{row.map((cell, cellIndex) => <td key={cellIndex} className={cn("max-w-72 px-3 py-3 align-top text-foreground", cellIndex > 0 && "text-muted-foreground")}><span className="line-clamp-3 break-words" title={cell}>{cell}</span></td>)}</tr>)}</tbody></table>;
}
