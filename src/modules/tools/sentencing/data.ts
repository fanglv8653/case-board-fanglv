import type {
  CrimeId,
  CrimeName,
  PrincipalPenaltyKind,
  SentenceOption,
  SentencingData,
  StatutoryPenaltyBand,
} from "./types.ts";

const formatDuration = (months: number) => {
  if (months < 12) return `${months}个月`;
  if (months % 12 === 0) return `${months / 12}年`;
  return `${Math.floor(months / 12)}年${months % 12}个月`;
};

const fixedTerm = (minimumMonths: number, maximumMonths: number | null): SentenceOption => ({
  kind: "fixed_term",
  minimumMonths,
  maximumMonths,
  label: maximumMonths == null
    ? `${formatDuration(minimumMonths)}以上有期徒刑`
    : `${formatDuration(minimumMonths)}至${formatDuration(maximumMonths)}有期徒刑`,
});

const detention: SentenceOption = {
  kind: "detention",
  minimumMonths: 1,
  maximumMonths: 6,
  label: "拘役1个月至6个月",
};
const control: SentenceOption = {
  kind: "control",
  minimumMonths: 3,
  maximumMonths: 24,
  label: "管制3个月至2年",
};
const life: SentenceOption = { kind: "life", label: "无期徒刑" };
const death: SentenceOption = { kind: "death", label: "死刑（仅法定特别情形）" };
const fineOnly: SentenceOption = { kind: "fine_only", label: "单处罚金" };

function statutory(
  article: string,
  label: string,
  monthBounds: [number, number | null],
  options: SentenceOption[],
  mitigatedMonthBounds?: [number, number | null],
  mitigatedOptions?: SentenceOption[],
): StatutoryPenaltyBand {
  return { article, label, monthBounds, mitigatedMonthBounds, options, mitigatedOptions };
}

const INTENTIONAL_CRIMES: CrimeId[] = [
  "theft",
  "fraud",
  "contract_fraud",
  "embezzlement",
  "disturbance",
  "dangerous_driving",
  "non_official_bribery",
  "corruption",
  "bribery",
  "intentional_injury",
  "robbery",
  "aiding_cyber_crime",
];
const INTENTIONAL_ACCOMPLICE_CRIMES = INTENTIONAL_CRIMES.filter(
  (crimeId) => crimeId !== "dangerous_driving",
);

const criminalLow = (article: string, withFineOnly = false) => statutory(
  article,
  "三年以下有期徒刑、拘役或者管制",
  [1, 36],
  [fixedTerm(6, 36), detention, control, ...(withFineOnly ? [fineOnly] : [])],
);
const financialLow = (article: string) => statutory(
  article,
  "三年以下有期徒刑或者拘役，并处或者单处罚金",
  [1, 36],
  [fixedTerm(6, 36), detention, fineOnly],
);
const financialMiddle = (article: string) => statutory(
  article,
  "三年以上十年以下有期徒刑，并处罚金",
  [36, 120],
  [fixedTerm(36, 120)],
  [1, 36],
  [fixedTerm(6, 36), detention, fineOnly],
);
const financialHigh = (article: string, allowLife = true) => statutory(
  article,
  allowLife
    ? "十年以上有期徒刑或者无期徒刑，并处罚金或者没收财产"
    : "十年以上有期徒刑，并处罚金或者没收财产",
  [120, 180],
  [fixedTerm(120, 180), ...(allowLife ? [life] : [])],
  [36, 120],
  [fixedTerm(36, 120)],
);
const corporateLow = (article: string) => statutory(
  article,
  "三年以下有期徒刑或者拘役，并处罚金",
  [1, 36],
  [fixedTerm(6, 36), detention],
);
const corporateMiddle = (article: string) => statutory(
  article,
  "三年以上十年以下有期徒刑，并处罚金",
  [36, 120],
  [fixedTerm(36, 120)],
  [1, 36],
  [fixedTerm(6, 36), detention],
);
const corporateHigh = (article: string) => statutory(
  article,
  "十年以上有期徒刑或者无期徒刑，并处罚金",
  [120, 180],
  [fixedTerm(120, 180), life],
  [36, 120],
  [fixedTerm(36, 120)],
);

/**
 * 全国层面的保守规则集。
 *
 * 粤高法发〔2017〕6号依附于已经废止的2017年全国指导意见，且尚未取得
 * 2021年后广东法检共同实施细则的权威现行文本，因此不再用于自动增加刑罚量。
 * 金额只是案件快照字段；普通财产犯罪和电信诈骗档位均由律师人工确认。
 * 2026贪污贿赂解释（二）的参照门槛需要先完成时间效力选择，才允许自动定位档位。
 */
