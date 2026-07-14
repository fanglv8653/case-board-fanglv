import { CRIME_NAME_TO_ID, LEGAL_REFERENCES, SENTENCING_DATA } from "./data.ts";
import type {
  AreaType,
  CalculationProcessEntry,
  CrimeId,
  CrimeName,
  ExtractedSentencingInput,
  FactorAdjustment,
  MonthRange,
  SentencingCalculationInput,
  SentencingCalculationResult,
  SentencingFactorRule,
  SentencingStandard,
} from "./types.ts";

interface TierSelection {
  tier: string | null;
  tierLabel: string | null;
  standard: SentencingStandard | null;
  error?: string;
}

interface FactorAdjustmentResult {
  priorityAdjustments: FactorAdjustment[];
  generalAdjustments: FactorAdjustment[];
  afterFactors: MonthRange;
}

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

/**
 * 量刑计算器 — 计算引擎
 * 翻译自 sentencing_engine.py v2.0
 *
 * 量刑三步法：量刑起点 → 基准刑 → 宣告刑
 * 优先情节先调（连乘）→ 一般情节后调（逐项作用于前一结果）→ ±20%微调 → 兜底条款
 */

export class SentencingEngine {
  /**
   * 完整量刑计算
   * @param {string} crimeName - 罪名中文名
   * @param {number} amount - 涉案金额（元）
   * @param {string} areaType - '一类地区'/'二类地区'/'全国'
   * @param {Object} factors - 量刑情节字典 { '自首': true, '累犯': true, ... }
   * @param {string} crimeDate - '2026-05-01' 格式日期
   * @param {number} judgeAdjustment - -20~20 审判员微调
   * @param {boolean} isTelecom - 是否为电信诈骗
   * @returns {Object} 完整计算结果（区间形式）
   */
  calculate(input: SentencingCalculationInput): SentencingCalculationResult {
    const {
      crimeName,
      amount,
      areaType,
      factors,
      crimeDate,
      judgeAdjustment = 0,
      isTelecom = false,
      factTier = null,
    } = input;
    const processLog: CalculationProcessEntry[] = [];
    const result: SentencingCalculationResult = {
      crimeName, amount, areaType, factors: { ...factors }, crimeDate, judgeAdjustment, factTier,
      isTelecom,
      process: processLog,
    };

    processLog.push({ step: '开始计算', detail: `${crimeName}，${amount ? `涉案金额${amount}元` : ''}，${areaType}，犯罪日期${crimeDate}` });

    if (!isCrimeName(crimeName)) {
      processLog.push({ step: '错误', detail: `未知罪名: ${crimeName}` });
      result.error = `未知罪名: ${crimeName}`;
      return result;
    }
    const crimeId = CRIME_NAME_TO_ID[crimeName];
    result.legalReferences = LEGAL_REFERENCES[crimeName];

    if (!Number.isFinite(amount) || amount < 0) {
      result.error = '涉案金额必须是大于或等于0的有效数字';
      return result;
    }

    // ===== Step 1: 量刑起点（区间） =====
    processLog.push({ step: '第一步：确定量刑起点', detail: '根据犯罪数额/事实确定量刑起点幅度' });

    const { tier, tierLabel, standard, error } = this._getTierAndStandard(
      crimeId, amount, areaType, crimeDate, processLog, isTelecom, factTier
    );
    if (!standard) {
      result.error = error || '无法确定量刑标准';
      return result;
    }

    const startRange = this._calcStartingRange(standard, processLog);
    result.startingPointRange = startRange;  // [下限, 上限]
    result.tier = tier;
    result.tierLabel = tierLabel;
    result.standardDetail = { ...standard };

    // ===== Step 2: 基准刑（区间） =====
    processLog.push({ step: '第二步：确定基准刑', detail: '量刑起点 + 超额增加刑罚量' });

    const extraRange = this._calcBasePenaltyRange(
      crimeId, amount, areaType, tier ?? standard.tier, crimeDate, processLog, isTelecom
    );
    const baseLow = startRange[0] + extraRange[0];
    const baseHigh = startRange[1] != null && extraRange[1] != null
      ? startRange[1] + extraRange[1]
      : null;
    result.extraPenaltyRange = extraRange;
    result.basePenaltyRange = [baseLow, baseHigh];

    const baseDetail = baseHigh != null
      ? `量刑起点${this._formatMonthRange(startRange)} + 超额刑罚量${this._formatMonthRange(extraRange)} = 基准刑${this._formatMonthRange([baseLow, baseHigh])}`
      : `量刑起点与超额刑罚量合并后，基准刑为${baseLow}个月以上`;
    processLog.push({ step: '基准刑', detail: baseDetail, valueRange: [baseLow, baseHigh] });

    // ===== Step 3: 宣告刑（区间） =====
    processLog.push({ step: '第三步：确定宣告刑', detail: '优先情节先调 → 一般情节后调 → 微调 → 兜底' });

    const adjResult = this._applyFactorAdjustmentsRange([baseLow, baseHigh], factors, processLog);
    result.priorityAdjustments = adjResult.priorityAdjustments;
    result.generalAdjustments = adjResult.generalAdjustments;

    let finalLow = adjResult.afterFactors[0];
    let finalHigh = adjResult.afterFactors[1];

    // 审判员微调（±20%）
    if (judgeAdjustment !== 0) {
      const judgePct = Math.max(-20, Math.min(20, judgeAdjustment));
      const adjLow = Math.round(finalLow * judgePct / 100);
      const adjHigh = finalHigh != null ? Math.round(finalHigh * judgePct / 100) : adjLow;
      finalLow += adjLow;
      if (finalHigh != null) finalHigh += adjHigh;
      processLog.push({
        step: '审判员微调',
        detail: `审判员调整${judgePct}%: ${finalLow}~${finalHigh != null ? finalHigh : '∞'}月`,
      });
    } else {
      processLog.push({ step: '审判员微调', detail: '审判员未做调整' });
    }

    // 兜底条款
    if (finalLow < 0) finalLow = 0;
    if (finalHigh != null && finalHigh < 0) finalHigh = 0;

    // 危险驾驶罪最高6个月拘役
    if (crimeName === '危险驾驶罪') {
      if (finalHigh == null || finalHigh > 6) finalHigh = 6;
      if (finalLow > 6) finalLow = 6;
    }

    if (finalLow > 0 && finalLow < 1) finalLow = 1;
    if (finalHigh != null && finalHigh > 0 && finalHigh < 1) finalHigh = 1;

    result.finalPenaltyRange = [finalLow, finalHigh];
    result.finalSentence = this._formatSentenceRange(finalLow, finalHigh);

    processLog.push({
      step: '宣告刑',
      detail: `最终宣告刑范围: ${result.finalSentence}`,
    });

    return result;
  }

