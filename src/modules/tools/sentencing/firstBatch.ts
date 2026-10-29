import type { CrimeId, CrimeName, OffenseFacts, SentencingStandard, PrincipalPenaltyKind, StatutoryPenaltyBand } from "./types.ts";

export type Truth = boolean | null;
export interface OffenseField {
  key: string; label: string; type: "boolean" | "number" | "select" | "text";
  options?: readonly [string, string][]; note?: string;
  when?: [string, readonly string[]]; integer?: boolean;
}
const b = (key: string, label: string, note?: string): OffenseField => ({ key, label, type: "boolean", note });
const n = (key: string, label: string, integer = false): OffenseField => ({ key, label, type: "number", integer });
const pick = (key: string, label: string, options: readonly [string, string][]): OffenseField => ({ key, label, type: "select", options });
const text = (key: string, label: string): OffenseField => ({ key, label, type: "text" });
const only = (field: OffenseField, key: string, values: string[]): OffenseField => ({ ...field, when: [key, values] });
const other = [b("otherEntry", "其他入罪情节已依法确认"), text("otherEntrySource", "其他入罪情节的具体事实与条款")];
const serious = b("otherSerious", "其他严重情节已依法确认");
const severe = b("otherSevere", "其他特别严重情节已依法确认");

export const FIRST_BATCH_NAMES = {
  concealment: "掩饰、隐瞒犯罪所得、犯罪所得收益罪", extortion: "敲诈勒索罪",
  illegal_detention: "非法拘禁罪", affray: "聚众斗殴罪", obstructing_duty: "妨害公务罪",
  casino: "开设赌场罪", illegal_deposits: "非法吸收公众存款罪", fundraising_fraud: "集资诈骗罪",
  disturbance: "寻衅滋事罪", forced_transaction: "强迫交易罪",
} as const satisfies Partial<Record<CrimeId, CrimeName>>;
export type FirstBatchId = keyof typeof FIRST_BATCH_NAMES;
export const FIRST_BATCH_IDS = Object.keys(FIRST_BATCH_NAMES) as FirstBatchId[];
export const FIRST_BATCH_NAME_TO_ID = Object.fromEntries(FIRST_BATCH_IDS.map(id => [FIRST_BATCH_NAMES[id], id])) as Record<typeof FIRST_BATCH_NAMES[FirstBatchId], FirstBatchId>;
export const GUIDANCE_2024_CONFIRMATION = {
  confirmedOn: "2026-10-29", confirmedBy: "经办律师",
  decision: "经大量检索未见新的替代性公文或废止依据，各地仍在实行，继续引用2024年量刑指导意见（二）",
  boundary: "属于律师人工适用确认，不代表取得全国续期公文；历史行为与个案异议仍复核。广东未见新版细则，地方增量继续人工核验。",
};
export function isFirstBatch(id: string): id is FirstBatchId { return FIRST_BATCH_IDS.includes(id as FirstBatchId); }

