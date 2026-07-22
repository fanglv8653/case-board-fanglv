# 0.7.6 最终验收

## 结果

- 状态：accepted / formal-install-passed
- 源码提交：`546e64443cc8ec9e847301c28ccac0794d37209e`
- 发布标签：`v0.7.6-fanglv`
- updater 清单提交：`ded96c99dc96c4302006312a321c68a3a6e35535`
- 正式安装程序及卸载项版本：`0.7.6`

## 自动验证

- Node 逻辑与 UI 契约：87/87
- Rust：120/120
- TypeScript、Vite、Cargo check、Clippy `-D warnings`：通过
- CI：`29888581994` 成功
- Windows 签名构建：`29888842399` 成功
- 安装包 SHA-256：`3141e640886ca33fbef9b8fd8719afa9c8a99795f050df49649a80e8b8546ec2`
- updater minisign、发布门禁及 GitHub 远端摘要：通过

## 数据库与飞书

- 迁移 1—52 成功，`quick_check=ok`。
- 首次正式启动按预期导入飞书进展 59、阶段 6、联系人 3；全部具有 `external_source=feishu` 和外部记录 ID。
- 升级前后 cases 核心字段和原人工工作记录一致。
- 重复启动不增加三类业务实体，仅追加同步审计/预览运行记录。
- 飞书侧无写请求；应用只读取“在办”案件及其关联明细。

## 证据

- 隔离升级：`agent-work/076/isolated-upgrade/20260722T033348Z`
- 正式升级最终报告：`agent-work/076/formal-upgrade-final/20260722T040910Z`
- 正式界面截图：`agent-work/076/formal-upgrade-final/20260722T040910Z/evidence/11-formal-window.png`
