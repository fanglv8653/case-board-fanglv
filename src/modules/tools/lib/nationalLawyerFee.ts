/**
 * 全国律师收费规则状态与计算约束。
 *
 * 这里严格区分现行强约束、市场调节、官方历史参考、冲突和未核实状态。
 * 历史费率不得作为现行官方报价；市场调节地区不得自动生成伪官方数值。
 */

export type RegionSupportStatus = "supported_mainland" | "unsupported_legal_system";

export interface ProvincialRegion {
  code: string;
  name: string;
  supportStatus: RegionSupportStatus;
}

const MAINLAND_REGIONS: Array<[string, string]> = [
  ["110000", "北京"], ["120000", "天津"], ["130000", "河北"],
  ["140000", "山西"], ["150000", "内蒙古"], ["210000", "辽宁"],
  ["220000", "吉林"], ["230000", "黑龙江"], ["310000", "上海"],
  ["320000", "江苏"], ["330000", "浙江"], ["340000", "安徽"],
  ["350000", "福建"], ["360000", "江西"], ["370000", "山东"],
  ["410000", "河南"], ["420000", "湖北"], ["430000", "湖南"],
  ["440000", "广东"], ["450000", "广西"], ["460000", "海南"],
  ["500000", "重庆"], ["510000", "四川"], ["520000", "贵州"],
  ["530000", "云南"], ["540000", "西藏"], ["610000", "陕西"],
  ["620000", "甘肃"], ["630000", "青海"], ["640000", "宁夏"],
  ["650000", "新疆"],
];

export const PROVINCIAL_REGIONS: readonly ProvincialRegion[] = [
  ...MAINLAND_REGIONS.map(([code, name]) => ({
    code,
    name,
    supportStatus: "supported_mainland" as const,
  })),
  { code: "710000", name: "台湾", supportStatus: "unsupported_legal_system" },
  { code: "810000", name: "香港", supportStatus: "unsupported_legal_system" },
  { code: "820000", name: "澳门", supportStatus: "unsupported_legal_system" },
];

export type FeeRegimeStatus =
  | "market_pricing"
  | "historical_only"
  | "conflict_unverified"
  | "unverified"
  | "unsupported";

export interface OfficialSource {
  title: string;
  issuer: string;
  documentNo?: string;
  url: string;
  effectiveFrom?: string;
  effectiveTo?: string;
  validityNote: string;
}

export interface RegionFeeRegime {
  regionCode: string;
  cityCode?: string;
  regionName: string;
  status: FeeRegimeStatus;
  asOfDate: string;
  summary: string;
  autoOfficialCalculation: boolean;
  sources: OfficialSource[];
}

export const NATIONAL_RISK_SOURCE: OfficialSource = {
  title: "关于进一步规范律师服务收费的意见",
  issuer: "司法部、国家发展改革委、国家市场监督管理总局",
  documentNo: "司发通〔2021〕87号",
  url: "https://www.moj.gov.cn/pub/sfbgwapp/zwgk/tzggApp/202203/t20220324_451433.html",
  effectiveFrom: "2021-12-28",
  validityNote: "现行全国风险代理禁止范围与分段最高限额依据",
};

