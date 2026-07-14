import { useMemo, useState } from "react";

import { openUrl } from "@/lib/api";
import {
  calculateRiskAgencyCap,
  calculateZhejiangHistoricalReference,
  createPracticeQuoteProfile,
  getRegionFeeRegime,
  NATIONAL_RISK_SOURCE,
  PROVINCIAL_REGIONS,
  type CriminalStage,
  type HistoricalFeeResult,
  type OfficialSource,
  type RegionFeeRegime,
  type RiskCapResult,
  type RiskMatterCategory,
  type ZhejiangHistoricalMatter,
} from "../lib/nationalLawyerFee";
import { CalculatorDisclaimer, TabBtn } from "./ui";

type Mode = "official" | "risk" | "internal";
type Matter =
  | "civil_property"
  | "civil_non_property"
  | "criminal"
  | "administrative"
  | "state_compensation";

const MATTERS: Array<{ value: Matter; label: string }> = [
  { value: "civil_property", label: "民事/商事（涉财产）" },
  { value: "civil_non_property", label: "民事/商事（不涉财产）" },
  { value: "criminal", label: "刑事" },
  { value: "administrative", label: "行政" },
  { value: "state_compensation", label: "国家赔偿" },
];

const CRIMINAL_STAGES: Array<{ value: CriminalStage; label: string }> = [
  { value: "investigation", label: "侦查阶段" },
  { value: "prosecution", label: "审查起诉阶段" },
  { value: "trial_first", label: "一审阶段" },
  { value: "private_prosecution", label: "自诉/被害人代理" },
];

const RISK_MATTERS: Array<{ value: RiskMatterCategory; label: string }> = [
  { value: "general_property_civil", label: "普通财产合同/债权案件" },
  { value: "criminal", label: "刑事案件" },
  { value: "administrative", label: "行政案件" },
  { value: "state_compensation", label: "国家赔偿案件" },
  { value: "group_litigation", label: "群体性诉讼" },
  { value: "marriage_inheritance", label: "婚姻、继承案件" },
  { value: "social_security", label: "社会保险待遇案件" },
  { value: "minimum_living_security", label: "最低生活保障案件" },
  { value: "support", label: "赡养、抚养、扶养案件" },
  { value: "pension_relief", label: "抚恤、救济案件" },
  { value: "work_injury", label: "工伤赔偿案件" },
  { value: "labor_remuneration", label: "劳动报酬案件" },
];

const STATUS_LABELS: Record<RegionFeeRegime["status"], string> = {
  market_pricing: "市场调节",
  historical_only: "旧规则已到期",
  conflict_unverified: "规则冲突",
  unverified: "当前规则未核实",
  unsupported: "当前模型不支持",
};

