# V081-SKILL：方法包导入、版本治理、导出与删除

## 结论

已补齐方律全局法律 Skills 的完整包管理后端，并注册为可供 Tauri 前端调用的命令。
目录/文件数组导入继续保留；新增 `.fanglv-skill.zip` 安全导入、版本历史与差异预览、
显式确认升级/回滚、ZIP 导出、导入包删除。

本次未修改前端。既有 `zip = 2` 依赖和锁文件已经满足实现需要，未新增 Cargo 依赖。

## 安全导入

`validate_package_archive(file_name, archive_bytes)` 在任何数据库写入前完成以下校验：

- 外层文件名必须以 `.fanglv-skill.zip` 结尾；
- 压缩包输入最大 1 MiB；
- 解包后文本总量最大 512 KiB；
- 文件最多 22 个，即根目录 `manifest.json`、`SKILL.md` 加最多 20 个 references；
- 只允许根目录 `manifest.json`、`SKILL.md` 和 `references/` 下的
  `.json/.md/.txt`；
- 使用 ZIP `enclosed_name` 和既有规范化路径校验双重阻断 Zip Slip、绝对路径、
  盘符、空段、`.`、`..`、隐藏段和 NUL；
- 拒绝符号链接；
- 只接受 Stored/Deflated；
- 明确拒绝 `.zip/.gz/.tgz/.tar/.7z/.rar/.bz2/.xz` 嵌套压缩；
- 依据条目声明大小预检，并使用“剩余额度 + 1”限制真实读取量，防止伪造 size 或
  高压缩比绕过 512 KiB 上限；
- 内容必须为 UTF-8，之后仍进入原有 manifest、哈希、工具声明和纯文本布局校验。

ZIP 内容只读入内存，不解压到文件系统，因此不存在临时目录残留或覆盖本地文件。

## 版本治理

- `list_package_versions` 同时返回同 slug 的版本记录和 revision 审计；
- `preview_package_diff` 返回 `manifest.json`、`SKILL.md`、references 全部变化，
  每项包含 `added/removed/modified` 及 before/after；
- `switch_package_version(..., "upgrade")` 只允许目标语义版本更高；
- `switch_package_version(..., "rollback")` 只允许目标语义版本更低；
- 只允许同一 imported slug 的不同版本切换，内置包不通过导入版本链更新；
- 目标为 quarantined/deleted 时拒绝；
- 当前默认绑定仅在目标 manifest 仍兼容对应领域和任务时原子迁移，否则整体拒绝；
- 当前版本禁用、目标版本启用、默认绑定迁移、revision 和 import audit 位于同一事务；
- Tauri 升级、回滚、删除命令都要求 `confirmed=true`，否则返回
  `SKILL_CONFIRMATION_REQUIRED`。

## 删除语义与历史审计

删除采用受控软删除：

- `origin=builtin` 永远拒绝，错误码 `SKILL_BUILTIN_DELETE_BLOCKED`；
- imported 包状态改为 `deleted`，普通列表和选择器不再展示/选择；
- 先记录其默认领域/任务的 suppression，再解除全部绑定；
- suppression 代表用户明确选择“无附加方法”，防止删除后自动回落到内置方法包；
- 后续人工重新绑定任一包时自动解除 suppression；
- `legal_skill_run_audits` 的 slug/version/content_hash 快照和 skill_id 仍保留，
  历史运行不失去审计依据；
- 同一包按原哈希重新导入时恢复为 disabled，不直接启用。

## Tauri 前端适配契约

Tauri 调用参数使用 camelCase，返回对象字段沿用 Rust/现有 API 的 snake_case。

| 命令 | 参数 | 返回 |
|---|---|---|
| `import_legal_skill_package` | `{ files }` | `LegalSkillRegistration`；既有目录导入 |
| `import_legal_skill_archive` | `{ fileName, archiveBytes }` | `LegalSkillRegistration` |
| `list_legal_skill_versions` | `{ slug }` | `{ packages, revisions }` |
| `preview_legal_skill_diff` | `{ currentSkillId, targetSkillId }` | `LegalSkillDiffPreview` |
| `upgrade_legal_skill_package` | `{ currentSkillId, targetSkillId, confirmed }` | 目标 `LegalSkillPackageRecord` |
| `rollback_legal_skill_package` | `{ currentSkillId, targetSkillId, confirmed }` | 目标 `LegalSkillPackageRecord` |
| `export_legal_skill_package` | `{ skillId }` | `{ file_name, bytes }` |
| `delete_legal_skill_package` | `{ skillId, confirmed }` | `void` |

建议前端固定顺序：

1. 选择目标版本；
2. 调 `preview_legal_skill_diff` 展示三类文件差异；
3. 用户勾选确认；
4. 再调用 upgrade/rollback 且传 `confirmed: true`；
5. 成功后刷新包列表、版本历史和默认绑定状态。

导出返回字节数组，前端应按返回的 `file_name` 保存，不应自行改变扩展名。

## 修改文件

- `src-tauri/migrations/0056_legal_skill_packages.sql`
- `src-tauri/src/chat/legal_skills.rs`
- `src-tauri/src/lib.rs`
- `agent-work/output/V081-SKILL-package-management.md`

未修改现有前端、案件数据、聊天内容、记忆或设备同步。

## 测试覆盖

在 `legal_skills.rs` 增加/扩展定向测试：

- 合法 ZIP 导入、导出、重新导入哈希一致；
- Zip Slip 拒绝；
- 符号链接拒绝；
- 嵌套压缩拒绝；
- 可执行扩展名拒绝；
- 文件数上限拒绝；
- 512 KiB 解包上限拒绝；
- 错误外层扩展名拒绝；
- manifest 与 SKILL 差异可见；
- 高版本升级、低版本回滚和默认绑定迁移；
- 删除后返回“无附加方法”，不自动回落内置包；
- 历史运行 slug/version/hash 快照保留；
- 内置包不可删除；
- 未显式确认时拒绝。

## 当前验证状态

- `legal_skills.rs` 指定文件 rustfmt：通过；
- 三个目标文件 `git diff --check`：通过，仅有共享工作树 `lib.rs` 的
  LF/CRLF 提示，无空白错误；
- 七个新增 Tauri 命令均核对为“函数定义一次、generate_handler 注册一次”；
- 按主控要求，设备同步任务仍持有共享 Cargo 构建锁，本代理未争抢构建锁。
  Rust 定向/全量测试需由主控在锁释放后统一执行；本报告不把静态检查冒充编译通过。