const VERIFIED_REGIMES: Record<string, RegionFeeRegime> = {
  "110000": {
    regionCode: "110000",
    regionName: "北京",
    status: "market_pricing",
    asOfDate: "2026-07-14",
    summary: "自2018年4月1日起律师法律服务收费全面实行市场调节价，无统一现行官方报价。",
    autoOfficialCalculation: false,
    sources: [{
      title: "关于全面放开本市律师法律服务收费的通知",
      issuer: "北京市司法局",
      url: "https://sfj.beijing.gov.cn/sfj/zwgk/tzgg75/382699/index.html",
      effectiveFrom: "2018-04-01",
      validityNote: "现行市场调节价状态",
    }],
  },
  "310000": {
    regionCode: "310000",
    regionName: "上海",
    status: "market_pricing",
    asOfDate: "2026-07-14",
    summary: "原地方收费办法已于2022年3月31日到期，普通收费原则上由律师事务所制定。",
    autoOfficialCalculation: false,
    sources: [{
      title: "上海市司法局关于律师收费依据的公开咨询答复",
      issuer: "上海市司法局",
      url: "https://hd.sfj.sh.gov.cn/sfj-interaction-front/biz/message/content/21e5dedc2299ce61748253e655454912746b1437233e7e652cd955850bf2e4830f2b86adb06e1a289d2cc0ef6987928c",
      effectiveTo: "2022-03-31",
      validityNote: "确认旧地方办法到期，当前按司发通〔2021〕87号执行",
    }, NATIONAL_RISK_SOURCE],
  },
  "440000": {
    regionCode: "440000",
    regionName: "广东",
    status: "market_pricing",
    asOfDate: "2026-07-14",
    summary: "旧收费管理实施办法已废止，旧费率仅供历史查询，不生成现行统一官方价。",
    autoOfficialCalculation: false,
    sources: [{
      title: "广东省发展改革委关于废止部分价格政策文件的通告",
      issuer: "广东省发展和改革委员会",
      url: "https://www.gd.gov.cn/attachment/0/511/511553/4082183.pdf",
      validityNote: "确认粤价〔2006〕298号等旧规则废止",
    }, NATIONAL_RISK_SOURCE],
  },
  "510000": {
    regionCode: "510000",
    regionName: "四川",
    status: "historical_only",
    asOfDate: "2026-07-14",
    summary: "川发改价格〔2018〕93号五年有效期已于2023年3月31日届满，未核到正式续期文本。",
    autoOfficialCalculation: false,
    sources: [{
      title: "四川省律师服务收费政府指导价标准",
      issuer: "四川省发展和改革委员会、四川省司法厅",
      documentNo: "川发改价格〔2018〕93号",
      url: "https://fgw.sc.gov.cn/sfgwsjd/c100105/2018/3/1/9ecbd9ac35c8469db4c253033f62d7fa.shtml",
      effectiveFrom: "2018-04-01",
      effectiveTo: "2023-03-31",
      validityNote: "已到期，仅作历史文件",
    }],
  },
  "320000": {
    regionCode: "320000",
    regionName: "江苏",
    status: "conflict_unverified",
    asOfDate: "2026-07-14",
    summary: "省级2016规则与无锡2020市场化公开口径存在表面冲突，停止生成官方价并等待进一步核验。",
    autoOfficialCalculation: false,
    sources: [{
      title: "江苏省律师服务收费管理办法",
      issuer: "江苏省司法厅",
      url: "https://sft.jiangsu.gov.cn/art/2016/11/3/art_48585_4140119.html",
      effectiveFrom: "2016-11-03",
      validityNote: "省级公开规则，后续效力需与地方市场化文件共同核验",
    }, {
      title: "无锡市关于进一步放开服务价格的实施方案",
      issuer: "无锡市发展和改革委员会",
      url: "https://dpc.wuxi.gov.cn/doc/2020/03/13/2861650.shtml",
      effectiveFrom: "2020-03-13",
      validityNote: "无锡公开口径称律师服务改为市场调节价",
    }],
  },
  "330000": {
    regionCode: "330000",
    regionName: "浙江",
    status: "market_pricing",
    asOfDate: "2026-07-14",
    summary: "2022年版定价目录已不列律师服务；2011/2015费率只可作为官方历史参考。",
    autoOfficialCalculation: false,
    sources: [{
      title: "浙江省定价目录（2022年版）",
      issuer: "浙江省发展和改革委员会",
      url: "https://zjjcmspublic.oss-cn-hangzhou-zwynet-d01-a.internet.cloud.zj.gov.cn/jcms_files/jcms1/web1902/site/attach/0/bdf90ce0cada43c4951ca8ca259045f1.pdf",
      effectiveFrom: "2022-08-01",
      validityNote: "现行定价目录不再包含律师服务",
    }, {
      title: "浙江省律师服务收费标准（历史参考）",
      issuer: "浙江省物价局、浙江省司法厅",
      documentNo: "浙价服〔2011〕212号、浙价服〔2015〕203号",
      url: "https://zjjcmspublic.oss-cn-hangzhou-zwynet-d01-a.internet.cloud.zj.gov.cn/jcms_files/jcms1/web1839/site/attach/0/3e367c73c20b41c98b623e01d2a96c06.pdf",
      effectiveTo: "2022-08-01",
      validityNote: "仅作官方历史参考，不是现行政府指导价",
    }],
  },
};

