import { useState } from "react";
import { RefreshCw, ShieldCheck } from "lucide-react";

export type LocalUsagePeriod = "day" | "month";

export interface LocalUsageMetric {
  period: LocalUsagePeriod;
  provider: string;
  stage: string;
  success_count: number;
  failure_count: number;
  average_elapsed_ms: number | null;
  rate_limited_429_count: number;
  page_count: number | null;
  page_count_unavailable_reason: string | null;
  fallback_count: number | null;
  fallback_unavailable_reason: string | null;
}

export interface LocalUsageSnapshot {
  metrics: LocalUsageMetric[];
  last_refreshed_at: string | null;
  official_balance: number | null;
  official_balance_unavailable_reason?: string | null;
  yuandian_estimate: {
    year_month: string;
    estimated_credits: number;
    recorded_api_calls: number;
    local_kb_hits: number;
    total_estimated_credits: number;
    estimate_basis: string;
    has_any_record: boolean;
  } | null;
}

export interface LocalUsageDashboardProps {
  snapshot: LocalUsageSnapshot | null;
  loading?: boolean;
  loadError?: string | null;
  onValidateConnection: () => Promise<boolean>;
  onRefreshLocal: () => Promise<void>;
}

function periodLabel(period: LocalUsagePeriod): string {
  return period === "day" ? "今日" : "本月";
}

function optionalMetric(value: number | null, reason: string | null, suffix = "") {
  return value === null ? `暂无：${reason || "数据源未提供"}` : `${value}${suffix}`;
}

export function LocalUsageDashboard({
  snapshot,
  loading = false,
  loadError,
  onValidateConnection,
  onRefreshLocal,
}: LocalUsageDashboardProps) {
  const [action, setAction] = useState<"validate" | "refresh" | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [validationMessage, setValidationMessage] = useState<string | null>(null);

  async function run(kind: "validate" | "refresh") {
    if (action) return;
    setAction(kind);
    setActionError(null);
    if (kind === "validate") setValidationMessage(null);
    try {
      if (kind === "validate") {
        const valid = await onValidateConnection();
        if (!valid) throw new Error("连接验证失败");
        setValidationMessage("连接验证成功；本地用量尚未刷新。");
      } else {
        await onRefreshLocal();
      }
    } catch (error) {
      setActionError(String(error));
    } finally {
      setAction(null);
    }
  }

  const officialBalanceText =
    snapshot?.official_balance === null || snapshot?.official_balance === undefined
      ? snapshot?.official_balance_unavailable_reason || "未提供官方余额接口"
      : `${snapshot.official_balance}`;

  return (
    <section className="rounded-lg border border-border bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-foreground">数据源用量与运行统计</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            连接状态、元典本地估算和识别服务运行指标分别展示。
          </p>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => run("validate")}
            disabled={action !== null}
            className="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs disabled:opacity-50"
          >
            <ShieldCheck className="size-3.5" />
            {action === "validate" ? "验证中…" : "验证连接"}
          </button>
          <button
            type="button"
            onClick={() => run("refresh")}
            disabled={action !== null}
            className="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs disabled:opacity-50"
          >
            <RefreshCw className="size-3.5" />
            {action === "refresh" ? "刷新中…" : "刷新本地统计"}
          </button>
        </div>
      </div>

      <div className="mt-3 rounded-md border border-sky-200 bg-sky-50/50 p-3 text-xs">
        <h4 className="font-semibold text-sky-900">元典本地用量估算</h4>
        <p className="mt-1 text-muted-foreground">
          基于本机元典调用日志估算，不等同于元典官方账单或 OCR 识别指标。
        </p>
        <p className="mt-2">官方余额：{officialBalanceText}</p>
        {snapshot?.yuandian_estimate ? (
          <div className="mt-2 grid gap-1 text-muted-foreground sm:grid-cols-3">
            <p>
              本月估算：{snapshot.yuandian_estimate.estimated_credits} 积分（
              {snapshot.yuandian_estimate.year_month}）
            </p>
            <p>已记录调用：{snapshot.yuandian_estimate.recorded_api_calls} 次</p>
            <p>本地命中：{snapshot.yuandian_estimate.local_kb_hits} 次</p>
            <p className="sm:col-span-3">{snapshot.yuandian_estimate.estimate_basis}</p>
          </div>
        ) : (
          <p className="mt-2 text-muted-foreground">暂无元典本地调用记录。</p>
        )}
        <p className="mt-1 text-muted-foreground">
          最后刷新：{snapshot?.last_refreshed_at || "尚未刷新"}
        </p>
      </div>

      {(loadError || actionError) && (
        <p role="alert" className="mt-3 text-xs text-red-600">
          {loadError || actionError}
        </p>
      )}
      {validationMessage && <p className="mt-3 text-xs text-emerald-700">{validationMessage}</p>}

      <div className="mt-4 border-t border-border pt-3">
        <h4 className="text-sm font-semibold text-foreground">识别服务本地用量</h4>
        <p className="mt-1 text-xs text-muted-foreground">
          下表按 provider / stage 汇总 OCR、抽取及其降级链路，不代表元典官方余额。
        </p>
        {loading ? (
          <p className="mt-3 text-xs text-muted-foreground">正在读取识别服务本地统计…</p>
        ) : !snapshot || snapshot.metrics.length === 0 ? (
          <p className="mt-3 text-xs text-muted-foreground">
            暂无识别服务用量记录。请先执行相关任务，再刷新本地统计。
          </p>
        ) : (
          <div className="mt-3 overflow-x-auto">
          <table className="w-full min-w-[760px] text-left text-xs">
            <thead className="text-muted-foreground">
              <tr>
                <th className="p-2">周期</th>
                <th className="p-2">Provider / Stage</th>
                <th className="p-2">成功 / 失败</th>
                <th className="p-2">平均耗时</th>
                <th className="p-2">429</th>
                <th className="p-2">页数</th>
                <th className="p-2">降级</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.metrics.map((metric, index) => (
                <tr key={`${metric.period}-${metric.provider}-${metric.stage}-${index}`} className="border-t">
                  <td className="p-2">{periodLabel(metric.period)}</td>
                  <td className="p-2">
                    {metric.provider} / {metric.stage}
                  </td>
                  <td className="p-2">
                    {metric.success_count} / {metric.failure_count}
                  </td>
                  <td className="p-2">
                    {metric.average_elapsed_ms === null
                      ? "暂无：未记录耗时"
                      : `${Math.round(metric.average_elapsed_ms)} ms`}
                  </td>
                  <td className="p-2">{metric.rate_limited_429_count}</td>
                  <td className="p-2">
                    {optionalMetric(
                      metric.page_count,
                      metric.page_count_unavailable_reason,
                      " 页",
                    )}
                  </td>
                  <td className="p-2">
                    {optionalMetric(
                      metric.fallback_count,
                      metric.fallback_unavailable_reason,
                      " 次",
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          </div>
        )}
      </div>
    </section>
  );
}