export function LawyerFeeCalculator() {
  const [mode, setMode] = useState<Mode>("official");
  const [regionCode, setRegionCode] = useState("110000");
  const [useWuxi, setUseWuxi] = useState(false);
  const [matter, setMatter] = useState<Matter>("civil_property");
  const [criminalStage, setCriminalStage] =
    useState<CriminalStage>("investigation");
  const [amountWan, setAmountWan] = useState("");
  const [showZhejiangHistory, setShowZhejiangHistory] = useState(false);
  const [riskMatter, setRiskMatter] = useState<RiskMatterCategory | "">("");
  const [internalLabel, setInternalLabel] = useState("本所参考标准");
  const [internalMinWan, setInternalMinWan] = useState("");
  const [internalMaxWan, setInternalMaxWan] = useState("");

  const regime = useMemo(
    () => getRegionFeeRegime(regionCode, useWuxi ? "320200" : undefined),
    [regionCode, useWuxi],
  );
  const amountYuan = useMemo(() => parseWanToYuan(amountWan), [amountWan]);

  const historicalResult = useMemo<HistoricalFeeResult | null>(() => {
    if (
      mode !== "official" ||
      regionCode !== "330000" ||
      !showZhejiangHistory
    ) {
      return null;
    }
    if (matter === "civil_property" && amountYuan == null) return null;
    return calculateZhejiangHistoricalReference({
      matter: mapHistoricalMatter(matter),
      criminalStage: matter === "criminal" ? criminalStage : undefined,
      propertyAmountYuan: matter === "civil_property" ? amountYuan : null,
      historicalReferenceConfirmed: true,
    });
  }, [amountYuan, criminalStage, matter, mode, regionCode, showZhejiangHistory]);

  const riskResult = useMemo<RiskCapResult | null>(() => {
    if (mode !== "risk" || !riskMatter) return null;
    return calculateRiskAgencyCap(amountYuan ?? 0, riskMatter);
  }, [amountYuan, mode, riskMatter]);

  const internalResult = useMemo(() => {
    if (mode !== "internal" || regime.status === "unsupported") return null;
    const minYuan = parseWanToYuan(internalMinWan);
    const maxYuan = parseWanToYuan(internalMaxWan || internalMinWan);
    if (minYuan == null || maxYuan == null || maxYuan < minYuan) return null;
    return createPracticeQuoteProfile({
      label: internalLabel,
      minYuan,
      maxYuan,
    });
  }, [internalLabel, internalMaxWan, internalMinWan, mode, regime.status]);

  function changeRegion(next: string) {
    setRegionCode(next);
    setUseWuxi(false);
    setShowZhejiangHistory(false);
  }

  return (
    <div className="space-y-5">
      <div className="rounded-md border border-amber-300/70 bg-amber-50/80 px-4 py-3 text-xs leading-relaxed text-amber-950 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-100">
        本工具先判断规则效力，再决定是否计算。市场调节、已到期、冲突、未核实或不支持的规则不会生成伪官方价格。
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <Field label="省级行政区">
          <select
            aria-label="省级行政区"
            value={regionCode}
            onChange={(event) => changeRegion(event.currentTarget.value)}
            className={inputClass}
          >
            {PROVINCIAL_REGIONS.map((region) => (
              <option key={region.code} value={region.code}>
                {region.name}
                {region.supportStatus === "unsupported_legal_system"
                  ? "（当前模型不支持）"
                  : ""}
              </option>
            ))}
          </select>
        </Field>
        {regionCode === "320000" ? (
          <Field label="江苏地方口径">
            <select
              aria-label="江苏地方口径"
              value={useWuxi ? "320200" : ""}
              onChange={(event) => setUseWuxi(event.currentTarget.value === "320200")}
              className={inputClass}
            >
              <option value="">江苏省级口径</option>
              <option value="320200">江苏·无锡公开口径</option>
            </select>
          </Field>
        ) : (
          <Field label="当前地区">
            <div className="flex min-h-10 items-center rounded-md border border-border bg-muted/20 px-3 text-sm text-foreground">
              {regime.regionName}
            </div>
          </Field>
        )}
      </div>

      <RegimeCard regime={regime} />

      <div className="flex flex-wrap gap-1 rounded-md border border-border bg-card p-1">
        <TabBtn active={mode === "official"} onClick={() => setMode("official")}>
          现行规则 / 历史参考
        </TabBtn>
        <TabBtn active={mode === "risk"} onClick={() => setMode("risk")}>
          风险代理强校验
        </TabBtn>
        <TabBtn active={mode === "internal"} onClick={() => setMode("internal")}>
          律所内部参考
        </TabBtn>
      </div>

      {mode === "official" && (
        <OfficialPanel
          regime={regime}
          regionCode={regionCode}
          matter={matter}
          onMatterChange={setMatter}
          criminalStage={criminalStage}
          onCriminalStageChange={setCriminalStage}
          amountWan={amountWan}
          onAmountWanChange={setAmountWan}
          showZhejiangHistory={showZhejiangHistory}
          onHistoryChange={setShowZhejiangHistory}
          historicalResult={historicalResult}
        />
      )}
      {mode === "risk" && (
        <RiskPanel
          matter={riskMatter}
          onMatterChange={setRiskMatter}
          amountWan={amountWan}
          onAmountWanChange={setAmountWan}
          result={riskResult}
        />
      )}
      {mode === "internal" && (
        <InternalPanel
          unsupported={regime.status === "unsupported"}
          label={internalLabel}
          onLabelChange={setInternalLabel}
          minWan={internalMinWan}
          onMinWanChange={setInternalMinWan}
          maxWan={internalMaxWan}
          onMaxWanChange={setInternalMaxWan}
          result={internalResult}
        />
      )}
    </div>
  );
}

