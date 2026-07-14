import assert from "node:assert/strict";
import test from "node:test";

import { CRIME_NAME_TO_ID, LEGAL_REFERENCES, SENTENCING_DATA } from "./data.ts";
import { SentencingEngine } from "./engine.ts";

const engine = new SentencingEngine();

function calculate(
  crimeName,
  amount,
  areaType,
  factors = {},
  crimeDate = "2026-01-01",
  judgeAdjustment = 0,
  isTelecom = false,
  factTier = null,
) {
  return engine.calculate({
    crimeName,
    amount,
    areaType,
    factors,
    crimeDate,
    judgeAdjustment,
    isTelecom,
    factTier,
  });
}

test("金额解析支持小数万元、亿元和千分位", () => {
  assert.equal(engine.extractAmount("涉案15.5万元"), 155000);
  assert.equal(engine.extractAmount("涉案1.2亿元"), 120000000);
  assert.equal(engine.extractAmount("涉案150,000元"), 150000);
  assert.equal(engine.extractAmount("涉案15万5000元"), 155000);
});

test("低于最低数额起点时返回错误，不再落入最高档", () => {
  const result = calculate("盗窃罪", 1000, "一类地区");
  assert.match(result.error, /低于.*最低数额起点/);
  assert.equal(result.tier, undefined);
});

test("量刑起点、增加刑罚量和情节幅度全程按区间传播", () => {
  const result = calculate(
    "盗窃罪",
    150000,
    "一类地区",
    { 自首: true, 退赃退赔: true },
  );
  assert.deepEqual(result.startingPointRange, [36, 48]);
  assert.deepEqual(result.extraPenaltyRange, [6, 12]);
  assert.deepEqual(result.basePenaltyRange, [42, 60]);
  assert.deepEqual(result.finalPenaltyRange, [17, 60]);
  assert.match(result.finalSentence, /～/);
  assert.ok(result.process.every((item) => !item.detail.includes("取中值")));
});

test("一般情节逐项按比例作用于前一结果而非一次合并百分比", () => {
  const result = calculate(
    "盗窃罪",
    150000,
    "一类地区",
    { 自首: true, 退赃退赔: true },
  );
  assert.deepEqual(result.basePenaltyRange, [42, 60]);
  assert.deepEqual(result.finalPenaltyRange, [17, 60]);
  assert.equal(result.generalAdjustments?.length, 2);
});

test("电信网络诈骗使用全国专用数额起点和基准数额", () => {
  const result = calculate("诈骗罪", 30000, "二类地区", {}, "2026-01-01", 0, true);
  assert.equal(result.tier, "巨大");
  assert.deepEqual(result.startingPointRange, [36, 120]);
  assert.deepEqual(result.extraPenaltyRange, [0, 0]);

  const below = calculate("诈骗罪", 2999, "一类地区", {}, "2026-01-01", 0, true);
  assert.match(below.error, /最低数额起点3000元/);
});

test("事实型罪名按明确档位计算，缺失档位时拒绝猜测", () => {
  const escaped = calculate("交通肇事罪", 0, "全国", {}, "2026-01-01", 0, false, "逃逸");
  assert.deepEqual(escaped.startingPointRange, [36, 60]);

  const missing = calculate("交通肇事罪", 0, "全国");
  assert.match(missing.error, /请补充案件事实档位/);
});

test("开放上限使用可读文本，不拼接重复刑种", () => {
  const result = calculate("抢劫罪", 0, "全国", {}, "2026-01-01", 0, false, "加重");
  assert.deepEqual(result.finalPenaltyRange, [120, null]);
  assert.equal(result.finalSentence, "10年以上有期徒刑");
});

test("危险驾驶区间受六个月上限约束", () => {
  const result = calculate("危险驾驶罪", 0, "全国");
  assert.deepEqual(result.finalPenaltyRange, [1, 6]);
});

test("重叠关键词不会重复叠加同类情节", () => {
  assert.deepEqual(
    engine.extractFactors("自首并如实供述，重大立功，认罪认罚"),
    { 自首: true, 重大立功: true, 认罪认罚: true },
  );
});

test("日期解析支持中文和短横线格式并拒绝无效日期", () => {
  assert.equal(engine.extractDate("2026年5月1日"), "2026-05-01");
  assert.equal(engine.extractDate("2026-04-10"), "2026-04-10");
  assert.equal(engine.extractDate("2026-02-31"), null);
});