  /**
   * 确定档位和量刑标准
   */
  _getTierAndStandard(
    crimeId: CrimeId,
    amount: number,
    areaType: AreaType,
    crimeDate: string,
    processLog: CalculationProcessEntry[],
    isTelecom: boolean,
    factTier: string | null,
  ): TierSelection {
    const standards = SENTENCING_DATA.standards[crimeId];
    if (!standards || standards.length === 0) {
      processLog.push({ step: '错误', detail: `未找到${crimeId}的量刑标准` });
      return { tier: null, tierLabel: null, standard: null, error: '当前罪名没有可用的量刑标准' };
    }

    if (standards.some((standard) => standard.effFrom || standard.effTo) && !isValidIsoDate(crimeDate)) {
      const message = "该罪名存在按犯罪日期区分的标准，请提供有效的 YYYY-MM-DD 犯罪日期";
      processLog.push({ step: "错误", detail: message });
      return { tier: null, tierLabel: null, standard: null, error: message };
    }

    // 过滤有效的标准（按日期）
    const effective = standards.filter(s => {
      if (s.effFrom && crimeDate < s.effFrom) return false;
      if (s.effTo && crimeDate > s.effTo) return false;
      return true;
    });
    if (effective.length === 0) {
      const message = `犯罪日期${crimeDate}没有可用的有效量刑标准`;
      processLog.push({ step: "错误", detail: message });
      return { tier: null, tierLabel: null, standard: null, error: message };
    }

    // 按地区优先
    let byArea = effective.filter(s => s.area === areaType || s.area === '全国');
    if (byArea.length === 0) {
      processLog.push({ step: '错误', detail: `未找到${areaType}的量刑标准` });
      return { tier: null, tierLabel: null, standard: null, error: `未找到${areaType}的有效量刑标准` };
    }

    // 电信诈骗：只选电信诈骗专用标准
    if (isTelecom && crimeId === 'fraud') {
      byArea = byArea.filter(s => s.subType === '电信诈骗');
      if (byArea.length === 0) {
        processLog.push({ step: '错误', detail: '未找到电信诈骗的独立量刑标准' });
        return { tier: null, tierLabel: null, standard: null, error: '未找到电信网络诈骗专用标准' };
      }
    } else if (crimeId === 'fraud') {
      // 普通诈骗：排除电信诈骗专用标准
      byArea = byArea.filter(s => !s.subType || s.subType !== '电信诈骗');
    }

    // 非金额犯罪必须按案件事实选择档位，不能静默使用第一档。
    if (byArea.every(s => s.minAmount == null && s.maxAmount == null)) {
      const s = factTier
        ? byArea.find(item => item.tier === factTier)
        : (byArea.length === 1 ? byArea[0] : null);
      if (!s) {
        return {
          tier: null,
          tierLabel: null,
          standard: null,
          error: `请补充案件事实档位，可选：${byArea.map(item => item.tier).join('、')}`,
        };
      }
      processLog.push({ step: '确定档位', detail: `根据案件事实选择“${s.tier}”档` });
      return { tier: s.tier, tierLabel: s.tier, standard: s };
    }

    // 按金额匹配档位
    for (const s of byArea) {
      const minA = s.minAmount != null ? s.minAmount : 0;
      const maxA = s.maxAmount != null ? s.maxAmount : Infinity;
      if (minA <= amount && amount < maxA) {
        processLog.push({ step: '确定档位', detail: `涉案金额${amount}元，属于"${s.tier}"档` });
        return { tier: s.tier, tierLabel: s.tier, standard: s };
      }
    }

    const thresholds = byArea
      .map(s => s.minAmount)
      .filter(value => value != null)
      .sort((a, b) => a - b);
    const minimum = thresholds[0];
    if (minimum != null && amount < minimum) {
      const message = `涉案金额${amount}元低于当前数据表的最低数额起点${minimum}元，不能按最高档计算`;
      processLog.push({ step: '错误', detail: message });
      return { tier: null, tierLabel: null, standard: null, error: message };
    }

    return {
      tier: null,
      tierLabel: null,
      standard: null,
      error: `涉案金额${amount}元未匹配到连续有效的数额档位，请复核数据表`,
    };
  }

