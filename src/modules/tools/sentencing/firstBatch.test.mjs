import assert from "node:assert/strict";
import test from "node:test";
import { FIRST_BATCH_FIELDS, FIRST_BATCH_IDS, FIRST_BATCH_NAMES, FIRST_BATCH_STANDARDS, assessOffense, visibleFields } from "./firstBatch.ts";
import { SENTENCING_DATA, factorAppliesTo } from "./data.ts";
import { sentencingEngine } from "./engine.ts";
import { buildSentencingPrefill } from "./prefill.ts";

const defaults = {
  concealment: { upstreamEstablished: true, knowledgeEstablished: true, upstreamClass: "ordinary", upstreamClassSource: "上游适用条款及责任明细第1页" },
  extortion: { region: "gd1" },
  illegal_detention: { region: "gd", hours: 24 },
  affray: { liableRole: true },
  obstructing_duty: { lawfulDuty: true, dutyType: "official", violenceThreat: true },
  casino: { casinoType: "ordinary", region: "gd", organizedPlayers: 3, stakes: 50000, countSource: "审计去重及人数反证核验第2页" },
  illegal_deposits: { fourElements: true, amountSource: "吸收全额及损失审计第3页" },
  fundraising_fraud: { fraudPurpose: true, amountSource: "案发前归还及实际骗取审计第3页" },
  disturbance: { behavior: "beating", socialOrder: true, origin: "unprovoked", slightInjuries: 1 },
  forced_transaction: { transactionType: "goods", violenceThreat: true, times: 1, persons: 1 },
};
const amounts = { concealment: 0, extortion: 4000, illegal_detention: 0, affray: 0, obstructing_duty: 0, casino: 0, illegal_deposits: 1000000, fundraising_fraud: 100000, disturbance: 0, forced_transaction: 10000 };
function facts(id, overrides = {}) {
  // 仅虚构测试夹具显式置否/0。真实表单始终待核，不共用这些默认值。
  return { ...Object.fromEntries(FIRST_BATCH_FIELDS[id].filter(f => ["boolean", "number"].includes(f.type)).map(f => [f.key, f.type === "boolean" ? false : 0])), ...defaults[id], ...overrides };
}
function assessment(id, amount = amounts[id], overrides = {}) { return assessOffense(id, amount, facts(id, overrides)); }
function calc(id, overrides = {}, review = {}, factOverrides = {}) {
  const a = assessment(id, overrides.amount ?? amounts[id], factOverrides);
  const minimum = { 普通: 12, 严重: id === "casino" ? 60 : id === "forced_transaction" ? 12 : 36, 巨大: id === "fundraising_fraud" ? 84 : 36, 特别巨大: 120, 重伤: 36, 死亡: 120, 加重: 36, 较大: 36, 一次: 12, 纠集他人三次且每次构罪: 60, 特别严重: 36 }[a.tier] ?? 12;
  return sentencingEngine.calculate({
    crimeName: FIRST_BATCH_NAMES[id], amount: amounts[id], crimeDate: "2026-10-29", areaType: "全国", factors: {},
    manualBasePenaltyRange: [minimum, minimum], manualBaseSource: "人工核定实施细则兼容条款及底稿第4页",
    firstBatchReview: { facts: facts(id, factOverrides), age: 30, subject: "individual", elementsConfirmed: true, evidenceSource: "证据材料与全部构成核实底稿第1页", mitigationDecision: "lighter", penaltyKind: "fixed_term", ...review },
    ...overrides,
  });
}
const factor = id => [...SENTENCING_DATA.priorityFactors, ...SENTENCING_DATA.generalFactors].find(f => f.id === id).name;
const selected = (...ids) => Object.fromEntries(ids.map(id => [factor(id), true]));

