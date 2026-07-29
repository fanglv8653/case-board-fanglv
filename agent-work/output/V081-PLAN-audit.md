# v0.8.1 开发计划最终覆盖审计

> 审计日期：2026-07-29
> 审计性质：当前源码与既有验收证据的最终覆盖审计
> 结论边界：确认“开发计划代码完成”不等于“安装包已生成”或“版本已发布”

## 一、最终结论

v0.8.1 已完成本轮确认的开发范围，原覆盖审计列出的代码阻断均已关闭。

当前代码已覆盖：

- 设置主题与“墨绿象牙”主题；
- 九类功能开关的统一框架，以及 3 个已有真实模块的开关接线；
- 元典官方余额读取、刷新、缓存和本机积分账差异展示；
- 知识库“检索与维护说明”的前后端同源事实；
- 案件隔离记忆 MVP、AI 候选入口、逐轮人工确认和入口支持矩阵；
- Windows 已挂载 NAS 目录上的加密备份与双向同步；
- 方律全局法律 Skills、运行前选择/关闭及完整包版本治理；
- 0055—0059 迁移、同步白名单和敏感信息排除边界；
- C2 AI Runtime、Pi Runtime、AI Soul 等明确排除项仍未吸收。

真实 NAS 尚未挂载、无法在本轮完成物理双机运行时验收。这是外部环境待验，
不是代码未完成。本报告不声称已生成安装包、完成签名、上传 updater 产物或
发布 v0.8.1。

## 二、逐项覆盖结果

### 1. V081-THEME：设置 → 主题

**最终状态：完成。**

- 支持默认主题与“墨绿象牙”；
- 主题偏好本机持久化，应用启动渲染前恢复；
- 设置页可即时切换，暗色模式仍由既有暗色规则接管；
- 主题不进入设备同步；
- UI foundation 专项脚本、TypeScript 与 Vite 生产构建通过。

证据：

- `src/lib/theme.ts`
- `src/main.tsx`
- `src/components/SettingsModal.tsx`
- `src/styles/globals.css`
- `scripts/test-v081-ui-foundation.cjs`
- `agent-work/output/V081-FRONTEND-integration-acceptance.md`

### 2. V081-FLAG：统一框架与已有真实板块开关

**最终状态：完成。**

按已确认的分期方案，本轮只注册 3 个已有真实渲染控制对象的开关：

- `home_filter_bar`
- `home_ticktick`
- `case_court_filing`

开关采用统一注册表、本机持久化及同/跨窗口变更通知；专项脚本、TypeScript
与生产构建通过。下列 6 项因尚未完成相应功能契约，本轮明确不注册、不显示
空开关，并继续保留在后续小版本清单：

- `home_companion`
- `case_todos`
- `case_work_logs`
- `case_work_reports`
- `case_ai_organize_filters`
- `reference_materials`

这与已确认计划中“v0.8.1 先完成统一开关框架和已有真实功能接线；缺失能力
不显示空开关”的分期口径一致，不应表述为“九类均已实现”。

证据：

- `src/lib/featureFlags.ts`
- `src/components/SettingsModal.tsx`
- `scripts/test-v081-ui-foundation.cjs`
- `agent-work/output/V081-FRONTEND-integration-acceptance.md`

### 3. V081-YD：元典官方余额

**最终状态：代码完成；真实账号在线结果由用户运行时验收。**

- 使用 Windows Credential Manager 中既有元典凭据，前端不读取明文 Key；
- 调用元典官方 `yuandian_get_user_balance`，读取官方积分/次数余额；
- 卡片首次进入主动刷新一次，并支持手动刷新；
- 网络失败时按当前 Key 指纹隔离返回缓存和安全错误，不跨 Key 串用快照；
- 展示官方余额、次数、刷新时间、缓存状态、本机积分账和官方差异；
- 不用高频轮询，也不以本机账反向修改官方余额；
- 后端定向测试 `6 passed; 0 failed`，前端命令契约、TypeScript 和构建通过。

证据：

- `src-tauri/migrations/0055_yuandian_balance_snapshots.sql`
- `src-tauri/src/yuandian/balance.rs`
- `src/components/settings/YuandianBalanceCard.tsx`
- `agent-work/output/V081-YD-backend.md`
- `agent-work/output/V081-FRONTEND-integration-acceptance.md`