export const FIRST_BATCH_FIELDS: Record<FirstBatchId, OffenseField[]> = {
  concealment: [
    b("upstreamEstablished", "上游犯罪事实及所得性质已确认"),
    b("knowledgeEstablished", "知道或应当知道、掩饰隐瞒行为及作用已确认"),
    pick("upstreamClass", "上游数额标准分类（随现行上游规则核验）", [["ordinary", "其他上游"], ["higher", "相对较高数额标准上游"]]),
    text("upstreamClassSource", "上游分类、责任数额及去重依据"),
    b("repeated", "多次实施"), b("specialProperty", "特定款物情节"),
    b("refusalRecovery", "拒绝配合追缴且造成无法追缴"), n("loss", "依法确认损失（元）"), serious,
    b("lenientConditions", "认罚、配合追缴及第四条轻微等全部资格已核实"),
  ],
  extortion: [
    pick("region", "具体数额地区", [["gd1", "广东一类：广深珠佛中东"], ["gd2", "广东二类：含惠州、江门、汕头等其余15市"], ["other", "其他地区（人工核有效标准）"]]),
    only(n("localLow", "当地较大标准（元）"), "region", ["other"]),
    only(n("localMiddle", "当地巨大标准（元）"), "region", ["other"]),
    only(n("localHigh", "当地特别巨大标准（元）"), "region", ["other"]),
    only(text("localSource", "当地数额文件、条款及现行依据"), "region", ["other"]),
    b("special12", "解释第二条（一）（二）：敲诈刑罚或一年内相关行政处罚"),
    b("special37", "第二条（三）至（七）项特殊资格（已核具体情节）"),
    n("timesTwoYears", "二年内独立敲诈次数", true),
    b("nearRelative", "敲诈近亲属财物"), b("forgiven", "被害人谅解"),
    b("repentant", "认罪、悔罪"), b("returned", "已退赃、退赔"),
    b("legalLeniency", "第五条法定从宽情节"), b("noShare", "没有参与分赃"),
    b("smallShare", "获赃较少"), b("notPrincipal", "不是主犯"), b("minorHarm", "其他轻微、危害不大"),
    n("obtainedAmount", "实际取得额（元；未取得填0）"), ...other,
  ],
  illegal_detention: [
    pick("region", "适用地域", [["gd", "广东"], ["other", "其他地区"]]),
    b("conversion", "使用暴力致伤残、死亡，需转伤害/杀人审查"),
    b("seriousInjury", "拘禁致重伤（因果及非转化已核）"), b("death", "拘禁致死亡（因果及非转化已核）"),
    n("hours", "单次拘禁小时数"), b("badMeans", "械具、捆绑等恶劣手段或殴虐等情节恶劣"),
    b("slightInjury", "造成轻伤"), b("officialAbuse", "国家机关工作人员利用职权且L17路径已核"),
    b("organizedGang", "黑恶势力有组织拘禁资格已核"),
    only(n("times", "拘禁次数", true), "organizedGang", ["true"]),
    only(n("overFourTimes", "每次持续超过4小时的次数", true), "organizedGang", ["true"]),
    only(n("totalHours", "历次累计小时数"), "organizedGang", ["true"]), ...other,
  ],
  affray: [
    b("liableRole", "首要分子或其他积极参加者（个人责任已核）"),
    b("conversion", "致重伤、死亡需按个人故意责任转罪"),
    b("repeated", "多次聚斗已依法认定"), b("largeBad", "人数多、规模大且社会影响恶劣"),
    b("publicPlace", "公共场所或交通要道"), b("seriousDisorder", "造成社会秩序严重混乱"),
    b("armed", "持械斗殴及个人归责已确认"),
  ],
  obstructing_duty: [
    b("lawfulDuty", "法定身份及依法履职已确认"),
    b("assaultPolice", "属于袭警或其他较重罪分流情形"),
    pick("dutyType", "履职路径", [["official", "国家机关工作人员"], ["deputy", "人大代表"], ["redcross", "灾害/突发事件红十字会"], ["security", "国家安全工作"]]),
    b("violenceThreat", "使用暴力、威胁"), only(b("seriousConsequence", "造成严重后果"), "dutyType", ["security"]),
  ],
  casino: [
    pick("casinoType", "赌场类型", [["ordinary", "普通赌场"], ["network", "网络赌场经营"], ["network_help", "网络赌场帮助"], ["machine", "赌博机"]]),
    only(pick("region", "普通赌场地域", [["gd", "广东"], ["other", "其他地区"]]), "casinoType", ["ordinary"]),
    n("skim", "抽头渔利/违法所得（元，按对应分支口径）"), n("stakes", "累计赌资（元，投注及回流去重）"), n("players", "依法核验的参赌人数", true),
    text("countSource", "赌资/人数/账号推定反证及个人责任范围核验"),
    only(n("organizedPlayers", "组织赌博人数", true), "casinoType", ["ordinary"]),
    only(b("networkConduct", "建站/代理投注/分成等经营路径已确认"), "casinoType", ["network"]),
    only(b("subAgents", "招募下级代理且下级接受投注"), "casinoType", ["network"]),
    only(b("recruitMinor", "招揽未成年人网络赌博"), "casinoType", ["network"]),
    only(pick("helpType", "明知帮助类型", [["tech", "技术等服务"], ["payment", "支付结算"], ["advert", "广告"]]), "casinoType", ["network_help"]),
    only(n("fee", "帮助收费（元）"), "casinoType", ["network_help"]),
    only(n("websites", "广告网站数量", true), "casinoType", ["network_help"]),
    only(n("ads", "广告条数", true), "casinoType", ["network_help"]),
    only(n("machines", "独立操作单元数", true), "casinoType", ["machine"]),
    only(b("minorUse", "赌博机供未成年人使用"), "casinoType", ["machine"]),
    only(b("nearSchool", "赌博机位于中小学附近"), "casinoType", ["machine"]),
    only(b("priorRelevant", "二年内相关行政处罚/五年内相关刑罚资格已核"), "casinoType", ["machine"]),
    only(b("ordinaryLabor", "仅一般劳务且不分成、不领高额固定工资等全部资格"), "casinoType", ["machine"]),
    ...other, serious,
  ],
  illegal_deposits: [
    b("fourElements", "非法性、公开性、利诱性、社会性四要件已核"),
    n("persons", "去重吸收对象人数", true), n("loss", "直接经济损失（元）"),
    b("criminalPursuit", "曾因非法集资受过刑事追究（不是仅受刑罚）"),
    b("administrativeTwoYears", "二年内因非法集资受过行政处罚"),
    b("badImpact", "恶劣社会影响或其他严重后果"),
    text("amountSource", "吸收全额、复投去重、损失日及责任额审计依据"),
    b("preProsecutionReturn", "公诉前积极退赃退赔、减少损害"),
    b("normalOperationCleared", "主要用于正常生产经营且公诉前清退全部资格已核"),
  ],
  fundraising_fraud: [
    b("fraudPurpose", "非法占有目的、诈骗方法及个人范围已核"),
    n("preCaseReturned", "案发前已返还数额（元；按2022解释含利息抵本核验）"),
    b("badImpact", "恶劣社会影响或其他严重后果"),
    text("amountSource", "实际骗取、案发日、返还与利息抵本、责任额依据"),
  ],
  disturbance: [
    pick("behavior", "寻衅行为类型", [["beating", "随意殴打"], ["harassment", "追逐拦截辱骂恐吓"], ["taking", "强拿硬要财物"], ["damage", "任意损毁占用财物"], ["public", "公共场所起哄闹事"]]),
    b("socialOrder", "破坏社会秩序及分支性质已核"),
    pick("origin", "起因", [["unprovoked", "无事生非"], ["incident", "日常偶发矛盾借故生非"], ["private", "婚恋家庭邻里债务纠纷"]]),
    only(b("victimProvoked", "被害人故意引发矛盾或对激化负主要责任"), "origin", ["incident"]),
    only(b("continuedAfterWarning", "经有关部门制止处理后继续、破坏社会秩序等例外已核"), "origin", ["private"]),
    only(n("minorInjuries", "轻微伤人数", true), "behavior", ["beating"]),
    only(n("slightInjuries", "轻伤人数", true), "behavior", ["beating"]),
    b("armed", "持凶器"), b("repeatedBad", "多次且对应分支恶劣影响已核"),
    b("vulnerableBad", "针对法定特殊对象且恶劣社会影响"),
    b("mentalOrSuicide", "精神失常、自杀等严重后果及因果已核"),
    b("severeLifeImpact", "严重影响工作生活生产经营"),
    b("seriousDisorder", "公共场所严重混乱，已综合场所等因素判断"), serious,
    b("gathered", "纠集他人"), only(n("gatheredCrimes", "每次构罪且未经处理的纠集次数", true), "gathered", ["true"]),
    only(b("severeOrder", "严重破坏社会秩序"), "gathered", ["true"]),
  ],
  forced_transaction: [
    pick("transactionType", "强迫交易行为", [["goods", "强买强卖商品"], ["service", "强迫提供接受服务（含真实借贷）"], ["bid", "强迫投标拍卖"], ["assets", "强迫转让收购股债资产"], ["business", "强迫参与退出特定经营"]]),
    b("violenceThreat", "暴力、威胁及强迫交易因果已核"), b("illegalPossession", "借交易名义非法占有，需抢劫/敲诈分流"),
    only(b("minorInjury", "致轻微伤"), "transactionType", ["goods", "service"]),
    only(n("loss", "直接损失（元）"), "transactionType", ["goods", "service"]),
    n("times", "强迫次数", true), b("repeated", "非商品服务类多次实施已依法认定"), n("persons", "被强迫人数", true), n("profit", "违法所得（元）"),
    only(b("fakeGoods", "伪劣商品性质已确认"), "transactionType", ["goods"]),
    b("badMeans", "手段恶劣"), b("badImpact", "严重后果或恶劣社会影响"),
    ...other, severe, only(text("severeSource", "特别严重的具体事实及适用依据（不采用河南倍率）"), "otherSevere", ["true"]),
  ],
};

