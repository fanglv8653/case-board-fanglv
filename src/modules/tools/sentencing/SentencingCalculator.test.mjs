import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const uiSource = readFileSync(new URL("./SentencingCalculator.tsx", import.meta.url), "utf8");
const panelSource = readFileSync(
  new URL("../../litigation/components/criminal/CriminalCasePanel.tsx", import.meta.url),
  "utf8",
);

test("UI 明示不自动计算、不自动保存并仅用独立记录 API", () => {
  assert.match(uiSource, /页面不会自动计算或保存/);
  assert.match(uiSource, /saveCriminalSentencingEstimate/);
  assert.match(uiSource, /保存到案件画像（测算记录）/);
  assert.match(uiSource, /确认追加独立记录/);
  assert.match(uiSource, /只会追加一条独立测算记录/);
  assert.doesNotMatch(uiSource, /upsertCriminalCaseProfile/);
  assert.match(uiSource, /const update =[\s\S]*?setConfirmingSave\(false\)/);
  assert.match(uiSource, /const calculate = \(\) => \{\s*setConfirmingSave\(false\)/);
  assert.match(uiSource, /toast\("量刑测算已另存为独立记录，未修改刑事画像。"[\s\S]*?setConfirmingSave\(false\)/);
});

test("无画像 revision 只允许测算并提示先保存画像", () => {
  assert.match(uiSource, /expectedProfileRevision == null/);
  assert.match(uiSource, /需先返回案件保存刑事画像/);
  assert.match(panelSource, /useState<number \| null>\(null\)/);
  assert.match(panelSource, /setProfileRevision\(profile\?\.profile_revision \?\? null\)/);
});

test("电诈只展示全国标准且切换时清空地区", () => {
  assert.match(uiSource, /isTelecom \? item\.subType === "电信诈骗" : item\.subType !== "电信诈骗"/);
  assert.match(uiSource, /setIsTelecom\(event\.target\.checked\); setAreaType\(""\)/);
});

test("缺失字段、无有效标准与危险推断均有阻断或提示", () => {
  assert.match(uiSource, /未知罪名不能测算/);
  assert.match(uiSource, /羁押等程序日期不能替代犯罪日期/);
  assert.match(uiSource, /请选择案件事实档位/);
  assert.match(uiSource, /请选择适用地区，不能从法院名称自动推断/);
  assert.match(uiSource, /next\.error \? null : next/);
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
