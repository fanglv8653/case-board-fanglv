import {
  CRIME_NAME_TO_ID,
  factorAppliesTo,
  LEGAL_REFERENCES,
  PRINCIPAL_PENALTY_LABELS,
  SENTENCING_DATA,
} from "./data.ts";
import type {
  AreaType,
  CalculationProcessEntry,
  CrimeId,
  CrimeName,
  ExtractedSentencingInput,
  FactorAdjustment,
  MonthRange,
  PrincipalPenaltyKind,
  SentencingCalculationInput,
  SentencingCalculationResult,
  SentencingFactorRule,
  SentencingStandard,
  TemporalRuleBasis,
} from "./types.ts";

interface TierSelection {
  tier: string | null;
  tierLabel: string | null;
  standard: SentencingStandard | null;
  error?: string;
}

interface FactorSelection {
  priority: SentencingFactorRule[];
  general: SentencingFactorRule[];
  qualitative: SentencingFactorRule[];
  exemption: SentencingFactorRule | null;
  errors: string[];
  warnings: string[];
}

interface FactorAdjustmentResult {
  priorityAdjustments: FactorAdjustment[];
  generalAdjustments: FactorAdjustment[];
  afterFactors: MonthRange;
}

const CURRENT_TEMPORAL_EFFECTIVE_DATE = "2026-05-01";
const PLEA_COMBINATION_FACTOR_IDS = new Set([
  "surrender",
  "confess_heavy",
  "restitution",
  "compensation_forgiven",
  "reconciliation",
]);

function isCrimeName(value: string): value is CrimeName {
  return value in CRIME_NAME_TO_ID;
}

function isValidIsoDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day;
}

function hasMitigatedEffect(rule: SentencingFactorRule): boolean {
  return [
    "lighter_or_mitigated",
    "lighter_or_mitigated_or_exempt",
    "mitigated_or_exempt",
  ].includes(rule.legalEffect);
}

function cloneRange(range: MonthRange | null | undefined): MonthRange | null {
  return range ? [range[0], range[1]] : null;
}

/**
 * 全国规则量刑辅助引擎。
 *
 * 计算边界：法定刑/量刑起点由规则集给出；除危险驾驶罪的直接宣告刑幅度外，
 * 基准刑必须由律师依据完整案情和案件实际适用的现行实施细则人工核验后录入。
 */