// 金额边界均用元直接比较，包含相同门槛上下0.01元及与附加情节的交叉。
const thresholds = [
  ["C01", "concealment", 499999.99, { repeated: true }, "普通"],
  ["C02", "concealment", 500000, {}, "普通"],
  ["C03", "concealment", 500000, { repeated: true }, "严重"],
  ["C04", "concealment", 490000, { loss: 250000 }, "普通"],
  ["C05", "concealment", 500000, { loss: 249999.99 }, "普通"],
  ["C06", "concealment", 500000, { loss: 250000 }, "严重"],
  ["C07", "concealment", 4999999.99, { upstreamClass: "higher", repeated: true }, "普通"],
  ["C08", "concealment", 5000000, { upstreamClass: "higher", repeated: true }, "严重"],
  ["C09", "concealment", 5000000, { upstreamClass: "higher", loss: 2499999.99 }, "普通"],
  ["C10", "concealment", 5000000, { upstreamClass: "higher", loss: 2500000 }, "严重"],
  ["E01", "extortion", 3999.99, {}, null],
  ["E02", "extortion", 4000, {}, "普通"],
  ["E03", "extortion", 100000, {}, "巨大"],
  ["E04", "extortion", 500000, {}, "特别巨大"],
  ["E05", "extortion", 2000, { special12: true }, "普通"],
  ["E06", "extortion", 1999.99, { special12: true }, null],
  ["E07", "extortion", 80000, { special12: true }, "普通"],
  ["E08", "extortion", 80000, { special37: true }, "巨大"],
  ["E09", "extortion", 79999.99, { special37: true }, "普通"],
  ["E10", "extortion", 400000, { special37: true }, "特别巨大"],
  ["E11", "extortion", 2499.99, { region: "gd2" }, null],
  ["E12", "extortion", 2500, { region: "gd2" }, "普通"],
  ["E13", "extortion", 60000, { region: "gd2" }, "巨大"],
  ["E14", "extortion", 400000, { region: "gd2" }, "特别巨大"],
  ["E15", "extortion", 1250, { region: "gd2", special37: true }, "普通"],
  ["E16", "extortion", 48000, { region: "gd2", special37: true }, "巨大"],
  ["E17", "extortion", 320000, { region: "gd2", special37: true }, "特别巨大"],
  ["E18", "extortion", 0, { timesTwoYears: 3 }, "普通"],
  ["E19", "extortion", 0, { timesTwoYears: 2 }, null],
  ["D01", "illegal_detention", 0, { hours: 23.99 }, null],
  ["D02", "illegal_detention", 0, { hours: 24 }, "普通"],
  ["D03", "illegal_detention", 0, { hours: 1, badMeans: true }, "普通"],
  ["D04", "illegal_detention", 0, { hours: 1, slightInjury: true }, "普通"],
  ["D05", "illegal_detention", 0, { hours: 1, organizedGang: true, times: 3, overFourTimes: 0, totalHours: 12 }, null],
  ["D06", "illegal_detention", 0, { hours: 1, organizedGang: true, times: 3, overFourTimes: 1, totalHours: 13 }, "普通"],
  ["D07", "illegal_detention", 0, { hours: 1, organizedGang: true, times: 3, overFourTimes: 3, totalHours: 12.1 }, "普通"],
  ["D08", "illegal_detention", 0, { seriousInjury: true }, "重伤"],
  ["D09", "illegal_detention", 0, { death: true }, "死亡"],
  ["A01", "affray", 0, { publicPlace: true }, "普通"],
  ["A02", "affray", 0, { publicPlace: true, seriousDisorder: true }, "加重"],
  ["A03", "affray", 0, { armed: true }, "加重"],
  ["A04", "affray", 0, { repeated: true }, "加重"],
  ["A05", "affray", 0, { largeBad: true }, "加重"],
  ["A06", "affray", 0, { liableRole: false }, null],
  ["O01", "obstructing_duty", 0, { violenceThreat: false }, null],
  ["O02", "obstructing_duty", 0, { dutyType: "security", violenceThreat: false, seriousConsequence: true }, "普通"],
  ["O03", "obstructing_duty", 0, { lawfulDuty: false }, null],
  ["K01", "casino", 0, { stakes: 49999.99 }, null],
  ["K02", "casino", 0, { stakes: 50000 }, "普通"],
  ["K03", "casino", 0, { stakes: 299999.99 }, "普通"],
  ["K04", "casino", 0, { stakes: 300000 }, "严重"],
  ["K05", "casino", 0, { stakes: 0, skim: 30000 }, "严重"],
  ["K06", "casino", 0, { stakes: 0, players: 120 }, "严重"],
  ["K07", "casino", 0, { stakes: 50000, organizedPlayers: 2 }, null],
  ["K08", "casino", 0, { casinoType: "network", networkConduct: true, stakes: 300000 }, "严重"],
  ["K09", "casino", 0, { casinoType: "network", networkConduct: true, stakes: 0, subAgents: true }, "严重"],
  ["K10", "casino", 0, { casinoType: "network", networkConduct: true, stakes: 0, recruitMinor: true }, "严重"],
  ["K11", "casino", 0, { casinoType: "network_help", helpType: "tech", fee: 19999.99, stakes: 0 }, null],
  ["K12", "casino", 0, { casinoType: "network_help", helpType: "tech", fee: 20000, stakes: 0 }, "普通"],
  ["K13", "casino", 0, { casinoType: "network_help", helpType: "tech", fee: 100000, stakes: 0 }, "严重"],
  ["K14", "casino", 0, { casinoType: "network_help", helpType: "payment", fee: 10000, stakes: 0 }, "普通"],
  ["K15", "casino", 0, { casinoType: "network_help", helpType: "payment", stakes: 1000000 }, "严重"],
  ["K16", "casino", 0, { casinoType: "network_help", helpType: "advert", websites: 10, stakes: 0 }, "普通"],
  ["K17", "casino", 0, { casinoType: "network_help", helpType: "advert", ads: 500, stakes: 0 }, "严重"],
  ["K18", "casino", 0, { casinoType: "machine", stakes: 0, machines: 9 }, null],
  ["K19", "casino", 0, { casinoType: "machine", stakes: 0, machines: 10 }, "普通"],
  ["K20", "casino", 0, { casinoType: "machine", stakes: 0, machines: 60 }, "严重"],
  ["K21", "casino", 0, { casinoType: "machine", stakes: 0, machines: 2, minorUse: true }, "普通"],
  ["K22", "casino", 0, { casinoType: "machine", stakes: 0, machines: 12, minorUse: true }, "严重"],
  ["K23", "casino", 0, { casinoType: "machine", stakes: 0, machines: 5, priorRelevant: true }, "普通"],
  ["K24", "casino", 0, { casinoType: "machine", stakes: 0, machines: 30, priorRelevant: true }, "严重"],
  ["N01", "illegal_deposits", 999999.99, {}, null],
  ["N02", "illegal_deposits", 1000000, {}, "普通"],
  ["N03", "illegal_deposits", 500000, { criminalPursuit: true }, "普通"],
  ["N04", "illegal_deposits", 500000, { administrativeTwoYears: true }, "普通"],
  ["N05", "illegal_deposits", 0, { loss: 250000, badImpact: true }, "普通"],
  ["N06", "illegal_deposits", 0, { persons: 150 }, "普通"],
  ["N07", "illegal_deposits", 0, { loss: 500000 }, "普通"],
  ["N08", "illegal_deposits", 2500000, { criminalPursuit: true }, "普通"],
  ["N09", "illegal_deposits", 2500000, { badImpact: true }, "巨大"],
  ["N10", "illegal_deposits", 5000000, {}, "巨大"],
  ["N11", "illegal_deposits", 0, { persons: 500 }, "巨大"],
  ["N12", "illegal_deposits", 0, { loss: 2500000 }, "巨大"],
  ["N13", "illegal_deposits", 1000000, { loss: 1500000, badImpact: true }, "巨大"],
  ["N14", "illegal_deposits", 50000000, {}, "特别巨大"],
  ["N15", "illegal_deposits", 25000000, { badImpact: true }, "特别巨大"],
  ["N16", "illegal_deposits", 0, { persons: 5000 }, "特别巨大"],
  ["N17", "illegal_deposits", 0, { loss: 25000000 }, "特别巨大"],
  ["N18", "illegal_deposits", 1000000, { loss: 15000000, badImpact: true }, "特别巨大"],
  ["F01", "fundraising_fraud", 99999.99, {}, null],
  ["F02", "fundraising_fraud", 100000, {}, "较大"],
  ["F03", "fundraising_fraud", 1000000, {}, "巨大"],
  ["F04", "fundraising_fraud", 1200000, { preCaseReturned: 300000 }, "较大"],
  ["F05", "fundraising_fraud", 500000, { badImpact: true }, "巨大"],
  ["F06", "fundraising_fraud", 499999.99, { badImpact: true }, "较大"],
  ["S01", "disturbance", 0, { slightInjuries: 0, minorInjuries: 2 }, "一次"],
  ["S02", "disturbance", 0, { slightInjuries: 0, minorInjuries: 1 }, null],
  ["S03", "disturbance", 999.99, { behavior: "taking" }, null],
  ["S04", "disturbance", 1000, { behavior: "taking" }, "一次"],
  ["S05", "disturbance", 1000, { behavior: "damage" }, null],
  ["S06", "disturbance", 2000, { behavior: "damage" }, "一次"],
  ["S07", "disturbance", 0, { behavior: "public" }, null],
  ["S08", "disturbance", 0, { behavior: "public", seriousDisorder: true }, "一次"],
  ["S09", "disturbance", 0, { gathered: true, gatheredCrimes: 2, severeOrder: true }, "一次"],
  ["S10", "disturbance", 0, { gathered: true, gatheredCrimes: 3, severeOrder: true }, "纠集他人三次且每次构罪"],
  ["S11", "disturbance", 0, { gathered: false, gatheredCrimes: 3, severeOrder: true }, "一次"],
  ["S12", "disturbance", 0, { behavior: "harassment", armed: true }, "一次"],
  ["T01", "forced_transaction", 9999.99, {}, null],
  ["T02", "forced_transaction", 10000, {}, "严重"],
  ["T03", "forced_transaction", 5000, { fakeGoods: true }, "严重"],
  ["T04", "forced_transaction", 0, { profit: 2000 }, "严重"],
  ["T05", "forced_transaction", 0, { fakeGoods: true, profit: 1000 }, "严重"],
  ["T06", "forced_transaction", 0, { times: 3 }, "严重"],
  ["T07", "forced_transaction", 0, { persons: 3 }, "严重"],
  ["T08", "forced_transaction", 0, { loss: 2000 }, "严重"],
  ["T09", "forced_transaction", 0, { minorInjury: true }, "严重"],
  ["T10", "forced_transaction", 10000, { transactionType: "bid" }, null],
  ["T11", "forced_transaction", 100000, {}, "严重"],
  ["T12", "forced_transaction", 0, { transactionType: "assets", badMeans: true }, "严重"],
  ["T13", "forced_transaction", 100000, { transactionType: "service", fakeGoods: true, profit: 0 }, "严重"],
];
for (const [code, id, amount, fs, tier] of thresholds) test(`首批数额/AND/OR边界 ${code} ${FIRST_BATCH_NAMES[id]}`, () => {
  const a = assessment(id, amount, fs);
  assert.equal(a.tier, tier, a.reasons.join("；"));
  if (tier) assert.equal(a.status, "candidate");
});

