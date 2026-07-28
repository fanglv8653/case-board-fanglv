import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const modal = readFileSync(new URL("./components/SettingsModal.tsx", import.meta.url), "utf8");
const usage = readFileSync(new URL("./components/settings/LocalUsageDashboard.tsx", import.meta.url), "utf8");
const api = readFileSync(new URL("./lib/api.ts", import.meta.url), "utf8");
const types = readFileSync(new URL("./lib/types.ts", import.meta.url), "utf8");
const localDateSource = readFileSync(new URL("./lib/localDate.ts", import.meta.url), "utf8");
const usageIntegration = modal.slice(
  modal.indexOf("function IntegratedUsageDashboard"),
  modal.indexOf("function IntegratedLocalKbRelocationCard"),
);

assert.match(modal, /<IntegratedUsageDashboard onValidateConnection=\{handleVerifyYuandian\}/);
assert.match(modal, /getLocalRecognitionUsage\(\{ granularity: "day"/);
assert.match(modal, /getLocalRecognitionUsage\(\{ granularity: "month"/);
assert.match(modal, /const today = localDateKey\(\)/);
assert.doesNotMatch(usageIntegration, /toISOString\(\)\.slice\(0, 10\)/);
assert.match(modal, /refreshYuandianLocalUsage\(\)/);
assert.match(modal, /official_balance: yuandian\.officialBalance/);
assert.match(modal, /"未提供官方余额接口"/);
assert.doesNotMatch(modal, /title="元典积分账"/);

assert.match(modal, /<IntegratedLocalKbRelocationCard/);
assert.match(modal, /dialogOpen\(\{ directory: true, multiple: false \}\)/);
assert.match(modal, /switchExistingLocalKb\(targetPath\)/);
assert.match(modal, /migrateCurrentLocalKb\(targetPath\)/);
assert.match(modal, /await buildLocalKbSemanticIndex\(\)/);
assert.match(modal, /旧目录不会删除/);
assert.match(modal, /旧目录仍保留为回退备份/);

for (const command of [
  "get_local_recognition_usage",
  "refresh_yuandian_local_usage",
  "switch_existing_local_kb",
  "migrate_current_local_kb",
]) {
  assert.match(api, new RegExp(command));
}
assert.match(types, /officialBalance: number \| null/);
assert.match(types, /index_rebuild_required: boolean/);

process.env.TZ = "Asia/Shanghai";
const executableLocalDate = localDateSource
  .replace("export function", "function")
  .replace(": string", "")
  .concat("\nreturn localDateKey;");
const localDateKey = new Function(executableLocalDate)();
assert.equal(
  localDateKey(new Date("2026-07-28T16:30:00.000Z")),
  "2026-07-29",
  "UTC+8 00:30 must remain the local calendar day",
);

assert.match(modal, /async function handleVerifyYuandian\(\): Promise<boolean>/);
assert.match(modal, /return false;/);
assert.match(modal, /return true;/);
assert.match(usage, /if \(!valid\) throw new Error\("连接验证失败"\)/);

console.log("V080-B45I settings integration contract tests passed");
