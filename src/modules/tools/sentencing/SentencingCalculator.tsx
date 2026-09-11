import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, History, Loader2, RefreshCw, Save } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import {
  listCriminalSentencingEstimates,
  openUrl,
  saveCriminalSentencingEstimate,
} from "@/lib/api";
import type { CriminalSentencingEstimate } from "@/lib/types";
import { factorAppliesTo, SENTENCING_DATA } from "./data.ts";
import { sentencingEngine } from "./engine.ts";
import type { SentencingPrefill } from "./prefill.ts";
import type {
  AreaType,
  MonthRange,
  SentencingCalculationResult,
  StatutoryPenaltyBand,
  TemporalRuleBasis,
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

function formatRange(range?: MonthRange | null): string {
  if (!range) return "—";
  if (range[0] === 0 && range[1] === 0) return "免予刑事处罚";
  return range[1] == null ? `${range[0]} 个月以上` : `${range[0]}～${range[1]} 个月`;
}

function formatStatutory(band?: StatutoryPenaltyBand): string {
  if (!band) return "—";
  return `${band.label}（${band.article}）`;
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function historyRange(item: CriminalSentencingEstimate): MonthRange {
  return [item.output_min_months, item.output_max_months];
}

function initialAreaForCrime(crimeName: string | null): AreaType | "" {
  const crime = SENTENCING_DATA.crimes.find((item) => item.name === crimeName);
  if (!crime) return "";
  const areas = [...new Set(SENTENCING_DATA.standards[crime.id].map((item) => item.area))];
  return areas.length === 1 ? areas[0] : "";
}

export function SentencingCalculator({ prefill = EMPTY_PREFILL }: { prefill?: SentencingPrefill | null }) {
  const context = prefill ?? EMPTY_PREFILL;
  const [crimeName, setCrimeName] = useState<string>(context.crimeName ?? "");
  const [amount, setAmount] = useState("");
  const [areaType, setAreaType] = useState<AreaType | "">(() => initialAreaForCrime(context.crimeName));
  const [crimeDate, setCrimeDate] = useState("");
  const [factTier, setFactTier] = useState("");
  const [isTelecom, setIsTelecom] = useState(false);
  const [temporalRuleBasis, setTemporalRuleBasis] = useState<TemporalRuleBasis | "">("");
  const [manualBaseMinimum, setManualBaseMinimum] = useState("");
  const [manualBaseMaximum, setManualBaseMaximum] = useState("");
  const [manualBaseSource, setManualBaseSource] = useState("");
  const [judgeAdjustment, setJudgeAdjustment] = useState("0");
  const [judgeAdjustmentReason, setJudgeAdjustmentReason] = useState("");
  const [factors, setFactors] = useState<Record<string, boolean>>({});
  const [result, setResult] = useState<SentencingCalculationResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmingSave, setConfirmingSave] = useState(false);
  const [history, setHistory] = useState<CriminalSentencingEstimate[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [expandedHistoryId, setExpandedHistoryId] = useState<string | null>(null);

  const crime = SENTENCING_DATA.crimes.find((item) => item.name === crimeName);
  const standards = crime ? SENTENCING_DATA.standards[crime.id] : [];
  const selectableStandards = crime?.id === "fraud"
    ? standards.filter((item) => isTelecom ? item.subType === "电信诈骗" : item.subType !== "电信诈骗")
    : standards;
  const availableAreas = [...new Set(selectableStandards.map((item) => item.area))];
  const factTiers = [...new Set(selectableStandards.map((item) => item.tier))];
  const requiresManualFactTier = factTiers.length > 1
    && selectableStandards.every((item) => item.minAmount == null && item.maxAmount == null);
  const requiresTemporalReview = crime?.id === "embezzlement" || crime?.id === "non_official_bribery";
  const availableFactors = useMemo(
    () => [...SENTENCING_DATA.priorityFactors, ...SENTENCING_DATA.generalFactors]
      .filter((factor) => !crime || factorAppliesTo(factor.id, crime.id, factTier || null)),
    [crime, factTier],
  );

  const loadHistory = useCallback(async () => {
    if (!context.caseId) return;
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      setHistory(await listCriminalSentencingEstimates(context.caseId));
    } catch (cause) {
      setHistoryError(`读取历史测算失败：${String(cause)}`);
    } finally {
      setHistoryLoading(false);
    }
  }, [context.caseId]);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

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
    const effectiveArea = areaType || (availableAreas.length === 1 ? availableAreas[0] : "");
    const hasAnyBaseInput = manualBaseMinimum.trim() !== "" || manualBaseMaximum.trim() !== "";
    const manualBase = hasAnyBaseInput
      ? [Number(manualBaseMinimum), Number(manualBaseMaximum)] as MonthRange
      : null;

    if (!crime) return setError("请选择数据表中支持的罪名。未知罪名不能测算。");
    if (!crimeDate) return setError("请填写犯罪日期；案件受理、羁押等程序日期不能替代犯罪日期。");
    if (amount.trim() === "" || !Number.isFinite(numericAmount) || numericAmount < 0) {
      return setError("请填写有效涉案金额；非金额型犯罪请明确填写 0。");
    }
    if (!effectiveArea) return setError("请选择适用地区，不能从法院名称自动推断。");
    if (requiresManualFactTier && !factTier) return setError("当前金额标准未自动配置，请人工确认案件事实档位。");
    if (hasAnyBaseInput && (manualBaseMinimum.trim() === "" || manualBaseMaximum.trim() === "")) {
      return setError("人工核验基准刑必须同时填写上下限。");
    }
    if (!Number.isFinite(adjustment) || adjustment < -20 || adjustment > 20) {
      return setError("裁判情景调整须为 -20% 至 20%。");
    }

    const next = sentencingEngine.calculate({
      crimeName,
      amount: numericAmount,
      areaType: effectiveArea,
      factors,
      crimeDate,
      judgeAdjustment: adjustment,
      judgeAdjustmentReason,
      isTelecom,
      factTier: factTier || null,
      manualBasePenaltyRange: manualBase,
      manualBaseSource,
      temporalRuleBasis: temporalRuleBasis || null,
    });
    setError(next.error ?? null);
    setResult(next);
  };

  const save = async () => {
    if (
      result?.calculationStatus !== "complete"
      || !result.finalPenaltyRange
      || !context.caseId
      || context.expectedProfileRevision == null
    ) return;
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
          temporalRuleBasis: result.temporalRuleBasis,
          manualBasePenaltyRange: result.manualBasePenaltyRange,
          manualBaseSource: result.manualBaseSource,
          judgeAdjustment: result.judgeAdjustment,
          judgeAdjustmentReason: result.judgeAdjustmentReason,
        },
        output_min_months: result.finalPenaltyRange[0],
        output_max_months: result.finalPenaltyRange[1],
        output_snapshot: {
          calculationStatus: result.calculationStatus,
          disposition: result.disposition,
          statutoryPenalty: result.statutoryPenalty,
          startingPointRange: result.startingPointRange,
          basePenaltyRange: result.basePenaltyRange,
          rawAdjustedPenaltyRange: result.rawAdjustedPenaltyRange,
          finalPenaltyRange: result.finalPenaltyRange,
          finalPenaltyKinds: result.finalPenaltyKinds,
          finalSentence: result.finalSentence,
          tier: result.tier,
          warnings: result.warnings,
        },
        process_snapshot: result.process,
        basis_snapshot: {
          ruleset: result.ruleset,
          standard: result.standardDetail,
          legalReferences: result.legalReferences,
          crimeDate: result.crimeDate,
          temporalRuleBasis: result.temporalRuleBasis,
          manualBaseSource: result.manualBaseSource,
        },
        created_source: "sentencing_calculator_ui",
      });
      toast("量刑测算已追加为独立、可追溯记录，未修改刑事画像。", "success");
      setConfirmingSave(false);
      await loadHistory();
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
          <select value={crimeName} onChange={(event) => update(() => {
            const nextName = event.target.value;
            const nextCrime = SENTENCING_DATA.crimes.find((item) => item.name === nextName);
            const nextAreas = nextCrime ? [...new Set(SENTENCING_DATA.standards[nextCrime.id].map((item) => item.area))] : [];
            setCrimeName(nextName);
            setFactTier("");
            setAreaType(nextAreas.length === 1 ? nextAreas[0] : "");
            setFactors({});
            setTemporalRuleBasis("");
          })} className="h-10 w-full rounded-md border border-border bg-background px-3">
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
            {availableAreas.map((area) => <option key={area} value={area}>{area}</option>)}
          </select>
        </label>
        <label className="space-y-1 text-sm">犯罪日期
          <input type="date" value={crimeDate} onChange={(event) => update(() => setCrimeDate(event.target.value))} className="h-10 w-full rounded-md border border-border bg-background px-3" />
        </label>
        {factTiers.length > 1 && (
          <label className="space-y-1 text-sm">案件事实档位{requiresManualFactTier ? "（必选）" : "（可人工覆盖金额初判）"}
            <select value={factTier} onChange={(event) => update(() => { setFactTier(event.target.value); setFactors({}); })} className="h-10 w-full rounded-md border border-border bg-background px-3">
              <option value="">{requiresManualFactTier ? "请选择" : "按金额初步定位"}</option>
              {factTiers.map((tier) => <option key={tier} value={tier}>{tier}</option>)}
            </select>
          </label>
        )}
        {crimeName === "诈骗罪" && (
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={isTelecom} onChange={(event) => update(() => { setIsTelecom(event.target.checked); setFactTier(""); setFactors({}); })} />
            电信网络诈骗（仍须人工确认数额档位）
          </label>
        )}
        {requiresTemporalReview && (
          <label className="space-y-1 text-sm md:col-span-2">法释〔2026〕6号时间效力核验
            <select value={temporalRuleBasis} onChange={(event) => update(() => setTemporalRuleBasis(event.target.value as TemporalRuleBasis | ""))} className="h-10 w-full rounded-md border border-border bg-background px-3">
              <option value="">请选择人工核验结论</option>
              <option value="conduct_on_or_after_2026_05_01">行为发生于2026年5月1日以后，适用现行参照标准</option>
              <option value="pre_effective_current_rule_reviewed">行为在施行日前，已核对时间效力并确认采用当前规则</option>
              <option value="individual_comparison_required">尚需个案新旧规则有利性比较（将停止数值测算）</option>
            </select>
          </label>
        )}
      </div>

      <section className="rounded-xl border border-border bg-card p-5">
        <h3 className="text-sm font-semibold">人工核验基准刑</h3>
        <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
          除危险驾驶罪外，本工具不再依据现行状态未核实的旧地方细则自动增加刑罚量。请先结合完整事实和本案实际适用依据确定基准刑，再进行情节调节。
        </p>
        <div className="mt-3 grid gap-4 md:grid-cols-3">
          <label className="space-y-1 text-sm">下限（月）
            <input type="number" min="0" value={manualBaseMinimum} onChange={(event) => update(() => setManualBaseMinimum(event.target.value))} className="h-10 w-full rounded-md border border-border bg-background px-3" />
          </label>
          <label className="space-y-1 text-sm">上限（月）
            <input type="number" min="0" value={manualBaseMaximum} onChange={(event) => update(() => setManualBaseMaximum(event.target.value))} className="h-10 w-full rounded-md border border-border bg-background px-3" />
          </label>
          <label className="space-y-1 text-sm">核验来源
            <input value={manualBaseSource} onChange={(event) => update(() => setManualBaseSource(event.target.value))} placeholder="实施细则条款/量刑建议/阅卷底稿定位" className="h-10 w-full rounded-md border border-border bg-background px-3" />
          </label>
        </div>
      </section>

      <section className="rounded-xl border border-border bg-card p-5">
        <h3 className="mb-3 text-sm font-semibold">量刑情节（须人工逐项确认）</h3>
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {availableFactors.map((factor) => (
            <label key={factor.id} className="flex items-start gap-2 text-sm">
              <input className="mt-0.5" type="checkbox" checked={!!factors[factor.name]} onChange={(event) => update(() => setFactors((current) => ({ ...current, [factor.name]: event.target.checked })))} />
              <span>{factor.name}{!factor.quantitative && <span className="ml-1 text-amber-700 dark:text-amber-300">（定性复核）</span>}</span>
            </label>
          ))}
        </div>
      </section>

      <section className="rounded-xl border border-border bg-card p-5">
        <h3 className="text-sm font-semibold">裁判情景模拟（非规范性自动计算）</h3>
        <div className="mt-3 grid gap-4 md:grid-cols-2">
          <label className="space-y-1 text-sm">调整幅度（-20%～20%）
            <input type="number" min="-20" max="20" value={judgeAdjustment} onChange={(event) => update(() => setJudgeAdjustment(event.target.value))} className="h-10 w-full rounded-md border border-border bg-background px-3" />
          </label>
          <label className="space-y-1 text-sm">具体理由（非零时必填）
            <input value={judgeAdjustmentReason} onChange={(event) => update(() => setJudgeAdjustmentReason(event.target.value))} placeholder="例如：同类案件量刑偏离情景，仅用于压力测试" className="h-10 w-full rounded-md border border-border bg-background px-3" />
          </label>
        </div>
      </section>

      {error && <div role="alert" className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-200">{error}</div>}
      <Button type="button" onClick={calculate}>开始测算</Button>

      {result && (
        <section className="space-y-4 rounded-xl border border-border bg-card p-5" data-testid="sentencing-result">
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <ResultCard label="法定刑" value={formatStatutory(result.statutoryPenalty)} />
            <ResultCard label="量刑起点" value={formatRange(result.startingPointRange)} />
            <ResultCard label="人工核验基准刑" value={formatRange(result.basePenaltyRange)} />
            <ResultCard label="月数调节结果" value={formatRange(result.finalPenaltyRange)} />
          </div>
          {result.finalSentence && <div className="rounded-lg border border-border bg-muted/40 p-3 text-sm font-semibold">辅助结论：{result.finalSentence}</div>}
          <div className="text-sm">档位：{result.tierLabel ?? "—"}；适用地区：{result.standardDetail?.area ?? "—"}；犯罪日期：{result.crimeDate}</div>
          {result.blockingIssues.length > 0 && (
            <div className="rounded-lg border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200">
              <div className="font-semibold">数值测算已停止</div>
              <ul className="mt-1 list-disc space-y-1 pl-5">{result.blockingIssues.map((issue) => <li key={issue}>{issue}</li>)}</ul>
            </div>
          )}
          {result.warnings.length > 0 && (
            <div className="rounded-lg border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-900 dark:border-blue-900/60 dark:bg-blue-950/30 dark:text-blue-200">
              <div className="font-semibold">复核提示</div>
              <ul className="mt-1 list-disc space-y-1 pl-5">{result.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>
            </div>
          )}
          <div>
            <h3 className="mb-2 text-sm font-semibold">计算过程</h3>
            <ol className="space-y-1 text-sm text-muted-foreground">{result.process.map((entry, index) => <li key={`${entry.step}-${index}`}>{index + 1}. {entry.step}：{entry.detail}</li>)}</ol>
          </div>
          <div>
            <h3 className="mb-2 text-sm font-semibold">规则与依据快照</h3>
            <div className="text-xs text-muted-foreground">
              规则集 {result.ruleset.version} / 引擎 {result.ruleset.engineVersion} / {result.ruleset.contentHash}
            </div>
            <ul className="mt-2 list-disc space-y-1 pl-5 text-sm text-muted-foreground">
              {result.ruleset.sources.filter((source) => result.standardDetail?.sourceIds.includes(source.id)).map((source) => (
                <li key={source.id}>
                  <button type="button" className="text-left underline" onClick={() => void openUrl(source.url).catch((cause) => setError(`打开法源失败：${String(cause)}`))}>{source.title}</button>
                  {source.documentNo ? `（${source.documentNo}）` : ""}
                </li>
              ))}
              {result.legalReferences?.map((reference) => <li key={reference}>{reference}</li>)}
            </ul>
          </div>
          {context.caseId && context.expectedProfileRevision == null && (
            <div className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
              当前案件尚无已保存的刑事画像。可继续独立测算，但需先返回案件保存刑事画像，才能保存测算记录。
            </div>
          )}
          {result.calculationStatus === "complete" && context.caseId && context.expectedProfileRevision != null && !confirmingSave && (
            <Button type="button" onClick={() => setConfirmingSave(true)}>
              <Save className="size-4" />
              追加独立测算记录
            </Button>
          )}
          {result.calculationStatus === "complete" && context.caseId && context.expectedProfileRevision != null && confirmingSave && (
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

      {context.caseId && (
        <section className="rounded-xl border border-border bg-card p-5" data-testid="sentencing-history">
          <div className="flex items-center justify-between gap-3">
            <h3 className="flex items-center gap-2 text-sm font-semibold"><History className="size-4" />历史测算记录</h3>
            <Button type="button" variant="ghost" size="sm" onClick={() => void loadHistory()} disabled={historyLoading}>
              <RefreshCw className={`size-4 ${historyLoading ? "animate-spin" : ""}`} />刷新
            </Button>
          </div>
          {historyError && <div role="alert" className="mt-3 text-sm text-red-700 dark:text-red-300">{historyError}</div>}
          {!historyLoading && history.length === 0 && !historyError && <p className="mt-3 text-sm text-muted-foreground">暂无已保存测算。</p>}
          <div className="mt-3 space-y-2">
            {history.map((item) => {
              const output = asRecord(item.output_snapshot);
              const input = asRecord(item.input_snapshot);
              const basis = asRecord(item.basis_snapshot);
              const ruleset = asRecord(basis.ruleset);
              const isExpanded = expandedHistoryId === item.id;
              return (
                <div key={item.id} className="rounded-lg border border-border">
                  <button type="button" className="flex w-full items-center justify-between gap-3 p-3 text-left text-sm" onClick={() => setExpandedHistoryId(isExpanded ? null : item.id)}>
                    <span className="flex min-w-0 items-center gap-2">{isExpanded ? <ChevronDown className="size-4 shrink-0" /> : <ChevronRight className="size-4 shrink-0" />}<span>{new Date(item.created_at).toLocaleString("zh-CN")}</span></span>
                    <span className="font-semibold">{formatRange(historyRange(item))}</span>
                  </button>
                  {isExpanded && (
                    <div className="space-y-2 border-t border-border px-4 py-3 text-xs text-muted-foreground">
                      <div>辅助结论：{typeof output.finalSentence === "string" ? output.finalSentence : formatRange(historyRange(item))}</div>
                      <div>罪名：{typeof input.crimeName === "string" ? input.crimeName : "未记录"}；犯罪日期：{typeof input.crimeDate === "string" ? input.crimeDate : "未记录"}</div>
                      <div>事实档位：{typeof input.factTier === "string" ? input.factTier : "未记录"}；人工基准刑来源：{typeof input.manualBaseSource === "string" ? input.manualBaseSource : "未记录"}</div>
                      <div>画像修订号：{item.profile_revision}；记录来源：{item.created_source}</div>
                      <div>{typeof ruleset.version === "string" ? `规则集 ${ruleset.version} / ${String(ruleset.contentHash ?? "无指纹")}` : "旧版历史记录：未保存规则集版本与指纹"}</div>
                      <div className="break-all">记录编号：{item.id}</div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      )}

      <p className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-xs leading-relaxed text-amber-900 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
        本工具仅供办案辅助与内部复核，不构成量刑承诺或法律意见。实际裁判须结合完整证据、犯罪事实、行为时法律、司法解释时间效力及现行规范，由办案人员人工判断。
      </p>
    </div>
  );
}

function ResultCard({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg bg-muted/50 p-3"><div className="text-xs text-muted-foreground">{label}</div><div className="mt-1 text-sm font-semibold leading-relaxed">{value}</div></div>;
}
