# V083-FORMAL-RELEASE-GATE｜0.8.3 正式远端签名发布只读门禁

- 核验日期：2026-08-09
- 逻辑线程：`worker-formal-release-gate`
- 核验边界：只读 Git/GitHub API、Actions 配置、secret 名称存在性、本机 updater key 文件存在性/ACL、公钥、发布脚本与离线门禁
- 禁止动作落实：未 push、未创建/推送 tag、未触发 workflow、未创建 Release、未上传资产、未修改 `release/latest.json`，未读取或输出私钥/密码
- 当前结论：**NO-GO／正式发布仍为 `blocked_external`；发布链结构可行，但当前提交、tag、产物和远端精确 CI 尚未达到执行条件。**

## 一、GitHub API 与配额

| 项目 | 只读结果 |
|---|---|
| GitHub CLI 登录 | 有效，当前账号 `fanglv8653`，HTTPS，具备 `repo`、`workflow` 等必要 scope |
| API Core 配额 | `5000 / 5000` remaining；**无配额阻塞** |
| 仓库 | `fanglv8653/case-board-fanglv` |
| 默认分支远端 SHA | `76e4788627bef621c500a3f82c5c63f6b21dcbed` |
| main 保护 | GitHub API 返回 `protected=false` |
| `v0.8.3-fanglv` tag | 本地不存在；GitHub Git Ref API 与 `ls-remote` 均不存在 |
| 对应 Release | GitHub API 返回 404，不存在 |

认证仅通过 `gh auth status` 和最小 API 请求验证；报告未记录 token，命令输出中的 token 也只有 CLI 自带掩码。

## 二、origin/main 与当前分支分叉

- 当前分支：`fix/v0.8.3-data-safety`
- 当前 HEAD：`e966a3a12458749d81f291efc3d59836bb434adb`
- 远端 `origin/main`：`76e4788627bef621c500a3f82c5c63f6b21dcbed`
- GitHub branch API、`git ls-remote origin main` 与本地 tracking ref 三者 SHA 一致。
- `origin/main...HEAD` 为 `behind 0 / ahead 13`，merge-base 正是远端 main；从提交图看具备快进关系。
- 远端尚无 `fix/v0.8.3-data-safety` 分支，因此当前候选 HEAD 没有 GitHub PR/branch CI 记录。最近一次 main CI 成功对应旧 SHA `76e4788`，不能替代 `e966a3a` 的远端验收。

当前工作树的产品/发布源相对 HEAD 无未提交变化；dirty/untracked 均在共享 `.agent-work`，包括其他正式 Gate 的线程状态和临时 DB WAL/SHM。任何正式远端操作都应在独立、干净的 release checkout 中进行，避免把共享调度现场误当发布输入。

### 当前提交范围的一个本地红灯

`git diff --check origin/main..HEAD` 当前失败：

```text
.agent-work/output/V083-RC-LOCAL.device-test.stdout.log:6: new blank line at EOF.
```

该文件由 `b91e691` 引入，不影响二进制语义，也不会被现有 Actions source gate 检出，但意味着“候选提交范围 diff check 全绿”这一发布卫生条件尚未满足。应由主控在正式候选 SHA 冻结前有界修正并重跑；本线程按只读要求未修改。

## 三、Actions workflow 核验

GitHub API 返回两个 active workflow：

| Workflow | 路径 | 状态 | 远端 main 与当前 HEAD 文件一致 |
|---|---|---|---|
| Build Windows | `.github/workflows/build-windows.yml` | active | 是，Git blob SHA 完全相同 |
| CI | `.github/workflows/ci.yml` | active | 是，Git blob SHA 完全相同 |

关键门禁静态成立：

1. `Build Windows` 只允许 `workflow_dispatch` 并要求 `release_tag`（`.github/workflows/build-windows.yml:8-13`）。
2. workflow 明确检查两个 updater signing secret 非空（第 57-69 行），构建 NSIS 时注入同名 secret（第 73-80 行）。
3. 产物阶段要求唯一 `*-setup.exe` 与同基名 `.sig`，随后对最终安装包字节执行 updater minisign 验证（第 118-144 行）。
4. Authenticode 单独记录并允许 `NotSigned`，不会把 updater minisign 冒充 Windows 代码签名（第 146-154 行）。
5. workflow 仅生成 `latest.json` draft 并上传 Actions artifact（第 156-172 行），不会自行创建 Release 或改 main。
6. `CI` 对 main push/PR 运行 Node、TS/Vite、Cargo check、Clippy、Rust test 和 Windows 发布工具测试；但 `Build Windows` 本身不跑 Clippy，因此安全顺序必须先让确切发布 SHA 的 CI 通过，再打 tag。

