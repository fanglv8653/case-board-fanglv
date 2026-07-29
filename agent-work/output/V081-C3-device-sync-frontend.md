# V081-C3 我的设备同步设置前端交付报告

日期：2026-07-29

## 结果

“设置 → 通用”已新增“我的设备同步”，覆盖：

- NAS 挂载目录选择、可写验证和绑定状态；
- 同步状态、暂停/恢复、立即双向同步；
- 创建同步组并强制同时导出 NAS 目录之外的离线加密恢复包；
- 一次性邀请、加入申请、受信设备审批和完成加入；
- 设备列表、指纹与非本机设备吊销；
- 冲突逐项对比，并人工选择保留本机版本或采用 NAS 版本；
- 手动加密快照、快照列表及隔离恢复预览；
- 离线恢复包只读验证与预览；
- 固定同步白名单和首期排除项说明。

界面允许 NAS 尚未挂载时保持“未配置”，不会阻塞其他设置。

## 安全边界

- NAS 仅作为加密变更包和快照的中转目录，不把 NAS 当作共享 SQLite 数据库。
- 冲突不静默覆盖，必须逐项人工选择。
- 设备吊销前显示原生确认对话框，并明确会触发密钥轮换。
- 快照与恢复包只提供隔离预览；本页不提供覆盖正式数据库的恢复按钮。
- 首期明确排除原始材料、OCR/抽取全文、聊天、记忆、凭证及 SQLite/WAL/SHM。
- 财务记录与飞书关联位于固定同步白名单。

## 前端命令契约

检查时 `src-tauri/src/lib.rs` 尚未注册设备同步 Tauri 命令。前端采用以下稳定命名，主控或后端包需一一注册或在 `src/lib/api.ts` 适配：

| 命令 | 前端参数 | 返回 |
|---|---|---|
| `get_device_sync_status` | 无 | `DeviceSyncStatus \| null` |
| `validate_device_sync_nas_path` | `connectorRoot` | `{connector_root,writable}` |
| `create_device_sync_group` | `input:{connector_root,display_name,recovery_destination,recovery_passphrase}` | `DeviceSyncCreatedGroup` |
| `set_device_sync_paused` | `groupId,paused` | `DeviceSyncStatus` |
| `run_device_sync` | `groupId` | `DeviceSyncRunResult` |
| `create_device_sync_invite` | `groupId` | `DeviceSyncInvite` |
| `create_device_sync_join_request` | `input:{connector_root,pairing_code,display_name}` | `DeviceSyncJoinRequest` |
| `approve_device_sync_join` | `groupId,requestPath` | `{completion_path}` |
| `complete_device_sync_join` | `input:{connector_root,request_path,completion_path,pairing_code}` | `DeviceSyncJoinCompletion` |
| `list_device_sync_members` | `groupId` | `DeviceSyncMember[]` |
| `revoke_device_sync_member` | `groupId,deviceId` | `DeviceSyncStatus` |
| `list_device_sync_conflicts` | `groupId` | `DeviceSyncConflict[]` |
| `resolve_device_sync_conflict` | `operationId,resolution` | `number` |
| `create_device_sync_snapshot` | `groupId,snapshotKind` | `DeviceSyncSnapshot` |
| `list_device_sync_snapshots` | `groupId` | `DeviceSyncSnapshot[]` |
| `preview_device_sync_restore` | `groupId,snapshotPath` | `DeviceSyncRestorePreview` |
| `preview_device_sync_recovery` | `packagePath,passphrase` | `DeviceSyncRecoveryPreview` |

其中 Rust 核心已有对应的身份、配对、引擎、冲突、快照和恢复能力；缺口主要是 Tauri 命令注册、列表查询包装及参数形状适配。

## 修改文件

- `src/components/SettingsModal.tsx`
- `src/components/settings/DeviceSyncSettingsCard.tsx`
- `src/lib/api.ts`
- `src/lib/types.ts`
- `scripts/test-v081-device-sync-frontend.cjs`

未修改任何 Rust 文件。

## 检查

- `node node_modules/typescript/bin/tsc --noEmit`：通过。
- `node scripts/test-v081-device-sync-frontend.cjs`：通过。
- 未运行 Rust fmt/build。

## 主控运行时验收点

1. 后端命令注册后，在无同步组状态下打开设置页，不产生全局崩溃。
2. 使用测试 NAS 目录完成创建组、恢复包导出、第二设备加入和双向同步。
3. 构造同字段双写，确认出现冲突且两种人工选择路径均可闭环。
4. 确认 NAS 目录内不存在材料、聊天、记忆、凭证和明文业务数据。
5. 隔离预览前后比对正式数据库哈希，确认预览不写正式库。