function OfficialPanel({
  regime,
  regionCode,
  matter,
  onMatterChange,
  criminalStage,
  onCriminalStageChange,
  amountWan,
  onAmountWanChange,
  showZhejiangHistory,
  onHistoryChange,
  historicalResult,
}: {
  regime: RegionFeeRegime;
  regionCode: string;
  matter: Matter;
  onMatterChange: (value: Matter) => void;
  criminalStage: CriminalStage;
  onCriminalStageChange: (value: CriminalStage) => void;
  amountWan: string;
  onAmountWanChange: (value: string) => void;
  showZhejiangHistory: boolean;
  onHistoryChange: (value: boolean) => void;
  historicalResult: HistoricalFeeResult | null;
}) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-2">
        <Field label="事项类别">
          <select
            aria-label="事项类别"
            value={matter}
            onChange={(event) => onMatterChange(event.currentTarget.value as Matter)}
            className={inputClass}
          >
            {MATTERS.map((item) => (
              <option key={item.value} value={item.value}>{item.label}</option>
            ))}
          </select>
        </Field>
        {matter === "criminal" ? (
          <Field label="刑事办理阶段">
            <select
              aria-label="刑事办理阶段"
              value={criminalStage}
              onChange={(event) =>
                onCriminalStageChange(event.currentTarget.value as CriminalStage)
              }
              className={inputClass}
            >
              {CRIMINAL_STAGES.map((stage) => (
                <option key={stage.value} value={stage.value}>{stage.label}</option>
              ))}
            </select>
          </Field>
        ) : matter === "civil_property" ? (
          <MoneyInput label="标的额" value={amountWan} onChange={onAmountWanChange} />
        ) : (
          <Field label="普通诉讼阶段">
            <div className="flex min-h-10 items-center rounded-md border border-border bg-muted/20 px-3 text-sm text-foreground">
              一审 / 普通诉讼阶段
            </div>
          </Field>
        )}
      </div>

      {regionCode === "330000" && (
        <label className="flex cursor-pointer items-start gap-2 rounded-md border border-amber-300/70 bg-amber-50/70 px-3 py-3 text-xs leading-relaxed text-amber-950 dark:border-amber-800 dark:bg-amber-950/20 dark:text-amber-100">
          <input
            type="checkbox"
            checked={showZhejiangHistory}
            onChange={(event) => onHistoryChange(event.currentTarget.checked)}
            className="mt-0.5"
          />
          <span>
            我已知悉并主动选择查看浙江官方历史参考。该标准非现行政府指导价，不构成报价。
          </span>
        </label>
      )}

      {historicalResult ? (
        <HistoricalResultCard result={historicalResult} />
      ) : regionCode === "330000" && showZhejiangHistory && matter === "civil_property" && !amountWan ? (
        <Placeholder>输入标的额后计算浙江官方历史参考区间。</Placeholder>
      ) : (
        <FailClosedResult regime={regime} />
      )}
      <SourceList sources={regime.sources} asOfDate={regime.asOfDate} />
    </div>
  );
}