  /**
   * Step 1: 量刑起点 — 返回区间
   * @returns {Array} [下限月数, 上限月数]  上限为null表示无限
   */
  _calcStartingRange(
    standard: SentencingStandard,
    processLog: CalculationProcessEntry[],
  ): MonthRange {
    const { startMin, startMax } = standard;
    if (startMin == null) {
      processLog.push({ step: '量刑起点', detail: '无明确量刑起点' });
      return [0, null];
    }

    const detail = startMax != null
      ? `量刑起点幅度：${startMin}~${startMax}月`
      : `量刑起点：${startMin}月以上`;
    processLog.push({ step: '量刑起点', detail, valueRange: [startMin, startMax] });
    return [startMin, startMax];
  }

  /**
   * Step 2: 增加刑罚量
   */
  _calcBasePenaltyRange(
    crimeId: CrimeId,
    amount: number,
    areaType: AreaType,
    tier: string,
    crimeDate: string,
    processLog: CalculationProcessEntry[],
    isTelecom: boolean,
  ): MonthRange {
    // 无金额的犯罪不计算增加刑罚量
    if (!amount || amount <= 0) {
      processLog.push({ step: '超额刑罚量', detail: '无涉案金额或非金额犯罪，无增加刑罚量', valueMonths: 0 });
      return [0, 0];
    }

    const incRules = SENTENCING_DATA.increments[crimeId];
    if (!incRules) {
      processLog.push({ step: '超额刑罚量', detail: `未找到${tier}档的增加刑罚量规则`, valueMonths: 0 });
      return [0, 0];
    }

    // 筛选适用规则
    let rules = incRules.filter(r => (r.area === areaType || r.area === '全国') && r.tier === tier);

    // 电信诈骗过滤
    if (isTelecom && crimeId === 'fraud') {
      rules = rules.filter(r => r.subType === '电信诈骗' || (!r.subType && r.tier === '特别巨大'));
    } else if (crimeId === 'fraud') {
      rules = rules.filter(r => r.subType === '一般诈骗' || !r.subType);
    }

    if (rules.length === 0) {
      processLog.push({ step: '超额刑罚量', detail: `未找到适用规则`, valueMonths: 0 });
      return [0, 0];
    }

    // 确定基准数额（该档位的最低金额）
    const standards = SENTENCING_DATA.standards[crimeId];
    let baseAmount = 0;
    const areaStandards = standards.filter(s => {
      if (!(s.area === areaType || s.area === '全国') || s.tier !== tier) return false;
      if (s.effFrom && crimeDate < s.effFrom) return false;
      if (s.effTo && crimeDate > s.effTo) return false;
      if (crimeId === 'fraud') {
        return isTelecom ? s.subType === '电信诈骗' : s.subType !== '电信诈骗';
      }
      return true;
    });
    for (const s of areaStandards) {
      if (s.minAmount != null) {
        baseAmount = s.minAmount;
        break;
      }
    }

    if (baseAmount <= 0) {
      processLog.push({ step: '超额刑罚量', detail: '无法确定基准数额', valueMonths: 0 });
      return [0, 0];
    }

    const excess = amount - baseAmount;
    if (excess <= 0) {
      processLog.push({ step: '超额刑罚量', detail: `未超过基准数额${baseAmount}元，无增加刑罚量`, valueMonths: 0 });
      return [0, 0];
    }

    processLog.push({
      step: '超额计算',
      detail: `涉案金额${amount}元 - 基准${baseAmount}元 = 超额${excess}元`,
    });

    let totalLow = 0;
    let totalHigh: number | null = 0;
    const proportionalRules = rules.filter(rule => rule.perAmount > 0);
    const fixedRules = rules.filter(rule => rule.perAmount === 0);

    for (const rule of proportionalRules) {
      const numUnits = Math.floor(excess / rule.perAmount);
      if (numUnits <= 0) continue;
      const low = numUnits * rule.penaltyMin;
      const high = rule.penaltyMax == null ? null : numUnits * rule.penaltyMax;
      totalLow += low;
      totalHigh = totalHigh == null || high == null ? null : totalHigh + high;
      const label = rule.subType ? `（${rule.subType}）` : '';
      processLog.push({
        step: '超额刑罚量（每金额）',
        detail: `超额${excess}元，每${rule.perAmount}元${label}增加${rule.penaltyMin}~${rule.penaltyMax ?? '以上'}月，共${this._formatMonthRange([low, high])}`,
        valueRange: [low, high],
      });
    }

    if (fixedRules.length > 0) {
      const selected = fixedRules.find(rule => rule.maxCap != null && excess <= rule.maxCap)
        || fixedRules.find(rule => rule.maxCap == null);
      if (selected) {
        totalLow += selected.penaltyMin;
        totalHigh = totalHigh == null || selected.penaltyMax == null
          ? null
          : totalHigh + selected.penaltyMax;
        processLog.push({
          step: '超额刑罚量（分档）',
          detail: `超额${excess}元适用分档增加幅度${this._formatMonthRange([selected.penaltyMin, selected.penaltyMax])}`,
          valueRange: [selected.penaltyMin, selected.penaltyMax],
        });
      }
    }

    return [totalLow, totalHigh];
  }

