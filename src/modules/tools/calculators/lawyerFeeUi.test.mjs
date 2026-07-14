import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(
  new URL("./LawyerFeeCalculator.tsx", import.meta.url),
  "utf8",
);

assert.match(source, /PROVINCIAL_REGIONS\.map/, "地区选择器必须由34地区引擎数据驱动");
assert.match(source, /getRegionFeeRegime/, "当前制度状态必须调用已验收引擎");
assert.match(source, /calculateZhejiangHistoricalReference/, "浙江历史参考必须调用引擎");
assert.match(source, /historicalReferenceConfirmed:\s*true/, "历史计算只应出现在主动确认分支");
assert.match(source, /非现行政府指导价，不构成报价/, "浙江历史结果必须显示醒目标识");
assert.match(source, /calculateRiskAgencyCap/, "风险代理必须走全国强校验");
assert.match(source, /各环节服务费合计最高限额/, "风险上限必须明确为全流程合计");
assert.match(source, /createPracticeQuoteProfile/, "内部参考必须使用带来源标签的引擎模型");
assert.match(source, /律所 \/ 内部参考标准/, "内部标准不得显示为官方价格");
assert.match(source, /openUrl\(source\.url\)/, "官方来源必须由系统浏览器打开");
assert.doesNotMatch(source, /from "\.\.\/lib\/lawyerFee"/, "不得继续调用旧经验公式");
assert.doesNotMatch(source, /calculateFixed|calculateRiskBreakdown|DIFFICULTY/, "旧默认报价逻辑不得残留");

console.log("lawyer fee UI contract assertions passed");
