# 21 V083-M1 主控验收

## 结论

`V083-M1` 的数据库迁移 fail-closed 主体 accepted，允许进入 `V083-S1`。

本阶段把既有数据库启动流程收紧为“sidecar 文件门禁 → immutable 只读预检 → 稳定分类 → RW/WAL + sqlx migrate”。任一 WAL/SHM、未知 checksum、未知已应用版本、失败迁移行、迁移历史缺失/为空但存在用户 schema、关键 sentinel 缺失，均在创建读写连接前阻断。

## 退回与修正

1. 首轮因“缺少迁移表但有用户 schema”仍可进入读写迁移而退回；修正为只放行真正空 schema，并增加逻辑和物理指纹断言。
2. 第二轮独立审计发现“空迁移表 + 用户 schema”绕过，以及普通 SQLite 只读连接可能创建或改变 WAL/SHM；修正为任一 sidecar 在首次 SQLite 连接前阻断，仅对无 sidecar 主库使用 immutable 只读预检。
3. 删除未具备可信旧值的 checksum allowlist、更新计划和 CAS 写框架；组合错误优先级冻结为失败行/未知版本/history gap → sentinel → checksum。
4. 实现线程曾因 crate 级 rustfmt 递归触及范围外文件；发现后立即中断并精确恢复，最终产品源码差异仅保留 4 个授权文件，未覆盖用户改动。

## 主控实测

- `cargo check --lib -j 1`：通过。
- `cargo clippy --lib -j 1 -- -D warnings`：通过。
- Windows Rust 清单：库测试 280 passed / 0 failed / 3 ignored；设备同步契约 23/23；3 个测试可执行文件通过。
- M1 迁移安全专项：12 项全部通过，包括 DB+WAL+SHM、缺 SHM、仅 SHM、空迁移历史、未知 checksum、sentinel+checksum 组合优先级和指纹不变。
- `pnpm test:logic`：119/119。
- `pnpm build`：通过，仅既有 chunk size warning。
- `pnpm validate:source`：通过。
- 两次独立静态审计：第三轮未发现剩余 P0/P1。

## 未冒充完成的边界

- 当前没有有来源、可核验的笔记本真实旧 checksum；所有 checksum mismatch 一律返回 `DB_MIGRATION_CHECKSUM_UNKNOWN`，不会自动改写迁移表。
- 历史 checksum 自动兼容标记为 `pending_verified_input`。RC 前只能在取得只读、脱敏、可追溯的旧迁移元数据后另建受控兼容任务；否则发布边界维持“原生提示 + 完整备份 + 隔离恢复”，不得宣称旧谱系已自动兼容。
- Windows 原生对话框视觉确认、隔离副本升级、`quick_check`/`foreign_key_check` 和正式恢复演练属于 RC 门禁。

## 安全边界

未读取或修改正式数据库、NAS 同步目录/同步组、成员密钥、飞书 Base、凭据或业务正文；未 push、未发布。