export class SentencingEngine {
  calculate(input: SentencingCalculationInput): SentencingCalculationResult {
    const {
      crimeName,
      amount,
      areaType,
      factors,
      crimeDate,
      judgeAdjustment = 0,
      judgeAdjustmentReason = "",
      isTelecom = false,
      factTier = null,
      manualBasePenaltyRange = null,
      manualBaseSource = "",
      temporalRuleBasis = null,
    } = input;
    const process: CalculationProcessEntry[] = [];
    const result: SentencingCalculationResult = {
      crimeName,
      amount,
      areaType,
      factors: { ...factors },
      crimeDate,
      judgeAdjustment,
      judgeAdjustmentReason,
      isTelecom,
      factTier,
      manualBasePenaltyRange: cloneRange(manualBasePenaltyRange),
      manualBaseSource,
      temporalRuleBasis,
      calculationStatus: "blocked",
      process,
      warnings: [],
      blockingIssues: [],
      ruleset: SENTENCING_DATA.ruleset,
    };

    const fail = (message: string) => {
      result.error = message;
      result.blockingIssues.push(message);
      process.push({ step: "阻断", detail: message });
      return result;
    };

    if (!isCrimeName(crimeName)) return fail(`未知罪名：${crimeName}`);
    if (!Number.isFinite(amount) || amount < 0) return fail("涉案金额必须是大于或等于0的有效数字");
    if (!isValidIsoDate(crimeDate)) return fail("请提供有效的 YYYY-MM-DD 犯罪日期");
    if (!Number.isFinite(judgeAdjustment) || judgeAdjustment < -20 || judgeAdjustment > 20) {
      return fail("裁判情景调整必须在-20%至20%之间");
    }
    if (judgeAdjustment !== 0 && judgeAdjustmentReason.trim().length < 4) {
      return fail("使用裁判情景调整时，必须填写具体调整理由");
    }

    const crimeId = CRIME_NAME_TO_ID[crimeName];
    result.legalReferences = LEGAL_REFERENCES[crimeName];
    process.push({
      step: "规则集",
      detail: `${SENTENCING_DATA.ruleset.id} ${SENTENCING_DATA.ruleset.version}；引擎${SENTENCING_DATA.ruleset.engineVersion}`,
    });

    const temporalError = this._validateTemporalBasis(crimeId, crimeDate, temporalRuleBasis);
    if (temporalError) return fail(temporalError);

    process.push({ step: "第一步：法定刑与量刑起点", detail: "先定位法定刑档，再单独展示量刑起点；二者不得混同" });
    const selection = this._getTierAndStandard(
      crimeId,
      amount,
      areaType,
      process,
      isTelecom,
      factTier,
    );
    if (!selection.standard) return fail(selection.error || "无法确定量刑标准");

    const standard = selection.standard;
    result.tier = selection.tier;
    result.tierLabel = selection.tierLabel;
    result.standardDetail = { ...standard };
    result.statutoryPenalty = standard.statutory;
    process.push({
      step: "法定刑",
      detail: `${standard.statutory.article}：${standard.statutory.label}`,
      valueRange: standard.statutory.monthBounds,
    });

    if (standard.startMin != null) {
      result.startingPointRange = [standard.startMin, standard.startMax ?? null];
      result.startingPointKinds = standard.startKinds ? [...standard.startKinds] : [];
      process.push({
        step: standard.quantitativeStatus === "direct_sentence" ? "直接宣告刑幅度" : "量刑起点",
        detail: `${this._formatMonthRange(result.startingPointRange)}；刑种：${this._formatKinds(result.startingPointKinds)}`,
        valueRange: result.startingPointRange,
      });
    } else {
      result.warnings.push("全国规则集未为该罪名提供可直接套用的量刑起点，当前仅展示法定刑档。");
      process.push({ step: "量刑起点", detail: "当前全国规则集没有可直接套用的量刑起点，须人工核对" });
    }

    const factorSelection = this._selectAndValidateFactors(crimeId, factTier, factors);
    result.warnings.push(...factorSelection.warnings);
    if (factorSelection.errors.length > 0) return fail(factorSelection.errors.join("；"));

    if (factorSelection.exemption) {
      result.calculationStatus = "complete";
      result.disposition = "exemption";
      result.finalPenaltyRange = [0, 0];
      result.finalPenaltyKinds = ["exemption"];
      result.finalSentence = "免予刑事处罚（中止犯未造成损害）";
      process.push({
        step: "法定处理",
        detail: "《刑法》第24条规定，中止犯没有造成损害的，应当免除处罚；此处的0仅表示明确的免予刑事处罚，不表示零月刑期。",
      });
      return result;
    }

    if (factorSelection.qualitative.length > 0) {
      const names = factorSelection.qualitative.map((item) => item.name).join("、");
      result.blockingIssues.push(`所选情节“${names}”需要依法作定性或下档判断，当前规则集不虚构百分比`);
      result.disposition = "manual_review";
      process.push({ step: "定性情节", detail: result.blockingIssues[result.blockingIssues.length - 1] ?? names });
      return result;
    }

    let baseRange: MonthRange;
    if (standard.quantitativeStatus === "direct_sentence" && !manualBasePenaltyRange) {
      baseRange = [standard.startMin ?? standard.statutory.monthBounds[0], standard.startMax ?? standard.statutory.monthBounds[1]];
      result.warnings.push("危险驾驶罪使用法发〔2021〕21号规定的1至6个月拘役直接宣告刑幅度；具体点位仍须结合行为、后果和罚金能力人工判断。");
      process.push({ step: "第二步：确定基准输入", detail: "该罪名适用直接宣告刑幅度，不使用未核实的地方增刑公式", valueRange: baseRange });
    } else {
      const baseError = this._validateManualBaseRange(manualBasePenaltyRange, manualBaseSource, standard);
      if (baseError) {
        result.calculationStatus = "requires_manual_base";
        result.blockingIssues.push(baseError);
        process.push({ step: "第二步：等待人工核验基准刑", detail: baseError });
        return result;
      }
      baseRange = cloneRange(manualBasePenaltyRange)!;
      process.push({
        step: "第二步：采用人工核验基准刑",
        detail: `${this._formatMonthRange(baseRange)}；来源：${manualBaseSource.trim()}`,
        valueRange: baseRange,
      });
    }
    result.basePenaltyRange = baseRange;

    process.push({
      step: "第三步：量刑情节调节",
      detail: "修正情节依次调节；一般情节以修正后的同一基数同向相加、逆向相减",
    });
    const adjusted = this._applyFactorAdjustmentsRange(
      baseRange,
      factorSelection.priority,
      factorSelection.general,
      process,
    );
    result.priorityAdjustments = adjusted.priorityAdjustments;
    result.generalAdjustments = adjusted.generalAdjustments;

    let rawRange = adjusted.afterFactors;
    if (judgeAdjustment !== 0) {
      rawRange = this._applyPercentRange(rawRange, judgeAdjustment, judgeAdjustment);
      process.push({
        step: "裁判情景模拟",
        detail: `${judgeAdjustment}%（理由：${judgeAdjustmentReason.trim()}）→ ${this._formatMonthRange(rawRange)}；该步骤不是规范性自动计算`,
        valueRange: rawRange,
      });
    } else {
      process.push({ step: "裁判情景模拟", detail: "未启用" });
    }
    result.rawAdjustedPenaltyRange = rawRange;

    const allQuantitativeFactors = [...factorSelection.priority, ...factorSelection.general];
    const permitsMitigation = allQuantitativeFactors.some(hasMitigatedEffect);
    const clamped = this._applyStatutoryBounds(rawRange, standard, permitsMitigation, result.warnings, process);
    result.finalPenaltyRange = clamped;
    result.finalPenaltyKinds = this._inferFiniteKinds(clamped, standard, permitsMitigation);
    result.finalSentence = this._formatSentenceRange(clamped, result.finalPenaltyKinds);
    result.disposition = "term_range";
    result.calculationStatus = "complete";

    if (standard.statutory.options.some((option) => ["life", "death", "fine_only", "control"].includes(option.kind))) {
      result.warnings.push("月数调节结果只覆盖可用月数表达的刑期；法定刑中的无期徒刑、死刑、管制或单处罚金须另行判断。 ");
    }
    process.push({ step: "量刑辅助结果", detail: result.finalSentence, valueRange: clamped });
    return result;
  }