export const FIRST_BATCH_NOTES: Record<FirstBatchId, string[]> = {
  concealment: ["2025解释无统一3000/1万元入罪线，旧10万元升档停用。", "其他上游按数额50万且（指定情节或损失25万）整理；相对较高上游为500万及情节。括号、分类及上下游均衡须律师确认。"],
  extortion: ["一类4000/10万/50万，二类2500/6万/40万；仅第二条（三）至（七）可用80%升档。", "近亲属获谅解一般不认为犯罪；仍认定犯罪须记录理由并应当酌情从宽。"],
  illegal_detention: ["24小时不是全国唯一最低线；广东恶劣手段、轻伤等另有路径。", "黑恶有组织拘禁用“超过4/12小时”，不含本数；索真实赌债不当然绑架。"],
  affray: ["仅首要分子和积极参加者承担本罪责任；公共场所须同时严重混乱才属该加重路径。", "持械、多次和转化分别按个人故意、共谋与因果核验；不将现场全员转罪。"],
  obstructing_duty: ["一般不配合不自动入罪；国家安全任务虽无暴力但严重后果可另审。", "袭警分流；辅警身份与人民警察不同。罚金是可选择的单处刑种，不自动并罚。"],
  casino: ["先分普通、网络、帮助和赌博机路径；网络五倍、机器六倍不得混用。", "广东普通赌场严重为抽头3万/赌资30万/120人等；人数推定保留反证，查实按实际人数校正。", "2024指导（二）按经办律师2026-10-29人工确认继续引用，未视为全国续期公文；广东仍未见新版细则，基准刑与地方增量仍人工核验，不套四川规则。"],
  illegal_deposits: ["吸收全额、损失、清退分开；2014追缴回报抵本不直接扣减犯罪全额。", "前受刑事追究/行政处罚只触发指定降入罪线，不替代中高档的恶劣影响/严重后果。"],
  fundraising_fraud: ["经营亏损不等于非法占有；按各行为人目的及责任范围核验。", "案发前已还可按解释扣，成本佣金不扣；案发后退赔不是犯罪额扣项。高档可能无期，不用15年遮蔽。"],
  disturbance: ["四类行为分别核验；强拿1000与损毁占用2000区分；公共场所不等于严重混乱。", "个人3次治安违法不等于纠集3次、每次构罪、未经处理并严重破坏秩序。"],
  forced_transaction: ["五类行为分别核；商品服务1万元追诉线不通用于投标、股债及经营。", "特别严重保留人工依据，不将追诉门槛乘5/10，不移用河南数字。真实强迫借贷与借贷名义非法占有分流。"],
};

