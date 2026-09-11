import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  CRIME_NAME_TO_ID,
  LEGAL_REFERENCES,
  SENTENCING_DATA,
  sentencingRulesetHashPayload,
} from "./data.ts";
import { SentencingEngine } from "./engine.ts";

const engine = new SentencingEngine();

function calculate(overrides = {}) {
  return engine.calculate({
    crimeName: "盗窃罪",
    amount: 10000,
    areaType: "全国",
    factors: {},
    crimeDate: "2026-06-01",
    factTier: "较大/其他入罪情形",
    manualBasePenaltyRange: [12, 18],
    manualBaseSource: "律师核验底稿第1页",
    ...overrides,
  });
}

test("规则集具有可重复校验的版本、引擎和内容指纹", () => {
  const digest = createHash("sha256").update(sentencingRulesetHashPayload()).digest("hex");
  assert.equal(SENTENCING_DATA.ruleset.contentHash, `sha256:${digest}`);
  assert.match(SENTENCING_DATA.ruleset.version, /^\d{4}\.\d{2}\.\d{2}-\d+$/);
  assert.match(SENTENCING_DATA.ruleset.engineVersion, /^\d+\.\d+\.\d+$/);
  assert.ok(SENTENCING_DATA.ruleset.sources.every((source) => source.url.startsWith("https://")));
});

test("法定刑、量刑起点、人工基准刑和月数结果分别保存", () => {
  const result = calculate();
  assert.equal(result.calculationStatus, "complete");
  assert.equal(result.statutoryPenalty?.article, "《刑法》第264条");
  assert.deepEqual(result.startingPointRange, [1, 12]);
  assert.deepEqual(result.basePenaltyRange, [12, 18]);
  assert.deepEqual(result.rawAdjustedPenaltyRange, [12, 18]);
  assert.deepEqual(result.finalPenaltyRange, [12, 18]);
  assert.equal(result.ruleset.contentHash, SENTENCING_DATA.ruleset.contentHash);
});

test("除危险驾驶外缺少人工核验基准刑即停止数值测算", () => {
  const result = calculate({ manualBasePenaltyRange: null, manualBaseSource: "" });
  assert.equal(result.calculationStatus, "requires_manual_base");
  assert.deepEqual(result.startingPointRange, [1, 12]);
  assert.equal(result.finalPenaltyRange, undefined);
  assert.match(result.blockingIssues.join("；"), /人工核验的基准刑区间/);
});

test("人工基准刑必须为有限区间、在本档法定刑内且有来源", () => {
  assert.match(calculate({ manualBasePenaltyRange: [12, null] }).blockingIssues[0], /有限月数区间/);
  assert.match(calculate({ manualBasePenaltyRange: [0, 12] }).blockingIssues[0], /超出当前法定刑/);
  assert.match(calculate({ manualBaseSource: "x" }).blockingIssues[0], /核验来源/);
});

test("一般情节以同一基数同向相加而非逐项连乘", () => {
  const result = calculate({
    factTier: "巨大/其他严重情节",
    amount: 200000,
    manualBasePenaltyRange: [42, 60],
    factors: { 自首: true, 退赃退赔: true },
  });
  assert.deepEqual(result.rawAdjustedPenaltyRange, [12, 60]);
  assert.deepEqual(result.finalPenaltyRange, [12, 60]);
  assert.equal(result.generalAdjustments?.length, 2);
  assert.match(result.process.find((item) => item.step === "一般情节合并调节")?.detail, /同一基数/);
});

test("认罪认罚与特定从宽情节合并时最宽幅度按60%封顶", () => {
  const result = calculate({
    factTier: "巨大/其他严重情节",
    amount: 200000,
    manualBasePenaltyRange: [100, 100],
    factors: { 自首: true, 退赃退赔: true, 认罪认罚: true },
  });
  assert.deepEqual(result.rawAdjustedPenaltyRange, [40, 100]);
  assert.ok(result.process.some((item) => item.step === "认罪认罚合并上限"));
});

test("仅有从轻情节不得突破本档法定下限，可减轻情节才可下档", () => {
  const lighter = calculate({
    factTier: "巨大/其他严重情节",
    amount: 200000,
    manualBasePenaltyRange: [42, 60],
    factors: { 退赃退赔: true },
  });
  assert.deepEqual(lighter.rawAdjustedPenaltyRange, [29, 60]);
  assert.deepEqual(lighter.finalPenaltyRange, [36, 60]);

  const mitigated = calculate({
    factTier: "巨大/其他严重情节",
    amount: 200000,
    manualBasePenaltyRange: [42, 60],
    factors: { 自首: true },
  });
  assert.deepEqual(mitigated.finalPenaltyRange, [25, 60]);
});

