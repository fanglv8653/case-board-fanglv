# V081 前端整合验收报告

日期：2026-07-29

## 验收结论

**通过。前端编译、生产构建和全部 V081 专项脚本通过；设置卡片及记忆模块的新增 Tauri 命令均已定义并注册，前后端参数和返回字段完成静态逐项核对。**

Skills 导出权限问题已由主控在本轮验收期间修复：`src-tauri/capabilities/default.json` 已加入最小权限 `fs:allow-write-file`，继续由保存对话框提供动态路径 scope。

## 自动化结果

| 检查 | 结果 | 证据 |
|---|---|---|
| TypeScript | 通过 | `node node_modules/typescript/bin/tsc --noEmit` |
| Vite 生产构建 | 通过 | Vite 7.3.3；2878 modules transformed；built in 7.61s |
| UI foundation | 通过 | `V081 UI foundation logic checks passed.` |
| 记忆前端 | 通过 | `V0.8.1 记忆前端专项检查通过` |
| 设备同步设置前端 | 通过 | `V0.8.1 设备同步设置前端专项检查通过` |
| Skills 包管理 | 通过 | `V0.8.1 法律 Skills 完整包管理前端专项检查通过` |
| 差异空白检查 | 通过 | 无 whitespace error；只有既有 LF/CRLF 提示 |

Vite 仍报告主 chunk 大于 500 kB，这是项目既有性能警告，不是构建失败。

## 设置页卡片与命令契约

### 设置 → 通用 → 我的设备同步

前端卡片已接入，以下 17 个命令均已在 `src-tauri/src/device_sync/commands.rs` 定义，并以 `device_sync::commands::*` 进入 `lib.rs` 的 `generate_handler!`：

- `get_device_sync_status`
- `validate_device_sync_nas_path`
- `create_device_sync_group`
- `set_device_sync_paused`
- `run_device_sync`
- `create_device_sync_invite`
- `create_device_sync_join_request`
- `approve_device_sync_join`
- `complete_device_sync_join`
- `list_device_sync_members`
- `revoke_device_sync_member`
- `list_device_sync_conflicts`
- `resolve_device_sync_conflict`
- `create_device_sync_snapshot`
- `list_device_sync_snapshots`
- `preview_device_sync_restore`
- `preview_device_sync_recovery`

逐项契约核对结果：

- `get_device_sync_status()` → `DeviceSyncStatus | null`：字段一致。
- `validate_device_sync_nas_path(connectorRoot)` → `{connector_root,writable}`：一致。
- `create_device_sync_group({input})` → `DeviceSyncCreatedGroup`：identity 与 recovery 字段一致。
- `set_device_sync_paused(groupId,paused)` → `DeviceSyncStatus`：一致。
- `run_device_sync(groupId)` → `DeviceSyncRunResult`：五个计数字段一致。
- 邀请、加入申请、审批、完成加入：参数展开和返回字段一致；审批返回的 `completion_path` 已由前端自动填入下一步。
- 成员列表与吊销：字段一致；Rust `i64/u32` 在前端统一为安全整数 `number`。
- 冲突列表与处理：字段一致；前端只允许后端支持的 `keep_local/keep_remote`。
- 快照创建/列表/隔离预览：参数和 `SnapshotResult/RestorePreview` 字段一致。
- 恢复包预览：参数和 `RecoveryPreview` 字段一致。

静态契约无差异。真实 NAS 双机往返仍属于发布前运行时验收，不是当前前端阻断。

### 设置 → 主题

- 纯前端本地主题偏好，不依赖 Tauri 命令。
- 默认主题与“墨绿象牙”均通过统一 CSS variables 生效。
- 切换逻辑和专项测试通过。

### 设置 → 大脑 → 全局法律 Skills

以下命令均已在 `lib.rs` 定义并进入 `generate_handler!`，camelCase 前端参数与 Rust snake_case 参数匹配：

- `list_legal_skill_packages`
- `import_legal_skill_package`
- `import_legal_skill_archive(fileName, archiveBytes)`
- `list_legal_skill_versions(slug)`
- `preview_legal_skill_diff(currentSkillId, targetSkillId)`
- `upgrade_legal_skill_package(currentSkillId, targetSkillId, confirmed)`
- `rollback_legal_skill_package(currentSkillId, targetSkillId, confirmed)`
- `export_legal_skill_package(skillId)`
- `delete_legal_skill_package(skillId, confirmed)`
- `set_legal_skill_package_enabled`
- `bind_default_legal_skill`

本轮验收修复一项前端契约问题：

- 后端要求版本切换的 `current_skill_id` 必须是当前已启用的 imported 版本。
- 原界面允许从任意版本（包括停用或内置版本）打开版本切换。
- 已改为：版本按钮只对 imported 包显示；读取历史后自动定位当前已启用 imported 版本；隔离/删除版本不进入目标列表。修复后重新通过全部检查。

运行时权限复核：

- `export_legal_skill_package` 返回字节，前端通过保存对话框和 `writeFile` 写盘。
- `src-tauri/capabilities/default.json` 已包含 `fs:allow-write-file`。
- 保存位置仍必须由原生保存对话框选择；对话框负责把所选路径动态加入 filesystem scope。
- 未扩大静态 scope 到任意磁盘目录，符合最小权限原则。

### 设置 → 知识库 → 检索与维护说明

- `get_local_kb_guide()` 已定义且进入 `generate_handler!`。
- 前端不传参数，与 Rust 无参数签名一致。
- 只读说明、当前根目录、真实检索范围和排除规则正常参与构建。

### 设置 → 数据源 → 元典官方余额

- `get_yuandian_balance(refresh)` 已定义且进入 `generate_handler!`。
- 前端 `{refresh:boolean}` 与 Rust `refresh: Option<bool>` 一致。
- 进入卡片刷新一次、手动刷新、缓存失败回退和本地积分账差异展示均通过静态检查。

### 设置 → 功能开关

- 九类板块开关使用前端本地持久化，不依赖新增 Tauri 命令。
- 专项逻辑检查通过。

## 记忆模块命令契约

虽然记忆不是设置页卡片，本轮一并核对：

- 13 个记忆命令均在 `lib.rs` 定义并进入 `generate_handler!`。
- 前端参数与 Rust 签名一致：
  - 案件 ID、记忆 ID、候选 ID、修订号；
  - 可选 reason；
  - 逐轮任务类型、选中记忆/偏好 ID；
  - 注入运行 ID 与预览 SHA-256。
- `CaseChatInput` 已保留：
  - `preferred_legal_skill_slug`
  - `memory_injection_run_id`
  - `memory_injection_preview_sha256`

## 未执行内容

- 未运行 Rust fmt/build/test。
- 未启动正式应用或修改正式数据库。
- 未进行真实 NAS 双机同步、真实元典在线余额或真实 Skills 文件保存。
- 真实 NAS、真实元典和正式 GUI 仍未在本前端静态验收中运行。

## 主控收口顺序

1. 使用隔离数据目录做 Settings 逐卡片运行时 smoke。
2. 用测试 NAS 完成创建组、双机加入、冲突处理和快照预览。
3. 用用户选择路径完成一次 Skills ZIP 导出与重新导入。