for (const id of FIRST_BATCH_IDS) {
  test(`首批 ${id} 全部条件已核、独立基准可完成情景测算`, () => {
    const r = calc(id);
    assert.equal(r.calculationStatus, "complete", r.blockingIssues.join("；"));
    assert.equal(r.finalPenaltyKinds[0], "fixed_term");
    assert.ok(r.ruleset.sources.some(s => s.id === "batch-L18"));
  });
  test(`首批 ${id} 未确认构成或无基准不能生成最终刑期`, () => {
    assert.equal(calc(id, {}, { elementsConfirmed: false }).finalPenaltyRange, undefined);
    const missing = calc(id, { manualBasePenaltyRange: null });
    assert.equal(missing.calculationStatus, "requires_manual_base");
    assert.equal(missing.finalPenaltyRange, undefined);
  });
}
const stopCases = [
  ["U01掩隐上游未知不默认普通", "concealment", { upstreamClass: null }],
  ["U02掩隐损失未知不当零", "concealment", { loss: null }],
  ["U03敲诈近亲属谅解", "extortion", { nearRelative: true, forgiven: true }],
  ["U04敲诈未分赃非主犯不额外必需", "extortion", { repentant: true, returned: true, noShare: true, notPrincipal: null }],
  ["U05敲诈获赃少但主犯无其他资格", "extortion", { repentant: true, returned: true, smallShare: true, notPrincipal: false }],
  ["U06非拘暴力转化", "illegal_detention", { conversion: true }],
  ["U07聚斗转化个人归责", "affray", { conversion: true }],
  ["U08妨公袭警分流", "obstructing_duty", { assaultPolice: true }],
  ["U09赌场类型未知", "casino", { casinoType: null }],
  ["U10机器一般劳务", "casino", { casinoType: "machine", ordinaryLabor: true }],
  ["U11非吸清退免罚候选", "illegal_deposits", { normalOperationCleared: true }],
  ["U12集诈非法占有目的未知", "fundraising_fraud", { fraudPurpose: null }],
  ["U13寻衅起因被害人例外", "disturbance", { origin: "incident", victimProvoked: true }],
  ["U14婚恋纠纷例外未核", "disturbance", { origin: "private" }],
  ["U15强迫交易抢劫敲诈分流", "forced_transaction", { illegalPossession: true }],
];
for (const [name, id, fs] of stopCases) test(name, () => {
  const a = assessment(id, name.startsWith("U02") ? 500000 : amounts[id], fs);
  if (name.startsWith("U05")) assert.equal(a.tier, "普通"); // 获赃少本身不足第五条轻微分支。
  else assert.notEqual(a.status, "candidate");
});