const WUXI_REGIME: RegionFeeRegime = {
  regionCode: "320000",
  cityCode: "320200",
  regionName: "江苏·无锡",
  status: "conflict_unverified",
  asOfDate: "2026-07-14",
  summary: "无锡2020公开文件称律师服务改为市场调节价，但与江苏省级旧规则存在表面冲突；不生成官方价。",
  autoOfficialCalculation: false,
  sources: VERIFIED_REGIMES["320000"].sources,
};

export function getRegionFeeRegime(regionCode: string, cityCode?: string): RegionFeeRegime {
  const region = PROVINCIAL_REGIONS.find((item) => item.code === regionCode);
  if (!region) throw new Error(`未知省级行政区代码：${regionCode}`);
  if (region.supportStatus === "unsupported_legal_system") {
    return {
      regionCode,
      regionName: region.name,
      status: "unsupported",
      asOfDate: "2026-07-14",
      summary: "该地区适用不同法律与律师收费制度，当前模型不支持，且不会回落内地规则。",
      autoOfficialCalculation: false,
      sources: [],
    };
  }
  if (regionCode === "320000" && cityCode === "320200") return WUXI_REGIME;
  return VERIFIED_REGIMES[regionCode] ?? {
    regionCode,
    regionName: region.name,
    status: "unverified",
    asOfDate: "2026-07-14",
    summary: "尚未完成现行官方收费制度核验，不生成官方价格；可录入本所参考标准。",
    autoOfficialCalculation: false,
    sources: [],
  };
}

export type RiskMatterCategory =
  | "general_property_civil"
  | "criminal"
  | "administrative"
  | "state_compensation"
  | "group_litigation"
  | "marriage_inheritance"
  | "social_security"
  | "minimum_living_security"
  | "support"
  | "pension_relief"
  | "work_injury"
  | "labor_remuneration";

const RISK_PROHIBITED = new Set<RiskMatterCategory>([
  "criminal", "administrative", "state_compensation", "group_litigation",
  "marriage_inheritance", "social_security", "minimum_living_security", "support",
  "pension_relief", "work_injury", "labor_remuneration",
]);

export interface RiskCapTierResult {
  fromExclusiveYuan: number;
  toInclusiveYuan: number | null;
  appliedYuan: number;
  rate: number;
  feeYuan: number;
}

export type RiskCapResult =
  | { allowed: false; error: "RISK_AGENT_PROHIBITED"; source: OfficialSource }
  | {
      allowed: true;
      basisAmountYuan: number;
      maximumFeeYuan: number;
      tiers: RiskCapTierResult[];
      aggregateAllStages: true;
      source: OfficialSource;
    };

const RISK_TIERS = [
  { upper: 1_000_000, rate: 0.18 },
  { upper: 5_000_000, rate: 0.15 },
  { upper: 10_000_000, rate: 0.12 },
  { upper: 50_000_000, rate: 0.09 },
  { upper: null, rate: 0.06 },
] as const;

function normalizeYuan(value: number): number {
  if (!Number.isFinite(value) || value < 0) throw new Error("金额必须是非负有限数值");
  return Math.round(value * 100) / 100;
}