function RiskPanel({
  matter,
  onMatterChange,
  amountWan,
  onAmountWanChange,
  result,
}: {
  matter: RiskMatterCategory | "";
  onMatterChange: (value: RiskMatterCategory | "") => void;
  amountWan: string;
  onAmountWanChange: (value: string) => void;
  result: RiskCapResult | null;
}) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-2">
        <Field label="风险代理事项（必须明确选择）">
          <select
            aria-label="风险代理事项"
            value={matter}
            onChange={(event) =>
              onMatterChange(event.currentTarget.value as RiskMatterCategory | "")
            }
            className={inputClass}
          >
            <option value="">请选择具体事项</option>
            {RISK_MATTERS.map((item) => (
              <option key={item.value} value={item.value}>{item.label}</option>
            ))}
          </select>
        </Field>
        <MoneyInput
          label="最终实现债权 / 减免债务金额"
          value={amountWan}
          onChange={onAmountWanChange}
          disabled={matter !== "general_property_civil"}
        />
      </div>

      {!matter ? (
        <Placeholder>先选择具体事项，系统将立即执行全国风险代理强校验。</Placeholder>
      ) : result && !result.allowed ? (
        <div className="rounded-md border border-red-300 bg-red-50 px-4 py-4 text-red-950 dark:border-red-900 dark:bg-red-950/30 dark:text-red-100">
          <p className="text-sm font-semibold">RISK_AGENT_PROHIBITED</p>
          <p className="mt-1 text-xs leading-relaxed">
            该类案件禁止实行或者变相实行风险代理收费，系统已阻止计算。
          </p>
        </div>
      ) : result?.allowed && result.basisAmountYuan > 0 ? (
        <div className="space-y-3 rounded-md border border-border bg-card px-5 py-4">
          <div>
            <p className="text-xs font-medium text-muted-foreground">现行全国强制上限</p>
            <p className="mt-1 font-mono text-3xl font-semibold text-foreground">
              {formatYuan(result.maximumFeeYuan)}
            </p>
            <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
              这是各环节服务费合计最高限额，不是推荐报价，不能按每个阶段重复计算。
            </p>
          </div>
          <div className="space-y-1 border-t border-border/70 pt-3">
            {result.tiers.map((tier) => (
              <div key={`${tier.fromExclusiveYuan}-${tier.toInclusiveYuan ?? "up"}`} className="flex justify-between gap-3 text-xs">
                <span className="text-muted-foreground">
                  {formatTierRange(tier.fromExclusiveYuan, tier.toInclusiveYuan)} × {formatRate(tier.rate)}
                </span>
                <span className="font-mono text-foreground">{formatYuan(tier.feeYuan)}</span>
              </div>
            ))}
          </div>
          <CalculatorDisclaimer />
        </div>
      ) : (
        <Placeholder>输入最终实现债权或减免债务金额后计算全流程合计上限。</Placeholder>
      )}
      <SourceList sources={[NATIONAL_RISK_SOURCE]} asOfDate="2026-07-14" />
    </div>
  );
}

function InternalPanel({
  unsupported,
  label,
  onLabelChange,
  minWan,
  onMinWanChange,
  maxWan,
  onMaxWanChange,
  result,
}: {
  unsupported: boolean;
  label: string;
  onLabelChange: (value: string) => void;
  minWan: string;
  onMinWanChange: (value: string) => void;
  maxWan: string;
  onMaxWanChange: (value: string) => void;
  result: ReturnType<typeof createPracticeQuoteProfile> | null;
}) {
  if (unsupported) {
    return (
      <Placeholder>
        港澳台适用不同法律与律师收费制度，当前模型不支持，也不会回落为内地内部参考。
      </Placeholder>
    );
  }
  return (
    <div className="space-y-4">
      <div className="rounded-md border border-sky-300/70 bg-sky-50/70 px-4 py-3 text-xs leading-relaxed text-sky-950 dark:border-sky-900 dark:bg-sky-950/30 dark:text-sky-100">
        此入口只记录律所自行制定的内部参考，不是官方收费标准；系统不再默认使用“无锡同行常用”经验公式。
      </div>
      <div className="grid gap-3 md:grid-cols-3">
        <Field label="内部标准名称">
          <input value={label} onChange={(event) => onLabelChange(event.currentTarget.value)} className={inputClass} />
        </Field>
        <MoneyInput label="参考下限 / 固定额" value={minWan} onChange={onMinWanChange} />
        <MoneyInput label="参考上限（可留空）" value={maxWan} onChange={onMaxWanChange} />
      </div>
      {result ? (
        <div className="space-y-2 rounded-md border border-sky-300/70 bg-card px-5 py-4">
          <p className="text-xs font-medium text-sky-700 dark:text-sky-300">律所 / 内部参考标准</p>
          <p className="font-mono text-2xl font-semibold text-foreground">
            {result.minYuan === result.maxYuan
              ? formatYuan(result.minYuan)
              : `${formatYuan(result.minYuan)} ～ ${formatYuan(result.maxYuan)}`}
          </p>
          <p className="text-xs text-muted-foreground">{result.label} · {result.note}</p>
          <CalculatorDisclaimer />
        </div>
      ) : (
        <Placeholder>输入本所已确认的固定额或参考区间；不会自动带入任何经验费率。</Placeholder>
      )}
    </div>
  );
}