const option = (kind: PrincipalPenaltyKind, min?: number, max?: number) => ({ kind, minimumMonths: min, maximumMonths: max, label: kind });
function band(article: string, label: string, min: number, max: number, kinds: PrincipalPenaltyKind[], lower?: [number, number]): StatutoryPenaltyBand {
  return { article, label, monthBounds: [min, max], options: kinds.map(k => k === "detention" ? option(k, 1, 6) : k === "control" ? option(k, 3, 24) : k === "fixed_term" ? option(k, Math.max(6, min), max) : option(k)),
    mitigatedMonthBounds: lower, mitigatedOptions: lower ? [option("fixed_term", Math.max(6, lower[0]), lower[1]), option("detention", 1, 6), option("control", 3, 24)] : undefined };
}
function standard(tier: string, article: string, label: string, bounds: [number, number], kinds: PrincipalPenaltyKind[], start?: [number, number], lower?: [number, number], source = "sentencing-guidance-2021"): SentencingStandard {
  return { area: "全国", tier, minAmount: null, maxAmount: null, quantitativeStatus: start ? "starting_point_only" : "statutory_only",
    startMin: start?.[0], startMax: start?.[1], startKinds: start ? (start[0] < 6 ? ["detention", "fixed_term"] : ["fixed_term"]) : undefined,
    statutory: band(article, label, ...bounds, kinds, lower), sourceIds: ["criminal-law-current", source] };
}
const lowKinds: PrincipalPenaltyKind[] = ["fixed_term", "detention", "control"];
export const FIRST_BATCH_STANDARDS: Record<FirstBatchId, SentencingStandard[]> = {
  concealment: [
    standard("普通", "《刑法》第312条", "三年以下有期徒刑、拘役或者管制，并处或者单处罚金", [1, 36], [...lowKinds, "fine_only"], [1, 12]),
    standard("严重", "《刑法》第312条", "三年以上七年以下有期徒刑，并处罚金", [36, 84], ["fixed_term"], [36, 48], [1, 36]),
  ],
  extortion: [
    standard("普通", "《刑法》第274条", "三年以下有期徒刑、拘役或者管制，并处或者单处罚金", [1, 36], [...lowKinds, "fine_only"], [1, 12]),
    standard("巨大", "《刑法》第274条", "三年以上十年以下有期徒刑，并处罚金", [36, 120], ["fixed_term"], [36, 60], [1, 36]),
    standard("特别巨大", "《刑法》第274条", "十年以上有期徒刑，并处罚金", [120, 180], ["fixed_term"], [120, 144], [36, 120]),
  ],
  illegal_detention: [
    standard("普通", "《刑法》第238条", "三年以下有期徒刑、拘役、管制或者剥夺政治权利", [1, 36], [...lowKinds, "political_rights"], [1, 12]),
    standard("重伤", "《刑法》第238条", "三年以上十年以下有期徒刑", [36, 120], ["fixed_term"], [36, 60], [1, 36]),
    standard("死亡", "《刑法》第238条", "十年以上有期徒刑", [120, 180], ["fixed_term"], [120, 156], [36, 120]),
  ],
  affray: [
    standard("普通", "《刑法》第292条", "三年以下有期徒刑、拘役或者管制", [1, 36], lowKinds, [1, 24]),
    standard("加重", "《刑法》第292条", "三年以上十年以下有期徒刑", [36, 120], ["fixed_term"], [36, 60], [1, 36]),
  ],
  obstructing_duty: [standard("普通", "《刑法》第277条第一至四款", "三年以下有期徒刑、拘役、管制或者罚金", [1, 36], [...lowKinds, "fine_only"], [1, 24])],
  casino: [
    standard("普通", "《刑法》第303条第二款", "五年以下有期徒刑、拘役或者管制，并处罚金", [1, 60], lowKinds, [1, 24], undefined, "batch-L03"),
    standard("严重", "《刑法》第303条第二款", "五年以上十年以下有期徒刑，并处罚金", [60, 120], ["fixed_term"], [60, 72], [1, 60], "batch-L03"),
  ],
  illegal_deposits: [
    standard("普通", "《刑法》第176条", "三年以下有期徒刑或者拘役，并处或者单处罚金", [1, 36], ["fixed_term", "detention", "fine_only"], [1, 12]),
    standard("巨大", "《刑法》第176条", "三年以上十年以下有期徒刑，并处罚金", [36, 120], ["fixed_term"], [36, 48], [1, 36]),
    standard("特别巨大", "《刑法》第176条", "十年以上有期徒刑，并处罚金", [120, 180], ["fixed_term"], [120, 144], [36, 120]),
  ],
  fundraising_fraud: [
    standard("较大", "《刑法》第192条", "三年以上七年以下有期徒刑，并处罚金", [36, 84], ["fixed_term"], [36, 48], [1, 36]),
    standard("巨大", "《刑法》第192条", "七年以上有期徒刑或者无期徒刑，并处罚金或者没收财产", [84, 180], ["fixed_term", "life"], [84, 108], [36, 84]),
  ],
  disturbance: [
    standard("一次", "《刑法》第293条", "五年以下有期徒刑、拘役或者管制", [1, 60], lowKinds, [1, 36]),
    standard("纠集他人三次且每次构罪", "《刑法》第293条第二款", "五年以上十年以下有期徒刑，可以并处罚金", [60, 120], ["fixed_term"], [60, 84], [1, 60]),
  ],
  forced_transaction: [
    standard("严重", "《刑法》第226、231条", "三年以下有期徒刑或者拘役，并处或者单处罚金", [1, 36], ["fixed_term", "detention", "fine_only"], undefined),
    standard("特别严重", "《刑法》第226、231条", "三年以上七年以下有期徒刑，并处罚金", [36, 84], ["fixed_term"], undefined, [1, 36]),
  ],
};