  /**
   * Step 3: 情节调节（区间版）
   * 对区间两端分别做相同的比例调节
   * 优先情节连乘 → 一般情节后调并逐项按比例作用于前一结果
   * @param {Array} penaltyRange - [下限月数, 上限月数]
   * @returns {Object} { priorityAdjustments, generalAdjustments, afterFactors: [low, high] }
   */
  _applyFactorAdjustmentsRange(
    penaltyRange: MonthRange,
    factors: Readonly<Record<string, boolean>>,
    processLog: CalculationProcessEntry[],
  ): FactorAdjustmentResult {
    const priorityAdjustments: FactorAdjustment[] = [];
    const generalAdjustments: FactorAdjustment[] = [];
    let currentLow = penaltyRange[0];
    let currentHigh = penaltyRange[1];

    // 分离优先情节和一般情节
    const priorityInputs: SentencingFactorRule[] = [];
    const generalInputs: SentencingFactorRule[] = [];

    for (const [factorName, value] of Object.entries(factors)) {
      if (!value) continue;
      const pf = SENTENCING_DATA.priorityFactors.find(f => f.name === factorName);
      if (pf) {
        priorityInputs.push(pf);
        continue;
      }
      const gf = SENTENCING_DATA.generalFactors.find(f => f.name === factorName);
      if (gf) {
        generalInputs.push(gf);
      }
    }

    processLog.push({
      step: '情节分类',
      detail: `优先情节（先调）: ${priorityInputs.length}个，一般情节（后调）: ${generalInputs.length}个`,
    });

    // 百分比本身也是幅度，按最宽边界传播，避免再取无依据的中值。
    for (const f of priorityInputs) {
      const before: MonthRange = [currentLow, currentHigh];
      [currentLow, currentHigh] = this._applyPercentRange(before, f.minPct, f.maxPct);
      priorityAdjustments.push({
        factor: f.name, percentRange: [f.minPct, f.maxPct],
        newRange: [currentLow, currentHigh],
      });
      processLog.push({
        step: '优先情节调节',
        detail: `${f.name}: ${f.minPct}%~${f.maxPct}% → ${this._formatMonthRange([currentLow, currentHigh])}`,
      });
    }

    // 一般情节后调：逐项按比例作用于前一结果；固定月数另行增加。
    for (const f of generalInputs) {
      if (f.fixMonths) {
        // 固定月份（如累犯固定+3月），区间两端都加同样的固定值
        currentLow += f.fixMonths;
        if (currentHigh != null) currentHigh += f.fixMonths;
        generalAdjustments.push({
          factor: f.name, fixMonths: f.fixMonths,
          newRange: [currentLow, currentHigh],
        });
        const highStr = currentHigh != null ? `${currentHigh}月` : '∞';
        processLog.push({
          step: '一般情节调节',
          detail: `${f.name}: 固定+${f.fixMonths}月 → 区间[${currentLow}~${highStr}]月`,
        });
      } else {
        const before: MonthRange = [currentLow, currentHigh];
        [currentLow, currentHigh] = this._applyPercentRange(before, f.minPct, f.maxPct);
        generalAdjustments.push({
          factor: f.name, percentRange: [f.minPct, f.maxPct],
          newRange: [currentLow, currentHigh],
        });
        processLog.push({
          step: '一般情节调节',
          detail: `${f.name}: ${f.minPct}%~${f.maxPct}% → ${this._formatMonthRange([currentLow, currentHigh])}`,
        });
      }
    }

    const highStr = currentHigh != null ? `${currentHigh}月` : '∞';
    processLog.push({
      step: '情节调节完成',
      detail: `经优先情节+一般情节调节后，量刑区间为[${currentLow}~${highStr}]月`,
    });

    return { priorityAdjustments, generalAdjustments, afterFactors: [currentLow, currentHigh] };
  }

