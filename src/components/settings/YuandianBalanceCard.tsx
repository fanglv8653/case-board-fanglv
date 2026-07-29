import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, Coins, Database, Loader2, RefreshCw } from "lucide-react";

import { getYuandianBalance } from "@/lib/api";

interface YuandianBalanceView {
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

function formatDateTime(value: string | null): string {
  if (!value) return "暂无";
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString("zh-CN");
}

function comparisonText(snapshot: YuandianBalanceView): string {
  switch (snapshot.comparison_status) {
    case "matched":
      return "官方消耗与本机积分账一致";
    case "difference":
      return `相差 ${snapshot.difference ?? 0} 积分（官方消耗减本机记录）`;
    case "recharged":
      return `两次刷新间余额增加 ${snapshot.balance_increased_since_previous ?? 0} 积分`;
    case "local_reset":
      return "本机积分账发生重置，暂不计算差异";
    default:
      return "首次取得余额，下一次刷新后开始对账";
  }
}

export function YuandianBalanceCard() {
  const mountedRefreshStarted = useRef(false);
  const [snapshot, setSnapshot] = useState<YuandianBalanceView | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const load = useCallback(async (refresh: boolean) => {
    setLoading(true);
    setLoadError(null);
    try {
      const next = (await getYuandianBalance(refresh)) as YuandianBalanceView | null;
      setSnapshot(next);
    } catch (reason) {
      setLoadError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (mountedRefreshStarted.current) return;
    mountedRefreshStarted.current = true;
    void load(true);
  }, [load]);

  return (
    <section className="rounded-lg border border-border bg-card p-4" aria-labelledby="yuandian-balance-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 id="yuandian-balance-title" className="text-sm font-semibold text-foreground">
            元典官方余额
          </h3>
          <p className="mt-1 text-xs text-muted-foreground">
            进入本页时刷新一次，也可手动刷新；不会定时轮询。
          </p>
        </div>
        <button
          type="button"
          onClick={() => void load(true)}
          disabled={loading}
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
        >
          {loading ? (
            <Loader2 className="size-3.5 animate-spin" aria-hidden="true" />
          ) : (
            <RefreshCw className="size-3.5" aria-hidden="true" />
          )}
          {loading ? "刷新中…" : "手动刷新"}
        </button>
      </div>

      {snapshot ? (
        <>
          <div className="mt-4 grid gap-3 sm:grid-cols-2">
            <div className="rounded-md border border-amber-200 bg-amber-50/60 p-3">
              <div className="flex items-center gap-2 text-xs text-amber-800">
                <Coins className="size-4" aria-hidden="true" />
                官方积分余额
              </div>
              <p className="mt-1 text-2xl font-semibold tabular-nums text-foreground">
                {snapshot.point_balance.toLocaleString("zh-CN")}
                <span className="ml-1 text-xs font-normal text-muted-foreground">积分</span>
              </p>
            </div>
            <div className="rounded-md border border-sky-200 bg-sky-50/60 p-3">
              <div className="flex items-center gap-2 text-xs text-sky-800">
                <Database className="size-4" aria-hidden="true" />
                官方次数余额
              </div>
              <p className="mt-1 text-2xl font-semibold tabular-nums text-foreground">
                {snapshot.count_balance.toLocaleString("zh-CN")}
                <span className="ml-1 text-xs font-normal text-muted-foreground">次</span>
              </p>
            </div>
          </div>

          <div className="mt-3 rounded-md border border-border bg-background/60 p-3 text-xs">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="font-medium text-foreground">本机积分账差异</p>
              {snapshot.cached && (
                <span className="rounded-full bg-muted px-2 py-0.5 text-muted-foreground">
                  缓存数据
                </span>
              )}
            </div>
            <p className="mt-1 text-muted-foreground">{comparisonText(snapshot)}</p>
            {snapshot.official_spent_since_previous !== null && (
              <div className="mt-2 grid gap-1 text-muted-foreground sm:grid-cols-3">
                <p>官方消耗：{snapshot.official_spent_since_previous} 积分</p>
                <p>本机记录：{snapshot.local_recorded_since_previous ?? "不可比较"} 积分</p>
                <p>本机调用：{snapshot.local_api_calls_since_previous ?? "不可比较"} 次</p>
              </div>
            )}
            <p className="mt-2 text-muted-foreground">
              上次刷新：{formatDateTime(snapshot.fetched_at)}
              {snapshot.previous_fetched_at
                ? `；对比基准：${formatDateTime(snapshot.previous_fetched_at)}`
                : ""}
            </p>
          </div>

          {snapshot.refresh_error && (
            <p
              role="alert"
              className="mt-3 flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50/60 p-2.5 text-xs text-amber-800"
            >
              <AlertCircle className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
              <span>
                官方刷新失败，当前显示该凭据最近一次缓存：{snapshot.refresh_error}
                {snapshot.refresh_error_code ? `（${snapshot.refresh_error_code}）` : ""}
              </span>
            </p>
          )}
        </>
      ) : loading ? (
        <p className="mt-4 text-xs text-muted-foreground">正在读取元典官方余额…</p>
      ) : (
        <p className="mt-4 text-xs text-muted-foreground">
          尚无该元典凭据对应的余额快照。请确认凭据已保存后手动刷新。
        </p>
      )}

      {loadError && (
        <p role="alert" className="mt-3 flex items-start gap-2 text-xs text-destructive">
          <AlertCircle className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          {loadError}
        </p>
      )}

      <p className="mt-3 text-label leading-5 text-muted-foreground">
        差异仅比较两次官方快照间的余额变化与同期本机积分账。其他客户端调用、充值赠送及平台计价调整可能造成差异，本机不会据此改写官方余额。
      </p>
    </section>
  );
}
