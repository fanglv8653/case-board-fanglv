import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const uiSource = readFileSync(new URL("./SentencingCalculator.tsx", import.meta.url), "utf8");
const panelSource = readFileSync(
  new URL("../../litigation/components/criminal/CriminalCasePanel.tsx", import.meta.url),
  "utf8",
);

test("UI 明示不自动计算、不自动保存并只追加独立记录", () => {
  assert.match(uiSource, /页面不会自动计算或保存/);
  assert.match(uiSource, /saveCriminalSentencingEstimate/);
  assert.match(uiSource, /追加独立测算记录/);
  assert.match(uiSource, /只会追加一条独立测算记录/);
  assert.doesNotMatch(uiSource, /upsertCriminalCaseProfile/);
  assert.match(uiSource, /calculationStatus !== "complete"/);
  assert.match(uiSource, /setResult\(next\)/);
});

test("UI 分开展示法定刑、量刑起点、人工基准刑和月数调节结果", () => {
  assert.match(uiSource, /label="法定刑"/);
  assert.match(uiSource, /label="量刑起点"/);
  assert.match(uiSource, /label="人工核验基准刑"/);
  assert.match(uiSource, /label="月数调节结果"/);
  assert.match(uiSource, /result\.statutoryPenalty/);
  assert.match(uiSource, /result\.startingPointRange/);
  assert.match(uiSource, /result\.basePenaltyRange/);
  assert.match(uiSource, /result\.finalPenaltyRange/);
});

test("缺少可靠自动规则时要求人工基准刑和来源", () => {
  assert.match(uiSource, /不再依据现行状态未核实的旧地方细则自动增加刑罚量/);
  assert.match(uiSource, /人工核验基准刑必须同时填写上下限/);
  assert.match(uiSource, /实施细则条款\/量刑建议\/阅卷底稿定位/);
  assert.match(uiSource, /数值测算已停止/);
  assert.match(uiSource, /result\.blockingIssues/);
});

test("地区、日期、事实档位与电诈不作危险推断", () => {
  assert.match(uiSource, /羁押等程序日期不能替代犯罪日期/);
  assert.match(uiSource, /请选择适用地区，不能从法院名称自动推断/);
  assert.match(uiSource, /当前金额标准未自动配置，请人工确认案件事实档位/);
  assert.match(uiSource, /电信网络诈骗（仍须人工确认数额档位）/);
  assert.match(uiSource, /isTelecom \? item\.subType === "电信诈骗" : item\.subType !== "电信诈骗"/);
});

test("时间效力和裁判情景模拟均需显式人工依据", () => {
  assert.match(uiSource, /法释〔2026〕6号时间效力核验/);
  assert.match(uiSource, /尚需个案新旧规则有利性比较/);
  assert.match(uiSource, /裁判情景模拟（非规范性自动计算）/);
  assert.match(uiSource, /具体理由（非零时必填）/);
  assert.match(uiSource, /judgeAdjustmentReason/);
});

test("情节按罪名和事实档位过滤且定性情节明确标识", () => {
  assert.match(uiSource, /factorAppliesTo\(factor\.id, crime\.id, factTier \|\| null\)/);
  assert.match(uiSource, /setFactors\(\{\}\)/);
  assert.match(uiSource, /（定性复核）/);
});

test("保存快照包含规则版本、指纹、时间依据和基准刑来源", () => {
  assert.match(uiSource, /basis_snapshot:[\s\S]*?ruleset: result\.ruleset/);
  assert.match(uiSource, /temporalRuleBasis: result\.temporalRuleBasis/);
  assert.match(uiSource, /manualBaseSource: result\.manualBaseSource/);
  assert.match(uiSource, /result\.ruleset\.contentHash/);
  assert.match(uiSource, /result\.ruleset\.sources/);
});

test("案件内可查看历史测算并识别旧版未版本化记录", () => {
  assert.match(uiSource, /listCriminalSentencingEstimates/);
  assert.match(uiSource, /data-testid="sentencing-history"/);
  assert.match(uiSource, /历史测算记录/);
  assert.match(uiSource, /旧版历史记录：未保存规则集版本与指纹/);
  assert.match(uiSource, /await loadHistory\(\)/);
});

test("无画像 revision 只允许测算并提示先保存画像", () => {
  assert.match(uiSource, /expectedProfileRevision == null/);
  assert.match(uiSource, /需先返回案件保存刑事画像/);
  assert.match(panelSource, /useState<number \| null>\(null\)/);
  assert.match(panelSource, /setProfileRevision\(profile\?\.profile_revision \?\? null\)/);
});

test("revision 冲突要求重新加载复核且禁止静默重试覆盖", () => {
  assert.match(uiSource, /重新加载并复核/);
  assert.match(uiSource, /不会自动重试或覆盖/);
  assert.match(uiSource, /SENTENCING_ESTIMATE_REVISION_CONFLICT/);
});

test("刑事面板入口只构建受限预填对象", () => {
  assert.match(panelSource, /buildSentencingPrefill/);
  assert.match(panelSource, /suspectedCharge: profileForm\.suspected_charge/);
  assert.match(panelSource, /profileRevision/);
  assert.doesNotMatch(panelSource, /restitution_amount[^\n]*buildSentencingPrefill/);
});