test("强迫非商品服务的多次由法律判断确认，不把两次直接当全国阈值", () => {
  assert.equal(assessment("forced_transaction", 0, { transactionType: "bid", times: 2 }).tier, null);
  assert.equal(assessment("forced_transaction", 0, { transactionType: "bid", repeated: true }).tier, "严重");
});
test("特别严重须独立事实及依据，不能河南倍率替代", () => {
  assert.equal(assessment("forced_transaction", 100000, { otherSevere: true }).status, "pending");
  assert.equal(assessment("forced_transaction", 100000, { otherSevere: true, severeSource: "个案特别严重事实及适用条款" }).tier, "特别严重");
});
test("广东以外普通赌场不得继承广东30万升档", () => assert.equal(assessment("casino", 0, { region: "other", stakes: 300000 }).tier, null));
test("未知、NaN、负值及非整数人数不得默认为0", () => {
  for (const value of [null, NaN, -1, 1.5]) assert.notEqual(assessment("illegal_deposits", 0, { persons: value }).status, "candidate");
});
test("实际返还不得超过骗取额或按全部成本扣减", () => assert.equal(assessment("fundraising_fraud", 100000, { preCaseReturned: 100001 }).status, "pending"));
test("非吸前受追究不单凭前史升巨大；已清退不是全额扣项", () => {
  assert.equal(assessment("illegal_deposits", 2500000, { criminalPursuit: true }).tier, "普通");
  assert.equal(assessment("illegal_deposits", 5000000, { returnedAmount: 4000000 }).tier, "巨大");
});
test("首批拒绝手动档位覆盖已核高档", () => assert.equal(calc("illegal_deposits", { amount: 5000000, factTier: "普通" }).calculationStatus, "blocked"));
test("首批16岁刑责、未成年从宽及身份核验", () => {
  assert.equal(calc("extortion", {}, { age: 15 }).finalPenaltyRange, undefined);
  assert.match(calc("extortion", {}, { age: 17 }).blockingIssues.join(""), /应当从轻/);
  assert.equal(calc("extortion", { factors: selected("minor_16_18") }, { age: 17, factorPercents: { minor_16_18: -25 }, adjustmentSource: "未成年资格核验，选择本档从轻" }).calculationStatus, "complete");
  assert.equal(calc("extortion", { factors: selected("minor_16_18") }, { age: 30 }).calculationStatus, "blocked");
});
test("单位罚金独立，责任人员分别测算；无单位依据不能推全员", () => {
  assert.match(calc("concealment", {}, { subject: "unit" }).blockingIssues.join(""), /单位仅列罚金/);
  assert.equal(calc("concealment", {}, { subject: "unit_responsible" }).calculationStatus, "complete");
  assert.equal(calc("affray", {}, { subject: "unit" }).calculationStatus, "blocked");
});
test("律师确认继续引用2024指导（二），显示参考起点但不替代人工基准", () => {
  const r = calc("casino");
  assert.equal(r.calculationStatus, "complete");
  assert.deepEqual(r.startingPointRange, [1, 24]);
  assert.equal(r.ruleset.sources.find(s => s.id === "batch-L03").status, "manually_confirmed_applicability");
  assert.match(r.process.find(p => p.step === "2024指导（二）人工适用确认").detail, /不代表取得全国续期公文/);
  assert.equal(calc("casino", { manualBasePenaltyRange: null }).calculationStatus, "requires_manual_base");
  const adopted = calc("casino", {}, { guidelineAdoptionSource: "办案地现行采纳条款及核验依据" });
  assert.deepEqual(adopted.startingPointRange, [1, 24]);
  assert.match(adopted.process.find(p => p.step === "2024指导（二）人工适用确认").detail, /办案地现行采纳/);
});