export function calculateRiskAgencyCap(
  basisAmountYuan: number,
  category: RiskMatterCategory,
): RiskCapResult {
  if (RISK_PROHIBITED.has(category)) {
    return { allowed: false, error: "RISK_AGENT_PROHIBITED", source: NATIONAL_RISK_SOURCE };
  }
  const amount = normalizeYuan(basisAmountYuan);
  let previous = 0;
  let remaining = amount;
  const tiers: RiskCapTierResult[] = [];
  for (const tier of RISK_TIERS) {
    if (remaining <= 0) break;
    const capacity = tier.upper === null ? remaining : tier.upper - previous;
    const applied = Math.min(remaining, capacity);
    tiers.push({
      fromExclusiveYuan: previous,
      toInclusiveYuan: tier.upper,
      appliedYuan: applied,
      rate: tier.rate,
      feeYuan: normalizeYuan(applied * tier.rate),
    });
    remaining -= applied;
    if (tier.upper !== null) previous = tier.upper;
  }
  return {
    allowed: true,
    basisAmountYuan: amount,
    maximumFeeYuan: normalizeYuan(tiers.reduce((sum, tier) => sum + tier.feeYuan, 0)),
    tiers,
    aggregateAllStages: true,
    source: NATIONAL_RISK_SOURCE,
  };
}

export type ZhejiangHistoricalMatter = "criminal" | "civil" | "administrative" | "state_compensation";
export type CriminalStage = "investigation" | "prosecution" | "trial_first" | "private_prosecution";

export interface HistoricalFeeResult {
  status: "reference_only";
  calculationAuthority: "official_historical_reference";
  minYuan: number | null;
  maxYuan: number | null;
  tiers: Array<{ appliedYuan: number; minRate: number; maxRate: number; minFeeYuan: number; maxFeeYuan: number }>;
  mayCharge2500: boolean;
  manualAdjustmentRequired: boolean;
  laterStageCapYuan: number | null;
  complexUpperLimitYuan: number | null;
  warning: string;
  sources: OfficialSource[];
}

const ZHEJIANG_PROPERTY_TIERS = [
  { upper: 100_000, min: 0.06, max: 0.08 },
  { upper: 500_000, min: 0.05, max: 0.06 },
  { upper: 1_000_000, min: 0.04, max: 0.05 },
  { upper: 5_000_000, min: 0.03, max: 0.04 },
  { upper: 10_000_000, min: 0.02, max: 0.03 },
  { upper: null, min: 0.01, max: 0.02 },
] as const;