  _validateTemporalBasis(
    crimeId: CrimeId,
    crimeDate: string,
    temporalRuleBasis: TemporalRuleBasis | null,
  ): string | null {
    if (!["embezzlement", "non_official_bribery"].includes(crimeId)) return null;
    if (!temporalRuleBasis) {
      return "该罪名涉及法释〔2026〕6号时间效力，不能仅按犯罪日期自动切换；请选择并确认规则时间依据";
    }
    if (temporalRuleBasis === "individual_comparison_required") {
      return "行为跨越规则施行日或新旧规则适用尚未核定，须先完成个案时间效力和有利性比较，本工具停止数值测算";
    }
    if (temporalRuleBasis === "conduct_on_or_after_2026_05_01" && crimeDate < CURRENT_TEMPORAL_EFFECTIVE_DATE) {
      return "犯罪日期早于2026-05-01，与所选“施行后行为”时间依据冲突";
    }
    if (temporalRuleBasis === "pre_effective_current_rule_reviewed" && crimeDate >= CURRENT_TEMPORAL_EFFECTIVE_DATE) {
      return "犯罪日期不早于2026-05-01，无需选择“施行前行为经核对适用现规则”";
    }
    return null;
  }

  _getTierAndStandard(
    crimeId: CrimeId,
    amount: number,
    areaType: AreaType,
    process: CalculationProcessEntry[],
    isTelecom: boolean,
    factTier: string | null,
  ): TierSelection {
    let standards = SENTENCING_DATA.standards[crimeId] ?? [];
    if (crimeId === "fraud") {
      standards = standards.filter((standard) => isTelecom
        ? standard.subType === "电信诈骗"
        : standard.subType !== "电信诈骗");
    }
    const availableAreas = [...new Set(standards.map((standard) => standard.area))];
    const resolvedArea = availableAreas.length === 1 ? availableAreas[0] : areaType;
    const byArea = standards.filter((standard) => standard.area === resolvedArea);
    if (byArea.length === 0) {
      return { tier: null, tierLabel: null, standard: null, error: `未找到${areaType}的有效量刑标准` };
    }

    if (factTier) {
      const manual = byArea.find((standard) => standard.tier === factTier);
      if (!manual) {
        return { tier: null, tierLabel: null, standard: null, error: `人工确认档位“${factTier}”不在当前罪名可选范围` };
      }
      process.push({ step: "确定档位", detail: `采用人工确认的“${manual.tier}”档；金额不替代其他严重情节判断` });
      return { tier: manual.tier, tierLabel: manual.tier, standard: manual };
    }

    if (byArea.length === 1) {
      const only = byArea[0];
      process.push({ step: "确定档位", detail: `该罪名当前只有“${only.tier}”档` });
      return { tier: only.tier, tierLabel: only.tier, standard: only };
    }

    if (byArea.every((standard) => standard.minAmount == null && standard.maxAmount == null)) {
      return {
        tier: null,
        tierLabel: null,
        standard: null,
        error: `请根据案件事实人工确认法定刑档位，可选：${byArea.map((item) => item.tier).join("、")}`,
      };
    }

    for (const standard of byArea) {
      const minimum = standard.minAmount ?? 0;
      const maximum = standard.maxAmount ?? Number.POSITIVE_INFINITY;
      if (minimum <= amount && amount < maximum) {
        process.push({ step: "确定档位", detail: `金额${amount}元初步匹配“${standard.tier}”档；仍须复核其他入罪或升档情节` });
        return { tier: standard.tier, tierLabel: standard.tier, standard };
      }
    }

    const minimum = Math.min(...byArea.map((standard) => standard.minAmount ?? Number.POSITIVE_INFINITY));
    if (Number.isFinite(minimum) && amount < minimum) {
      return {
        tier: null,
        tierLabel: null,
        standard: null,
        error: `涉案金额${amount}元低于当前规则集数额起点${minimum}元；如有其他入罪情节，请人工选择档位并核对法源`,
      };
    }
    return { tier: null, tierLabel: null, standard: null, error: "金额未匹配连续档位，请复核规则和输入" };
  }

