/**
 * 律师费计算器 — React 原生实现(2026-05-24 e)。
 *
 * 两种模式:
 *   1. 一口价(fixed):按标的额分档算固定费 + ±20% 浮动区间
 *   2. 基础 + 风险(risk):按案件难度(简单/一般/困难)分基础 + 风险代理两部分
 *
 * 计算逻辑见 ../lib/lawyerFee.ts(100% 移植自 lawtools.top/fee.html)。
 */

import { useMemo, useState } from "react";

import { CalculatorDisclaimer, DetailRow, TabBtn } from "./ui";

import {
  calculateFixed,
  calculateRiskBreakdown,
  type Difficulty,
  DIFFICULTY,
  formatMoneyWan,
} from "../lib/lawyerFee";

type Mode = "fixed" | "risk";

export function LawyerFeeCalculator() {
  const [amountStr, setAmountStr] = useState("");
  const [mode, setMode] = useState<Mode>("fixed");
  const [difficulty, setDifficulty] = useState<Difficulty>("normal");

  const amount = useMemo(() => {
    const n = parseFloat(amountStr);
    return isNaN(n) || n <= 0 ? null : n;
  }, [amountStr]);

  return (
    <div className="space-y-5">
      {/* 标的额输入 */}
      <div className="space-y-1.5">
        <label
          htmlFor="fee-amount"
          className="block text-xs font-medium text-muted-foreground"
        >
          标的额(单位:万元)
        </label>
        <div className="relative">
          <input
            id="fee-amount"
            type="number"
            inputMode="decimal"
            min={0}
            step={0.1}
            placeholder="例如:300"
            value={amountStr}
            onChange={(e) => setAmountStr(e.target.value)}
            className="w-full rounded-md border border-border bg-card px-4 py-3 pr-14 font-mono text-lg text-foreground outline-none focus:border-foreground/50 focus:ring-1 focus:ring-foreground/20"
            autoFocus
          />
          <span className="pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 text-sm text-muted-foreground">
            万元
          </span>
        </div>
      </div>

      {/* 模式 tab */}
      <div className="inline-flex rounded-md border border-border bg-card p-0.5">
        <TabBtn active={mode === "fixed"} onClick={() => setMode("fixed")}>
          一口价
        </TabBtn>
        <TabBtn active={mode === "risk"} onClick={() => setMode("risk")}>
          基础 + 风险
        </TabBtn>
      </div>

      {/* 案件难度(仅 risk 模式) */}
      {mode === "risk" && (
        <div className="space-y-1.5">
          <label className="block text-xs font-medium text-muted-foreground">
            案件难度
          </label>
          <div className="inline-flex rounded-md border border-border bg-card p-0.5">
            {(Object.keys(DIFFICULTY) as Difficulty[]).map((k) => (
              <TabBtn
                key={k}
                active={difficulty === k}
                onClick={() => setDifficulty(k)}
              >
                {DIFFICULTY[k].label}
              </TabBtn>
            ))}
          </div>
        </div>
      )}

      {/* 结果 */}
      {amount === null ? (
        <Placeholder>输入标的额,自动生成报价方案</Placeholder>
      ) : mode === "fixed" ? (
        <FixedResult amount={amount} />
      ) : (
        <RiskResult amount={amount} difficulty={difficulty} />
      )}
    </div>
  );
}

/* ============================ 一口价结果 ============================ */
function FixedResult({ amount }: { amount: number }) {
  const fee = calculateFixed(amount);
  const low = fee * 0.8;
  const high = fee * 1.2;

  return (
    <ResultCard label="一口价 · 建议报价" main={formatMoneyWan(fee)} sub={`标的额 ${amount} 万元 · 固定收费模式`}>
      <div className="grid grid-cols-2 gap-3 pt-2">
        <RangeItem label="浮动下限 ×0.8" value={formatMoneyWan(low)} />
        <RangeItem label="浮动上限 ×1.2" value={formatMoneyWan(high)} />
      </div>
      <dl className="border-t border-border/70 pt-3 text-sm">
        <DetailRow label="合理区间" value={`${formatMoneyWan(low)} ~ ${formatMoneyWan(high)}`} />
        <DetailRow label="计费基准" value={`${amount} 万元标的额`} />
        <DetailRow label="收费方式" value="一次性固定收费" />
      </dl>
    </ResultCard>
  );
}

/* ============================ 基础 + 风险结果 ============================ */
function RiskResult({
  amount,
  difficulty,
}: {
  amount: number;
  difficulty: Difficulty;
}) {
  const r = calculateRiskBreakdown(amount, difficulty);
  return (
    <ResultCard
      label={`基础 + 风险 · ${r.def.label}`}
      main={formatMoneyWan(r.total)}
      sub={`标的额 ${amount} 万元 · 混合收费模式`}
    >
      <div className="grid grid-cols-2 gap-3 pt-2">
        <RangeItem label="基础服务费" value={formatMoneyWan(r.baseFee)} />
        <RangeItem label="风险代理费率" value={`${r.def.riskRate}%`} />
      </div>
      <dl className="border-t border-border/70 pt-3 text-sm">
        <DetailRow label="预期回款" value={`${formatMoneyWan(r.expectAmount)}(回款率 ${r.def.expectRate}%)`} />
        <DetailRow label="实际风险代理费" value={formatMoneyWan(r.actualRiskFee)} />
        <DetailRow label="基础 / 风险比例" value={`${r.def.ratio[0]}% / ${r.def.ratio[1]}%`} />
        <DetailRow label="合计期望费用" value={formatMoneyWan(r.total)} />
      </dl>
    </ResultCard>
  );
}

/* ============================ 共享 ============================ */
function ResultCard({
  label,
  main,
  sub,
  children,
}: {
  label: string;
  main: string;
  sub: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-3 rounded-md border border-border bg-card px-5 py-4">
      <div>
        <p className="text-caption uppercase tracking-wider text-muted-foreground">
          {label}
        </p>
        <p className="mt-1 font-mono text-3xl font-semibold text-foreground">
          {main}
        </p>
        <p className="mt-0.5 text-xs text-muted-foreground">{sub}</p>
      </div>
      {children}
      <CalculatorDisclaimer />
    </div>
  );
}

function RangeItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border/70 bg-muted/20 px-3 py-2">
      <p className="text-caption uppercase tracking-wider text-muted-foreground">
        {label}
      </p>
      <p className="mt-0.5 font-mono text-base font-medium text-foreground">
        {value}
      </p>
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
