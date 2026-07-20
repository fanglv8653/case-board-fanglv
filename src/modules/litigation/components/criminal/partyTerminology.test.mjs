import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import vm from "node:vm";

const source = readFileSync(new URL("./partyTerminology.ts", import.meta.url), "utf8");
const js = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const module = { exports: {} };
vm.runInNewContext(js, { module, exports: module.exports });
const { criminalPartyTermForStage, criminalPartyNameLabel, normalizeCriminalPartyRoleForStage } = module.exports;

for (const stage of ["侦查阶段", "审查逮捕", "审查起诉阶段", "退回补充侦查"]) {
  assert.equal(criminalPartyTermForStage(stage), "犯罪嫌疑人");
}
for (const stage of ["一审阶段", "二审阶段", "审判阶段", "再审", "死刑复核", "开庭"]) {
  assert.equal(criminalPartyTermForStage(stage), "被告人");
}
assert.equal(criminalPartyTermForStage(""), "犯罪嫌疑人/被告人");
assert.equal(criminalPartyNameLabel("二审阶段"), "被告人姓名");
assert.equal(normalizeCriminalPartyRoleForStage("犯罪嫌疑人", "一审阶段"), "被告人");
assert.equal(normalizeCriminalPartyRoleForStage("被告人", "审查起诉阶段"), "犯罪嫌疑人");
assert.equal(normalizeCriminalPartyRoleForStage("原告", "一审阶段"), "原告");

console.log("party terminology tests passed");
