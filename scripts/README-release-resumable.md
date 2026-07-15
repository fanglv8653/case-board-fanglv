# 可恢复 GitHub Release 发布

`publish-release-resumable.ps1` 用于已经完成 tag 推送和签名构建之后，将本地正式资产安全收敛到 GitHub Release。默认只读且不会修改 `release/latest.json`；只有显式同时启用 `-Apply -PublishUpdaterManifest` 并通过全部清单和 `main` 防漂移门禁后，才会提交并快进推送升级清单。脚本不会读取 updater 私钥。

## 先做只读预检

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/publish-release-resumable.ps1 `
  -Repository fanglv8653/case-board-fanglv `
  -Tag v0.6.2-fanglv `
  -ExpectedCommit fb67c4750640d1efa66ef9ce3fcbbad5bf115999 `
  -ArtifactDirectory release/v0.6.2-fanglv `
  -NotesFile release/v0.6.2-fanglv/RELEASE_NOTES.md `
  -PreflightOnly
```

也可以使用 `-WhatIf` 显示写入计划。预检会查询 GitHub 登录状态、远端 tag、Release 和资产，但不会创建或上传。

## 实际执行

只有增加 `-Apply` 后才会执行 Release 创建或资产上传；默认及 `-PreflightOnly` 均保持只读。脚本遵循以下规则：

- 每次创建或上传尝试前都重新查询远端状态；
- EOF、TLS、超时、HTTP 408/429/5xx 等瞬时错误使用有限指数退避；
- 每条外部命令默认 90 秒超时，可用 `-CommandTimeoutSeconds` 在 5—600 秒内调整；
- 已存在且名称、大小、SHA-256 一致的资产跳过；API 未提供 digest 时使用 `curl.exe --http1.1 --continue-at -` 断点回下载校验；
- 同名但大小或 SHA-256 不一致时立即失败，不自动覆盖；
- 上传返回超时后，以远端实际资产状态决定继续或停止；
- Token 由 `gh` 自身凭据存储提供，不接受 Token、私钥或密码参数。

资产验证完成后，如需继续发布 updater 清单，额外提供：

```powershell
-Apply -PublishUpdaterManifest `
  -DraftManifestPath agent-work/tmp/latest-draft.json `
  -ExpectedMainCommit <发布前远端 main 的完整 40 位提交>
```

脚本会校验 draft 的版本、安装包 URL 和 signature 与已经验证的 Release 资产完全一致，再原子替换并仅提交 `release/latest.json`。推送前重新查询远端 `main`；仅允许从 `ExpectedMainCommit` 正常快进，网络超时后也会先查远端是否已经收敛。任何远端漂移都会停止，不使用强推。

## 离线测试

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-release-resume.ps1
```

测试不访问网络，覆盖 EOF 恢复、超时耗尽、指数退避、正确资产跳过、缺失资产上传计划、本地/远端错误资产 fail closed、旧 API 无 digest 时回下载校验，以及 manifest 版本/URL/签名不一致、main 漂移和已推送收敛状态。