  _validateManualBaseRange(
    range: MonthRange | null | undefined,
    source: string,
    standard: SentencingStandard,
  ): string | null {
    if (!range) {
      return "当前规则只确定法定刑和量刑起点；请录入已按本案适用现行实施细则及完整事实人工核验的基准刑区间";
    }
    const [minimum, maximum] = range;
    if (!Number.isFinite(minimum) || maximum == null || !Number.isFinite(maximum) || minimum < 0 || maximum < minimum) {
      return "人工基准刑必须是下限不小于0、上限不小于下限的有限月数区间";
    }
    const [statutoryMinimum, statutoryMaximum] = standard.statutory.monthBounds;
    if (minimum < statutoryMinimum || (statutoryMaximum != null && maximum > statutoryMaximum)) {
      return `人工基准刑${this._formatMonthRange(range)}超出当前法定刑月数边界${this._formatMonthRange(standard.statutory.monthBounds)}`;
    }
    if (source.trim().length < 4) {
      return "请填写人工基准刑的核验来源，例如适用实施细则条款、量刑建议或阅卷底稿定位";
    }
    return null;
  }

  _selectAndValidateFactors(
    crimeId: CrimeId,
    factTier: string | null,
    factors: Readonly<Record<string, boolean>>,
  ): FactorSelection {
    const allRules = [...SENTENCING_DATA.priorityFactors, ...SENTENCING_DATA.generalFactors];
    const selected: SentencingFactorRule[] = [];
    const errors: string[] = [];
    const warnings: string[] = [];
    for (const [name, enabled] of Object.entries(factors)) {
      if (!enabled) continue;
      const rule = allRules.find((item) => item.name === name);
      if (!rule) {
        errors.push(`未知量刑情节“${name}”`);
        continue;
      }
      if (!factorAppliesTo(rule.id, crimeId, factTier)) {
        errors.push(`量刑情节“${name}”不适用于当前罪名或事实档位`);
        continue;
      }
      selected.push(rule);
      if (rule.note) warnings.push(`${rule.name}：${rule.note}`);
    }

    const groups = new Map<string, SentencingFactorRule[]>();
    for (const rule of selected) {
      if (!rule.conflictGroup) continue;
      groups.set(rule.conflictGroup, [...(groups.get(rule.conflictGroup) ?? []), rule]);
    }
    for (const rules of groups.values()) {
      if (rules.length > 1) errors.push(`互斥情节不能同时选择：${rules.map((item) => item.name).join("、")}`);
    }

    const selectedIds = new Set(selected.map((item) => item.id));
    const conflictPairs = new Set<string>();
    for (const rule of selected) {
      for (const conflictId of rule.conflictsWith ?? []) {
        if (!selectedIds.has(conflictId)) continue;
        const pair = [rule.id, conflictId].sort().join("|");
        if (conflictPairs.has(pair)) continue;
        conflictPairs.add(pair);
        const other = allRules.find((item) => item.id === conflictId);
        errors.push(`为避免重复评价，不能同时选择：${rule.name}、${other?.name ?? conflictId}`);
      }
    }

    const exemption = selected.find((item) => item.legalEffect === "exempt") ?? null;
    const qualitative = selected.filter((item) => !item.quantitative && item.legalEffect !== "exempt");
    return {
      priority: SENTENCING_DATA.priorityFactors.filter((item) => selectedIds.has(item.id) && item.quantitative),
      general: SENTENCING_DATA.generalFactors.filter((item) => selectedIds.has(item.id) && item.quantitative),
      qualitative,
      exemption,
      errors,
      warnings,
    };
  }

