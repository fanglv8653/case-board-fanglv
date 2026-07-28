import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const usage = readFileSync(new URL("./components/settings/LocalUsageDashboard.tsx", import.meta.url), "utf8");
const kb = readFileSync(new URL("./components/settings/LocalKbRelocationCard.tsx", import.meta.url), "utf8");

assert.match(usage, /元典本地用量估算/);
assert.match(usage, /识别服务本地用量/);
assert.match(usage, /不等同于元典官方账单或 OCR 识别指标/);
assert.match(usage, /不代表元典官方余额/);
assert.match(usage, /验证连接/);
assert.match(usage, /刷新本地统计/);
assert.match(usage, /未提供官方余额接口/);
assert.match(usage, /last_refreshed_at/);
assert.match(usage, /rate_limited_429_count/);
assert.match(usage, /page_count_unavailable_reason/);
assert.match(usage, /fallback_unavailable_reason/);

assert.match(kb, /当前绝对路径/);
assert.match(kb, /切换已有库/);
assert.match(kb, /迁移当前库/);
assert.match(kb, /onPickDirectory/);
assert.match(kb, /onConfirm/);
assert.match(kb, /backup_path/);
assert.match(kb, /recovery_path/);
assert.match(kb, /index_rebuild_required/);
assert.match(kb, /当前不能宣称迁移闭环/);
assert.match(kb, /await rebuildIndex\(relocation\)/);
assert.match(kb, /async function rebuildIndex[\s\S]*setError\(null\)[\s\S]*setState\("rebuild_required"\)/);

assert.doesNotMatch(usage + kb, /invoke\(|from "@\/lib\/api"|SettingsModal/);

console.log("V080-B45F component contract tests passed");