test("赌场严重起点按律师确认参考5至6年，旧行为仍须版本比较", () => {
  assert.deepEqual(calc("casino", {}, {}, { stakes: 300000 }).startingPointRange, [60, 72]);
  const old = calc("casino", { crimeDate: "2024-06-30" });
  assert.equal(old.finalPenaltyRange, undefined);
  assert.match(old.blockingIssues.join(""), /历史/);
});
test("历史案不得仅用现行解释，须人工整体比较", () => {
  const old = { crimeDate: "2025-08-25" };
  assert.match(calc("concealment", old).blockingIssues.join(""), /历史/);
  assert.equal(calc("concealment", old, { currentRulesConfirmed: true, temporalSource: "历史解释与现行解释整体有利性比较" }).calculationStatus, "complete");
});
test("罚金、剥政、无期、免罚不表达为0月或180月", () => {
  for (const [id, kind, extra] of [["obstructing_duty", "fine_only", {}], ["illegal_detention", "political_rights", {}], ["fundraising_fraud", "life", { amount: 1000000 }]]) {
    const r = calc(id, extra, { penaltyKind: kind });
    assert.equal(r.finalPenaltyRange, undefined); assert.equal(r.calculationStatus, "requires_manual_review");
  }
  assert.equal(calc("disturbance").additionalPenaltyReview[0].includes("无一般罚金"), true);
  assert.equal(calc("affray", {}, { penaltyKind: "fine_only" }).finalPenaltyRange, undefined);
});
test("自首从轻与减轻分开，不能只凭勾选下档", () => {
  const input = { amount: 100000, factors: selected("surrender"), manualBasePenaltyRange: [40, 40] };
  const common = { factorPercents: { surrender: -20 }, adjustmentSource: "自首资格和比例已核定，分别审查法律效果" };
  assert.deepEqual(calc("extortion", input, { ...common, mitigationDecision: "lighter" }).finalPenaltyRange, [36, 36]);
  assert.deepEqual(calc("extortion", input, { ...common, mitigationDecision: "mitigated" }).finalPenaltyRange, [32, 32]);
  assert.equal(calc("extortion", { amount: 100000 }, { mitigationDecision: "mitigated" }).calculationStatus, "blocked");
});
test("多特殊情节未核顺序不连乘，确认后仍须个别比例", () => {
  const input = { factors: selected("attempted", "accessory"), manualBasePenaltyRange: [40, 40], amount: 100000 };
  const r = calc("extortion", input, { factorPercents: { attempted: -20, accessory: -25 }, adjustmentSource: "未遂从犯资格及比例去重核实" });
  assert.match(r.blockingIssues.join(""), /多个特殊/);
  const yes = calc("extortion", input, { factorPercents: { attempted: -20, accessory: -25 }, adjustmentSource: "未遂从犯资格及比例去重核实", specialOrderSource: "经核当地特殊情节顺序和方法", specialAdjustmentMethod: "sequential", specialOrder: ["accessory", "attempted"], mitigationDecision: "mitigated" });
  assert.deepEqual(yes.rawAdjustedPenaltyRange, [24, 24]);
});
test("小数25.92不默认向下取25，须人工不足月依据", () => {
  const input = { factors: selected("restitution"), manualBasePenaltyRange: [36, 36] };
  const review = { factorPercents: { restitution: -28 }, adjustmentSource: "退赔事实、独立性和28%理由已核" };
  assert.deepEqual(calc("extortion", input, review).rawAdjustedPenaltyRange, [25.92, 25.92]);
  assert.equal(calc("extortion", input, review).finalPenaltyRange, undefined);
  assert.deepEqual(calc("extortion", input, { ...review, roundedRange: [26, 26], roundingSource: "当前办案地不足月处理核验条款" }).finalPenaltyRange, [26, 26]);
  assert.equal(calc("extortion", input, { ...review, roundedRange: [24, 24], roundingSource: "不应任意改写核定结果" }).calculationStatus, "blocked");
});
test("认罪认罚综合组吸收同一供述和退赔只计一次", () => {
  const r = calc("extortion", { factors: selected("surrender", "confess", "plea_guilty", "restitution"), manualBasePenaltyRange: [100, 100], amount: 100000 },
    { integratedReductionPct: 60, adjustmentSource: "2026认罚新版资格核实，供述退赔综合组去重", mitigationDecision: "lighter" });
  assert.deepEqual(r.rawAdjustedPenaltyRange, [40, 40]);
  assert.equal(r.process.filter(p => p.step === "认罪认罚综合评价").length, 1);
});
test("较轻例外可超过60但须明确依据，不自动放行或免罚", () => {
  const input = { factors: selected("surrender", "plea_guilty"), manualBasePenaltyRange: [100, 100], amount: 100000 };
  const review = { integratedReductionPct: 70, adjustmentSource: "较轻例外、法定从宽资格及依据已核", mitigationDecision: "mitigated" };
  assert.equal(calc("extortion", input, review).finalPenaltyRange, undefined);
  assert.deepEqual(calc("extortion", input, { ...review, lighterCaseException: true }).finalPenaltyRange, [30, 30]);
  assert.equal(calc("extortion", input, { ...review, lighterCaseException: true, integratedReductionPct: 100 }).finalPenaltyRange, undefined);
});
test("非吸专属退赔与通用不叠70，公诉阶段未知不得通过", () => {
  const input = { factors: selected("deposit_return", "restitution") };
  assert.equal(calc("illegal_deposits", input, {}).calculationStatus, "blocked");
  assert.equal(calc("illegal_deposits", input, { adjustmentSource: "核定退赔资格与阶段" }, { preProsecutionReturn: true }).calculationStatus, "blocked");
});
test("刑诉和解不能由谅解推定，非第四五章的首批罪名不展示该减幅", () => {
  assert.equal(factorAppliesTo("reconciliation", "disturbance", "一次"), false);
  assert.equal(calc("extortion", { factors: selected("reconciliation") }).calculationStatus, "blocked");
  assert.equal(calc("extortion", { factors: selected("reconciliation") }, { factorPercents: { reconciliation: -25 }, adjustmentSource: "民间纠纷等全部288条要件核定" }, { reconciliationEligible: true }).calculationStatus, "complete");
});
test("累犯前科不重复、弱势已升档不再固定加重", () => {
  assert.equal(calc("extortion", { factors: selected("recidivist", "prior_crime") }).calculationStatus, "blocked");
  assert.equal(calc("extortion", { amount: 80000, factors: selected("vulnerable_target") }, {}, { special37: true }).calculationStatus, "blocked");
});
test("缓刑三年整只作资格审查，累犯及超过三年不通行", () => {
  const base = { manualBasePenaltyRange: [36, 36] };
  assert.match(calc("extortion", base).probationReview, /待人工/);
  assert.match(calc("extortion", base, { probationConditionsConfirmed: true, probationExcluded: false }).probationReview, /可进入/);
  assert.match(calc("disturbance", { manualBasePenaltyRange: [37, 37] }).probationReview, /不符合/);
  assert.match(calc("extortion", base, { probationConditionsConfirmed: true, probationExcluded: true }).probationReview, /不适用/);
});
test("罪名全称中的顿号保留，集诈不先命中普通诈骗", () => {
  const full = FIRST_BATCH_NAMES.concealment;
  assert.equal(buildSentencingPrefill({ caseId: "synthetic", profileRevision: 1, suspectedCharge: full }).crimeName, full);
  assert.deepEqual(buildSentencingPrefill({ caseId: "synthetic", profileRevision: 1, suspectedCharge: full + "、敲诈勒索罪" }).crimeCandidates, [full, "敲诈勒索罪"]);
  assert.equal(sentencingEngine.extractCrime("涉嫌集资诈骗罪"), FIRST_BATCH_NAMES.fundraising_fraud);
  assert.equal(sentencingEngine.extractFactTier("个人三次以上寻衅滋事", "寻衅滋事罪"), null);
});
test("切换分支仅显示本类字段，未知及不合法select不认定", () => {
  assert.ok(!visibleFields("forced_transaction", { transactionType: "bid" }).some(f => f.key === "fakeGoods"));
  assert.equal(assessment("forced_transaction", 5000, { transactionType: "service", fakeGoods: true, profit: 1000 }).tier, null);
  assert.equal(assessment("casino", 0, { casinoType: "arbitrary" }).status, "pending");
});
test("规则数22，寻衅唯一，不写入或修改案件材料，输入及快照不共享可变对象", () => {
  assert.equal(SENTENCING_DATA.crimes.length, 22);
  assert.equal(SENTENCING_DATA.crimes.filter(c => c.id === "disturbance").length, 1);
  const review = { facts: facts("extortion"), age: 30, subject: "individual", elementsConfirmed: true, evidenceSource: "虚构核验材料第1页", mitigationDecision: "lighter", penaltyKind: "fixed_term" };
  const input = { crimeName: "敲诈勒索罪", amount: 4000, areaType: "全国", factors: {}, crimeDate: "2026-10-29", firstBatchReview: review };
  const before = JSON.stringify(input);
  const r = sentencingEngine.calculate(input);
  assert.equal(JSON.stringify(input), before);
  assert.notEqual(r.firstBatchReview.facts, review.facts);
});