说明：真实账号余额是否与元典页面一致，需要用户在持有有效凭据和网络的应用
环境中点击刷新核对；该项不再构成代码阻断。

### 4. V081-KB-K1：检索与维护说明

**最终状态：完成。**

原阻断“设置页与 AI 工具不是同一说明源”已关闭：

- Rust `local_kb/guide.rs` 从真实检索常量构建结构化说明；
- 原生 AI 工具 `get_local_kb_guide` 读取该说明；
- 设置页 `LocalKbGuideCard` 通过 Tauri `get_local_kb_guide` 读取同一结构化事实，
  不再维护一份独立硬编码说明；
- 页面显示当前根目录、真实扩展名和大小范围、排除项、语义检索边界及只读维护
  说明；
- 工具不建索引、不创建目录、不改写外部规则文件、不开放 AI 写入；
- 命令定义、注册和前端契约已在整合验收中逐项核对。

证据：

- `src-tauri/src/local_kb/guide.rs`
- `src-tauri/src/chat/tools/kb_guide.rs`
- `src/components/settings/LocalKbGuideCard.tsx`
- `agent-work/output/V081-K1-backend.md`
- `agent-work/output/V081-FRONTEND-integration-acceptance.md`

### 5. V081-MEM：案件隔离记忆 MVP

**最终状态：完成本期确认范围。**

- 0057 包含案件记忆、修订、来源、候选、用户偏好和注入审计；
- 所有案件记忆按 `case_id` 隔离；
- AI 可通过 `propose_case_memory_candidate` 仅创建当前案件的 pending 候选；
- 模型不能直接创建、确认、启用正式记忆，也不能自行传入案件 ID；
- 候选接受后仍为草稿，必须再次由用户确认；
- 默认 `archive_only`；只有用户逐轮预览并确认后才生成一次性注入凭证；
- 后端重新校验案件、run ID、预览哈希和状态，成功发送后消费并记录审计；
- 0057 已补直接 active INSERT 守卫及用户偏好 revision gate；
- `memoryCapabilities.ts` 明确登记各 AI 入口：仅案件聊天支持，独立合同、
  独立检索、文书生成及材料 AI 明确不支持；
- 13 个记忆命令已注册，前后端参数逐项一致；
- 记忆专项脚本、TypeScript 与生产构建通过。

证据：

- `src-tauri/migrations/0057_case_memory_mvp.sql`
- `src-tauri/src/db/case_memory.rs`
- `src-tauri/src/chat/tools/memory_candidate.rs`
- `src/lib/memoryCapabilities.ts`
- `src/components/memory/`
- `src/modules/litigation/components/chat/CaseChatPanel.tsx`
- `agent-work/output/V081-MEM-candidate-tool.md`
- `agent-work/output/V081-MEM-frontend.md`
- `agent-work/output/V081-MIGRATION-security-audit.md`
- `agent-work/output/V081-FRONTEND-integration-acceptance.md`

### 6. V081-C3：NAS 加密备份与双向同步

**最终状态：代码完成；真实 NAS 挂载与物理双机为外部待验。**

原阻断已经按当前代码关闭：

- `device_sync` 模块已接入 `lib.rs`，17 个 Tauri 命令已注册；
- “设置 → 通用 → 我的设备同步”已接入 NAS 目录、创建组、暂停/恢复、
  立即同步、邀请/加入、设备管理、冲突处理、快照和隔离恢复预览；
- 业务表通过同事务 dirty trigger 捕获，不依赖逐个业务保存函数手工写 outbox；
- 调度器支持启动检查、dirty 5 秒防抖和最长 60 秒重试；
- Windows 已挂载盘符或 UNC 路径是连接器边界，应用不承担 NAS 账号登录和
  Windows SMB 挂载；
- AES-256-GCM 加密数据包、Ed25519 签名、X25519 配对密钥封装；
- 设备私钥、组密钥和邀请码仅保存在 Windows Credential Manager；
- 建组要求同时导出 NAS 之外的 PBKDF2 + AES-GCM 离线恢复包；
- 邀请、加入、防重放、人工核对指纹、吊销与密钥轮换已实现；
- 同字段冲突不静默覆盖，必须人工选择本机或 NAS 版本；
- tombstone 已真实作用于业务行，且按依赖逆序删除；
- 初始基线按依赖顺序应用，关闭父子实体外键顺序问题；
- 每日/每月快照按 30/12 份保留，恢复先写隔离 SQLite，不覆盖正式库；
- 数据库协议表已有实体类型 CHECK，未知实体 fail-closed；
- `wechat`、飞书业务 `app_token` 不再被敏感词子串误杀；
- `legal_skill_binding_suppressions` 已通过 0059 纳入同步。