  /**
   * 格式化宣告刑（区间版）
   * 输出"X~Y年有期徒刑"或"X年有期徒刑"等
   */
  _formatSentenceRange(low: number, high: number | null): string {
    if (low <= 0 && high != null && high <= 0) return '免予刑事处罚';
    if (low <= 0 && high == null) return '刑期上限未配置，需人工判断';

    const lowStr = this._monthsToText(low);
    if (high == null) return `${this._monthsToDuration(low)}以上有期徒刑`;

    const highStr = this._monthsToText(high);
    if (low === high) return lowStr;

    return `${lowStr}～${highStr}`;
  }

  /**
   * 月数转文字：如 23月 → "1年11个月"，6月 → "6个月拘役"
   */
  _monthsToText(months: number): string {
    if (months <= 0) return '免予刑事处罚';
    if (months <= 6) return `${months}个月拘役`;
    if (months < 12) return `${months}个月有期徒刑`;

    const years = Math.floor(months / 12);
    const remain = months % 12;
    if (remain === 0) return `${years}年有期徒刑`;
    return `${years}年${remain}个月有期徒刑`;
  }

  _monthsToDuration(months: number): string {
    if (months < 12) return `${months}个月`;
    const years = Math.floor(months / 12);
    const remain = months % 12;
    return remain === 0 ? `${years}年` : `${years}年${remain}个月`;
  }

