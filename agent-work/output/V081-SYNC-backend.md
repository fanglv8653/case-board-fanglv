# V0.8.1 设备同步后端实施与验收报告

日期：2026-07-29
状态：`completed`

## 1. 交付结论

已完成 Windows 挂载盘符/UNC 目录型 NAS 连接器、端到端加密双向同步、设备配对与吊销轮换、冲突处理、快照与隔离恢复、离线恢复包、后台调度以及前后端命令接线。

同步范围严格限定为：

- 案件基础信息；
- 当事人及联系人；
- 办案事项、程序阶段、刑事流程任务、待办与日历；
- 财务记录及回款；
- 飞书绑定和经本机确认的安全投影；
- 全局法律 Skills 包、默认绑定和“无默认方法”抑制记录。

明确排除：

- 原始材料、材料路径和抽取文本；
- 聊天记录；
- 案件记忆；
- 飞书原始载荷、OAuth 凭据及其他密钥。

## 2. 主要实现

- `src-tauri/src/device_sync/`：注册表、捕获器、加密、身份、配对、清单、NAS 文件协议、操作合并、同步引擎、调度器、快照、隔离恢复、离线恢复和 Tauri 命令。
- `0058_device_sync_core.sql`：协议表、业务表同事务 dirty trigger、实体白名单 CHECK、冲突/回执/隔离/审计表。
- `0059_device_sync_security_hardening.sql`：法律 Skills 抑制记录稳定逻辑 ID 与同步 trigger。
- `lib.rs`：注册 17 个设备同步命令，并在应用启动时启动 scheduler。
- `api.ts` / `DeviceSyncSettingsCard.tsx`：17 个命令调用；审批加入必须人工输入并核对设备指纹。

加密与密钥边界：

- 数据包使用 AES-256-GCM；
- 设备签名使用 Ed25519；
- 配对密钥封装使用 X25519 派生密钥；
- 设备私钥、组密钥和一次性邀请码仅存 Windows Credential Manager；
- NAS 仅保存签名加密包及非敏感协议元数据。

运行与恢复：

- 本地事务只写 dirty 标记，NAS 离线不回滚本地业务；
- 启动立即检查，dirty 防抖 5 秒，最长 60 秒周期重试；
- 手动和后台同步共用进程锁；
- 每日/每月加密快照保留 30/12 份；
- 恢复先只读预览，随后只能写入独立 SQLite，不直接覆盖正式库；
- 建组必须同时成功导出 NAS 目录之外的 PBKDF2 + AES-GCM 离线恢复包。

## 3. 安全审计闭环

- 修复字段过滤子串误杀：`wechat`、`app_token` 可正常同步，敏感字段改为显式拒绝。
- 修复初始基线父子顺序：upsert 按依赖正序，tombstone 按依赖逆序。
- 修复入站 tombstone：无冲突时真实删除对应业务行，并在同事务清除回声 dirty 标记。
- 纳入 `legal_skill_binding_suppressions`，保证两台电脑对“无附加方法”的选择一致。
- 协议表增加数据库级 entity type CHECK，未知实体在落库前即失败关闭。
- 加入审批要求通过可信渠道人工核对设备指纹，不能从申请文件自动信任。

## 4. 验收证据

### Rust 全量编译

命令：

```text
cargo check --manifest-path src-tauri/Cargo.toml
```

结果：通过，`Finished dev profile`，无编译错误。

### 设备同步强化契约

命令：

```text
cargo test --manifest-path src-tauri/Cargo.toml --test device_sync_contract -- --nocapture
```

结果：`18 passed; 0 failed`。

覆盖：

- 完整 0058/0059 迁移链；
- Windows Credential Manager 和真实双库配对；
- 一次性邀请、防重放、过期拒绝、指纹不一致拒绝；
- X25519/AES-GCM/Ed25519、篡改拒绝；
- 父子基线排序；
- 案件、财务、飞书、Skills 抑制记录捕获；
- 排除材料/聊天/记忆和飞书网络路径；
- 不同字段合并、同字段冲突、幂等重复；
- tombstone 真实删除；
- 未知实体数据库约束拒绝；
- 设备吊销与密钥轮换；
- 快照留存和隔离恢复。

### TypeScript

`pnpm exec tsc --noEmit` 因当前非交互环境触发 pnpm 的 node_modules 清理确认而中止；未允许其改写共享依赖目录。随后直接使用现有本地编译器：

```text
node_modules/.bin/tsc.cmd --noEmit
```

结果：通过。

### 变更卫生

`git diff --check`：通过，仅有 Git 的 LF/CRLF 提示，无空白错误。

## 5. 诚实边界

- 首期连接器依赖 Windows 已挂载的 SMB 盘符或 UNC 路径；应用不负责 NAS 账号登录和系统挂载。
- NAS 离线时本地继续工作，恢复可访问后自动重试。
- 恢复流程不会直接覆盖正式数据库，正式切换仍需用户在隔离预览后另行确认。
- 本模块不调用飞书网络接口；只同步本地已确认的绑定与安全投影。
