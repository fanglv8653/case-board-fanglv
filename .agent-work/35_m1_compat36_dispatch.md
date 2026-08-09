# 35 V083-M1-COMPAT36 实现派发

## 目标

仅兼容本机正式 0.8.2 数据库中已由两次独立正式审计确认的缺失迁移 36；其余未知版本、checksum 或结构继续在写入前失败关闭。

## 唯一可信 tuple

- version: `36`
- description: `feishu reminder runs`
- success: `1`
- stored SQLx SHA-384: `84F859102447ACB5DBEE9E179A0AE3493D7ED2483B28A447BDF0F4F9360CC2399FC1F7AA08CBA2F0BE50F444F4841480`
- 必须存在精确定义的 `feishu_reminder_runs`：`sent_date TEXT PRIMARY KEY NOT NULL`、`sent_at TEXT NOT NULL DEFAULT datetime('now')`、`item_count INTEGER NOT NULL DEFAULT 0`
- 其余已应用迁移必须仍与当前嵌入 checksum 一致，现有全部 schema sentinel 必须通过。

## 实现边界

1. 不新增、恢复或重编号 `0036` 迁移文件，不修改正式 `_sqlx_migrations`，不新增 0064。
2. 只允许上述 tuple 作为“已应用但当前未嵌入”的兼容例外；任何字段、checksum、description、表结构或其他版本不同均返回既有稳定错误码并在写入前阻断。
3. SQLx 写入阶段只有在只读预检完整通过后才可容忍已证明的 missing migration；不得用全局放宽替代预检。
4. 新增正反例：正式形状可升级到 0063且版本36原记录不变；错误 checksum/description/缺表/错列/额外未知版本均失败且物理文件不变；正常当前谱系与全新库不回归。
5. 只修改迁移安全、初始化及对应测试/报告；不得读取或修改正式数据库、WAL/SHM、凭据、NAS、飞书或发布状态。

## 门禁

- 最窄兼容测试、全部迁移安全测试、Cargo check、Clippy `-D warnings`、Windows Rust 全量。
- 报告必须列出兼容谓词、失败优先级、测试计数和正式资源零访问声明。
