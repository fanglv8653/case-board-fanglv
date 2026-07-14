import { useMemo, useState } from "react";
import { Loader2, Save } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { saveCriminalSentencingEstimate } from "@/lib/api";
import { SENTENCING_DATA } from "./data.ts";
import { sentencingEngine } from "./engine.ts";
import type { SentencingPrefill } from "./prefill.ts";
import type {
  AreaType,
  MonthRange,
  SentencingCalculationResult,
} from "./types.ts";

export const SENTENCING_REVISION_CONFLICT_COPY =
  "刑事画像已被其他操作更新。请返回案件重新加载并复核后，再重新测算保存；系统不会自动重试或覆盖。";

const EMPTY_PREFILL: SentencingPrefill = {
  caseId: null,
  expectedProfileRevision: null,
  crimeName: null,
  crimeCandidates: [],
  amount: null,
  crimeDate: null,
  areaType: null,
  factTier: null,
  factors: {},
  requiresCrimeConfirmation: false,
};

function formatRange(range?: MonthRange): string {
  if (!range) return "—";
  return range[1] == null ? `${range[0]} 个月以上` : `${range[0]}～${range[1]} 个月`;
}

export function SentencingCalculator({ prefill = EMPTY_PREFILL }: { prefill?: SentencingPrefill | null }) {
  const context = prefill ?? EMPTY_PREFILL;
  const [crimeName, setCrimeName] = useState<string>(context.crimeName ?? "");
  const [amount, setAmount] = useState("");
  const [areaType, setAreaType] = useState<AreaType | "">("");
  const [crimeDate, setCrimeDate] = useState("");
  const [factTier, setFactTier] = useState("");
  const [isTelecom, setIsTelecom] = useState(false);
  const [judgeAdjustment, setJudgeAdjustment] = useState("0");
  const [factors, setFactors] = useState<Record<string, boolean>>({});
  const [result, setResult] = useState<SentencingCalculationResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmingSave, setConfirmingSave] = useState(false);

  const crime = SENTENCING_DATA.crimes.find((item) => item.name === crimeName);
  const standards = crime ? SENTENCING_DATA.standards[crime.id] : [];
  const selectableStandards = crime?.id === "fraud"
    ? standards.filter((item) => isTelecom ? item.subType === "电信诈骗" : item.subType !== "电信诈骗")
    : standards;
  const availableAreas = [...new Set(selectableStandards.map((item) => item.area))];
  const factTiers = [...new Set(
    standards
      .filter((item) => item.minAmount == null && item.maxAmount == null)
      .map((item) => item.tier),
  )];
  const allFactors = useMemo(
    () => [...SENTENCING_DATA.priorityFactors, ...SENTENCING_DATA.generalFactors],
    [],
  );

  const update = (action: () => void) => {
    action();
    setResult(null);
    setError(null);
    setConfirmingSave(false);
  };

  const calculate = () => {
    setConfirmingSave(false);
    const numericAmount = Number(amount);
    const adjustment = Number(judgeAdjustment);
    if (!crime) return setError("请选择数据表中支持的罪名。未知罪名不能测算。");
    if (!crimeDate) return setError("请填写犯罪日期；案件受理、羁押等程序日期不能替代犯罪日期。");
    if (amount.trim() === "" || !Number.isFinite(numericAmount) || numericAmount < 0) {
      return setError("请填写有效涉案金额；非金额型犯罪请明确填写 0。");
    }
    if (!areaType) return setError("请选择适用地区，不能从法院名称自动推断。");
    if (factTiers.length > 1 && !factTier) return setError("请选择案件事实档位。");
    if (!Number.isFinite(adjustment) || adjustment < -20 || adjustment > 20) {
      return setError("人工微调须为 -20% 至 20%。");
    }
    const next = sentencingEngine.calculate({
      crimeName,
      amount: numericAmount,
      areaType,
      factors,
      crimeDate,
      judgeAdjustment: adjustment,
      isTelecom,
      factTier: factTier || null,
    });
    setError(next.error ?? null);
    setResult(next.error ? null : next);
  };

  const save = async () => {
    if (!result?.finalPenaltyRange || !context.caseId || context.expectedProfileRevision == null) return;
    setSaving(true);
    try {
      await saveCriminalSentencingEstimate({
        case_id: context.caseId,
        expected_profile_revision: context.expectedProfileRevision,
        input_snapshot: {
          crimeName: result.crimeName,
          amount: result.amount,
          areaType: result.areaType,
          crimeDate: result.crimeDate,
          factTier: result.factTier,
          factors: result.factors,
          isTelecom: result.isTelecom,
          judgeAdjustment: result.judgeAdjustment,
        },
        output_min_months: result.finalPenaltyRange[0],
        output_max_months: result.finalPenaltyRange[1],
        output_snapshot: {
          startingPointRange: result.startingPointRange,
          basePenaltyRange: result.basePenaltyRange,
          finalPenaltyRange: result.finalPenaltyRange,
          finalSentence: result.finalSentence,
          tier: result.tier,
        },
        process_snapshot: result.process,
        basis_snapshot: {
          standard: result.standardDetail,
          legalReferences: result.legalReferences,
          crimeDate: result.crimeDate,
        },
        created_source: "sentencing_calculator_ui",
      });
      toast("量刑测算已另存为独立记录，未修改刑事画像。", "success");
      setConfirmingSave(false);
    } catch (cause) {
      const message = String(cause);
      setError(
        message.includes("SENTENCING_ESTIMATE_REVISION_CONFLICT")
          ? SENTENCING_REVISION_CONFLICT_COPY
          : `保存测算记录失败：${message}`,
      );
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-5" data-testid="sentencing-calculator">
      {context.caseId && (
        <div className="rounded-lg border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-900 dark:border-blue-900/60 dark:bg-blue-950/30 dark:text-blue-200">
          已从刑事案件进入。仅精确罪名可预填，其余关键字段必须人工复核；页面不会自动计算或保存。
          {context.requiresCrimeConfirmation && (
            <div className="mt-2">检测到多个候选罪名，请人工选择：{context.crimeCandidates.join("、")}</div>
          )}
        </div>
      )}

      <div className="grid gap-4 rounded-xl border border-border bg-card p-5 md:grid-cols-2">
        <label className="space-y-1 text-sm">罪名
          <select value={crimeName} onChange={(event) => update(() => { setCrimeName(event.target.value); setFactTier(""); setAreaType(""); })} className="h-10 w-full rounded-md border border-border bg-background px-3">
            <option value="">请选择</option>
            {SENTENCING_DATA.crimes.map((item) => <option key={item.id} value={item.name}>{item.name}</option>)}
          </select>
        </label>
        <label className="space-y-1 text-sm">涉案金额（元）
          <input type="number" min="0" value={amount} onChange={(event) => update(() => setAmount(event.target.value))} placeholder="非金额型犯罪请填写 0" className="h-10 w-full rounded-md border border-border bg-background px-3" />
        </label>
        <label className="space-y-1 text-sm">适用地区
          <select value={areaType} onChange={(event) => update(() => setAreaType(event.target.value as AreaType | ""))} className="h-10 w-full rounded-md border border-border bg-background px-3">
            <option value="">请选择</option>
            {(availableAreas.length ? availableAreas : ["一类地区", "二类地区", "全国"] as AreaType[]).map((area) => <option key={area} value={area}>{area}</option>)}
          </select>
        </label>
        <label className="space-y-1 text-sm">犯罪日期
          <input type="date" value={crimeDate} onChange={(event) => update(() => setCrimeDate(event.target.value))} className="h-10 w-full rounded-md border border-border bg-background px-3" />
        </label>
        {factTiers.length > 1 && (
          <label className="space-y-1 text-sm">案件事实档位
            <select value={factTier} onChange={(event) => update(() => setFactTier(event.target.value))} className="h-10 w-full rounded-md border border-border bg-background px-3">
              <option value="">请选择</option>
              {factTiers.map((tier) => <option key={tier} value={tier}>{tier}</option>)}
            </select>
          </label>
        )}
        <label className="space-y-1 text-sm">人工微调（-20%～20%）
          <input type="number" min="-20" max="20" value={judgeAdjustment} onChange={(event) => update(() => setJudgeAdjustment(event.target.value))} className="h-10 w-full rounded-md border border-border bg-background px-3" />
        </label>
        {crimeName === "诈骗罪" && (
          <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={isTelecom} onChange={(event) => update(() => { setIsTelecom(event.target.checked); setAreaType(""); })} />电信网络诈骗</label>
        )}
      </div>

      <section className="rounded-xl border border-border bg-card p-5">
        <h3 className="mb-3 text-sm font-semibold">量刑情节（须人工逐项确认）</h3>
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {allFactors.map((factor) => (
            <label key={factor.id} className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={!!factors[factor.name]} onChange={(event) => update(() => setFactors((current) => ({ ...current, [factor.name]: event.target.checked })))} />
              {factor.name}
            </label>
          ))}
        </div>
      </section>

      {error && <div role="alert" className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-200">{error}</div>}
      <Button type="button" onClick={calculate}>开始测算</Button>

      {result && (
        <section className="space-y-4 rounded-xl border border-border bg-card p-5" data-testid="sentencing-result">
          <div className="grid gap-3 sm:grid-cols-3">
            <ResultCard label="量刑起点" value={formatRange(result.startingPointRange)} />
            <ResultCard label="基准刑" value={formatRange(result.basePenaltyRange)} />
            <ResultCard label="最终区间" value={formatRange(result.finalPenaltyRange)} />
          </div>
          <div className="text-sm">档位：{result.tierLabel ?? "—"}；适用地区：{result.standardDetail?.area ?? "—"}；犯罪日期：{result.crimeDate}</div>
          <div>
            <h3 className="mb-2 text-sm font-semibold">计算过程</h3>
            <ol className="space-y-1 text-sm text-muted-foreground">{result.process.map((entry, index) => <li key={`${entry.step}-${index}`}>{index + 1}. {entry.step}：{entry.detail}</li>)}</ol>
          </div>
          <div>
            <h3 className="mb-2 text-sm font-semibold">依据快照</h3>
            <ul className="list-disc space-y-1 pl-5 text-sm text-muted-foreground">{result.legalReferences?.map((reference) => <li key={reference}>{reference}</li>)}</ul>
          </div>
          {context.caseId && context.expectedProfileRevision == null && (
            <div className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
              当前案件尚无已保存的刑事画像。可继续独立测算，但需先返回案件保存刑事画像，才能保存测算记录。
            </div>
          )}
          {context.caseId && context.expectedProfileRevision != null && !confirmingSave && (
            <Button type="button" onClick={() => setConfirmingSave(true)}>
              <Save className="size-4" />
              保存到案件画像（测算记录）
            </Button>
          )}
          {context.caseId && context.expectedProfileRevision != null && confirmingSave && (
            <div className="rounded-lg border border-border bg-muted/40 p-4">
              <p className="text-sm font-medium">确认保存本次测算？</p>
              <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                本操作只会追加一条独立测算记录，不会覆盖刑事画像字段，也不会改变案件阶段；保存后不会自动回写测算结论。
              </p>
              <div className="mt-3 flex gap-2">
                <Button type="button" variant="ghost" onClick={() => setConfirmingSave(false)} disabled={saving}>取消</Button>
                <Button type="button" onClick={save} disabled={saving}>
                  {saving && <Loader2 className="size-4 animate-spin" />}
                  确认追加独立记录
                </Button>
              </div>
            </div>
          )}
        </section>
      )}

      <p className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-xs leading-relaxed text-amber-900 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
        本工具仅供办案辅助与内部复核，不构成量刑承诺或法律意见。实际裁判须结合完整证据、犯罪事实、行为时法律及现行规范，由办案人员人工判断。
      </p>
    </div>
  );
}

function ResultCard({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg bg-muted/50 p-3"><div className="text-xs text-muted-foreground">{label}</div><div className="mt-1 font-semibold">{value}</div></div>;
}