  _formatMonthRange(range: MonthRange): string {
    const [low, high] = range;
    if (high == null) return `${low}个月以上`;
    if (low === high) return `${low}个月`;
    return `${low}~${high}个月`;
  }

  _applyPercentRange(range: MonthRange, minPct: number, maxPct: number): MonthRange {
    const [low, high] = range;
    const nextLow = Math.floor(low * (100 + minPct) / 100);
    const nextHigh = high == null ? null : Math.ceil(high * (100 + maxPct) / 100);
    return [Math.min(nextLow, nextHigh ?? nextLow), nextHigh];
  }

  /**
   * 从自然语言中提取罪名
   */
  extractCrime(text: string): string | null {
    for (const [crimeName, keywords] of SENTENCING_DATA.keywords.crime) {
      for (const kw of keywords) {
        if (text.includes(kw)) return crimeName;
      }
    }
    return null;
  }

  /**
   * 从自然语言中提取地区
   */
  extractRegion(text: string): AreaType | null {
    for (const [region, cities] of SENTENCING_DATA.keywords.region) {
      for (const city of cities) {
        if (text.includes(city)) return region;
      }
    }
    return null;
  }

  /**
   * 从自然语言中提取金额（元）
   */
  extractAmount(text: string): number | null {
    const normalized = text.replace(/[,，]/g, '');
    let m = normalized.match(/(\d+(?:\.\d+)?)\s*亿\s*(\d+(?:\.\d+)?)?\s*万?/);
    if (m) return Math.round(Number(m[1]) * 100000000 + Number(m[2] || 0) * 10000);

    m = normalized.match(/(\d+(?:\.\d+)?)\s*万\s*(\d+)?\s*元?/);
    if (m) return Math.round(Number(m[1]) * 10000 + Number(m[2] || 0));

    m = normalized.match(/(\d+(?:\.\d+)?)\s*元/);
    if (m) return Math.round(Number(m[1]));

    return null;
  }

  /**
   * 从自然语言中提取情节
   */
  extractFactors(text: string): Record<string, boolean> {
    const factors: Record<string, boolean> = {};
    for (const [factorName, keywords] of Object.entries(SENTENCING_DATA.keywords.factor)) {
      for (const kw of keywords) {
        if (text.includes(kw)) {
          factors[factorName] = true;
          break;
        }
      }
    }
    if (factors['重大立功']) delete factors['一般立功'];
    if (factors['认罪认罚']) delete factors['当庭认罪'];
    if (factors['自首']) delete factors['坦白'];
    return factors;
  }

