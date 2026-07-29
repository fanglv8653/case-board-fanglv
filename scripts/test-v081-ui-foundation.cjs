const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const read = (relative) => fs.readFileSync(path.join(root, relative), "utf8");
const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const theme = read("src/lib/theme.ts");
const main = read("src/main.tsx");
const css = read("src/styles/globals.css");
const flags = read("src/lib/featureFlags.ts");
const settings = read("src/components/SettingsModal.tsx");
const kbGuide = read("src/components/settings/LocalKbGuideCard.tsx");
const kbGuideSource = read("src-tauri/src/local_kb/guide.rs");

assert(theme.includes('id: "default"'), "缺少方律默认主题");
assert(theme.includes('id: "emerald_ivory"'), "缺少墨绿象牙主题");
assert(theme.includes("localStorage.setItem(THEME_STORAGE_KEY"), "主题未做本机持久化");
assert(theme.includes("aria") === false, "主题逻辑层不应包含界面标记");
assert(
  main.indexOf("applyThemePreference();") < main.indexOf("ReactDOM.createRoot"),
  "主题必须在 React 渲染前应用",
);
assert(
  css.includes(':root[data-theme="emerald_ivory"]:not(.dark)'),
  "墨绿象牙必须只覆盖亮色模式",
);
assert(settings.includes('aria-pressed={selected}'), "主题选项缺少可访问选中状态");

const realFlags = ["home_filter_bar", "home_ticktick", "case_court_filing"];
for (const name of realFlags) {
  assert(flags.includes(`"${name}"`), `缺少真实功能开关 ${name}`);
}
for (const absent of [
  "home_companion",
  "case_todos",
  "case_work_logs",
  "case_work_reports",
  "case_ai_organize_filters",
  "reference_materials",
]) {
  assert(!flags.includes(`"${absent}"`), `不得提前注册空开关 ${absent}`);
}
assert(settings.includes("const flags = FEATURE_FLAGS;"), "设置页未统一渲染真实功能开关");

assert(kbGuide.includes("getLocalKbGuide"), "设置页未读取后端同源知识库说明");
assert(kbGuideSource.includes("KEYWORD_FILE_EXTENSIONS"), "知识库说明未复用真实扩展名常量");
assert(kbGuideSource.includes("MAX_FILE_SIZE"), "知识库说明未复用真实大小上限");
assert(kbGuideSource.includes("KEYWORD_EXCLUDED_ROOT_PREFIX"), "知识库说明未复用真实排除规则");
assert(kbGuideSource.includes("AGENTS.md、CLAUDE.md"), "知识库只读边界不完整");
assert(kbGuideSource.includes("不会开放 AI 写入"), "知识库说明未关闭 AI 写入");

console.log("V081 UI foundation logic checks passed.");
