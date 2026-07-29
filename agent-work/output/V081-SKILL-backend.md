# V081-SKILL 后端与内置方法包实施报告

> 日期：2026-07-29
> 范围：迁移、Rust 受控注册表、五个方律内置方法包及定向测试
> 状态：已完成，待主控集成 Tauri 命令、前端类型、设置界面和 Native AI 注入

## 一、完成内容

### 1. 数据模型

新增 `0056_legal_skill_packages.sql`：

- `legal_skill_packages`：slug、语义版本、来源、启停/隔离状态、规范化 manifest、完整文本包和 SHA-256；
- `legal_skill_bindings`：领域＋任务默认绑定，同一组合仅允许一个默认包；
- `legal_skill_revisions`：manifest、正文和哈希快照；
- `legal_skill_run_audits`：只记录 run、slug、version、hash、选择来源和截断状态；
- `legal_skill_import_audits`：注册、启停、绑定、升级/回滚/隔离等动作审计。

迁移不保存具体案件事实，不授予任何工具权限。

### 2. Rust 受控注册表

新增 `src-tauri/src/chat/legal_skills.rs`，并在 `chat/mod.rs` 注册模块。主要能力：

- 只接受已经读取为 UTF-8 的相对路径＋文本内容，不接收任意本地绝对路径；
- 只允许根目录 `manifest.json`、`SKILL.md` 和 `references/`；
- 只允许 `.json/.md/.txt`，拒绝脚本、二进制、隐藏路径、路径穿越和布局外文件；
- 单包 512 KB、正文 128 KB、引用 20 个、导入包 32 个上限；
- 校验 slug、语义版本、领域、现有 task type 和已知工具兼容声明；
- `requested_tools` 只保存兼容性声明，代码中不据此注册、放行或调用工具；
- 内容统一换行并按稳定路径顺序计算 SHA-256；
- 同 slug/version/同哈希幂等；异哈希将既有版本置为 `quarantined` 并拒绝覆盖；
- 五个内置 slug 为保留标识，导入包不得冒充；
- 启停、默认绑定、兼容选择和运行审计均有稳定接口；
- 每次最多返回一个主方法包；无用户默认时按内置包稳定顺序选择兼容包；
- 单次正文预算 4,000 字符，超限截断并写入运行审计。

### 3. 五个方律内置方法包

新增：

1. `fanglv-criminal-defense-cn`：刑事辩护闭环；
2. `fanglv-civil-litigation-cn`：民事争议分析；
3. `fanglv-enforcement-recovery-cn`：执行与回款管理；
4. `fanglv-contract-nonlitigation-cn`：合同审查与非诉工作；
5. `fanglv-legal-research-cn`：法律检索与依据核验。

每个包均包含 `manifest.json`、`SKILL.md` 和 `references/guardrails.md`。内容按现有 Constitution、刑事深度分析、诉讼分析、合同审查增强及法律检索场景编写，没有复制上游六个简版提示词，也没有加入 Pi Runtime、AI Soul、脚本或额外权限。

## 二、安全边界

- Constitution、场景策略、工具白名单、案件隔离和律师复核始终高于方法包；
- 方法包正文不会改变 Tauri 命令、MCP、网络或文件权限；
- 不读取包外路径，不接受符号链接对象或压缩包内嵌套执行内容；
- 刑事包明确区分原始材料、抽取候选、律师确认事实和分析判断；
- 财务包只能使用本轮已授权记录，金额冲突不静默覆盖；
- 法律检索包遇授权/正文获取失败时停止补写未经核验的法条和案例；
- 运行审计不保存用户问题、案件材料或模型回答副本。

## 三、验证

执行：

```text
cargo test --manifest-path .\src-tauri\Cargo.toml legal_skills --lib --locked
```

结果：`6 passed; 0 failed; 233 filtered out`。

覆盖：

- 五个内置包全部可解析且 slug/hash 独立；
- 脚本、路径穿越、未知工具声明被拒绝；
- LF/CRLF 规范化后哈希一致；
- 注册幂等；
- 同版本异哈希隔离；
- 启用＋兼容＋唯一默认选择；
- 无显式绑定时内置包确定性回退；
- 运行审计按 run id 幂等。

补充“导入包不得占用内置 slug”后，单文件 `rustfmt --check --edition 2021` 通过；按主控要求未继续等待共享 Cargo 构建锁，该新增断言留给主控综合测试复验。

五份 manifest 另以 PowerShell `ConvertFrom-Json` 全部解析成功。目标文件 `git diff --check` 无空白错误。

## 四、主控集成提示

主控后续需：

1. 在合适的启动/首次读取路径调用 `seed_builtin_packages`；
2. 在保留文件中增加 Tauri 命令和前端类型；
3. 设置页只展示、预览、启停和绑定，不能把 `requested_tools` 变成授权开关；
4. Native AI 接入时把方法正文放在 Constitution 和 scene/task workflow 之后、案件记忆之前；
5. 每次任务最终选定后调用 `audit_run`；
6. NAS 同步只同步规范化包内容、哈希、状态和已确认绑定，不同步绝对导入路径。

## 五、实际文件范围

- `src-tauri/migrations/0056_legal_skill_packages.sql`
- `src-tauri/src/chat/legal_skills.rs`
- `src-tauri/src/chat/mod.rs`
- `src-tauri/resources/legal-skills/**`
- `agent-work/output/V081-SKILL-backend.md`

未修改主控明确保留的 `src-tauri/src/lib.rs`、`src/lib/api.ts`、`src/lib/types.ts`、`SettingsModal.tsx` 和 `chat/commands.rs`，未提交 Git。