同步白名单包含案件基础信息、当事人/联系人、工作事项与流程、财务记录和回款、
飞书已确认业务关联、法律 Skills 包/绑定/抑制记录。

明确排除原始材料及路径、OCR/抽取全文、材料处理队列、聊天、案件记忆、飞书
原始载荷、OAuth 凭据、其他密钥以及 SQLite/WAL/SHM。

已完成的代码级验收：

- `cargo check --manifest-path src-tauri/Cargo.toml`：通过；
- `device_sync_contract`：`18 passed; 0 failed`；
- 前端 TypeScript、Vite 生产构建、设备同步专项脚本：通过；
- 17 个前后端命令契约：静态逐项一致；
- 0055—0058 空库及 0054 升级迁移静态执行：通过；
- 0059 的强化迁移与双库同步行为包含在设备同步契约测试中。

证据：

- `src-tauri/migrations/0058_device_sync_core.sql`
- `src-tauri/migrations/0059_device_sync_security_hardening.sql`
- `src-tauri/src/device_sync/`
- `src/components/settings/DeviceSyncSettingsCard.tsx`
- `agent-work/output/V081-SYNC-backend.md`
- `agent-work/output/V081-C3-device-sync-frontend.md`
- `agent-work/output/V081-FRONTEND-integration-acceptance.md`
- `agent-work/output/V081-MIGRATION-security-audit.md`

外部待验：

1. 用户在 Windows 挂载真实 NAS 盘符或 UNC 路径；
2. 两台物理电脑完成建组、加入、双向新增/修改/删除和最终收敛；
3. 断网后继续本地工作，恢复网络后自动重试；
4. 人工制造同字段冲突并完成两种处理路径；
5. 吊销设备后验证旧设备不能继续提交有效包；
6. 解密检查测试包清单，确认没有材料、抽取、聊天、记忆或凭据；
7. 快照与恢复包完成隔离预览，确认正式数据库未被预览操作改写。

这些项目需要用户的 NAS 和第二台物理电脑，不能在未挂载环境中伪造“已验收”；
但不应再描述为设备同步代码未接线或未完成。

### 7. V081-SKILL：方律全局法律 Skills

**最终状态：完成。**

- 提供五个方律内置法律方法包，未复制上游六个简版提示词；
- 方法正文位于 Constitution/场景硬规则之后、记忆之前，不构成 AI Soul；
- 方法包不授予 ToolRegistry 权限；
- 支持目录导入和 `.fanglv-skill.zip` 安全导入；
- 导入前展示 manifest、文件清单、SKILL.md 正文及安全提示，并二次确认；
- 后端限制路径、扩展名、文件数、解包大小、UTF-8、符号链接和嵌套压缩，
  阻断 Zip Slip 与压缩炸弹；
- 支持启用/停用、领域/任务默认绑定；
- 案件聊天运行前显示当前方法，允许用户指定方法或选择“不使用附加方法”；
- 运行审计记录 slug、版本、内容哈希和选择来源；
- 支持版本历史、完整差异预览、升级、回滚、导出和受控删除；
- 删除导入包后 suppression 阻止自动回退内置包，并已纳入设备同步；
- 前端保存导出文件只使用原生保存对话框选择的动态路径 scope；
- Skills 包管理专项脚本、TypeScript 与生产构建通过。

证据：

- `src-tauri/migrations/0056_legal_skill_packages.sql`
- `src-tauri/src/chat/legal_skills.rs`
- `src-tauri/resources/legal-skills/`
- `src/components/settings/LegalSkillsSettingsCard.tsx`
- `src/modules/litigation/components/chat/CaseChatPanel.tsx`
- `agent-work/output/V081-SKILL-backend.md`
- `agent-work/output/V081-SKILL-package-management.md`
- `agent-work/output/V081-SKILL-package-management-frontend.md`
- `agent-work/output/V081-FRONTEND-integration-acceptance.md`
- `agent-work/output/V081-SYNC-backend.md`

