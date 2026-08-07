# 32 V083-RC 集成、恢复和发布派发计划

## 顺序

1. RC-GATE：只读盘点版本源、release gate、Windows 安装/updater/signing 配置和资源缺口。
2. RC-DBSYNC-GATE：只读盘点全新库、0.8.2 副本、兼容/不兼容谱系与双设备隔离收敛夹具。
3. RC-LOCAL：唯一写入/构建窗口按两份结论完成版本准备、自动化集成、隔离双端模拟、bundle/updater 可执行门禁。
4. RC-REVIEW：独立只读复审本地证据和正式资源边界。
5. 只有本地门禁通过后，才评估正式在线一致性副本、两台物理设备、签名凭据和隔离测试 Base；缺少资源或授权时不得伪造完成。

## 本地必须完成

- Node logic、Vite build、Cargo check、全目标 Clippy `-D warnings`、Windows Rust 清单、release gate。
- 全新库、当前 0.8.2 正常形状、已知 checksum 兼容形状、不兼容谱系的启动/升级夹具；quick/FK/失败前后指纹。
- 临时数据库与临时目录中的双端设备同步两轮收敛、重复幂等、隔离/恢复门禁；不得接当前 NAS 失败组。
- 版本源一致性；如 RC 需要进入 0.8.3 候选，最小修改 package/Cargo/Tauri/updater 相关源并验证。
- 在不读取/输出秘密的前提下检查签名能力；有可用且获授权的凭据才生成签名安装包、updater 和 latest.json。

## 禁止

- 不访问或修改唯一正式数据库、当前 NAS 事件目录、同步组、成员密钥、正式飞书 Base、OAuth/API 凭据。
- 不 push、不创建 Release、不上传资产、不恢复正式设备同步。
- 不把 unsigned、测试签名或缺少物理两端验证的产物描述为可发布。
- 同一仓库构建串行执行。

## 交付

- 本地 RC 报告必须逐项区分 passed / blocked_external / not_run，并给出命令、计数、产物哈希或阻塞资源。
- 外部阻塞只在所有安全本地替代已完成后报告，并明确需要用户提供的最小授权/资源。