test("普通成人罚金1000、未成年500是底线，特别罚金不能被覆盖", () => {
  const fine = { fineSource: "罚金条件及能力和特别条款已独立核实", propertyPenaltyKind: "fine" };
  assert.equal(calc("casino", {}, { ...fine, fineRange: [800, 800] }).calculationStatus, "blocked");
  assert.equal(calc("casino", {}, { ...fine, fineRange: [1000, 1000] }).calculationStatus, "complete");
  const minor = { ...fine, age: 17, factorPercents: { minor_16_18: -25 }, adjustmentSource: "未成年法定从宽及比例核实" };
  assert.equal(calc("casino", { factors: selected("minor_16_18") }, { ...minor, fineRange: [400, 400] }).calculationStatus, "blocked");
  assert.equal(calc("casino", { factors: selected("minor_16_18") }, { ...minor, fineRange: [500, 500] }).calculationStatus, "complete");
  assert.equal(calc("illegal_deposits", {}, { ...fine, fineRange: [1000, 1000] }).calculationStatus, "blocked");
  assert.equal(calc("illegal_deposits", {}, { ...fine, fineRange: [50000, 1000000] }).calculationStatus, "complete");
});
test("无并罚规定的妨公有期、非拘、聚斗和普通寻衅不能额外并罚", () => {
  for (const id of ["obstructing_duty", "illegal_detention", "affray", "disturbance"]) assert.equal(calc(id, {}, { fineRange: [1000, 1000], fineSource: "不应擅自附加罚金", propertyPenaltyKind: "fine" }).calculationStatus, "blocked");
  assert.equal(calc("casino", {}, { propertyPenaltyKind: "none" }).calculationStatus, "blocked");
});
test("敲诈未取得罚金2000至10万、取得时按合法金额范围独立核", () => {
  const fine = { fineSource: "敲诈罚金所据分支及金额独立核实", propertyPenaltyKind: "fine" };
  assert.equal(calc("extortion", {}, { ...fine, fineRange: [2000, 100000] }).calculationStatus, "complete");
  assert.equal(calc("extortion", {}, { ...fine, fineRange: [2000, 100001] }).calculationStatus, "blocked");
  assert.equal(calc("extortion", {}, { ...fine, fineRange: [2000, 8001] }, { obtainedAmount: 4000 }).calculationStatus, "blocked");
});
test("集诈没收财产只在法定备选分支、与罚金分开", () => {
  assert.equal(calc("fundraising_fraud", { amount: 1000000 }, { propertyPenaltyKind: "confiscation", fineSource: "巨大档没收财产依据已独立核实" }).calculationStatus, "complete");
  assert.equal(calc("fundraising_fraud", {}, { propertyPenaltyKind: "confiscation", fineSource: "低档不能选择没收财产" }).calculationStatus, "blocked");
});

