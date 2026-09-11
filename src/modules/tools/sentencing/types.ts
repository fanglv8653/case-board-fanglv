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
export type PrincipalPenaltyKind =
  | "control"
  | "detention"
  | "fixed_term"
  | "life"
  | "death"
  | "fine_only"
  | "exemption";
export type QuantitativeStatus = "starting_point_only" | "statutory_only" | "direct_sentence";
export type TemporalRuleBasis =
  | "conduct_on_or_after_2026_05_01"
  | "pre_effective_current_rule_reviewed"
  | "individual_comparison_required";
export type CalculationStatus = "complete" | "requires_manual_base" | "blocked";

export interface CrimeDefinition {
  id: CrimeId;
  name: CrimeName;
  desc: string;
  culpability: "intentional" | "negligent";
  amountRequired: boolean;
}

export interface SentenceOption {
  kind: PrincipalPenaltyKind;
  minimumMonths?: number;
  maximumMonths?: number | null;
  label: string;
}

export interface StatutoryPenaltyBand {
  label: string;
  article: string;
  monthBounds: MonthRange;
  mitigatedMonthBounds?: MonthRange;
  options: SentenceOption[];
  mitigatedOptions?: SentenceOption[];
}

export interface SentencingStandard {
  area: AreaType;
  tier: string;
  minAmount: number | null;
  maxAmount: number | null;
  startMin?: number;
  startMax?: number | null;
  startKinds?: PrincipalPenaltyKind[];
  subType?: "电信诈骗";
  quantitativeStatus: QuantitativeStatus;
  statutory: StatutoryPenaltyBand;
  sourceIds: string[];
  temporalRuleRequired?: boolean;
}

export type FactorLegalEffect =
  | "lighter"
  | "lighter_or_mitigated"
  | "lighter_or_mitigated_or_exempt"
  | "mitigated_or_exempt"
  | "heavier"
  | "exempt"
  | "qualitative";

export interface SentencingFactorRule {
  id: string;
  name: string;
  direction: "reduce" | "increase";
  minPct?: number;
  maxPct?: number;
  minimumIncreaseMonths?: number;
  quantitative: boolean;
  legalEffect: FactorLegalEffect;
  conflictGroup?: string;
  conflictsWith?: string[];
  applicableCrimeIds?: CrimeId[];
  excludedCrimeIds?: CrimeId[];
  applicableFactTiers?: string[];
  note?: string;
}

export interface SentencingKeywords {
  crime: Array<[crimeName: string, keywords: string[]]>;
  region: Array<[area: AreaType, cities: string[]]>;
  factor: Record<string, string[]>;
  telecom: string[];
}

export interface SentencingSource {
  id: string;
  title: string;
  authority: string;
  documentNo?: string;
  effectiveFrom?: string;
  url: string;
  verifiedOn: string;
  status: "current" | "scope_limited" | "current_applicability_unverified";
  note?: string;
}

export interface SentencingRulesetMetadata {
  id: string;
  version: string;
  engineVersion: string;
  schemaVersion: string;
  contentHash: string;
  verifiedOn: string;
  sources: SentencingSource[];
  limitations: string[];
}

export interface SentencingData {
  ruleset: SentencingRulesetMetadata;
  crimes: CrimeDefinition[];
  standards: Record<CrimeId, SentencingStandard[]>;
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
  minimumIncreaseMonths?: number;
  newRange: MonthRange;
}

export interface SentencingCalculationInput {
  crimeName: string;
  amount: number;
  areaType: AreaType;
  factors: Readonly<Record<string, boolean>>;
  crimeDate: string;
  judgeAdjustment?: number;
  judgeAdjustmentReason?: string;
  isTelecom?: boolean;
  factTier?: string | null;
  manualBasePenaltyRange?: MonthRange | null;
  manualBaseSource?: string;
  temporalRuleBasis?: TemporalRuleBasis | null;
}

export interface SentencingCalculationResult extends SentencingCalculationInput {
  judgeAdjustment: number;
  factTier: string | null;
  manualBasePenaltyRange: MonthRange | null;
  temporalRuleBasis: TemporalRuleBasis | null;
  calculationStatus: CalculationStatus;
  process: CalculationProcessEntry[];
  warnings: string[];
  blockingIssues: string[];
  ruleset: SentencingRulesetMetadata;
  error?: string;
  statutoryPenalty?: StatutoryPenaltyBand;
  startingPointRange?: MonthRange;
  startingPointKinds?: PrincipalPenaltyKind[];
  tier?: string | null;
  tierLabel?: string | null;
  standardDetail?: SentencingStandard;
  legalReferences?: readonly string[];
  basePenaltyRange?: MonthRange;
  priorityAdjustments?: FactorAdjustment[];
  generalAdjustments?: FactorAdjustment[];
  rawAdjustedPenaltyRange?: MonthRange;
  finalPenaltyRange?: MonthRange;
  finalPenaltyKinds?: PrincipalPenaltyKind[];
  finalSentence?: string;
  disposition?: "term_range" | "exemption" | "manual_review";
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
