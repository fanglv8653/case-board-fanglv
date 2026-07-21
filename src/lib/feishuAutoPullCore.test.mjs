import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import vm from "node:vm";

const source = readFileSync(new URL("./feishuAutoPullCore.ts", import.meta.url), "utf8");
const js = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const module = { exports: {} };
vm.runInNewContext(js, { module, exports: module.exports });
const { createFeishuAutoPullCoordinator } = module.exports;

let now = 1_000;
let pulls = 0;
let connected = true;
let online = true;
const run = createFeishuAutoPullCoordinator({
  isOnline: () => online,
  now: () => now,
  getConnectionStatus: async () => ({ connected, reauthorization_required: false }),
  pullPreview: async () => { pulls += 1; },
}, 30 * 60 * 1000);

assert.equal((await run()).reason, "pulled");
assert.equal(pulls, 1);
assert.equal((await run()).reason, "throttled");
assert.equal(pulls, 1);

now += 30 * 60 * 1000;
connected = false;
assert.equal((await run()).reason, "disconnected");
assert.equal(pulls, 1);

connected = true;
online = false;
assert.equal((await run()).reason, "offline");
assert.equal(pulls, 1);

console.log("feishu auto pull core tests passed");