  /**
   * 判断是否为电信诈骗
   */
  isTelecom(text: string): boolean {
    return SENTENCING_DATA.keywords.telecom.some(kw => text.includes(kw));
  }

  /**
   * 提取犯罪日期
   */
  extractDate(text: string): string | null {
    const m = text.match(/(\d{4})(?:\s*年|[-/.])(\d{1,2})(?:\s*月|[-/.]?)(\d{1,2})?/);
    if (m) {
      const year = Number(m[1]);
      const month = Number(m[2]);
      const day = Number(m[3] || 1);
      const d = new Date(year, month - 1, day);
      if (d.getFullYear() !== year || d.getMonth() !== month - 1 || d.getDate() !== day) return null;
      return `${year}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
    }
    return null;
  }

  extractFactTier(text: string, crimeName: string | null): string | null {
    if (crimeName === '寻衅滋事罪') {
      if (/三次以上|三次|多次纠集|纠集/.test(text)) return '三次';
      if (/一次|单次/.test(text)) return '一次';
    }
    if (crimeName === '交通肇事罪') {
      if (/逃逸致死|因逃逸致人死亡/.test(text)) return '逃逸致死';
      if (/逃逸/.test(text)) return '逃逸';
      if (/未逃逸|基本情形|一般情形/.test(text)) return '基本';
    }
    if (crimeName === '故意伤害罪') {
      if (/致人死亡|死亡|严重残疾|特别残忍/.test(text)) return '致死/严重残疾';
      if (/重伤/.test(text)) return '重伤';
      if (/轻伤/.test(text)) return '轻伤';
    }
    if (crimeName === '抢劫罪') {
      if (/入户|公共交通|银行|多次抢劫|抢劫数额巨大|致人重伤|致人死亡|冒充军警|持枪|军用物资|救灾/.test(text)) return '加重';
      if (/普通抢劫|基本情形|一般抢劫/.test(text)) return '基本';
    }
    return null;
  }

  /**
   * 分析用户输入，提取所有信息
   */
  analyzeInput(text: string, contextCrime: string | null = null): ExtractedSentencingInput {
    const crime = this.extractCrime(text) || contextCrime;
    const result = {
      crime: this.extractCrime(text),
      region: this.extractRegion(text),
      amount: this.extractAmount(text),
      date: this.extractDate(text),
      factors: this.extractFactors(text),
      isTelecom: this.isTelecom(text) ? true as const : null,
      factTier: this.extractFactTier(text, crime),
    };
    return result;
  }

  /**
   * 返回缺失的要素列表
   */
  getMissingFields(extracted: ExtractedSentencingInput): string[] {
    const missing: string[] = [];
    if (!extracted.crime) missing.push('罪名');
    if (!extracted.crime) return missing;

    if (!isCrimeName(extracted.crime)) return [...missing, '罪名'];
    const crimeId = CRIME_NAME_TO_ID[extracted.crime];
    const standards = SENTENCING_DATA.standards[crimeId] || [];
    if (!extracted.isTelecom
        && standards.some(item => item.area === '一类地区' || item.area === '二类地区')
        && !extracted.region) {
      missing.push('地区');
    }
    if (standards.some(item => item.minAmount != null) && extracted.amount == null) {
      missing.push('涉案金额');
    }
    if (standards.some(item => item.effFrom || item.effTo) && !extracted.date) {
      missing.push('犯罪时间');
    }
    const factTiers = [...new Set(
      standards
        .filter(item => item.minAmount == null && item.maxAmount == null)
        .map(item => item.tier)
    )];
    if (factTiers.length > 1 && !extracted.factTier) {
      missing.push('案件事实档位');
    }
    return missing;
  }
}

export const sentencingEngine = new SentencingEngine();
