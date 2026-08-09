# V083-CANDIDATE-SCOPE-REVIEW 候选冻结前范围审计

## 结论

**候选范围审计通过，可按显式 allowlist 冻结候选。**

- P0：0
- P1：0
- P2：2

未发现越界产品代码、迁移 SQL、正式备份文件、数据库 sidecar、凭据/密钥、常见高置信 token 或业务正文进入候选范围。版本边界正确，tracked diff-check 通过。

## 1. 实质代码范围

`git diff --name-status/--stat` 识别的实质产品 Rust 差异只有：

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`

`git status` 另把 27 个 `src-tauri/src/db/*.rs` 标为 modified，但这些文件逐一执行 `git diff --quiet -- <file>` 均为 exit 0，`git diff --name-only` 也不包含它们；属于工作区行尾/索引状态噪声，不是实质源码补丁。

正式验收工具的实质范围为固定目录下 6 个文件：

- `scripts/windows-upgrade-validation/Invoke-UpgradeValidation.ps1`
- `scripts/windows-upgrade-validation/README.md`
- `scripts/windows-upgrade-validation/db_audit.py`
- `scripts/windows-upgrade-validation/tests/test_db_audit.py`
- `scripts/windows-upgrade-validation/tests/test_tooling_contract.py`
- `scripts/windows-upgrade-validation/tests/test_formal_stages.py`（untracked）

工具源码静态检索未发现应用启动/停止/删除、安装或网络调用实现；变化内容限于升级审计、manifest/artifact 绑定、数据库指纹、sidecar 处理及合成测试。其独立 R3 复核记录为 P0=0、P1=0、P2=0。

其余 tracked/untracked 内容均位于 `.agent-work`，属于派工、量表、报告、review、线程状态或审计辅助证据；另有既有 RC 本机日志的一行删除。未发现其他产品目录或无关源码的实质 diff。

## 2. 迁移与备份边界

- `git diff --name-only -- src-tauri/migrations`：空。
- 无新增/修改 0036、0064 或其他迁移 SQL。
- tracked 文件中备份/数据库类扩展名计数为 0。
- 111 个 untracked 文件共约 300 KB，扩展名仅为 107 个 `.md`、2 个 `.py`、1 个 `.json`、1 个 `.ps1`；不存在 `.db/.sqlite/.wal/.shm/.journal/.bak/.backup/.zip/.7z/.rar/.exe/.msi`。
- untracked 路径中不存在 data、backup、raw-data、main-only 目录或 `caseboard.db` 文件。
- workflow 审计 JSON登记的 snapshot 路径当前文件不存在、未受 Git 跟踪，并被 `.gitignore` 命中；正式备份本体没有进入工作树候选清单。

正式 DB 审计 JSON只包含 SQLite 状态、迁移 tuple/checksum、schema hash、表行数及按表的 `rows/primary_key/sha256` 投影；未包含业务行正文。正式备份/审计报告包含文件级大小、时间和哈希证据，但不包含备份文件本体、密钥或业务内容。

## 3. 敏感内容检查

对全部 tracked diff 文件和 untracked 候选文本执行高置信模式扫描，结果均为 0：

- PEM/OpenSSH/RSA/EC private key；
- AWS access key；
- GitHub token；
- OpenAI `sk-` key；
- JWT；
- 带长值的 `api_key/app_secret/client_secret/password/access_token` 赋值。

未发现 `.env`、证书、私钥、凭据容器或可执行安装包。未发现案件名称、当事人陈述、聊天正文、文书正文等业务内容进入代码/证据候选。

## 4. 版本边界

应用候选版本一致为 `0.8.3`：

- `package.json`：0.8.3；
- `src-tauri/Cargo.toml`：0.8.3；
- `src-tauri/tauri.conf.json`：0.8.3；
- `Cargo.lock` 中 `caseboard` package：0.8.3。

`release/latest.json` 仍为 `0.8.2`，且工作树无该文件差异；因此当前候选尚未越权切换 latest/updater 发布指针，版本边界正确。

## 5. diff-check

- 全部 tracked worktree：`git diff --check` exit 0，仅输出既有 LF/CRLF 提示。
- 111 个 untracked 文件逐一用 `git diff --no-index --check -- /dev/null <file>` 检查：94 个无问题；17 个仅报告 `new blank line at EOF`，无 trailing whitespace、space-before-tab 或其他 whitespace error。

## P2-01：状态噪声要求显式 staging

27 个无实质 diff 的 Rust 文件仍出现在 `git status`。冻结候选时应使用本报告列出的显式文件 allowlist并在 staged 后重新运行 `git diff --cached --name-status/--check`，不要依赖宽泛 `git add -A`。该问题当前未形成补丁，不属于 P1。

## P2-02：部分 workflow 证据有多余 EOF 空行

17 个 untracked workflow/报告文件仅存在 EOF 空行提示，其中包括一个 workflow-local 补充备份脚本；不影响语义、秘密边界或 tracked diff-check。若主控要求“staged 全范围 diff-check 绝对零提示”，应在冻结阶段由相应文件所有者统一清理后再复核；本只读任务未修改这些文件。

## 候选冻结建议

1. 仅显式 stage：COMPAT36 三个 Rust 文件、正式验收工具 6 文件、经主控选择的 `.agent-work` 派工/报告/review/线程证据。
2. 不 stage 任何被忽略路径、临时审计快照、备份目录或工作区状态噪声。
3. staged 后复核：`git diff --cached --name-status`、`git diff --cached --check`、迁移目录零差异、版本四处为 0.8.3、`release/latest.json` 仍为 0.8.2。
4. 动态 Cargo/Windows Rust/工具测试门禁由主控已有串行流程完成；本任务未运行 Cargo。

## 操作边界

本任务只读检查 Git、源码、版本文件和仓库内 workflow 证据；未打开正式数据库或正式备份，未运行 Cargo/Node/构建，未访问凭据、NAS、飞书、网络或发布资源。唯一写入为本报告及 workflow 状态更新。