test("缺失字段按罪名数据要求生成", () => {
  assert.deepEqual(
    engine.getMissingFields({ crime: "盗窃罪", region: null, amount: null, date: null, factTier: null }),
    ["地区", "涉案金额"],
  );
  assert.deepEqual(
    engine.getMissingFields({ crime: "职务侵占罪", region: null, amount: 30000, date: null, factTier: null }),
    ["犯罪时间"],
  );
  assert.deepEqual(
    engine.getMissingFields({ crime: "故意伤害罪", region: null, amount: null, date: null, factTier: null }),
    ["案件事实档位"],
  );
});

test("负数或非数字金额被拒绝", () => {
  assert.match(calculate("盗窃罪", -1, "一类地区").error, /有效数字/);
  assert.match(calculate("盗窃罪", Number.NaN, "一类地区").error, /有效数字/);
});

test("所有展示罪名均存在ID映射和标准数据", () => {
  for (const crime of SENTENCING_DATA.crimes) {
    const id = CRIME_NAME_TO_ID[crime.name];
    assert.ok(id, `${crime.name}缺少ID映射`);
    assert.ok(SENTENCING_DATA.standards[id]?.length, `${crime.name}缺少量刑标准`);
    assert.ok(LEGAL_REFERENCES[crime.name]?.length, `${crime.name}缺少法律依据`);
  }
  const calculated = calculate("危险驾驶罪", 0, "全国");
  assert.deepEqual(calculated.legalReferences, LEGAL_REFERENCES["危险驾驶罪"]);
});

test("每条标准的数据边界有效且最低值可命中自身档位", () => {
  const crimeNameById = new Map(SENTENCING_DATA.crimes.map((crime) => [crime.id, crime.name]));
  for (const [crimeId, standards] of Object.entries(SENTENCING_DATA.standards)) {
    const crimeName = crimeNameById.get(crimeId);
    assert.ok(crimeName, `${crimeId}缺少展示罪名`);
    for (const standard of standards) {
      if (standard.maxAmount != null) {
        assert.ok(standard.minAmount < standard.maxAmount, `${crimeName}/${standard.tier}数额边界倒置`);
      }
      if (standard.startMax != null) {
        assert.ok(standard.startMin <= standard.startMax, `${crimeName}/${standard.tier}刑期边界倒置`);
      }
      const date = standard.effFrom || "2026-01-01";
      const amount = standard.minAmount ?? 0;
      const isTelecom = standard.subType === "电信诈骗";
      const factTier = standard.minAmount == null && standard.maxAmount == null ? standard.tier : null;
      const result = calculate(
        crimeName,
        amount,
        standard.area,
        {},
        date,
        0,
        isTelecom,
        factTier,
      );
      assert.equal(result.error, undefined, `${crimeName}/${standard.tier}: ${result.error}`);
      assert.equal(result.tier, standard.tier, `${crimeName}/${standard.tier}最低边界未命中自身档位`);
    }
  }
});

test("未知罪名、地区无标准和无效日期均明确拒绝", () => {
  assert.match(calculate("不存在罪名", 0, "全国").error, /未知罪名/);
  assert.match(calculate("盗窃罪", 3000, "全国").error, /未找到全国/);
  assert.match(calculate("职务侵占罪", 30000, "全国", {}, "").error, /有效的 YYYY-MM-DD/);
});

test("有效日期未命中任何生效标准时明确拒绝", () => {
  const original = SENTENCING_DATA.standards.embezzlement;
  SENTENCING_DATA.standards.embezzlement = [
    {
      area: "全国",
      tier: "未来档",
      minAmount: 30000,
      maxAmount: null,
      startMin: 0,
      startMax: 12,
      effFrom: "2030-01-01",
    },
  ];
  try {
    assert.match(
      calculate("职务侵占罪", 30000, "全国", {}, "2026-01-01").error,
      /没有可用的有效量刑标准/,
    );
  } finally {
    SENTENCING_DATA.standards.embezzlement = original;
  }
});

test("金额档位不连续时拒绝猜测", () => {
  const original = SENTENCING_DATA.standards.theft;
  SENTENCING_DATA.standards.theft = [
    { area: "一类地区", tier: "第一档", minAmount: 3000, maxAmount: 10000, startMin: 0, startMax: 12 },
    { area: "一类地区", tier: "第二档", minAmount: 20000, maxAmount: null, startMin: 36, startMax: 48 },
  ];
  try {
    assert.match(calculate("盗窃罪", 15000, "一类地区").error, /未匹配到连续有效/);
  } finally {
    SENTENCING_DATA.standards.theft = original;
  }
});

test("人工微调限制在正负20%并继续保留区间", () => {
  const plus = calculate("危险驾驶罪", 0, "全国", {}, "2026-01-01", 99);
  assert.deepEqual(plus.finalPenaltyRange, [1, 6]);
  assert.match(plus.process.find((item) => item.step === "审判员微调")?.detail, /20%/);
});