test("累犯增加10%-40%且低端增量一般不少于3个月", () => {
  const low = calculate({
    manualBasePenaltyRange: [12, 12],
    factors: { "累犯（已核对构成要件及法定例外）": true },
  });
  assert.deepEqual(low.rawAdjustedPenaltyRange, [15, 17]);

  const capped = calculate({
    manualBasePenaltyRange: [30, 36],
    factors: { "累犯（已核对构成要件及法定例外）": true },
  });
  assert.deepEqual(capped.rawAdjustedPenaltyRange, [33, 51]);
  assert.deepEqual(capped.finalPenaltyRange, [33, 36]);
});

test("年龄门槛按罪名和事实结果限制，并与累犯互斥", () => {
  const ageRule = SENTENCING_DATA.priorityFactors.find((item) => item.id === "minor_14_16");
  const recidivist = SENTENCING_DATA.generalFactors.find((item) => item.id === "recidivist");
  assert.ok(ageRule && recidivist);

  const inapplicable = calculate({ factors: { [ageRule.name]: true } });
  assert.match(inapplicable.error, /不适用于当前罪名或事实档位/);

  const conflict = calculate({
    crimeName: "抢劫罪",
    amount: 0,
    factTier: "抢劫一次（基本情形）",
    manualBasePenaltyRange: [36, 72],
    factors: { [ageRule.name]: true, [recidivist.name]: true },
  });
  assert.match(conflict.error, /不能同时选择/);
});

test("教唆、胁从等无统一幅度情节停止数值测算", () => {
  const solicitor = SENTENCING_DATA.priorityFactors.find((item) => item.id === "solicitor_role");
  assert.ok(solicitor);
  const result = calculate({ factors: { [solicitor.name]: true } });
  assert.equal(result.calculationStatus, "blocked");
  assert.equal(result.disposition, "manual_review");
  assert.equal(result.finalPenaltyRange, undefined);
  assert.match(result.blockingIssues.join("；"), /不虚构百分比/);
});

test("中止犯未造成损害才明确输出免予刑事处罚，0不表示零月刑期", () => {
  const noHarm = SENTENCING_DATA.priorityFactors.find((item) => item.id === "suspension_no_harm");
  assert.ok(noHarm);
  const result = calculate({
    manualBasePenaltyRange: null,
    manualBaseSource: "",
    factors: { [noHarm.name]: true },
  });
  assert.equal(result.calculationStatus, "complete");
  assert.equal(result.disposition, "exemption");
  assert.deepEqual(result.finalPenaltyRange, [0, 0]);
  assert.deepEqual(result.finalPenaltyKinds, ["exemption"]);
  assert.match(result.finalSentence, /免予刑事处罚/);
});

test("罚种不被月数区间吞并或误写成有期徒刑", () => {
  const theft = calculate({ manualBasePenaltyRange: [1, 6] });
  assert.ok(theft.statutoryPenalty?.options.some((item) => item.kind === "fine_only"));
  assert.ok(theft.finalPenaltyKinds?.includes("detention"));
  assert.ok(!theft.finalPenaltyKinds?.includes("fine_only"));

  const corporate = calculate({
    crimeName: "职务侵占罪",
    amount: 30000,
    factTier: null,
    temporalRuleBasis: "conduct_on_or_after_2026_05_01",
    manualBasePenaltyRange: [1, 12],
  });
  assert.ok(!corporate.statutoryPenalty?.options.some((item) => item.kind === "fine_only"));

  const robbery = calculate({
    crimeName: "抢劫罪",
    amount: 0,
    factTier: "法定加重情形",
    manualBasePenaltyRange: [120, 156],
  });
  assert.deepEqual(robbery.statutoryPenalty?.options.map((item) => item.kind), ["fixed_term", "life", "death"]);
  assert.match(robbery.warnings.join("；"), /无期徒刑、死刑/);
});

test("危险驾驶是唯一无需人工基准刑的直接宣告刑幅度", () => {
  const result = calculate({
    crimeName: "危险驾驶罪",
    amount: 0,
    factTier: null,
    manualBasePenaltyRange: null,
    manualBaseSource: "",
  });
  assert.equal(result.calculationStatus, "complete");
  assert.deepEqual(result.basePenaltyRange, [1, 6]);
  assert.deepEqual(result.finalPenaltyRange, [1, 6]);
  assert.deepEqual(result.finalPenaltyKinds, ["detention"]);
});

test("裁判情景调整与规范计算分离，非零必须写明理由", () => {
  const blocked = calculate({ judgeAdjustment: 10, judgeAdjustmentReason: "" });
  assert.match(blocked.error, /具体调整理由/);

  const scenario = calculate({ judgeAdjustment: 10, judgeAdjustmentReason: "同类案件压力测试" });
  assert.deepEqual(scenario.rawAdjustedPenaltyRange, [13, 20]);
  assert.ok(scenario.process.some((item) => item.step === "裁判情景模拟" && item.detail.includes("压力测试")));
});