test("多特殊情节方法与顺序必须显式，合并与逐项不能混同", () => {
  const input = { factors: selected("attempted", "accessory"), manualBasePenaltyRange: [40, 40], amount: 100000 };
  const review = { factorPercents: { attempted: -20, accessory: -25 }, adjustmentSource: "特殊情节资格及比例核验", specialOrderSource: "经核当地特殊情节具体方法条款", mitigationDecision: "mitigated" };
  assert.equal(calc("extortion", input, { ...review, specialAdjustmentMethod: "sequential" }).finalPenaltyRange, undefined);
  assert.equal(calc("extortion", input, { ...review, specialAdjustmentMethod: "sequential", specialOrder: ["attempted", "attempted"] }).finalPenaltyRange, undefined);
  assert.deepEqual(calc("extortion", input, { ...review, specialAdjustmentMethod: "combined" }).rawAdjustedPenaltyRange, [22, 22]);
  const sequential = calc("extortion", input, { ...review, specialAdjustmentMethod: "sequential", specialOrder: ["accessory", "attempted"] });
  assert.deepEqual(sequential.rawAdjustedPenaltyRange, [24, 24]);
  assert.match(sequential.process.find(p => p.step === "特殊情节人工核定顺序").detail, /^从犯/);
});
test("减轻后的可选刑种来自本罪下一法定档，不通用添加管制拘役", () => {
  assert.deepEqual(FIRST_BATCH_STANDARDS.illegal_deposits[2].statutory.mitigatedOptions.map(o => o.kind), ["fixed_term"]);
  assert.deepEqual(FIRST_BATCH_STANDARDS.fundraising_fraud[1].statutory.mitigatedOptions.map(o => o.kind), ["fixed_term"]);
  assert.ok(!FIRST_BATCH_STANDARDS.forced_transaction[1].statutory.mitigatedOptions.some(o => o.kind === "control"));
  assert.ok(FIRST_BATCH_STANDARDS.extortion[1].statutory.mitigatedOptions.some(o => o.kind === "control"));
});
