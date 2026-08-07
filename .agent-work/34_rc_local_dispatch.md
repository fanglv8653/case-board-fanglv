# 34 V083-RC-LOCAL 唯一写入与构建任务

## 本轮必须完成

1. 将 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、根 `Cargo.lock` 的项目版本最小同步为 `0.8.3`，在 `CHANGELOG.md` 增加可审计的 0.8.3 条目；不改 `release/latest.json`。
2. 新增真实 pre-0063 临时库升级夹具：只应用 0001—0062，插入脱敏标记，再走生产初始化升级至 0063；断言迁移行/最大版本、标记保留、重复重开幂等、quick ok、FK 空。
3. 不得猜测或实现历史 checksum allowlist。继续验证所有未知 mismatch fail closed，并把“来源核验旧 checksum 正向兼容”标记为 `blocked_external/pending_verified_input`。
4. 新增临时双端+临时 mounted-folder 的 RC 综合反例：A→B、B→A 第二轮、无变更重复幂等、双方 canonical 投影/quick/FK/pending/conflict/active quarantine 终态；在同一环境用生产 `sync_once` 证明确定性坏包隔离、显式 resume、修复重放和恢复收敛。测试凭据必须精确清理，不能枚举或触碰正式凭据。
5. 运行 Node、TS、Vite、Cargo check、全目标 Clippy、Windows Rust 清单、source gate、release-resume、升级工具 Python 契约、截图 self-test 和 release gate 可本地执行项。
6. 尝试不需秘密的 release executable/bundle 前置检查。缺 updater 私钥时不得生成伪签名、不得改成 unsigned 成功、不得改 `latest.json`；记录为 `blocked_external`。

## 边界

- 只允许一个窗口写源码/测试/版本和串行构建。
- 不访问正式数据库、NAS、同步组、凭据、飞书或 GitHub；不 push/tag/Release。
- 不创建 `0064`，不改迁移或 M1 checksum 策略。
- 不把本地双端模拟、unsigned 产物或安装脚本契约冒充物理双端、正式签名及在线升级。
- 不 commit；主控验收后统一提交。

## 交付状态

- 本地可执行门禁全部通过时提交 `submitted_for_review`。
- 历史 checksum、正式签名资产、远端 Release/latest、0.8.2 实机在线升级和物理双端只能列 `blocked_external`，并写出最小所需资源与授权。
