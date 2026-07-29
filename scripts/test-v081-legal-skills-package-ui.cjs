const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
const api = read("src/lib/api.ts");
const card = read("src/components/settings/LegalSkillsSettingsCard.tsx");

const commands = [
  "import_legal_skill_archive",
  "list_legal_skill_versions",
  "preview_legal_skill_diff",
  "upgrade_legal_skill_package",
  "rollback_legal_skill_package",
  "export_legal_skill_package",
  "delete_legal_skill_package",
];
for (const command of commands) {
  if (!api.includes(`"${command}"`)) throw new Error(`缺少方法包命令：${command}`);
}

for (const gate of [
  ".fanglv-skill.zip",
  "压缩包安全预检（尚未写入）",
  "ZIP 条目不能超过 23 个",
  "拒绝符号链接",
  "不安全或不受支持的 ZIP 路径",
  "预览差异",
  "确认升级",
  "确认回滚",
  "版本历史",
  "导出",
  "删除",
  "内置方法包不可删除",
]) {
  if (!card.includes(gate)) throw new Error(`缺少包管理门禁或功能：${gate}`);
}

if (!card.includes("confirmDialog(") || !api.includes("confirmed: true")) {
  throw new Error("升级、回滚或删除未形成前后端双重确认门禁");
}
if (!card.includes("writeFile(destination, Uint8Array.from(archive.bytes))")) {
  throw new Error("导出字节未写入用户选择路径");
}
if (!card.includes("record.origin === \"imported\"")) {
  throw new Error("删除按钮未限定为导入包");
}
if (!card.includes("bindDefaultLegalSkill")) {
  throw new Error("现有运行前默认 Skill 选择能力被移除");
}

console.log("V0.8.1 法律 Skills 完整包管理前端专项检查通过");