test("2026公司类犯罪参照标准必须先人工核验时间效力", () => {
  const common = {
    crimeName: "职务侵占罪",
    amount: 30000,
    factTier: null,
    manualBasePenaltyRange: [1, 12],
  };
  assert.match(calculate({ ...common, temporalRuleBasis: null }).error, /不能仅按犯罪日期自动切换/);
  assert.match(calculate({ ...common, crimeDate: "2026-04-30", temporalRuleBasis: "conduct_on_or_after_2026_05_01" }).error, /时间依据冲突/);
  assert.match(calculate({ ...common, temporalRuleBasis: "individual_comparison_required" }).error, /有利性比较/);
  assert.equal(calculate({ ...common, temporalRuleBasis: "conduct_on_or_after_2026_05_01" }).calculationStatus, "complete");
  assert.equal(calculate({ ...common, crimeDate: "2026-04-30", temporalRuleBasis: "pre_effective_current_rule_reviewed" }).calculationStatus, "complete");
});

test("普通财产犯罪和电信诈骗金额不自动映射未核实现行状态的地区档位", () => {
  const ordinary = calculate({ factTier: null, manualBasePenaltyRange: null, manualBaseSource: "" });
  assert.match(ordinary.error, /人工确认法定刑档位/);
  const telecom = calculate({
    crimeName: "诈骗罪",
    amount: 500000,
    factTier: null,
    isTelecom: true,
    manualBasePenaltyRange: null,
    manualBaseSource: "",
  });
  assert.match(telecom.error, /人工确认法定刑档位/);
});

test("旧版增加刑罚量字段已从规则和引擎移除", () => {
  for (const standards of Object.values(SENTENCING_DATA.standards)) {
    for (const standard of standards) {
      assert.ok(!("extraPerAmount" in standard));
      assert.ok(!("extraMin" in standard));
      assert.ok(!("extraMax" in standard));
    }
  }
  const engineSource = readFileSync(new URL("./engine.ts", import.meta.url), "utf8");
  assert.doesNotMatch(engineSource, /extraPenaltyRange|extraPerAmount/);
});

test("自然语言仅识别明确情节，不把泛化表述升级为法定量刑情节", () => {
  assert.deepEqual(engine.extractFactors("曾经故意犯罪，认罪态度好，已经赔偿"), {});
  assert.deepEqual(
    engine.extractFactors("主动投案并如实供述，重大立功，认罪认罚，退赃退赔"),
    { 自首: true, 重大立功: true, 退赃退赔: true, 认罪认罚: true },
  );
  assert.equal(engine.extractCrime("挪用公司资金"), null);
});

test("金额和日期解析支持常见格式并拒绝无效日期", () => {
  assert.equal(engine.extractAmount("涉案15.5万元"), 155000);
  assert.equal(engine.extractAmount("涉案1.2亿元"), 120000000);
  assert.equal(engine.extractAmount("涉案150,000元"), 150000);
  assert.equal(engine.extractDate("2026年5月1日"), "2026-05-01");
  assert.equal(engine.extractDate("2026-02-31"), null);
});

test("缺失字段按全国规则安全边界生成", () => {
  assert.deepEqual(
    engine.getMissingFields({ crime: "盗窃罪", region: null, amount: null, date: null, factTier: null }),
    ["涉案金额", "犯罪时间", "案件事实档位"],
  );
  assert.deepEqual(
    engine.getMissingFields({ crime: "职务侵占罪", region: null, amount: 30000, date: null, factTier: null }),
    ["犯罪时间", "适用规则时间依据"],
  );
  assert.deepEqual(
    engine.getMissingFields({ crime: "危险驾驶罪", region: null, amount: null, date: null, factTier: null }),
    ["犯罪时间"],
  );
});

test("所有展示罪名都有法定刑、法源和有效数据边界", () => {
  for (const crime of SENTENCING_DATA.crimes) {
    const id = CRIME_NAME_TO_ID[crime.name];
    assert.ok(id, `${crime.name}缺少ID映射`);
    assert.ok(LEGAL_REFERENCES[crime.name]?.length, `${crime.name}缺少法律依据`);
    for (const standard of SENTENCING_DATA.standards[id]) {
      assert.ok(standard.statutory.article);
      assert.ok(standard.statutory.options.length > 0);
      assert.ok(standard.statutory.monthBounds[0] >= 1);
      if (standard.maxAmount != null) assert.ok((standard.minAmount ?? 0) < standard.maxAmount);
      if (standard.startMin != null) {
        assert.ok(standard.startMin >= 1);
        assert.ok(standard.startMax == null || standard.startMin <= standard.startMax);
      }
      assert.ok(standard.sourceIds.every((sourceId) => SENTENCING_DATA.ruleset.sources.some((source) => source.id === sourceId)));
    }
  }
});

test("未知罪名、无效输入和不存在档位均明确阻断", () => {
  assert.match(calculate({ crimeName: "不存在罪名" }).error, /未知罪名/);
  assert.match(calculate({ amount: -1 }).error, /有效数字/);
  assert.match(calculate({ crimeDate: "2026-02-31" }).error, /有效的 YYYY-MM-DD/);
  assert.match(calculate({ factTier: "虚构档位" }).error, /不在当前罪名可选范围/);
  assert.match(calculate({ judgeAdjustment: 21 }).error, /-20%至20%/);
});