远端 main 未启用 branch protection，技术上可直接推送；这不是授权，也不是安全门禁。正式流程必须依靠 PR/精确 SHA 核对、CI 绿灯和发布脚本的 `ExpectedCommit/ExpectedMainCommit` 防漂移，不得因“可直接推”跳过验收。

## 四、GitHub repo secrets：仅存在性

使用 `gh secret list --json name` 后只在内存中比对目标名称，未输出其他 secret 名称或任何值：

| Secret 名称 | 名称存在 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 是 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 是 |

该结果只证明 GitHub Actions 配置槽存在；不能证明值可解密、私钥与应用公钥匹配或构建一定成功。真实性必须由 `Build Windows` 生成 setup/.sig 后的最终字节 minisign 验证闭合。

## 五、本机 updater secret 文件、ACL 与公钥一致性

仅核验文件元数据和 ACL；**未读取私钥文件内容**。

| 项目 | 结果 |
|---|---|
| 私钥文件 | `D:\CodexWorkspace\_secrets\case-board-fanglv\updater.key` 存在，348 bytes |
| 公钥文件 | 同目录 `updater.key.pub` 存在，152 bytes |
| secret 目录 ACL | 继承已关闭；仅当前用户与 `NT AUTHORITY\SYSTEM`，均为 FullControl |
| 私钥文件 ACL | 从上述受保护目录继承；有效规则仍仅当前用户与 SYSTEM，均为 FullControl |
| 私钥属性 | 普通 Archive；未声称 EFS/硬件保护 |
| 公钥一致性 | `updater.key.pub` 与 `src-tauri/tauri.conf.json:61` 的 updater pubkey 原始文本精确一致；二者均为合法 Base64，解码后也一致 |
| 公钥文件 SHA-256 | `a9a2c4e0dda49d42f02effdd6b0d2f862689bd58164c6ed00bc68a2065664c38` |
| 本进程签名环境变量 | 私钥与密码均不存在；没有把文件内容加载进环境 |

因此本机“文件与公钥配置”门禁通过，但本次会话不能执行本机签名 bundle：密码没有安全加载，且任务禁止读取 secret。远端 CI 两个 secret 名称存在，受控 workflow 是当前可行的签名入口。

## 六、版本、产物与发布脚本

### 当前版本与产物

