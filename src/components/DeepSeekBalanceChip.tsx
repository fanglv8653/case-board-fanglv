/**
 * DeepSeek 余额 / 今日消费 chip — 整个界面右上角(2026-05-24 e)。
 *
 * 仅在 settings.llm_provider="cloud" + 有 api_key 时显示。
 *
 * 显示规则(作者:先显示今日消费,后显示余额):
 *   - 主显:今日 ¥X.XX
 *   - 副显:余额 ¥X.XX
 *   - 鼠标悬停:展开 tooltip 显示 granted / topped_up 明细 + 最后刷新时间
 *
 * 数据来源:
 *   - 启动时 invoke get_deepseek_balance(refresh=false) → 立即显示缓存
 *   - 立即 invoke get_deepseek_balance(refresh=true) → 拉新值更新
 *   - 每 5 分钟 setInterval 刷新一次
 *   - 点击 chip 手工触发刷新
 */

import { useCallback, useEffect, useState } from "react";
import { Loader2, RefreshCw } from "lucide-react";

import { Chip } from "@/components/ui/chip";
import { type DeepSeekBalance, getDeepSeekBalance } from "@/lib/api";
import { cn } from "@/lib/utils";

const REFRESH_INTERVAL_MS = 5 * 60 * 1000;

export function DeepSeekBalanceChip() {
  const [balance, setBalance] = useState<DeepSeekBalance | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      const b = await getDeepSeekBalance(true);
      setBalance(b);
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, []);

  // 启动:先读缓存立即显示,再异步 refresh
  useEffect(() => {
    let cancelled = false;
    getDeepSeekBalance(false).then((cached) => {
      if (!cancelled && cached) setBalance(cached);
    });
    // 立即拉一次最新(失败不打扰)
    refresh().catch(() => {});
    // 5 分钟周期刷新
    const t = window.setInterval(() => refresh().catch(() => {}), REFRESH_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(t);
    };
  }, [refresh]);

  // 没数据 + 失败:显示 placeholder(说明 API key 没填 / 没连通)
  if (!balance && error) {
    return (
      <Chip asChild size="sm" className="border-dashed px-2.5">
        <button
          type="button"
          onClick={refresh}
          disabled={refreshing}
          title={`DeepSeek 余额获取失败:${error}\n点击重试`}
        >
          {refreshing ? (
            <Loader2 className="size-3 animate-spin" />
          ) : (
            <RefreshCw className="size-3" />
          )}
          余额 —
        </button>
      </Chip>
    );
  }

  // 还没拿到任何数据(加载中)
  if (!balance) {
    return (
      <Chip size="sm" className="px-2.5">
        <Loader2 className="size-3 animate-spin" />
        DeepSeek 加载中
      </Chip>
    );
  }

  const todayText =
    balance.today_consumed != null
      ? `¥${balance.today_consumed.toFixed(2)}`
      : "—";
  const remainText = `¥${balance.total_balance.toFixed(2)}`;
  const fetchedTime = new Date(balance.fetched_at).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
  });

  // 余额不足警告:< 5 元
  const lowBalance = balance.total_balance < 5;

  return (
    <Chip
      asChild
      size="sm"
      variant={lowBalance ? "warning" : "default"}
      className={cn("group gap-1.5 px-2.5 py-1", lowBalance && "bg-amber-50/50")}
    >
      <button
        type="button"
        onClick={refresh}
        disabled={refreshing}
        title={[
          `DeepSeek 余额`,
          `  · 总余额:¥${balance.total_balance.toFixed(2)}`,
          `  · 充值:¥${balance.topped_up_balance.toFixed(2)}`,
          `  · 赠送:¥${balance.granted_balance.toFixed(2)}`,
          `今日消费:${
            balance.today_consumed != null
              ? `¥${balance.today_consumed.toFixed(2)}`
              : "无昨日快照(明天起就有)"
          }`,
          `最后刷新:${fetchedTime}`,
          ``,
          `点击刷新`,
        ].join("\n")}
      >
        <span className="font-medium text-foreground">今日</span>
        <span className="font-mono text-foreground">{todayText}</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="text-muted-foreground">余额</span>
        <span
          className={cn(
            "font-mono",
            lowBalance ? "text-amber-800" : "text-foreground",
          )}
        >
          {remainText}
        </span>
        {refreshing && (
          <Loader2 className="ml-0.5 size-3 animate-spin text-muted-foreground" />
        )}
      </button>
    </Chip>
  );
}