  _applyFactorAdjustmentsRange(
    penaltyRange: MonthRange,
    priority: SentencingFactorRule[],
    general: SentencingFactorRule[],
    process: CalculationProcessEntry[],
  ): FactorAdjustmentResult {
    const priorityAdjustments: FactorAdjustment[] = [];
    let current = cloneRange(penaltyRange)!;
    for (const factor of priority) {
      current = this._applyPercentRange(current, factor.minPct ?? 0, factor.maxPct ?? 0);
      priorityAdjustments.push({
        factor: factor.name,
        percentRange: [factor.minPct ?? 0, factor.maxPct ?? 0],
        newRange: cloneRange(current)!,
      });
      process.push({
        step: "修正情节依次调节",
        detail: `${factor.name} ${factor.minPct ?? 0}%至${factor.maxPct ?? 0}% → ${this._formatMonthRange(current)}`,
        valueRange: current,
      });
    }

    if (general.length === 0) {
      process.push({ step: "一般情节合并调节", detail: "未选择一般情节" });
      return { priorityAdjustments, generalAdjustments: [], afterFactors: current };
    }

    const reductionFactors = general.filter((item) => item.direction === "reduce");
    let reduceMinPct = reductionFactors.reduce((sum, item) => sum + (item.minPct ?? 0), 0);
    const reduceMaxPct = reductionFactors.reduce((sum, item) => sum + (item.maxPct ?? 0), 0);
    const hasPlea = general.some((item) => item.id === "plea_guilty");
    const hasPleaCombination = general.some((item) => PLEA_COMBINATION_FACTOR_IDS.has(item.id));
    if (hasPlea && hasPleaCombination) {
      const cappedIds = new Set([...PLEA_COMBINATION_FACTOR_IDS, "plea_guilty"]);
      const combinationMinPct = reductionFactors
        .filter((item) => cappedIds.has(item.id))
        .reduce((sum, item) => sum + (item.minPct ?? 0), 0);
      if (combinationMinPct < -60) {
        const otherMinPct = reductionFactors
          .filter((item) => !cappedIds.has(item.id))
          .reduce((sum, item) => sum + (item.minPct ?? 0), 0);
        reduceMinPct = -60 + otherMinPct;
        process.push({ step: "认罪认罚合并上限", detail: "认罪认罚与指定相关从宽情节的合并幅度按60%封顶；其他独立情节仍另行评价" });
      }
    }

    const [baseLow, baseHigh] = current;
    const lowAfterReduction = Math.max(0, Math.floor(baseLow * (100 + reduceMinPct) / 100));
    const highAfterReduction = baseHigh == null
      ? null
      : Math.max(0, Math.ceil(baseHigh * (100 + reduceMaxPct) / 100));
    let lowIncrease = 0;
    let highIncrease = 0;
    for (const factor of general.filter((item) => item.direction === "increase")) {
      const lowDelta = Math.floor(baseLow * (factor.minPct ?? 0) / 100);
      const highDelta = baseHigh == null ? 0 : Math.ceil(baseHigh * (factor.maxPct ?? 0) / 100);
      lowIncrease += factor.minimumIncreaseMonths != null
        ? Math.max(lowDelta, factor.minimumIncreaseMonths)
        : lowDelta;
      highIncrease += factor.minimumIncreaseMonths != null
        ? Math.max(highDelta, factor.minimumIncreaseMonths)
        : highDelta;
    }
    const afterGeneral: MonthRange = [
      Math.max(0, lowAfterReduction + lowIncrease),
      highAfterReduction == null ? null : Math.max(0, highAfterReduction + highIncrease),
    ];
    const normalized: MonthRange = afterGeneral[1] == null
      ? afterGeneral
      : [Math.min(afterGeneral[0], afterGeneral[1]), afterGeneral[1]];
    const generalAdjustments = general.map((factor): FactorAdjustment => ({
      factor: factor.name,
      percentRange: [factor.minPct ?? 0, factor.maxPct ?? 0],
      minimumIncreaseMonths: factor.minimumIncreaseMonths,
      newRange: cloneRange(normalized)!,
    }));
    process.push({
      step: "一般情节合并调节",
      detail: `以${this._formatMonthRange(current)}为同一基数，同向相加、逆向相减 → ${this._formatMonthRange(normalized)}`,
      valueRange: normalized,
    });
    return { priorityAdjustments, generalAdjustments, afterFactors: normalized };
  }