function RegimeCard({ regime }: { regime: RegionFeeRegime }) {
  const danger = regime.status === "unsupported" || regime.status === "conflict_unverified";
  return (
    <div className="rounded-md border border-border bg-card px-4 py-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="text-xs text-muted-foreground">{regime.regionName} · 当前制度状态</p>
          <p className="mt-0.5 text-base font-semibold text-foreground">{STATUS_LABELS[regime.status]}</p>
        </div>
        <span className={`rounded-full px-2.5 py-1 text-[11px] font-medium ${danger ? "bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-200" : "bg-amber-100 text-amber-900 dark:bg-amber-950 dark:text-amber-200"}`}>
          {regime.autoOfficialCalculation ? "可自动计算" : "停止自动官方计算"}
        </span>
      </div>
      <p className="mt-3 text-xs leading-relaxed text-muted-foreground">{regime.summary}</p>
    </div>
  );
}

function FailClosedResult({ regime }: { regime: RegionFeeRegime }) {
  const title = regime.status === "conflict_unverified"
    ? "规则冲突，已停止自动计算"
    : regime.status === "unsupported"
      ? "当前模型不支持"
      : regime.status === "historical_only"
        ? "旧标准已到期，当前规则未核实"
        : regime.status === "unverified"
          ? "当前规则未核实"
          : "无统一现行官方价格";
  return (
    <div className="rounded-md border border-dashed border-border bg-muted/20 px-5 py-6 text-center">
      <p className="text-base font-semibold text-foreground">{title}</p>
      <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
        请与律师事务所协商并录入“律所内部参考”；历史或冲突规则不会自动回落为现行报价。
      </p>
    </div>
  );
}

function HistoricalResultCard({ result }: { result: HistoricalFeeResult }) {
  return (
    <div className="space-y-3 rounded-md border-2 border-amber-400 bg-amber-50/40 px-5 py-4 dark:border-amber-800 dark:bg-amber-950/20">
      <div>
        <p className="text-xs font-semibold text-amber-800 dark:text-amber-200">
          OFFICIAL HISTORICAL REFERENCE · 非现行政府指导价
        </p>
        <p className="mt-1 font-mono text-2xl font-semibold text-foreground">
          {result.manualAdjustmentRequired || result.minYuan == null || result.maxYuan == null
            ? "需人工酌定"
            : `${formatYuan(result.minYuan)} ～ ${formatYuan(result.maxYuan)}`}
        </p>
        <p className="mt-1 text-xs leading-relaxed text-amber-900 dark:text-amber-100">{result.warning}</p>
      </div>
      {result.tiers.length > 0 && (
        <div className="space-y-1 border-t border-amber-300/70 pt-3 dark:border-amber-800">
          {result.tiers.map((tier, index) => (
            <div key={`${index}-${tier.appliedYuan}`} className="flex justify-between gap-3 text-xs">
              <span className="text-muted-foreground">
                分段 {index + 1}：{formatYuan(tier.appliedYuan)} × {formatRate(tier.minRate)}–{formatRate(tier.maxRate)}
              </span>
              <span className="font-mono text-foreground">{formatYuan(tier.minFeeYuan)}–{formatYuan(tier.maxFeeYuan)}</span>
            </div>
          ))}
        </div>
      )}
      {result.mayCharge2500 && (
        <p className="text-xs text-amber-900 dark:text-amber-100">
          比例结果不足 2,500 元；历史文件表述为“可按 2,500 元”，不是强制最低价。
        </p>
      )}
      <CalculatorDisclaimer />
    </div>
  );
}