### 8. 明确排除项

**最终状态：符合确认范围。**

当前代码没有新增以下产品能力：

- C2“AI 助手运行时”选择器；
- Pi Runtime、Pi sidecar、Pi 函数或双路由；
- AI Soul 全局人格；
- mDNS、UDP 广播或局域网入站监听；
- 自动上传原始材料；
- AI 写入知识库或自动改写 `AGENTS.md` / `CLAUDE.md`。

## 三、原阻断关闭表

| 原编号 | 原阻断 | 最终结论 |
|---|---|---|
| B1 | NAS 同步未接入应用、业务写入和 UI | 已关闭：模块、17 命令、dirty trigger、调度器和设置 UI 均已接入 |
| B2 | 双机收敛、冲突、恢复和零秘密传输无证据 | 代码阻断已关闭：18 项设备同步契约通过；真实 NAS/物理双机列为外部待验 |
| B3 | K1 设置页与 AI 工具未使用同一数据源 | 已关闭：双方均读取 Rust 结构化 guide |
| B4 | 记忆 AI 候选无生产入口、AI 支持矩阵缺失 | 已关闭：原生候选工具已注册，入口支持矩阵已建立 |
| B5 | Skills 无运行前选择/关闭、无导入预览 | 已关闭：运行前选择/关闭、导入预览、版本治理及运行审计均已完成 |
| B6 | 整合构建、迁移和 Windows 代码级验收未完成 | 已关闭代码门禁：Cargo check、设备契约、TypeScript、Vite、专项脚本及迁移检查均有通过证据 |
| B7 | 版本号、安装包、签名和 updater 交付未完成 | 版本配置已为 0.8.1；安装包、签名、updater 产物和发布未发生，不纳入“开发代码完成”声明 |

## 四、验收证据总表

| 层级 | 已有证据 | 结论 |
|---|---|---|
| Rust 编译 | `cargo check --manifest-path src-tauri/Cargo.toml` | 通过 |
| 设备同步契约 | `18 passed; 0 failed` | 通过 |
| 元典余额后端 | `6 passed; 0 failed` | 通过 |
| 前端类型检查 | `tsc --noEmit` | 通过 |
| 前端生产构建 | Vite 7.3.3，2878 modules transformed | 通过 |
| V081 前端专项 | UI foundation、记忆、设备同步、Skills 包管理 | 全部通过 |
| 迁移 | 空库全量、0054→0058 升级、0059 强化契约 | 通过 |
| 敏感边界 | 固定白名单、数据库 CHECK、显式敏感字段拒绝 | 通过代码级检查 |
| 差异卫生 | 定向及整合 `git diff --check` | 无 whitespace error |

本表仅转述现有报告中已实际执行的结果，不把未执行的全量 Clippy、正式数据库
升级、真实在线服务、安装包构建或发布流程写成通过。

## 五、开发完成后的外部验收与发布边界

### 外部运行时待验

- 真实 NAS 挂载及两台物理电脑端到端同步；
- 真实元典账号在线余额与官方页面核对；
- Windows Tauri GUI 的人工视觉、交互和危险确认路径检查；
- 合法/恶意 Skills 文件在正式 GUI 中的导入、导出和回滚 smoke；
- 正式业务数据库升级前的备份、`quick_check`、表计数及抽样数据前后比对。

### 尚未发生，禁止声称已完成

- 未确认已执行正式安装包构建；
- 未确认已生成或签名 NSIS/MSI；
- 未确认已生成并签名 updater manifest；
- 未确认已上传发布附件或更新下载哈希；
- 未确认已创建 Git tag、GitHub Release 或向用户发布 v0.8.1。

## 六、最终判定

**v0.8.1 开发总计划的代码实现与现有自动化证据已覆盖完成。**

后续应进入用户环境验收与发布准备，而不是继续把已接线能力登记为开发缺口。
其中真实 NAS/物理双机验收必须等待用户完成 Windows 挂载；在该验收完成前，
只能表述为“设备同步代码和模拟双库契约已完成”，不能表述为“真实双机已验收”。

同理，当前 `package.json` 与 `tauri.conf.json` 已配置版本 `0.8.1`，但这不代表
安装包、签名、updater 产物或发布已经发生。