  _applyStatutoryBounds(
    rawRange: MonthRange,
    standard: SentencingStandard,
    permitsMitigation: boolean,
    warnings: string[],
    process: CalculationProcessEntry[],
  ): MonthRange {
    const currentBounds = standard.statutory.monthBounds;
    const lowerBounds = permitsMitigation && standard.statutory.mitigatedMonthBounds
      ? standard.statutory.mitigatedMonthBounds
      : currentBounds;
    const floor = lowerBounds[0];
    const cap = currentBounds[1];
    let low = Math.max(rawRange[0], floor);
    let high = rawRange[1] == null ? cap : rawRange[1];
    if (cap != null) high = Math.min(high ?? cap, cap);
    if (high != null && high < floor) high = floor;
    if (cap != null && low > cap) low = cap;
    if (high != null && low > high) low = high;

    if (low !== rawRange[0] || high !== rawRange[1]) {
      const rule = permitsMitigation && standard.statutory.mitigatedMonthBounds
        ? "存在可减轻情节，最低边界按下一法定刑幅度控制；最高边界仍受本档法定最高刑控制"
        : "只有从轻或从重情节时，结果控制在本档法定刑月数边界内";
      warnings.push(rule);
      process.push({ step: "法定刑边界校正", detail: `${rule}：${this._formatMonthRange(rawRange)} → ${this._formatMonthRange([low, high])}` });
    }
    return [low, high];
  }