function SourceList({ sources, asOfDate }: { sources: OfficialSource[]; asOfDate: string }) {
  return (
    <div className="space-y-2 rounded-md border border-border bg-card px-4 py-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs font-semibold text-foreground">规则来源</p>
        <span className="text-[10px] text-muted-foreground">核验截至 {asOfDate}</span>
      </div>
      {sources.length === 0 ? (
        <p className="text-xs text-muted-foreground">尚无完成核验的现行官方来源，系统已停止计算。</p>
      ) : (
        sources.map((source) => (
          <div key={`${source.url}-${source.documentNo ?? source.title}`} className="border-t border-border/50 pt-2 first:border-0 first:pt-0">
            <button
              type="button"
              onClick={() => void openUrl(source.url).catch((error) => console.warn("openUrl failed", error))}
              className="text-left text-xs font-medium text-foreground underline decoration-border underline-offset-2 hover:decoration-foreground"
            >
              {source.title}{source.documentNo ? `（${source.documentNo}）` : ""}
            </button>
            <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
              {source.issuer} · {formatValidity(source)} · {source.validityNote}
            </p>
          </div>
        ))
      )}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="space-y-1.5">
      <span className="block text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

function MoneyInput({
  label,
  value,
  onChange,
  disabled = false,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <Field label={`${label}（万元）`}>
      <input
        type="number"
        inputMode="decimal"
        min="0"
        step="0.01"
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.currentTarget.value)}
        className={inputClass}
        placeholder={disabled ? "该事项禁止计算" : "例如：200"}
      />
    </Field>
  );
}

function Placeholder({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-md border border-dashed border-border/70 bg-muted/20 px-4 py-8 text-center text-xs leading-relaxed text-muted-foreground">
      {children}
    </div>
  );
}

function mapHistoricalMatter(matter: Matter): ZhejiangHistoricalMatter {
  if (matter === "criminal") return "criminal";
  if (matter === "administrative") return "administrative";
  if (matter === "state_compensation") return "state_compensation";
  return "civil";
}

function parseWanToYuan(raw: string): number | null {
  if (!raw.trim()) return null;
  const value = Number(raw);
  if (!Number.isFinite(value) || value <= 0) return null;
  return Math.round(value * 1_000_000) / 100;
}

function formatYuan(value: number): string {
  return `${new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(value)} 元`;
}

function formatRate(rate: number): string {
  return `${(rate * 100).toFixed(Number.isInteger(rate * 100) ? 0 : 2)}%`;
}

function formatTierRange(fromExclusiveYuan: number, toInclusiveYuan: number | null): string {
  if (toInclusiveYuan == null) return `${formatYuan(fromExclusiveYuan)}以上部分`;
  if (fromExclusiveYuan === 0) return `${formatYuan(toInclusiveYuan)}以内部分`;
  return `${formatYuan(fromExclusiveYuan)}至${formatYuan(toInclusiveYuan)}部分`;
}

function formatValidity(source: OfficialSource): string {
  if (source.effectiveFrom && source.effectiveTo) {
    return `适用 ${source.effectiveFrom} 至 ${source.effectiveTo}`;
  }
  if (source.effectiveFrom) return `${source.effectiveFrom} 起`;
  if (source.effectiveTo) return `截至 ${source.effectiveTo}`;
  return "效力日期见原文";
}

const inputClass =
  "min-h-10 w-full rounded-md border border-border bg-card px-3 py-2 text-sm text-foreground outline-none focus:border-foreground/50 focus:ring-1 focus:ring-foreground/20 disabled:cursor-not-allowed disabled:opacity-50";
