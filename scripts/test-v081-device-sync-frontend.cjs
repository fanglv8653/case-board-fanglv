const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
const api = read("src/lib/api.ts");
const card = read("src/components/settings/DeviceSyncSettingsCard.tsx");
const settings = read("src/components/SettingsModal.tsx");

const commands = [
  "get_device_sync_status",
  "validate_device_sync_nas_path",
  "create_device_sync_group",
  "set_device_sync_paused",
  "run_device_sync",
  "create_device_sync_invite",
  "create_device_sync_join_request",
  "approve_device_sync_join",
  "complete_device_sync_join",
  "list_device_sync_members",
  "revoke_device_sync_member",
  "list_device_sync_conflicts",
  "resolve_device_sync_conflict",
  "create_device_sync_snapshot",
  "list_device_sync_snapshots",
  "preview_device_sync_restore",
  "preview_device_sync_recovery",
];
for (const command of commands) {
  if (!api.includes(`"${command}"`)) throw new Error(`缺少设备同步命令封装：${command}`);
}

for (const text of [
  "我的设备同步",
  "NAS 挂载目录进行端到端加密备份与双向同步",
]) {
  if (!settings.includes(text)) throw new Error(`设置页未接入：${text}`);
}

for (const text of [
  "NAS 只是加密中转目录，不是数据库共享盘",
  "创建首个同步组",
  "邀请、加入与审批",
  "受信设备",
  "保留本机版本",
  "采用 NAS 版本",
  "正式数据库未改变",
  "固定同步白名单与排除项",
  "收入记录、收付款记录及核算字段",
  "飞书关联、同步快照、冲突与收件箱",
  "原始材料、附件路径、OCR/抽取全文",
  "聊天记录与案件记忆",
]) {
  if (!card.includes(text)) throw new Error(`缺少设备同步界面要素：${text}`);
}

if (card.includes("restoreDeviceSync") || card.includes("applyDeviceSyncRestore")) {
  throw new Error("首期界面不应提供直接写入正式数据库的恢复动作");
}

console.log("V0.8.1 设备同步设置前端专项检查通过");
