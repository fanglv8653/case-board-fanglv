import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import vm from "node:vm";

const source = readFileSync(new URL("./criminalWorkspaceViewModel.ts", import.meta.url), "utf8");
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const module = { exports: {} };
vm.runInNewContext(js, { module, exports: module.exports, require: () => ({}) });
const { WORKSPACE_ZONES, availableProviders, canConfirmFinding, citationLocation, confirmedSelectionIds, reviewLabel } = module.exports;

assert.deepEqual(Array.from(WORKSPACE_ZONES, (item) => item.label), ["材料阅卷", "证据争点", "案件分析", "文书草拟", "流程任务"]);
assert.equal(reviewLabel("pending_review"), "待律师复核");
assert.equal(citationLocation({ page_start: 3, page_end: 5 }), "第 3-5 页");
assert.equal(canConfirmFinding("material_fact", []), false);
assert.equal(canConfirmFinding("material_fact", [{ citation_kind: "material", integrity_status: "valid" }]), true);
assert.equal(canConfirmFinding("analysis", []), true);
assert.deepEqual(
  Array.from(confirmedSelectionIds([
    { id: "confirmed", review_status: "confirmed" },
    { id: "pending", review_status: "pending_review" },
  ], new Set(["confirmed", "pending"]))),
  ["confirmed"],
);
assert.deepEqual(
  Array.from(availableProviders({ manual: true, native_llm: { available: false, reason: "off" }, codex: { available: true, experimental: true, reason: null } }), (item) => item.id),
  ["manual", "codex"],
);

console.log("criminal workspace view model tests passed");
