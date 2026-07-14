export type CrimeId =
  | "theft"
  | "fraud"
  | "contract_fraud"
  | "embezzlement"
  | "disturbance"
  | "dangerous_driving"
  | "traffic_accident"
  | "non_official_bribery"
  | "corruption"
  | "bribery"
  | "intentional_injury"
  | "robbery"
  | "aiding_cyber_crime";

export type CrimeName =
  | "盗窃罪"
  | "诈骗罪"
  | "合同诈骗罪"
  | "职务侵占罪"
  | "寻衅滋事罪"
  | "危险驾驶罪"
  | "交通肇事罪"
  | "非国家工作人员受贿罪"
  | "贪污罪"
  | "受贿罪"
  | "故意伤害罪"
  | "抢劫罪"
  | "帮助信息网络犯罪活动罪";

export type AreaType = "一类地区" | "二类地区" | "全国";
export type MonthRange = [minimum: number, maximum: number | null];

export interface CrimeDefinition {
  id: CrimeId;
  name: CrimeName;
  desc: string;
}

export interface SentencingStandard {
  area: AreaType;
  tier: string;
  minAmount: number | null;
  maxAmount: number | null;
  startMin: number;
  startMax: number | null;
  subType?: "电信诈骗";
  effFrom?: string;
  effTo?: string;
}

export interface SentencingIncrementRule {
  area: AreaType;
  tier: string;
  perAmount: number;
  penaltyMin: number;
  penaltyMax: number | null;
  maxCap?: number;
  subType?: "一般诈骗" | "电信诈骗";
}

export interface SentencingFactorRule {
  id: string;
  name: string;
  direction: "reduce" | "increase";
  minPct: number;
  maxPct: number;
  fixMonths?: number;
}

export interface SentencingKeywords {
  crime: Array<[crimeName: string, keywords: string[]]>;
  region: Array<[area: AreaType, cities: string[]]>;
  factor: Record<string, string[]>;
  telecom: string[];
}

export interface SentencingData {
  crimes: CrimeDefinition[];
  standards: Record<CrimeId, SentencingStandard[]>;
  increments: Partial<Record<CrimeId, SentencingIncrementRule[]>>;
  priorityFactors: SentencingFactorRule[];
  generalFactors: SentencingFactorRule[];
  keywords: SentencingKeywords;
}

export interface CalculationProcessEntry {
  step: string;
  detail: string;
  valueRange?: MonthRange;
  valueMonths?: number;
}

export interface FactorAdjustment {
  factor: string;
  percentRange?: [minimum: number, maximum: number];
  fixMonths?: number;
  newRange: MonthRange;
}

export interface SentencingCalculationInput {
  crimeName: string;
  amount: number;
  areaType: AreaType;
  factors: Readonly<Record<string, boolean>>;
  crimeDate: string;
  judgeAdjustment?: number;
  isTelecom?: boolean;
  factTier?: string | null;
}

export interface SentencingCalculationResult extends SentencingCalculationInput {
  judgeAdjustment: number;
  factTier: string | null;
  process: CalculationProcessEntry[];
  error?: string;
  startingPointRange?: MonthRange;
  tier?: string | null;
  tierLabel?: string | null;
  standardDetail?: SentencingStandard;
  legalReferences?: readonly string[];
  extraPenaltyRange?: MonthRange;
  basePenaltyRange?: MonthRange;
  priorityAdjustments?: FactorAdjustment[];
  generalAdjustments?: FactorAdjustment[];
  finalPenaltyRange?: MonthRange;
  finalSentence?: string;
}

export interface ExtractedSentencingInput {
  crime: string | null;
  region: AreaType | null;
  amount: number | null;
  date: string | null;
  factors?: Record<string, boolean>;
  isTelecom?: true | null;
  factTier: string | null;
}