  _inferFiniteKinds(
    range: MonthRange,
    standard: SentencingStandard,
    permitsMitigation: boolean,
  ): PrincipalPenaltyKind[] {
    const usesMitigatedBand = permitsMitigation
      && standard.statutory.mitigatedMonthBounds
      && range[0] < standard.statutory.monthBounds[0];
    const candidateOptions = usesMitigatedBand
      ? [...standard.statutory.options, ...(standard.statutory.mitigatedOptions ?? [])]
      : standard.statutory.options;
    const kinds = candidateOptions
      .filter((option) => option.minimumMonths != null)
      .filter((option) => {
        const maximum = option.maximumMonths ?? Number.POSITIVE_INFINITY;
        const rangeMaximum = range[1] ?? Number.POSITIVE_INFINITY;
        return option.minimumMonths! <= rangeMaximum && maximum >= range[0];
      })
      .map((option) => option.kind);
    return [...new Set(kinds)];
  }

  _formatSentenceRange(range: MonthRange, kinds: PrincipalPenaltyKind[]): string {
    return `月数调节结果${this._formatMonthRange(range)}（可能刑种：${this._formatKinds(kinds)}；具体宣告刑须人工判断）`;
  }

  _formatKinds(kinds: readonly PrincipalPenaltyKind[]): string {
    if (kinds.length === 0) return "须人工判断";
    return [...new Set(kinds)].map((kind) => PRINCIPAL_PENALTY_LABELS[kind]).join("、");
  }

  _formatMonthRange(range: MonthRange): string {
    const [low, high] = range;
    if (high == null) return `${low}个月以上`;
    if (low === high) return `${low}个月`;
    return `${low}至${high}个月`;
  }

  _applyPercentRange(range: MonthRange, minPct: number, maxPct: number): MonthRange {
    const [low, high] = range;
    const nextLow = Math.max(0, Math.floor(low * (100 + minPct) / 100));
    const nextHigh = high == null ? null : Math.max(0, Math.ceil(high * (100 + maxPct) / 100));
    return [Math.min(nextLow, nextHigh ?? nextLow), nextHigh];
  }

  extractCrime(text: string): string | null {
    for (const [crimeName, keywords] of SENTENCING_DATA.keywords.crime) {
      if (keywords.some((keyword) => text.includes(keyword))) return crimeName;
    }
    return null;
  }

  extractRegion(text: string): AreaType | null {
    for (const [region, cities] of SENTENCING_DATA.keywords.region) {
      if (cities.some((city) => text.includes(city))) return region;
    }
    return null;
  }

  extractAmount(text: string): number | null {
    const normalized = text.replace(/[,，]/g, "");
    let match = normalized.match(/(\d+(?:\.\d+)?)\s*亿\s*(\d+(?:\.\d+)?)?\s*万?/);
    if (match) return Math.round(Number(match[1]) * 100000000 + Number(match[2] || 0) * 10000);
    match = normalized.match(/(\d+(?:\.\d+)?)\s*万\s*(\d+)?\s*元?/);
    if (match) return Math.round(Number(match[1]) * 10000 + Number(match[2] || 0));
    match = normalized.match(/(\d+(?:\.\d+)?)\s*元/);
    return match ? Math.round(Number(match[1])) : null;
  }

