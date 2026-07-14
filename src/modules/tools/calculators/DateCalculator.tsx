/**
 * 天数计算器 — React 原生实现(2026-05-24 e)。
 *
 * 两个子模式(tab):
 *   1. 间隔模式:算两日期之间天数 + 周 / 月 / 年统计
 *   2. 推算模式:从某日加 / 减若干天得到新日期
 *
 * 计算逻辑见 ../lib/dateMath.ts(100% 移植自 lawtools.top/daycal.html)。
 */

import { useMemo, useState } from "react";
import { Calendar } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { CalculatorDisclaimer } from "./ui";

import {
  addDaysIso,
  diffDaysIso,
  diffMonths,
  formatChineseDate,
  getWeekday,
  todayIso,
} from "../lib/dateMath";

type Mode = "interval" | "calc";

export function DateCalculator() {
  const [mode, setMode] = useState<Mode>("interval");

  return (
    <div className="space-y-5">
      {/* 模式切换 */}
      <div className="inline-flex rounded-md border border-border bg-card p-0.5">
        <SegBtn active={mode === "interval"} onClick={() => setMode("interval")}>
          间隔计算
        </SegBtn>
        <SegBtn active={mode === "calc"} onClick={() => setMode("calc")}>
          日期推算
        </SegBtn>
      </div>

      {mode === "interval" ? <IntervalPanel /> : <CalcPanel />}
    </div>
  );
}

function SegBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "rounded px-3 py-1 text-xs font-medium transition-colors",
        active
          ? "bg-foreground text-background"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

/* ============================ 间隔模式 ============================ */
function IntervalPanel() {
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");

  const result = useMemo(() => {
    if (!start || !end) return null;
    const { days, reversed } = diffDaysIso(start, end);
    const weeks = Math.floor(days / 7);
    const remainDays = days % 7;
    const months = diffMonths(start, end);
    const years = (days / 365.25).toFixed(1);
    return { days, reversed, weeks, remainDays, months, years };
  }, [start, end]);

  const setQuickInterval = (n: number) => {
    const today = todayIso();
    setStart(today);
    setEnd(addDaysIso(today, n));
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field label="开始日期">
          <DateInput value={start} onChange={setStart} />
        </Field>
        <Field label="结束日期">
          <DateInput value={end} onChange={setEnd} />
        </Field>
      </div>

      {/* 快捷间隔 */}
      <div className="flex flex-wrap gap-2">
        <span className="self-center text-label text-muted-foreground">快捷:</span>
        {[
          { n: 7, label: "今天 → 7 天后" },
          { n: 15, label: "今天 → 15 天后" },
          { n: 30, label: "今天 → 30 天后" },
          { n: 90, label: "今天 → 90 天后" },
        ].map((q) => (
          <Button
            key={q.n}
            variant="outline"
            size="sm"
            onClick={() => setQuickInterval(q.n)}
            className="h-7 text-xs"
          >
            {q.label}
          </Button>
        ))}
      </div>

      {result ? (
        <div className="rounded-md border border-border bg-card px-5 py-4">
          <div className="flex items-baseline gap-3">
            <span className="font-mono text-3xl font-semibold text-foreground">
              {result.days}
            </span>
            <span className="text-sm text-muted-foreground">天</span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {formatChineseDate(start)}
            {result.reversed ? " ← " : " → "}
            {formatChineseDate(end)}
            {result.reversed && (
              <span className="text-amber-700">(结束早于开始)</span>
            )}
          </p>

          <dl className="mt-4 grid grid-cols-2 gap-3 border-t border-border pt-3 text-sm sm:grid-cols-4">
            <StatItem label="天" value={String(result.days)} />
            <StatItem
              label="周"
              value={`${result.weeks}${result.remainDays > 0 ? `+${result.remainDays}天` : ""}`}
            />
            <StatItem label="月" value={String(result.months)} />
            <StatItem label="年" value={result.years} />
          </dl>
          <CalculatorDisclaimer />
        </div>
      ) : (
        <Placeholder>选择两个日期开始计算</Placeholder>
      )}
    </div>
  );
}

/* ============================ 推算模式 ============================ */
function CalcPanel() {
  const [start, setStart] = useState("");
  const [days, setDays] = useState("");
  const [direction, setDirection] = useState<1 | -1>(1);

  const result = useMemo(() => {
    if (!start) return null;
    const n = parseInt(days, 10) || 0;
    const resultIso = n === 0 ? start : addDaysIso(start, n * direction);
    return { iso: resultIso, weekday: getWeekday(resultIso) };
  }, [start, days, direction]);

  return (
    <div className="space-y-4">
      <Field label="起始日期">
        <DateInput value={start} onChange={setStart} />
      </Field>

      <Field label="方向 + 天数">
        <div className="flex items-center gap-2">
          <div className="inline-flex rounded-md border border-border bg-card p-0.5">
            <SegBtn active={direction === 1} onClick={() => setDirection(1)}>
              + 之后
            </SegBtn>
            <SegBtn active={direction === -1} onClick={() => setDirection(-1)}>
              − 之前
            </SegBtn>
          </div>
          <input
            type="number"
            inputMode="numeric"
            min={0}
            placeholder="0"
            value={days}
            onChange={(e) => setDays(e.target.value.replace(/\D/g, ""))}
            className="w-24 rounded-md border border-border bg-card px-3 py-2 font-mono text-sm text-foreground outline-none focus:border-foreground/50 focus:ring-1 focus:ring-foreground/20"
          />
          <span className="text-xs text-muted-foreground">天</span>
        </div>
      </Field>

      {/* 快捷天数 */}
      <div className="flex flex-wrap gap-2">
        <span className="self-center text-label text-muted-foreground">快捷:</span>
        {[7, 15, 30, 60, 90, 180, 365].map((n) => (
          <Button
            key={n}
            variant="outline"
            size="sm"
            onClick={() => {
              if (!start) setStart(todayIso());
              setDays(String(n));
            }}
            className="h-7 text-xs"
          >
            {n} 天
          </Button>
        ))}
      </div>

      {result ? (
        <div className="rounded-md border border-border bg-card px-5 py-4">
          <p className="font-mono text-2xl font-semibold text-foreground">
            {formatChineseDate(result.iso)}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {result.weekday}
            {start && (
              <>
                {" · 从 "}
                {formatChineseDate(start)}
                {direction === 1 ? " 之后 " : " 之前 "}
                {parseInt(days, 10) || 0} 天
              </>
            )}
          </p>
          <CalculatorDisclaimer />
        </div>
      ) : (
        <Placeholder>选择起始日期开始推算</Placeholder>
      )}
    </div>
  );
}

/* ============================ 共享 ============================ */
function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <label className="block text-xs font-medium text-muted-foreground">
        {label}
      </label>
      {children}
    </div>
  );
}

function DateInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <div className="relative flex-1">
        <input
          type="date"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full rounded-md border border-border bg-card px-3 py-2 font-mono text-sm text-foreground outline-none focus:border-foreground/50 focus:ring-1 focus:ring-foreground/20"
        />
      </div>
      <Button
        variant="outline"
        size="sm"
        onClick={() => onChange(todayIso())}
        className="h-9 shrink-0 gap-1 text-xs"
      >
        <Calendar className="size-3.5" />
        今天
      </Button>
    </div>
  );
}

function StatItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-caption uppercase tracking-wider text-muted-foreground">
        {label}
      </dt>
      <dd className="mt-0.5 font-mono text-base font-medium text-foreground">
        {value}
      </dd>
    </div>
  );
}

function Placeholder({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-md border border-dashed border-border/70 bg-muted/20 px-4 py-8 text-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}