// 减轻到下一档时，刑种来自该罪真实的下一法定幅度；不把拘役/管制通用地塞进所有高档。
for (const standards of Object.values(FIRST_BATCH_STANDARDS)) {
  for (let i = 1; i < standards.length; i++) {
    standards[i].statutory.mitigatedOptions = standards[i - 1].statutory.options.map(option => ({ ...option }));
  }
}
// 集诈最低档以下的其他刑种适用未单独闭合，月份路径仅列有期/拘役，其余人工审查。
FIRST_BATCH_STANDARDS.fundraising_fraud[0].statutory.mitigatedOptions = [option("fixed_term", 6, 36), option("detention", 1, 6)];

export const FIRST_BATCH_CONTRACT = { version: "2026.10.29-3", names: FIRST_BATCH_NAMES, fields: FIRST_BATCH_FIELDS, standards: FIRST_BATCH_STANDARDS, notes: FIRST_BATCH_NOTES,
  manualConfirmation: GUIDANCE_2024_CONFIRMATION,
  gaps: ["广东现行全省共同细则及增量：仍未取得，人工基准刑", "2024指导（二）：律师已确认继续引用，未取得正式续期公文，不据此自动认定已失效"] };

export function visibleFields(id: FirstBatchId, facts: OffenseFacts): OffenseField[] {
  return FIRST_BATCH_FIELDS[id].filter(f => !f.when || f.when[1].includes(String(facts[f.when[0]])));
}
export const all = (...values: Truth[]): Truth => values.includes(false) ? false : values.includes(null) ? null : true;
export const any = (...values: Truth[]): Truth => values.includes(true) ? true : values.includes(null) ? null : false;
export const truth = (facts: OffenseFacts, key: string): Truth => typeof facts[key] === "boolean" ? facts[key] as boolean : null;
export const num = (facts: OffenseFacts, key: string): number | null => typeof facts[key] === "number" && Number.isFinite(facts[key]) && (facts[key] as number) >= 0 ? facts[key] as number : null;
export const ge = (value: number | null, threshold: number): Truth => value == null ? null : value >= threshold;
export const gt = (value: number | null, threshold: number): Truth => value == null ? null : value > threshold;

