# V084-N0 主控验收

状态：`accepted`

## 验收结果

| task_id | 结果 | 关键复核 |
| --- | --- | --- |
| V084-N0-TODO | accepted | 0064 演进旧表、nullable case_id、软删除、复制单事务及唯一索引防重成立 |
| V084-N0-FEISHU | accepted after R2 | 修正事项时间必填冲突；字段列存在但值可空，null 哈希和 due_date 兼容投影闭环 |
| V084-N0-UPDATER | accepted after R3 | 修正同步 hook 不可 await 与命令行秘密问题；采用 helper、专用 coordinator、耐久屏障及受限 ACL 回执 |

## 主控裁决

1. 接受 `.agent-work/output/V084-N0-CONTRACT.md` 为后续唯一上游契约。
2. 迁移号固定：0064 为待办业务表，0065 起为飞书待办同步账本。
3. U1、R1、T1 可在非重叠范围推进；F1 必须等待 T1。
4. 共享注册、迁移谱系、版本和发布清单由主控串行整合。
5. RC 前不得触碰正式数据库、正式飞书 Base、NAS/Hermes 生产实例或公开发布状态。

## 基线验证

- `pnpm install --frozen-lockfile`：通过，使用 pnpm 11.19.0 和锁文件；
- `pnpm test:logic`：126/126；
- `pnpm build`：通过；
- `cargo check --lib -j 1`：通过，固定 Rust 1.96.0，冷缓存 12 分 34 秒；
- `cargo clippy --lib -j 1 -- -D warnings`：通过；
- `pnpm validate:source`：通过，source=published=0.8.3；
- `agent_workflow.py audit`：42 tasks，状态一致。

首次 Rust/源码门禁命令因子进程 PATH 中找不到 `rustc/cargo` 失败；仅在当前进程前置 `C:\Users\William Feng\.rustup\toolchains\1.96.0-x86_64-pc-windows-msvc\bin` 后复跑通过，未修改系统 PATH。