export function calculateZhejiangHistoricalReference(input: {
  matter: ZhejiangHistoricalMatter;
  criminalStage?: CriminalStage;
  propertyAmountYuan?: number | null;
  historicalReferenceConfirmed: boolean;
  procedureStage?: "first" | "later_same_firm";
  priorStageStandardYuan?: number | null;
  complexRequested?: boolean;
  complexQualified?: boolean;
}): HistoricalFeeResult {
  if (!input.historicalReferenceConfirmed) {
    throw new Error("必须主动确认查看非现行的浙江官方历史参考标准");
  }
  const sources = VERIFIED_REGIMES["330000"].sources;
  const base = {
    status: "reference_only" as const,
    calculationAuthority: "official_historical_reference" as const,
    tiers: [] as HistoricalFeeResult["tiers"],
    mayCharge2500: false,
    manualAdjustmentRequired: false,
    laterStageCapYuan: null,
    complexUpperLimitYuan: null,
    warning: "该费率仅为浙江官方历史参考，不是现行政府指导价，也不构成律师事务所报价。",
    sources,
  };
  if (input.matter === "criminal") {
    const ranges: Record<Exclude<CriminalStage, "private_prosecution">, [number, number]> = {
      investigation: [1_500, 8_000],
      prosecution: [1_500, 10_000],
      trial_first: [2_500, 25_000],
    };
    if (input.criminalStage === "private_prosecution") {
      return applyZhejiangHistoricalModifiers(
        { ...base, minYuan: null, maxYuan: null, manualAdjustmentRequired: true },
        input,
      );
    }
    if (!input.criminalStage) throw new Error("刑事历史参考必须选择办理阶段");
    const [minYuan, maxYuan] = ranges[input.criminalStage];
    return applyZhejiangHistoricalModifiers({ ...base, minYuan, maxYuan }, input);
  }
  if (input.propertyAmountYuan == null) {
    return applyZhejiangHistoricalModifiers(
      { ...base, minYuan: 2_500, maxYuan: 10_000 },
      input,
    );
  }
  const amount = normalizeYuan(input.propertyAmountYuan);
  let remaining = amount;
  let previous = 0;
  const tiers: HistoricalFeeResult["tiers"] = [];
  for (const tier of ZHEJIANG_PROPERTY_TIERS) {
    if (remaining <= 0) break;
    const capacity = tier.upper === null ? remaining : tier.upper - previous;
    const applied = Math.min(remaining, capacity);
    tiers.push({
      appliedYuan: applied,
      minRate: tier.min,
      maxRate: tier.max,
      minFeeYuan: normalizeYuan(applied * tier.min),
      maxFeeYuan: normalizeYuan(applied * tier.max),
    });
    remaining -= applied;
    if (tier.upper !== null) previous = tier.upper;
  }
  const minYuan = normalizeYuan(tiers.reduce((sum, tier) => sum + tier.minFeeYuan, 0));
  const maxYuan = normalizeYuan(tiers.reduce((sum, tier) => sum + tier.maxFeeYuan, 0));
  return applyZhejiangHistoricalModifiers(
    { ...base, minYuan, maxYuan, tiers, mayCharge2500: minYuan < 2_500 },
    input,
  );
}

function applyZhejiangHistoricalModifiers(
  result: HistoricalFeeResult,
  input: {
    procedureStage?: "first" | "later_same_firm";
    priorStageStandardYuan?: number | null;
    complexRequested?: boolean;
    complexQualified?: boolean;
  },
): HistoricalFeeResult {
  let next = { ...result };
  if (input.procedureStage === "later_same_firm") {
    if (input.priorStageStandardYuan == null) {
      next.manualAdjustmentRequired = true;
    } else {
      const laterStageCapYuan = normalizeYuan(input.priorStageStandardYuan * 0.7);
      next.laterStageCapYuan = laterStageCapYuan;
      if (next.maxYuan != null) next.maxYuan = Math.min(next.maxYuan, laterStageCapYuan);
      if (next.minYuan != null && next.maxYuan != null && next.minYuan > next.maxYuan) {
        next.minYuan = next.maxYuan;
      }
    }
  }
  if (input.complexRequested) {
    if (!input.complexQualified || next.maxYuan == null) {
      next.manualAdjustmentRequired = true;
    } else {
      next.complexUpperLimitYuan = normalizeYuan(next.maxYuan * 5);
      next.maxYuan = next.complexUpperLimitYuan;
    }
  }
  return next;
}

export interface PracticeQuoteProfile {
  label: string;
  source: "law_firm_internal";
  minYuan: number;
  maxYuan: number;
  note: string;
}

export function createPracticeQuoteProfile(input: {
  label: string;
  minYuan: number;
  maxYuan: number;
}): PracticeQuoteProfile {
  const minYuan = normalizeYuan(input.minYuan);
  const maxYuan = normalizeYuan(input.maxYuan);
  if (!input.label.trim()) throw new Error("内部参考标准必须填写名称");
  if (maxYuan < minYuan) throw new Error("内部参考区间上限不得低于下限");
  return {
    label: input.label.trim(),
    source: "law_firm_internal",
    minYuan,
    maxYuan,
    note: "律所/内部参考标准，非官方收费标准。",
  };
}