export interface OffenseAssessment {
  status: "candidate" | "pending" | "excluded" | "diverted";
  tier: string | null; reasons: string[]; sourceIds: string[];
}
const SOURCES: Record<FirstBatchId, string[]> = {
  concealment: ["batch-L04"], extortion: ["batch-L05", "batch-L08"], illegal_detention: ["batch-L21", "batch-L16", "batch-L17"],
  affray: ["criminal-law-current"], obstructing_duty: ["batch-L10"], casino: ["batch-L21", "batch-L13", "batch-L14", "batch-L03"],
  illegal_deposits: ["batch-L07", "batch-B06"], fundraising_fraud: ["batch-L07"], disturbance: ["batch-L06"], forced_transaction: ["batch-L12", "batch-L25"],
};
/** 条件只生成候选；未知不当作否或0，开放情节由律师给出事实和依据。 */
export function assessOffense(id: FirstBatchId, amount: number, facts: OffenseFacts): OffenseAssessment {
  const reasons: string[] = [];
  const out = (status: OffenseAssessment["status"], tier: string | null, reason: string) => ({ status, tier, reasons: [...reasons, reason], sourceIds: SOURCES[id] });
  const pending = (why: string) => out("pending", null, why);
  const t = (k: string) => truth(facts, k);
  const v = (k: string) => num(facts, k);
  const has = (key: string) => typeof facts[key] === "string" && (facts[key] as string).trim().length >= 4;
  for (const f of visibleFields(id, facts)) {
    const value = facts[f.key];
    if (f.type === "number" && value != null && (typeof value !== "number" || !Number.isFinite(value) || value < 0 || (f.integer && !Number.isInteger(value)))) return pending(`“${f.label}”必须为有效非负${f.integer ? "整数" : "数值"}`);
    if (f.type === "boolean" && value != null && typeof value !== "boolean") return pending(`“${f.label}”须明确是、否或待核`);
    if (f.type === "select" && value != null && value !== "" && !f.options?.some(o => o[0] === value)) return pending(`“${f.label}”选项无效`);
  }
  for (const [flag, source] of [["otherEntry", "otherEntrySource"], ["otherSevere", "severeSource"]] as const) {
    if (t(flag) === true && !has(source)) return pending("开放性情节须填写具体事实及适用条款");
  }
  let entry: Truth = true;
  let middle: Truth = false;
  let high: Truth = false;
  let tiers = ["普通", "严重", "特别严重"];
  switch (id) {
    case "concealment": {
      if (!["ordinary", "higher"].includes(String(facts.upstreamClass)) || !has("upstreamClassSource")) return pending("上游分类、责任数额及去重依据待核");
      entry = all(t("upstreamEstablished"), t("knowledgeEstablished"));
      const higher = facts.upstreamClass === "higher";
      middle = all(amount >= (higher ? 5000000 : 500000), any(t("repeated"), t("specialProperty"), t("refusalRecovery"), ge(v("loss"), higher ? 2500000 : 250000), t("otherSerious")));
      reasons.push("第五条按数额与情节双重限定整理，分类、括号解释、可以认定及上下游均衡仍须律师确认；不设旧统一入罪线");
      if (t("lenientConditions")) reasons.push("第四条轻微、不起诉或免罚资格进入人工审查，不自动无罪");
      break;
    }
    case "extortion": {
      let limits: number[];
      if (facts.region === "gd1") limits = [4000, 100000, 500000];
      else if (facts.region === "gd2") limits = [2500, 60000, 400000];
      else if (facts.region === "other" && has("localSource") && v("localLow") != null && v("localMiddle") != null && v("localHigh") != null) {
        limits = [v("localLow")!, v("localMiddle")!, v("localHigh")!];
        if (!(limits[0] >= 2000 && limits[0] <= 5000 && limits[1] >= 30000 && limits[1] <= 100000 && limits[2] >= 300000 && limits[2] <= 500000)) return pending("当地数额标准须在全国授权区间并核验");
      } else return pending("须选择实际适用数额地区并核验当地依据");
      const special = any(t("special12"), t("special37"));
      entry = any(amount >= limits[0], all(special, amount >= limits[0] * 0.5), ge(v("timesTwoYears"), 3), t("otherEntry"));
      middle = any(amount >= limits[1], all(t("special37"), amount >= limits[1] * 0.8));
      high = any(amount >= limits[2], all(t("special37"), amount >= limits[2] * 0.8));
      tiers = ["普通", "巨大", "特别巨大"];
      if (all(t("nearRelative"), t("forgiven")) === true) return pending("近亲属获谅解一般不认为犯罪；若仍认定犯罪须人工论证并应当酌情从宽");
      const light = all(amount >= limits[0] && amount < limits[1], t("repentant"), t("returned"), any(t("legalLeniency"), t("noShare"), all(t("smallShare"), t("notPrincipal")), t("forgiven"), t("minorHarm")));
      if (light === true) return pending("第五条轻微、不起诉或免罚候选：未分赃不额外要求非主犯；由有权机关判断");
      break;
    }
    case "illegal_detention": {
      if (t("conversion") === true) return out("diverted", null, "暴力伤残死亡转故意伤害/杀人，按个人责任另审");
      if (t("conversion") == null) return pending("先核暴力转化及因果，未知不能默认普通拘禁");
      const gang = all(t("organizedGang"), ge(v("times"), 3), any(ge(v("overFourTimes"), 3), gt(v("totalHours"), 12)));
      entry = facts.region === "gd" ? any(ge(v("hours"), 24), t("badMeans"), t("slightInjury"), gang, t("officialAbuse"), t("otherEntry"), t("seriousInjury"), t("death")) : t("otherEntry");
      middle = t("seriousInjury"); high = t("death"); tiers = ["普通", "重伤", "死亡"];
      if (facts.region !== "gd" && facts.region !== "other") return pending("拘禁实际地域待核");
      if (v("overFourTimes") != null && v("times") != null && v("overFourTimes")! > v("times")!) return pending("超过4小时的次数不能大于全部次数");
      reasons.push("不足24小时不自动出罪；其他地区可按其他入罪路径及明确依据人工确认");
      break;
    }
    case "affray":
      if (t("conversion") === true) return out("diverted", null, "重伤死亡按各人的故意、共谋及因果转伤害/杀人审查");
      if (t("conversion") == null) return pending("聚斗转罪及个人责任待核");
      entry = t("liableRole");
      middle = any(t("repeated"), t("largeBad"), all(t("publicPlace"), t("seriousDisorder")), t("armed"));
      tiers = ["普通", "加重", ""]; break;
    case "obstructing_duty":
      if (t("assaultPolice") === true) return out("diverted", null, "转袭警或较重罪核验，不套妨公三年上限");
      if (t("assaultPolice") == null) return pending("袭警及较重罪分流尚未核实");
      if (!["official", "deputy", "redcross", "security"].includes(String(facts.dutyType))) return pending("法定履职路径待核");
      entry = all(t("lawfulDuty"), facts.dutyType === "security" ? any(t("violenceThreat"), t("seriousConsequence")) : t("violenceThreat")); break;
    case "casino": {
      if (!has("countSource")) return pending("赌资、人数推定及反证、去重和责任范围须有依据");
      const a = v("skim"), stakes = v("stakes"), players = v("players");
      const moneyEntry = any(ge(a, 5000), ge(stakes, 50000), ge(players, 20));
      const moneySerious = any(ge(a, 30000), ge(stakes, 300000), ge(players, 120));
      if (facts.casinoType === "ordinary") {
        if (facts.region === "gd") { entry = any(all(ge(v("organizedPlayers"), 3), moneyEntry), t("otherEntry")); middle = any(moneySerious, t("otherSerious")); }
        else if (facts.region === "other") { entry = t("otherEntry"); middle = t("otherSerious"); if (middle === true && !has("otherEntrySource")) return pending("非广东普通赌场须核地域严重标准"); }
        else return pending("普通赌场地域待核");
      } else if (facts.casinoType === "network") {
        entry = t("networkConduct"); middle = any(moneySerious, t("subAgents"), t("recruitMinor"), t("otherSerious"));
      } else if (facts.casinoType === "network_help") {
        const fee = v("fee");
        if (facts.helpType === "tech") { entry = ge(fee, 20000); middle = ge(fee, 100000); }
        else if (facts.helpType === "payment") { entry = any(ge(fee, 10000), ge(stakes, 200000)); middle = any(ge(fee, 50000), ge(stakes, 1000000)); }
        else if (facts.helpType === "advert") { entry = any(ge(v("websites"), 10), ge(v("ads"), 100)); middle = any(ge(v("websites"), 50), ge(v("ads"), 500)); }
        else return pending("帮助类型待核");
        entry = any(entry, t("otherEntry")); middle = any(middle, t("otherSerious"));
        reasons.push("帮助须明知及实际帮助，反证由律师核验；五倍仅限本帮助条款");
      } else if (facts.casinoType === "machine") {
        if (t("ordinaryLabor") === true) return pending("赌博机一般劳务不追刑资格进入人工审查，不扩大到所有赌场雇员");
        const machines = v("machines"), special = any(t("minorUse"), t("nearSchool"));
        entry = any(ge(machines, 10), all(special, ge(machines, 2)), moneyEntry, all(t("priorRelevant"), ge(machines, 5)), t("otherEntry"));
        middle = any(ge(machines, 60), all(special, ge(machines, 12)), moneySerious, all(t("priorRelevant"), ge(machines, 30)), t("otherSerious"));
        reasons.push("前六项六倍，相关前史30台；独立操作单元和前处罚资格须核");
      } else return pending("赌场类型待核");
      break;
    }
    case "illegal_deposits": {
      if (!has("amountSource")) return pending("吸收全额/损失日/复投及责任额审计依据待核");
      const persons = v("persons"), loss = v("loss"), bad = t("badImpact");
      entry = all(t("fourElements"), any(amount >= 1000000, ge(persons, 150), ge(loss, 500000), all(any(amount >= 500000, ge(loss, 250000)), any(t("criminalPursuit"), t("administrativeTwoYears"), bad))));
      middle = any(amount >= 5000000, ge(persons, 500), ge(loss, 2500000), all(any(amount >= 2500000, ge(loss, 1500000)), bad));
      high = any(amount >= 50000000, ge(persons, 5000), ge(loss, 25000000), all(any(amount >= 25000000, ge(loss, 15000000)), bad));
      tiers = ["普通", "巨大", "特别巨大"];
      if (t("normalOperationCleared") === true) return pending("正常经营、公诉前清退等免罚/显轻分支须人工判断，不自动普通测算");
      break;
    }
    case "fundraising_fraud": {
      if (!has("amountSource") || v("preCaseReturned") == null) return pending("实际骗取、案发前返还及利息抵本口径待核");
      if (v("preCaseReturned")! > amount) return pending("案发前返还超过对应实际骗取，须核明细，不截零");
      const actual = Math.round((amount - v("preCaseReturned")!) * 100) / 100;
      reasons.push(`实际骗取${amount}－案发前依法返还${v("preCaseReturned")}＝${actual}元；成本佣金不扣、案发后退赔另评`);
      entry = all(t("fraudPurpose"), actual >= 100000);
      middle = any(actual >= 1000000, all(actual >= 500000, t("badImpact"))); tiers = ["较大", "巨大", ""]; break;
    }
    case "disturbance": {
      if (facts.origin === "incident") {
        if (t("victimProvoked") === true) return out("excluded", null, "排除该借故生非路径；被害人故意引发/激化主责，其他罪及正当化另审");
        if (t("victimProvoked") == null) return pending("偶发矛盾的被害人例外待核");
      } else if (facts.origin === "private") {
        if (t("continuedAfterWarning") !== true) return pending("婚恋家庭邻里债务纠纷一般不认定；处理后继续等例外待核");
      } else if (facts.origin !== "unprovoked") return pending("寻衅起因待核");
      const broad = any(t("repeatedBad"), t("vulnerableBad"), t("mentalOrSuicide"), t("otherSerious"));
      let behavior: Truth;
      if (facts.behavior === "beating") behavior = any(ge(v("slightInjuries"), 1), ge(v("minorInjuries"), 2), t("armed"), broad, t("seriousDisorder"));
      else if (facts.behavior === "harassment") behavior = any(t("armed"), broad, t("severeLifeImpact"));
      else if (facts.behavior === "taking" || facts.behavior === "damage") behavior = any(amount >= (facts.behavior === "taking" ? 1000 : 2000), broad, t("severeLifeImpact"));
      else if (facts.behavior === "public") behavior = t("seriousDisorder");
      else return pending("寻衅四类行为分支待选");
      entry = all(t("socialOrder"), behavior);
      middle = all(t("gathered"), ge(v("gatheredCrimes"), 3), t("severeOrder")); tiers = ["一次", "纠集他人三次且每次构罪", ""]; break;
    }
    case "forced_transaction": {
      if (t("illegalPossession") === true) return out("diverted", null, "交易/借贷名义非法占有，转抢劫或敲诈等审查");
      if (t("illegalPossession") == null) return pending("非法占有及相邻罪分流待核");
      let path: Truth;
      if (["goods", "service"].includes(String(facts.transactionType))) path = any(t("minorInjury"), ge(v("loss"), 2000), ge(v("times"), 3), ge(v("persons"), 3), amount >= 10000, ge(v("profit"), 2000), all(facts.transactionType === "goods", t("fakeGoods"), any(amount >= 5000, ge(v("profit"), 1000))), t("otherEntry"));
      else if (["bid", "assets", "business"].includes(String(facts.transactionType))) path = any(t("repeated"), t("badMeans"), t("badImpact"), t("otherEntry"));
      else return pending("强迫交易五类行为待选");
      entry = all(t("violenceThreat"), path); middle = t("otherSevere"); tiers = ["严重", "特别严重", ""];
      reasons.push("本分支只提示追诉条件，不能替代全部构成证据；特别严重必须独立论证，无统一倍率"); break;
    }
  }
  if (entry !== true) return out(entry === false ? "excluded" : "pending", null, entry === false ? "未命中已核条件；不等于全案无罪，开放情节/相邻罪另审" : "入罪条件仍有待核事实；未知不当作否或0");
  if (high === true) return out("candidate", tiers[2], "命中最高档候选，仍需完整要件、个体责任及律师确认");
  if (high == null) return pending("特别严重/最高档条件仍有未知，不能默认降档");
  if (middle === true) return out("candidate", tiers[1], "命中加重档候选，仍需完整要件及律师确认");
  if (middle == null) return pending("严重/加重档条件仍有未知，不能默认普通档");
  return out("candidate", tiers[0], "命中普通档候选；未自动认定有罪或确定刑期");
}
