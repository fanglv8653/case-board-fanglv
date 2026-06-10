/**
 * 诉讼费计算器 — React 原生实现(2026-05-24 e)。
 *
 * 两个案件类型:
 *   1. 财产案件:按标的额 9 档累进
 *   2. 离婚案件:每件 200 元 + 可选财产分割超 20 万部分加 0.5%
 * 都可勾选「同时计算保全费」。
 *
 * 计算逻辑见 ../lib/litigationFee.ts(100% 移植自 lawtools.top/legalfee.html)。
 */

import { useMemo, useState } from "react";

import { DetailRow, TabBtn } from "./ui";

import {
  LegalBasisButton,
  LegalBasisModal,
} from "../components/LegalBasisModal";
import { LITIGATION_FEE_BASIS } from "../lib/legalBasisData";
import {
  calculateDivorceFee,
  calculatePreservationFee,
  calculatePropertyFee,
  formatFeeYuan,
} from "../lib/litigationFee";

type CaseType = "property" | "divorce";

export function LitigationFeeCalculator() {
  const [type, setType] = useState<CaseType>("property");
  const [basisOpen, setBasisOpen] = useState(false);

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="inline-flex rounded-md border border-border bg-card p-0.5">
          <TabBtn active={type === "property"} onClick={() => setType("property")}>
            财产案件
          </TabBtn>
          <TabBtn active={type === "divorce"} onClick={() => setType("divorce")}>
            离婚案件
          </TabBtn>
        </div>
        <LegalBasisButton onClick={() => setBasisOpen(true)}>
          查看计费依据 · 《诉讼费用交纳办法》
        </LegalBasisButton>
      </div>

      {type === "property" ? <PropertyPanel /> : <DivorcePanel />}

      <LegalBasisModal
        open={basisOpen}
        onClose={() => setBasisOpen(false)}
        title="诉讼费计费法律依据"
        sections={LITIGATION_FEE_BASIS}
      />
    </div>
  );
}

/* ============================ 财产案件 ============================ */
function PropertyPanel() {
  const [amountStr, setAmountStr] = useState("");
  const [withPreservation, setWithPreservation] = useState(false);

  const result = useMemo(() => {
    const n = parseFloat(amountStr);
    if (isNaN(n) || n < 0) return null;
    const base = calculatePropertyFee(n);
    const pres = withPreservation ? calculatePreservationFee(n) : null;
    return { base, pres, total: base + (pres ?? 0) };
  }, [amountStr, withPreservation]);

  return (
    <div className="space-y-4">
      <Field label="争议标的额(单位:万元)">
        <AmountInput value={amountStr} onChange={setAmountStr} placeholder="例如:50" />
      </Field>

      <Checkbox
        checked={withPreservation}
        onChange={setWithPreservation}
        label="同时计算财产保全费(按争议标的额计)"
      />

      {result ? (
        <ResultDisplay base={result.base} pres={result.pres} subLabel="案件受理费" />
      ) : (
        <Placeholder>输入争议标的额,实时计算诉讼费</Placeholder>
      )}
    </div>
  );
}

/* ============================ 离婚案件 ============================ */
function DivorcePanel() {
  const [hasSplit, setHasSplit] = useState(false);
  const [splitAmountStr, setSplitAmountStr] = useState("");
  const [withPreservation, setWithPreservation] = useState(false);
  const [presAmountStr, setPresAmountStr] = useState("");

  const result = useMemo(() => {
    const splitAmount = parseFloat(splitAmountStr);
    const splitValid = !isNaN(splitAmount) && splitAmount >= 0;
    const amount = hasSplit && splitValid ? splitAmount : 0;
    const base = calculateDivorceFee(amount, hasSplit);

    let pres: number | null = null;
    if (withPreservation) {
      const p = parseFloat(presAmountStr);
      if (!isNaN(p) && p >= 0) {
        pres = calculatePreservationFee(p);
      }
    }
    return { base, pres };
  }, [hasSplit, splitAmountStr, withPreservation, presAmountStr]);

  return (
    <div className="space-y-4">
      <Checkbox
        checked={hasSplit}
        onChange={setHasSplit}
        label="涉及财产分割(超过 20 万元部分按 0.5% 累加)"
      />

      {hasSplit && (
        <Field label="财产分割金额(单位:万元)">
          <AmountInput
            value={splitAmountStr}
            onChange={setSplitAmountStr}
            placeholder="例如:30"
          />
        </Field>
      )}

      <Checkbox
        checked={withPreservation}
        onChange={setWithPreservation}
        label="同时计算财产保全费"
      />

      {withPreservation && (
        <Field label="保全标的额(单位:万元)">
          <AmountInput
            value={presAmountStr}
            onChange={setPresAmountStr}
            placeholder="例如:50"
          />
        </Field>
      )}

      <ResultDisplay base={result.base} pres={result.pres} subLabel="案件受理费(离婚案件)" />
    </div>
  );
}

/* ============================ 共享:结果显示 ============================ */
function ResultDisplay({
  base,
  pres,
  subLabel,
}: {
  base: number;
  pres: number | null;
  subLabel: string;
}) {
  const total = base + (pres ?? 0);
  const showBreakdown = pres !== null;

  return (
    <div className="space-y-3 rounded-md border border-border bg-card px-5 py-4">
      <div>
        <p className="text-caption uppercase tracking-wider text-muted-foreground">
          {showBreakdown ? "诉讼费 + 保全费合计" : subLabel}
        </p>
        <p className="mt-1 font-mono text-3xl font-semibold text-foreground">
          {formatFeeYuan(total)}
        </p>
      </div>

      {showBreakdown && (
        <dl className="border-t border-border/70 pt-3 text-sm">
          <DetailRow label="案件受理费" value={formatFeeYuan(base)} />
          <DetailRow label="财产保全费" value={formatFeeYuan(pres)} />
          <DetailRow label="合计" value={formatFeeYuan(total)} strong />
        </dl>
      )}
    </div>
  );
}

/* ============================ 共享 UI 元件 ============================ */
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

function AmountInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <div className="relative">
      <input
        type="number"
        inputMode="decimal"
        min={0}
        step={0.1}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full rounded-md border border-border bg-card px-3 py-2 pr-12 font-mono text-sm text-foreground outline-none focus:border-foreground/50 focus:ring-1 focus:ring-foreground/20"
      />
      <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground">
        万元
      </span>
    </div>
  );
}

function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2 text-sm text-foreground">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-4 cursor-pointer accent-foreground"
      />
      <span>{label}</span>
    </label>
  );
}

function Placeholder({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-md border border-dashed border-border/70 bg-muted/20 px-4 py-8 text-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}
