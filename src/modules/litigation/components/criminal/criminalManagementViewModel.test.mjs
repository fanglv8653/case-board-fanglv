import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import vm from "node:vm";

const source = readFileSync(new URL("./criminalManagementViewModel.ts", import.meta.url), "utf8");
const js = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const module = { exports: {} };
vm.runInNewContext(js, { module, exports: module.exports });
const { MANAGEMENT_TABS, isManagementTab } = module.exports;

assert.deepEqual(
  Array.from(MANAGEMENT_TABS, (item) => item.label),
  ["进展与任务", "案件信息", "阶段与期限", "案件通讯录", "工作记录"],
);
assert.equal(isManagementTab("progress"), true);
assert.equal(isManagementTab("materials"), false);
assert.equal(isManagementTab("drafting"), false);

console.log("criminal management view model tests passed");