export const SENTENCING_DATA: SentencingData = {
  ruleset: {
    id: "cn-national-sentencing-assist",
    version: "2026.09.11-1",
    engineVersion: "3.0.0",
    schemaVersion: "sentencing-estimate-v2",
    contentHash: "sha256:838470d24f63415385e761195ec3eab2b144f4cdf1f472ab07c402e4ede69c21",
    verifiedOn: "2026-09-11",
    sources: [
      {
        id: "criminal-law-current",
        title: "中华人民共和国刑法",
        authority: "全国人民代表大会及其常务委员会",
        url: "https://www.samr.gov.cn/zw/zfxxgk/fdzdgknr/bgt/art/2025/art_890f1333b6284c3cbb225c3cd2647c4b.html",
        verifiedOn: "2026-09-11",
        status: "scope_limited",
        note: "用于本工具涉及的总则和分则条款；刑法修正案（十二）未修改这些条款。",
      },
      {
        id: "sentencing-guidance-2021",
        title: "关于常见犯罪的量刑指导意见（试行）",
        authority: "最高人民法院、最高人民检察院",
        documentNo: "法发〔2021〕21号",
        effectiveFrom: "2021-07-01",
        url: "https://www.court.gov.cn/zixun/xiangqing/312301.html",
        verifiedOn: "2026-09-11",
        status: "current",
      },
      {
        id: "sentencing-guidance-2021-fulltext",
        title: "关于常见犯罪的量刑指导意见（试行）全文",
        authority: "政府网站转载的两高文件",
        documentNo: "法发〔2021〕21号",
        effectiveFrom: "2021-07-01",
        url: "https://www.jinjiang.gov.cn/ztzl/jjgazl/flfg/202107/t20210706_2583807.htm",
        verifiedOn: "2026-09-11",
        status: "current",
      },
      {
        id: "corruption-interpretation-2016",
        title: "关于办理贪污贿赂刑事案件适用法律若干问题的解释",
        authority: "最高人民法院、最高人民检察院",
        documentNo: "法释〔2016〕9号",
        effectiveFrom: "2016-04-18",
        url: "https://www.court.gov.cn/fabu/xiangqing/19612.html",
        verifiedOn: "2026-09-11",
        status: "current",
      },
      {
        id: "corruption-interpretation-2026",
        title: "关于办理贪污贿赂刑事案件适用法律若干问题的解释（二）",
        authority: "最高人民法院、最高人民检察院",
        documentNo: "法释〔2026〕6号",
        effectiveFrom: "2026-05-01",
        url: "https://www.court.gov.cn/zixun/xiangqing/497181.html",
        verifiedOn: "2026-09-11",
        status: "current",
      },
      {
        id: "criminal-interpretation-time-effect",
        title: "关于适用刑事司法解释时间效力问题的规定",
        authority: "最高人民法院、最高人民检察院",
        documentNo: "高检发释字〔2001〕5号",
        effectiveFrom: "2001-12-17",
        url: "https://www.spp.gov.cn/llyj/201705/t20170531_191834.shtml",
        verifiedOn: "2026-09-11",
        status: "current",
      },
    ],
    limitations: [
      "本规则集不使用现行适用状态未核实的粤高法发〔2017〕6号自动推导增加刑罚量。",
      "量刑起点不是法定刑；除危险驾驶罪外，进入情节调节前必须录入已人工核验的基准刑区间和来源。",
      "月数区间不替代无期徒刑、死刑、管制、单处罚金、缓刑或免予刑事处罚的独立判断。",
      "单罪辅助测算不处理数罪并罚、单位犯罪、共同犯罪全案责任分配和类案偏离校准。",
    ],
  },

  crimes: [
    { id: "theft", name: "盗窃罪", desc: "以非法占有为目的，盗窃公私财物", culpability: "intentional", amountRequired: true },
    { id: "fraud", name: "诈骗罪", desc: "以非法占有为目的，骗取公私财物", culpability: "intentional", amountRequired: true },
    { id: "contract_fraud", name: "合同诈骗罪", desc: "在签订、履行合同过程中骗取对方财物", culpability: "intentional", amountRequired: true },
    { id: "embezzlement", name: "职务侵占罪", desc: "利用职务便利非法占有本单位财物", culpability: "intentional", amountRequired: true },
    { id: "disturbance", name: "寻衅滋事罪", desc: "刑法第二百九十三条规定的寻衅滋事行为", culpability: "intentional", amountRequired: false },
    { id: "dangerous_driving", name: "危险驾驶罪", desc: "刑法第一百三十三条之一规定的危险驾驶行为", culpability: "intentional", amountRequired: false },
    { id: "traffic_accident", name: "交通肇事罪", desc: "违反交通运输管理法规造成重大事故", culpability: "negligent", amountRequired: false },
    { id: "non_official_bribery", name: "非国家工作人员受贿罪", desc: "利用职务便利索取或非法收受他人财物", culpability: "intentional", amountRequired: true },
    { id: "corruption", name: "贪污罪", desc: "国家工作人员利用职务便利非法占有公共财物", culpability: "intentional", amountRequired: true },
    { id: "bribery", name: "受贿罪", desc: "国家工作人员利用职务便利索取或非法收受他人财物", culpability: "intentional", amountRequired: true },
    { id: "intentional_injury", name: "故意伤害罪", desc: "故意非法损害他人身体健康", culpability: "intentional", amountRequired: false },
    { id: "robbery", name: "抢劫罪", desc: "以暴力、胁迫或者其他方法强行劫取财物", culpability: "intentional", amountRequired: false },
    { id: "aiding_cyber_crime", name: "帮助信息网络犯罪活动罪", desc: "明知他人利用信息网络实施犯罪而提供帮助", culpability: "intentional", amountRequired: false },
  ],

  standards: {
    theft: [
      { area: "全国", tier: "较大/其他入罪情形", minAmount: null, maxAmount: null, startMin: 1, startMax: 12, startKinds: ["detention", "fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialLow("《刑法》第264条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "巨大/其他严重情节", minAmount: null, maxAmount: null, startMin: 36, startMax: 48, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialMiddle("《刑法》第264条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "特别巨大/其他特别严重情节", minAmount: null, maxAmount: null, startMin: 120, startMax: 144, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialHigh("《刑法》第264条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    fraud: [
      { area: "全国", tier: "较大", minAmount: null, maxAmount: null, startMin: 1, startMax: 12, startKinds: ["detention", "fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialLow("《刑法》第266条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "巨大/其他严重情节", minAmount: null, maxAmount: null, startMin: 36, startMax: 48, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialMiddle("《刑法》第266条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "特别巨大/其他特别严重情节", minAmount: null, maxAmount: null, startMin: 120, startMax: 144, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialHigh("《刑法》第266条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "较大", minAmount: null, maxAmount: null, startMin: 1, startMax: 12, startKinds: ["detention", "fixed_term"], subType: "电信诈骗", quantitativeStatus: "starting_point_only", statutory: financialLow("《刑法》第266条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "巨大/其他严重情节", minAmount: null, maxAmount: null, startMin: 36, startMax: 48, startKinds: ["fixed_term"], subType: "电信诈骗", quantitativeStatus: "starting_point_only", statutory: financialMiddle("《刑法》第266条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "特别巨大/其他特别严重情节", minAmount: null, maxAmount: null, startMin: 120, startMax: 144, startKinds: ["fixed_term"], subType: "电信诈骗", quantitativeStatus: "starting_point_only", statutory: financialHigh("《刑法》第266条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    contract_fraud: [
      { area: "全国", tier: "数额较大", minAmount: null, maxAmount: null, startMin: 1, startMax: 12, startKinds: ["detention", "fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialLow("《刑法》第224条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "数额巨大/其他严重情节", minAmount: null, maxAmount: null, startMin: 36, startMax: 48, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialMiddle("《刑法》第224条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "数额特别巨大/其他特别严重情节", minAmount: null, maxAmount: null, startMin: 120, startMax: 144, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: financialHigh("《刑法》第224条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    embezzlement: [
      { area: "全国", tier: "数额较大", minAmount: 30000, maxAmount: 200000, startMin: 1, startMax: 12, startKinds: ["detention", "fixed_term"], quantitativeStatus: "starting_point_only", statutory: corporateLow("《刑法》第271条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021", "corruption-interpretation-2026"], temporalRuleRequired: true },
      { area: "全国", tier: "数额巨大", minAmount: 200000, maxAmount: 3000000, startMin: 36, startMax: 48, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: corporateMiddle("《刑法》第271条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021", "corruption-interpretation-2026"], temporalRuleRequired: true },
      { area: "全国", tier: "数额特别巨大", minAmount: 3000000, maxAmount: null, startMin: 120, startMax: 132, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: corporateHigh("《刑法》第271条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021", "corruption-interpretation-2026"], temporalRuleRequired: true },
    ],
    disturbance: [
      { area: "全国", tier: "一次", minAmount: null, maxAmount: null, startMin: 1, startMax: 36, startKinds: ["detention", "fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第293条", "五年以下有期徒刑、拘役或者管制", [1, 60], [fixedTerm(6, 60), detention, control]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "纠集他人三次且每次构罪", minAmount: null, maxAmount: null, startMin: 60, startMax: 84, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第293条第二款", "五年以上十年以下有期徒刑，可以并处罚金", [60, 120], [fixedTerm(60, 120)], [1, 60], [fixedTerm(6, 60), detention, control]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    dangerous_driving: [
      { area: "全国", tier: "构成犯罪", minAmount: null, maxAmount: null, startMin: 1, startMax: 6, startKinds: ["detention"], quantitativeStatus: "direct_sentence", statutory: statutory("《刑法》第133条之一", "拘役，并处罚金", [1, 6], [detention]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    traffic_accident: [
      { area: "全国", tier: "基本情形", minAmount: null, maxAmount: null, startMin: 1, startMax: 24, startKinds: ["detention", "fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第133条", "三年以下有期徒刑或者拘役", [1, 36], [fixedTerm(6, 36), detention]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "逃逸/其他特别恶劣情节", minAmount: null, maxAmount: null, startMin: 36, startMax: 60, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第133条", "三年以上七年以下有期徒刑", [36, 84], [fixedTerm(36, 84)], [1, 36], [fixedTerm(6, 36), detention]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "因逃逸致人死亡", minAmount: null, maxAmount: null, startMin: 84, startMax: 120, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第133条", "七年以上有期徒刑", [84, 180], [fixedTerm(84, 180)], [36, 84], [fixedTerm(36, 84)]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    non_official_bribery: [
      { area: "全国", tier: "数额较大", minAmount: 30000, maxAmount: 200000, quantitativeStatus: "statutory_only", statutory: corporateLow("《刑法》第163条"), sourceIds: ["criminal-law-current", "corruption-interpretation-2026"], temporalRuleRequired: true },
      { area: "全国", tier: "数额巨大", minAmount: 200000, maxAmount: 3000000, quantitativeStatus: "statutory_only", statutory: corporateMiddle("《刑法》第163条"), sourceIds: ["criminal-law-current", "corruption-interpretation-2026"], temporalRuleRequired: true },
      { area: "全国", tier: "数额特别巨大", minAmount: 3000000, maxAmount: null, quantitativeStatus: "statutory_only", statutory: corporateHigh("《刑法》第163条"), sourceIds: ["criminal-law-current", "corruption-interpretation-2026"], temporalRuleRequired: true },
    ],
    corruption: [
      { area: "全国", tier: "数额较大/其他较重情节", minAmount: 30000, maxAmount: 200000, quantitativeStatus: "statutory_only", statutory: corporateLow("《刑法》第383条"), sourceIds: ["criminal-law-current", "corruption-interpretation-2016"] },
      { area: "全国", tier: "数额巨大/其他严重情节", minAmount: 200000, maxAmount: 3000000, quantitativeStatus: "statutory_only", statutory: statutory("《刑法》第383条", "三年以上十年以下有期徒刑，并处罚金或者没收财产", [36, 120], [fixedTerm(36, 120)], [1, 36], [fixedTerm(6, 36), detention]), sourceIds: ["criminal-law-current", "corruption-interpretation-2016"] },
      { area: "全国", tier: "数额特别巨大/其他特别严重情节", minAmount: 3000000, maxAmount: null, quantitativeStatus: "statutory_only", statutory: statutory("《刑法》第383条", "十年以上有期徒刑、无期徒刑；特定情形可判死刑", [120, 180], [fixedTerm(120, 180), life, death], [36, 120], [fixedTerm(36, 120)]), sourceIds: ["criminal-law-current", "corruption-interpretation-2016"] },
    ],
    bribery: [
      { area: "全国", tier: "数额较大/其他较重情节", minAmount: 30000, maxAmount: 200000, quantitativeStatus: "statutory_only", statutory: corporateLow("《刑法》第386条、第383条"), sourceIds: ["criminal-law-current", "corruption-interpretation-2016"] },
      { area: "全国", tier: "数额巨大/其他严重情节", minAmount: 200000, maxAmount: 3000000, quantitativeStatus: "statutory_only", statutory: statutory("《刑法》第386条、第383条", "三年以上十年以下有期徒刑，并处罚金或者没收财产", [36, 120], [fixedTerm(36, 120)], [1, 36], [fixedTerm(6, 36), detention]), sourceIds: ["criminal-law-current", "corruption-interpretation-2016"] },
      { area: "全国", tier: "数额特别巨大/其他特别严重情节", minAmount: 3000000, maxAmount: null, quantitativeStatus: "statutory_only", statutory: statutory("《刑法》第386条、第383条", "十年以上有期徒刑、无期徒刑；特定情形可判死刑", [120, 180], [fixedTerm(120, 180), life, death], [36, 120], [fixedTerm(36, 120)]), sourceIds: ["criminal-law-current", "corruption-interpretation-2016"] },
    ],
    intentional_injury: [
      { area: "全国", tier: "致一人轻伤", minAmount: null, maxAmount: null, startMin: 1, startMax: 24, startKinds: ["detention", "fixed_term"], quantitativeStatus: "starting_point_only", statutory: criminalLow("《刑法》第234条"), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "致一人重伤", minAmount: null, maxAmount: null, startMin: 36, startMax: 60, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第234条", "三年以上十年以下有期徒刑", [36, 120], [fixedTerm(36, 120)], [1, 36], [fixedTerm(6, 36), detention, control]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "特别残忍手段致重伤严重残疾/致死", minAmount: null, maxAmount: null, startMin: 120, startMax: 156, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第234条", "十年以上有期徒刑、无期徒刑或者死刑", [120, 180], [fixedTerm(120, 180), life, death], [36, 120], [fixedTerm(36, 120)]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    robbery: [
      { area: "全国", tier: "抢劫一次（基本情形）", minAmount: null, maxAmount: null, startMin: 36, startMax: 72, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第263条", "三年以上十年以下有期徒刑，并处罚金", [36, 120], [fixedTerm(36, 120)]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
      { area: "全国", tier: "法定加重情形", minAmount: null, maxAmount: null, startMin: 120, startMax: 156, startKinds: ["fixed_term"], quantitativeStatus: "starting_point_only", statutory: statutory("《刑法》第263条", "十年以上有期徒刑、无期徒刑或者死刑，并处罚金或者没收财产", [120, 180], [fixedTerm(120, 180), life, death], [36, 120], [fixedTerm(36, 120)]), sourceIds: ["criminal-law-current", "sentencing-guidance-2021"] },
    ],
    aiding_cyber_crime: [
      { area: "全国", tier: "情节严重", minAmount: null, maxAmount: null, quantitativeStatus: "statutory_only", statutory: financialLow("《刑法》第287条之二"), sourceIds: ["criminal-law-current"] },
    ],
  },

  priorityFactors: [
    { id: "minor_12_14", name: "12-14岁（已核准追诉且符合限定罪名、结果）", direction: "reduce", minPct: -60, maxPct: -30, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "age", applicableCrimeIds: ["intentional_injury"], applicableFactTiers: ["特别残忍手段致重伤严重残疾/致死"], note: "仅限刑法第十七条第三款规定并经最高人民检察院核准追诉的情形。" },
    { id: "minor_14_16", name: "14-16岁（已核对刑法第17条限定罪名）", direction: "reduce", minPct: -60, maxPct: -30, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "age", applicableCrimeIds: ["intentional_injury", "robbery"], applicableFactTiers: ["致一人重伤", "特别残忍手段致重伤严重残疾/致死", "抢劫一次（基本情形）", "法定加重情形"], note: "故意伤害仅限致人重伤或者死亡；勾选即表示已核对刑事责任年龄门槛。" },
    { id: "minor_16_18", name: "16-18岁", direction: "reduce", minPct: -50, maxPct: -10, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "age" },
    { id: "elderly_intent", name: "已满75岁故意犯罪", direction: "reduce", minPct: -40, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "elderly_culpability", applicableCrimeIds: INTENTIONAL_CRIMES },
    { id: "elderly_negligent", name: "已满75岁过失犯罪", direction: "reduce", minPct: -50, maxPct: -20, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "elderly_culpability", applicableCrimeIds: ["traffic_accident"] },
    { id: "deaf_mute", name: "又聋又哑的人", direction: "reduce", minPct: -50, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated_or_exempt", conflictGroup: "sensory_disability" },
    { id: "blind", name: "盲人", direction: "reduce", minPct: -50, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated_or_exempt", conflictGroup: "sensory_disability" },
    { id: "mental_disorder", name: "尚未完全丧失辨认或控制能力", direction: "reduce", minPct: -40, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated" },
    { id: "attempted", name: "未遂犯", direction: "reduce", minPct: -50, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "crime_completion", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES },
    { id: "accessory", name: "从犯", direction: "reduce", minPct: -50, maxPct: -20, quantitative: true, legalEffect: "lighter_or_mitigated_or_exempt", conflictGroup: "participation_role", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES },
    { id: "coerced", name: "胁从犯（依法减轻或免除，幅度人工判断）", direction: "reduce", quantitative: false, legalEffect: "mitigated_or_exempt", conflictGroup: "participation_role", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES, note: "全国指导意见未给出可直接套用的统一百分比。" },
    { id: "solicitor_role", name: "教唆犯（按共同犯罪作用评价）", direction: "reduce", quantitative: false, legalEffect: "qualitative", conflictGroup: "solicitor", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES, note: "教唆犯不是当然从轻情节。" },
    { id: "solicitor_minor", name: "教唆未成年人犯罪（依法从重）", direction: "increase", quantitative: false, legalEffect: "heavier", conflictGroup: "solicitor", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES },
    { id: "solicitor_unsuccessful", name: "被教唆人未实施被教唆罪（可从轻或减轻）", direction: "reduce", quantitative: false, legalEffect: "lighter_or_mitigated", conflictGroup: "solicitor", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES },
    { id: "suspension_no_harm", name: "中止犯（未造成损害）", direction: "reduce", quantitative: false, legalEffect: "exempt", conflictGroup: "crime_completion", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES },
    { id: "suspension_harm", name: "中止犯（造成损害，依法减轻）", direction: "reduce", quantitative: false, legalEffect: "mitigated_or_exempt", conflictGroup: "crime_completion", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES },
    { id: "preparation", name: "预备犯（从轻、减轻或免除幅度人工判断）", direction: "reduce", quantitative: false, legalEffect: "lighter_or_mitigated_or_exempt", conflictGroup: "crime_completion", applicableCrimeIds: INTENTIONAL_ACCOMPLICE_CRIMES },
    { id: "excessive_defense", name: "防卫过当（依法减轻或免除）", direction: "reduce", quantitative: false, legalEffect: "mitigated_or_exempt", conflictGroup: "excess", applicableCrimeIds: ["intentional_injury"] },
    { id: "excessive_escape", name: "避险过当（依法减轻或免除）", direction: "reduce", quantitative: false, legalEffect: "mitigated_or_exempt", conflictGroup: "excess", applicableCrimeIds: ["intentional_injury"] },
  ],

  generalFactors: [
    { id: "surrender", name: "自首", direction: "reduce", minPct: -40, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated_or_exempt", conflictGroup: "confession" },
    { id: "confess", name: "坦白", direction: "reduce", minPct: -20, maxPct: 0, quantitative: true, legalEffect: "lighter", conflictGroup: "confession", conflictsWith: ["plea_guilty"] },
    { id: "confess_heavy", name: "坦白（供述未掌握同种较重罪）", direction: "reduce", minPct: -30, maxPct: -10, quantitative: true, legalEffect: "lighter", conflictGroup: "confession" },
    { id: "confess_avoid", name: "坦白（避免特别严重后果）", direction: "reduce", minPct: -50, maxPct: -30, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "confession", conflictsWith: ["plea_guilty"] },
    { id: "court_guilty", name: "当庭自愿认罪", direction: "reduce", minPct: -10, maxPct: 0, quantitative: true, legalEffect: "lighter", conflictsWith: ["surrender", "confess", "confess_heavy", "confess_avoid", "plea_guilty"] },
    { id: "merit", name: "一般立功", direction: "reduce", minPct: -20, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated", conflictGroup: "merit" },
    { id: "major_merit", name: "重大立功", direction: "reduce", minPct: -50, maxPct: -20, quantitative: true, legalEffect: "mitigated_or_exempt", conflictGroup: "merit" },
    { id: "restitution", name: "退赃退赔", direction: "reduce", minPct: -30, maxPct: 0, quantitative: true, legalEffect: "lighter" },
    { id: "compensation_forgiven", name: "积极赔偿并取得谅解", direction: "reduce", minPct: -40, maxPct: 0, quantitative: true, legalEffect: "lighter", conflictGroup: "victim_repair" },
    { id: "forgiven_no_compensation", name: "未赔偿但取得谅解", direction: "reduce", minPct: -20, maxPct: 0, quantitative: true, legalEffect: "lighter", conflictGroup: "victim_repair" },
    { id: "reconciliation", name: "刑事和解", direction: "reduce", minPct: -50, maxPct: 0, quantitative: true, legalEffect: "lighter_or_mitigated_or_exempt", conflictGroup: "victim_repair" },
    { id: "good_detention", name: "羁押期间表现好", direction: "reduce", minPct: -10, maxPct: 0, quantitative: true, legalEffect: "lighter", conflictsWith: ["plea_guilty"] },
    { id: "plea_guilty", name: "认罪认罚", direction: "reduce", minPct: -30, maxPct: 0, quantitative: true, legalEffect: "lighter", conflictsWith: ["confess", "confess_avoid", "court_guilty", "good_detention"], note: "与自首、重大坦白、退赃退赔、赔偿谅解、刑事和解等合并从宽通常不超过60%，重合部分不得重复评价。" },
    { id: "victim_fault", name: "被害人过错", direction: "reduce", minPct: -40, maxPct: 0, quantitative: true, legalEffect: "lighter", applicableCrimeIds: ["intentional_injury", "robbery", "disturbance"] },
    { id: "civil_dispute", name: "民间矛盾引发", direction: "reduce", minPct: -30, maxPct: 0, quantitative: true, legalEffect: "lighter", applicableCrimeIds: ["intentional_injury", "disturbance"] },
    { id: "recidivist", name: "累犯（已核对构成要件及法定例外）", direction: "increase", minPct: 10, maxPct: 40, minimumIncreaseMonths: 3, quantitative: true, legalEffect: "heavier", excludedCrimeIds: ["traffic_accident"], conflictsWith: ["minor_12_14", "minor_14_16", "minor_16_18"], note: "增加基准刑10%-40%，一般不少于3个月；过失犯罪和不满十八周岁的人犯罪不构成一般累犯。" },
    { id: "prior_crime", name: "前科（已排除过失犯罪、未成年犯罪）", direction: "increase", minPct: 0, maxPct: 10, quantitative: true, legalEffect: "heavier" },
    { id: "vulnerable_target", name: "以弱势人员为犯罪对象", direction: "increase", minPct: 0, maxPct: 20, quantitative: true, legalEffect: "heavier" },
    { id: "disaster_crime", name: "重大灾害期间故意犯罪", direction: "increase", minPct: 0, maxPct: 20, quantitative: true, legalEffect: "heavier", applicableCrimeIds: INTENTIONAL_CRIMES },
  ],

  keywords: {
    crime: [
      ["合同诈骗罪", ["合同诈骗"]],
      ["盗窃罪", ["盗窃罪", "涉嫌盗窃"]],
      ["诈骗罪", ["诈骗罪", "电信网络诈骗罪", "涉嫌诈骗"]],
      ["职务侵占罪", ["职务侵占罪", "涉嫌职务侵占"]],
      ["寻衅滋事罪", ["寻衅滋事罪", "涉嫌寻衅滋事"]],
      ["危险驾驶罪", ["危险驾驶罪", "涉嫌危险驾驶"]],
      ["交通肇事罪", ["交通肇事罪", "涉嫌交通肇事"]],
      ["非国家工作人员受贿罪", ["非国家工作人员受贿罪"]],
      ["贪污罪", ["贪污罪", "涉嫌贪污"]],
      ["受贿罪", ["受贿罪", "涉嫌受贿"]],
      ["故意伤害罪", ["故意伤害罪", "涉嫌故意伤害"]],
      ["抢劫罪", ["抢劫罪", "涉嫌抢劫"]],
      ["帮助信息网络犯罪活动罪", ["帮助信息网络犯罪活动罪", "帮信罪"]],
    ],
    region: [
      ["一类地区", ["广州", "深圳", "珠海", "佛山", "中山", "东莞"]],
      ["二类地区", ["韶关", "河源", "梅州", "汕尾", "阳江", "湛江", "茂名", "肇庆", "清远", "潮州", "揭阳", "云浮"]],
    ],
    factor: {
      "自首": ["自首", "主动投案", "自动投案并如实供述"],
      "坦白": ["坦白", "到案后如实供述"],
      "当庭自愿认罪": ["当庭自愿认罪", "当庭认罪"],
      "一般立功": ["一般立功"],
      "重大立功": ["重大立功"],
      "退赃退赔": ["退赃退赔", "退赃", "退赔"],
      "积极赔偿并取得谅解": ["赔偿并取得谅解", "积极赔偿并取得谅解"],
      "未赔偿但取得谅解": ["未赔偿但取得谅解"],
      "刑事和解": ["刑事和解"],
      "认罪认罚": ["认罪认罚"],
      "被害人过错": ["被害人有过错", "被害人挑衅", "被害人先动手"],
      "累犯（已核对构成要件及法定例外）": ["构成累犯", "系累犯"],
      "从犯": ["从犯", "起次要作用", "起辅助作用"],
      "未遂犯": ["犯罪未遂", "系未遂犯"],
    },
    telecom: ["电信网络诈骗", "电信诈骗"],
  },
};

/**
 * 供发布门禁复算规则内容指纹。contentHash 自身置空，避免自引用；其余规则、
 * 法源、关键词和限制均进入稳定的 JSON 载荷。
 */
export function sentencingRulesetHashPayload(): string {
  return JSON.stringify(
    SENTENCING_DATA,
    (key, value) => key === "contentHash" ? "" : value,
  );
}

export const CRIME_NAME_TO_ID: Record<CrimeName, CrimeId> = {
  "盗窃罪": "theft",
  "诈骗罪": "fraud",
  "合同诈骗罪": "contract_fraud",
  "职务侵占罪": "embezzlement",
  "寻衅滋事罪": "disturbance",
  "危险驾驶罪": "dangerous_driving",
  "交通肇事罪": "traffic_accident",
  "非国家工作人员受贿罪": "non_official_bribery",
  "贪污罪": "corruption",
  "受贿罪": "bribery",
  "故意伤害罪": "intentional_injury",
  "抢劫罪": "robbery",
  "帮助信息网络犯罪活动罪": "aiding_cyber_crime",
};

export const LEGAL_REFERENCES: Record<CrimeName, readonly string[]> = {
  "盗窃罪": ["《刑法》第264条", "法发〔2021〕21号盗窃罪部分", "具体数额档位和增加刑罚量须核对案件适用地区的现行实施细则"],
  "诈骗罪": ["《刑法》第266条", "法发〔2021〕21号诈骗罪部分", "电信网络诈骗另核对法发〔2016〕32号及后续规范"],
  "合同诈骗罪": ["《刑法》第224条", "法发〔2021〕21号合同诈骗罪部分", "具体数额档位须人工核对现行司法解释或属地规则"],
  "职务侵占罪": ["《刑法》第271条", "法发〔2021〕21号职务侵占罪部分", "法释〔2026〕6号第8条、第24条及刑事司法解释时间效力规则"],
  "寻衅滋事罪": ["《刑法》第293条", "法发〔2021〕21号寻衅滋事罪部分"],
  "危险驾驶罪": ["《刑法》第133条之一", "法发〔2021〕21号危险驾驶罪部分", "具体类型另核对2023年四部门意见"],
  "交通肇事罪": ["《刑法》第133条", "法发〔2021〕21号交通肇事罪部分"],
  "非国家工作人员受贿罪": ["《刑法》第163条", "法释〔2026〕6号第8条、第24条及刑事司法解释时间效力规则"],
  "贪污罪": ["《刑法》第382条、第383条", "法释〔2016〕9号"],
  "受贿罪": ["《刑法》第385条、第386条、第383条", "法释〔2016〕9号"],
  "故意伤害罪": ["《刑法》第234条", "法发〔2021〕21号故意伤害罪部分"],
  "抢劫罪": ["《刑法》第263条", "法发〔2021〕21号抢劫罪部分"],
  "帮助信息网络犯罪活动罪": ["《刑法》第287条之二", "法释〔2019〕15号第12条"],
};

export function factorAppliesTo(
  factorId: string,
  crimeId: CrimeId,
  factTier: string | null,
): boolean {
  const factor = [...SENTENCING_DATA.priorityFactors, ...SENTENCING_DATA.generalFactors]
    .find((item) => item.id === factorId);
  if (!factor) return false;
  if (factor.applicableCrimeIds && !factor.applicableCrimeIds.includes(crimeId)) return false;
  if (factor.excludedCrimeIds?.includes(crimeId)) return false;
  if (factor.applicableFactTiers && (!factTier || !factor.applicableFactTiers.includes(factTier))) return false;
  return true;
}

export const PRINCIPAL_PENALTY_LABELS: Record<PrincipalPenaltyKind, string> = {
  control: "管制",
  detention: "拘役",
  fixed_term: "有期徒刑",
  life: "无期徒刑",
  death: "死刑",
  fine_only: "单处罚金",
  exemption: "免予刑事处罚",
};