- `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 均为 `0.8.3`，CHANGELOG 有 0.8.3 条目。
- `release/latest.json` 仍为公开的 `0.8.2`，这是 tag/资产/验签前的正确状态。
- 本次实跑 `pnpm validate:source`：通过，`source=0.8.3, published=0.8.2`。
- `target/release/bundle/nsis` 与 `release/v0.8.3-fanglv` 均不存在；当前没有 0.8.3 setup、`.sig`、latest draft 或正式发布资产。

### 发布脚本

`scripts/test-release-resume.ps1` 本次离线复跑 **28/28 通过**，覆盖有限重试、EOF/超时、远端资产一致性、manifest 与 main 防漂移。

`scripts/publish-release-resumable.ps1` 的关键安全语义成立：

- 默认/`-PreflightOnly` 为只读（第 45 行）；
- 远端 tag 必须先存在并解析到 `ExpectedCommit`，否则 fail closed（第 133-138 行）；
- 同名资产大小/hash 不一致即拒绝，不自动覆盖；
- 发布 updater manifest 必须提供完整 `ExpectedMainCommit`，且其后只允许 `release/latest.json` 提交（第 215-232 行）；
- 推送前重新读取远端 main，只允许正常快进，不强推（第 263-279 行）。

当前不能实际运行完整 `-PreflightOnly`：脚本在进入远端 Release 计划前就要求仓库内正式资产目录和远端 tag，而两者现在都不存在。这是预期的前置阻塞，不是脚本故障。

## 七、先本机、后远端的安全执行顺序

下面顺序可行，但每个写远端阶段都需要主控/用户明确授权；本 Gate 没有执行任何一步写操作。

### A. 本机冻结阶段

1. 等待正式 DB Gate、设备 Gate 等并行验收全部收口，冻结唯一发布提交；当前候选是 `e966a3a`，但只要后续修正 diff-check 或追加验收提交，候选 SHA 就必须更新。
2. 在独立干净 checkout 中确认 `origin/main` 没有新提交，重跑 `git diff --check origin/main..<candidate>`；先消除当前日志空行红灯。
3. 对最终 candidate 重跑本地 RC 全门禁；至少保留 source、Node/TS/Vite、Cargo check/Clippy/Windows Rust、release-resume、升级契约与隔离数据证据。确认 `release/latest.json` 仍为 0.8.2。
4. 只核验本机 key ACL/公钥；若选择本机签名，必须由用户在受控交互环境安全加载密码，不把私钥/密码放入命令参数或日志。否则直接使用受控 CI 签名。

### B. 远端源码与 CI 阶段

5. 获授权后先推 candidate 到远端功能分支并创建 PR；等待该精确 SHA 的 CI 全绿。由于 main 未保护，不能把“可直接推”当作通过。
6. 将 main 快进/合并到经 CI 验证的唯一发布提交，重新从 GitHub API读取 main SHA；后续把它记为 `RELEASE_COMMIT`。若采用 merge commit，tag 与所有 `ExpectedCommit` 必须改用 merge 后的 main SHA。
7. 在 `RELEASE_COMMIT` 上创建并推送 `v0.8.3-fanglv`；立即用 API/`ls-remote` 核对 tag peeled SHA 等于 `RELEASE_COMMIT`。

### C. 签名构建与本机验资阶段

8. 以 tag 为 ref 手动触发 `Build Windows`，input 也必须是 `v0.8.3-fanglv`；等待 workflow 成功。secret 有效性、公私钥匹配到此才由实际 `.sig` 验证证明。
9. 下载 Actions artifact 到仓库内隔离目录；本机再次确认唯一 setup/同基名 `.sig`、最终字节 minisign、SHA-256、0.8.3 PE 版本，并完成隔离升级。此时仍不改 `latest.json`。
10. 生成/核对 release notes 与 latest draft，再执行 `publish-release-resumable.ps1 -PreflightOnly`；参数中的 `ExpectedCommit` 必须是 `RELEASE_COMMIT`。

### D. Release 与 updater manifest 分阶段授权

11. 第一次明确授权：只用 `-Apply` 创建/收敛 GitHub Release 和资产，**不带** `-PublishUpdaterManifest`。随后从 API 和回下载独立核验资产大小、hash、签名与 URL。
12. 第二次明确授权：从干净 main checkout 执行 `-Apply -PublishUpdaterManifest -DraftManifestPath ... -ExpectedMainCommit $RELEASE_COMMIT`；脚本只允许生成一个 `release/latest.json` 快进提交。发布后再次确认远端 main 与 raw endpoint 已收敛。
13. 最后才在用户指定、有完整备份的 0.8.2 物理测试端执行在线更新、验签、安装、重启和数据库检查。由于 0.8.2 endpoint 直连 production main，发布 latest 后可能面向所有 0.8.2 客户端，必须作为最终授权步骤处理。

## 八、最终状态矩阵

| 门禁 | 状态 | 说明 |
|---|---|---|
| GitHub API 登录/配额 | `passed` | 登录有效，配额充足 |
| origin/main 远端真实性 | `passed` | API、ls-remote、tracking ref 一致 |
| 当前分支快进关系 | `passed` | behind 0 / ahead 13 |
| 当前候选提交范围 diff check | `failed_local_hygiene` | 一条已提交测试日志尾部空行 |
| Actions workflow | `passed_config` | 两个 active，远端 main 与 HEAD workflow blob 一致 |
| GitHub signing secret 名称 | `passed_existence_only` | 两个目标名称存在；值有效性未证明 |
| 本机 key 文件与 ACL | `passed_existence_acl` | 私钥未读；仅用户/SYSTEM；公钥匹配配置 |
| source gate / release-resume | `passed` | source gate 通过；离线 28/28 |
| 0.8.3 tag | `blocked_external` | 不存在，且本任务无创建授权 |
| 0.8.3 signed setup/.sig | `blocked_external` | 未构建，目录不存在 |
| GitHub Release/assets | `blocked_external` | 不存在 |
| `release/latest.json=0.8.3` | `blocked_external` | 正确保留 0.8.2，必须最后发布 |
| 正式远端发布 | `NO-GO` | 先完成 diff-check 修正、精确 SHA 远端 CI、tag、签名资产和分阶段授权 |

本报告仅建议上述顺序，不自行宣布正式发布通过。
