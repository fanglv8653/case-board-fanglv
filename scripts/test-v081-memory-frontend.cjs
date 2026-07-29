const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
const api = read("src/lib/api.ts");
const tabs = read("src/components/ModuleTabs.tsx");
const view = read("src/components/memory/MemoryView.tsx");
const panel = read("src/components/memory/CaseMemoryPanel.tsx");
const capabilities = read("src/lib/memoryCapabilities.ts");

const commands = [
  "list_case_memories",
  "create_case_memory_draft",
  "confirm_case_memory",
  "revise_case_memory",
  "set_case_memory_status",
  "list_memory_candidates",
  "accept_memory_candidate",
  "reject_memory_candidate",
  "list_user_memory_preferences",
  "create_user_memory_preference",
  "confirm_user_memory_preference",
  "preview_memory_injection",
  "confirm_memory_injection",
];

for (const command of commands) {
  if (!api.includes(`"${command}"`)) {
    throw new Error(`缺少记忆命令封装：${command}`);
  }
}
if (!tabs.includes('id: "memory"') || !tabs.includes('label: "记忆"')) {
  throw new Error("顶部导航未注册记忆模块");
}
for (const text of [
  "任何记忆默认都不会静默注入 AI",
  "启用不等于注入",
  "请明确选择一个案件",
]) {
  if (!view.includes(text)) throw new Error(`缺少全局安全提示：${text}`);
}
for (const text of [
  "仅归档（默认，不注入）",
  "接受为草稿",
  "生成预览（不注入）",
  "仍不自动发送",
]) {
  if (!panel.includes(text)) throw new Error(`缺少案件记忆门禁：${text}`);
}
if (!panel.includes('"deleted"') || !panel.includes("setCaseMemoryStatus")) {
  throw new Error("case memory soft-delete control is missing");
}
if (panel.includes('<option value="verified">')) {
  throw new Error("manual creation must not claim verified status without a locatable source");
}
if (
  !capabilities.includes('id: "case_chat"') ||
  !capabilities.includes("supported: true") ||
  !capabilities.includes('id: "material_ai"') ||
  !capabilities.includes("supported: false")
) {
  throw new Error("explicit AI memory capability matrix is missing");
}

if (!panel.includes("setSelectedMemoryIds([])") || !panel.includes("invalidatePreview()")) {
  throw new Error("案件切换或选择变化时未清空逐轮选择/旧预览");
}

console.log("V0.8.1 记忆前端专项检查通过");