  extractFactors(text: string): Record<string, boolean> {
    const factors: Record<string, boolean> = {};
    for (const [factorName, keywords] of Object.entries(SENTENCING_DATA.keywords.factor)) {
      if (keywords.some((keyword) => text.includes(keyword))) factors[factorName] = true;
    }
    if (factors["重大立功"]) delete factors["一般立功"];
    if (factors["认罪认罚"]) {
      delete factors["当庭自愿认罪"];
      delete factors["坦白"];
    }
    if (factors["自首"]) delete factors["坦白"];
    return factors;
  }

  isTelecom(text: string): boolean {
    return SENTENCING_DATA.keywords.telecom.some((keyword) => text.includes(keyword));
  }

  extractDate(text: string): string | null {
    const match = text.match(/(\d{4})(?:\s*年|[-/.])(\d{1,2})(?:\s*月|[-/.]?)(\d{1,2})?/);
    if (!match) return null;
    const year = Number(match[1]);
    const month = Number(match[2]);
    const day = Number(match[3] || 1);
    const date = new Date(year, month - 1, day);
    if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) return null;
    return `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
  }

  extractFactTier(text: string, crimeName: string | null): string | null {
    if (crimeName === "寻衅滋事罪") {
      if (/三次以上|纠集他人三次|每次构罪/.test(text)) return "纠集他人三次且每次构罪";
      if (/一次|单次/.test(text)) return "一次";
    }
    if (crimeName === "交通肇事罪") {
      if (/逃逸致死|因逃逸致人死亡/.test(text)) return "因逃逸致人死亡";
      if (/逃逸|特别恶劣/.test(text)) return "逃逸/其他特别恶劣情节";
      if (/未逃逸|基本情形|一般情形/.test(text)) return "基本情形";
    }
    if (crimeName === "故意伤害罪") {
      if (/致人死亡|特别残忍.*严重残疾/.test(text)) return "特别残忍手段致重伤严重残疾/致死";
      if (/重伤/.test(text)) return "致一人重伤";
      if (/轻伤/.test(text)) return "致一人轻伤";
    }
    if (crimeName === "抢劫罪") {
      if (/入户|公共交通|银行|抢劫三次|抢劫数额巨大|致人重伤|致人死亡|冒充军警|持枪|军用物资|救灾/.test(text)) return "法定加重情形";
      if (/抢劫一次|普通抢劫|基本情形/.test(text)) return "抢劫一次（基本情形）";
    }
    return null;
  }

  analyzeInput(text: string, contextCrime: string | null = null): ExtractedSentencingInput {
    const extractedCrime = this.extractCrime(text);
    const crime = extractedCrime || contextCrime;
    return {
      crime: extractedCrime,
      region: this.extractRegion(text),
      amount: this.extractAmount(text),
      date: this.extractDate(text),
      factors: this.extractFactors(text),
      isTelecom: this.isTelecom(text) ? true : null,
      factTier: this.extractFactTier(text, crime),
    };
  }

  getMissingFields(extracted: ExtractedSentencingInput): string[] {
    const missing: string[] = [];
    if (!extracted.crime || !isCrimeName(extracted.crime)) return ["罪名"];
    const crimeId = CRIME_NAME_TO_ID[extracted.crime];
    const crime = SENTENCING_DATA.crimes.find((item) => item.id === crimeId)!;
    let standards = SENTENCING_DATA.standards[crimeId] ?? [];
    if (crimeId === "fraud") {
      standards = standards.filter((item) => extracted.isTelecom
        ? item.subType === "电信诈骗"
        : item.subType !== "电信诈骗");
    }
    if (crime.amountRequired && extracted.amount == null) missing.push("涉案金额");
    if (!extracted.date) missing.push("犯罪时间");
    if (["embezzlement", "non_official_bribery"].includes(crimeId)) missing.push("适用规则时间依据");
    const canSelectByAmount = standards.some((item) => item.minAmount != null);
    if (standards.length > 1 && !canSelectByAmount && !extracted.factTier) missing.push("案件事实档位");
    return missing;
  }
}

export const sentencingEngine = new SentencingEngine();
