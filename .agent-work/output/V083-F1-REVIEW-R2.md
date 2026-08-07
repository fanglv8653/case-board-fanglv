# V083-F1-REVIEW-R2｜F1 返工最终独立复审

- 复审时间：2026-08-07
- 执行线程：`worker-f1-review-r2`
- 复审范围：R2 全部差异、设备同步生产入口、共享生命周期协议、active orphan 缺 inbox 恢复、主库/契约模块边界、迁移与越界差异、全量门禁
- 最终结论：**建议主控接受。P0=0，P1=0，P2=0。**
- 操作边界：仅只读检查产品源码、测试与迁移；未修改产品代码、测试或迁移。

## 一、31 号量表逐项结论

### 1. 生产入口与单一共享协议：通过

- 手动命令 `device_sync::commands::run_device_sync` 与后台 `device_sync::scheduler::start` 均调用唯一生产入口 `engine::sync_once`。
- `sync_once` 先取得 `SYNC_RUN_LOCK`，再通过 `run_device_sync_action` 取得 `BINDING_LIFECYCLE_LOCK`；共享锁成功后才执行 `mark_sync_attempt` 和 `sync_once_inner`，所以 busy 在 mark、导入和业务表应用前返回。
- `operations::resolve_operation_conflicts` 整体进入同一 `run_device_sync_action`；`KeepRemote` 的 `apply_upsert` 不存在旁路。
- 设备同步对 `feishu_sync_links` 的包导入只由上述 `sync_once` 链路到达；生产调用点搜索未发现第二个 `sync_once`、`apply_incoming_package` 或 `resolve_operation_conflicts` 入口绕过共享协议。
- 显式生命周期/网络动作 `delete_case`、bind、unbind、ignore、restore、pull、字段处理、明细处理均在 `lib.rs` 的生产命令入口通过 `run_explicit_action`。旧 `FEISHU_WRITE_LOCK` 已消除。

### 2. 锁序、自锁与可重试 busy：通过

- 固定锁序为 `SYNC_RUN_LOCK -> BINDING_LIFECYCLE_LOCK`；显式动作不取得 `SYNC_RUN_LOCK`，未形成反向边。
- `sync_once_inner` 不调用 `resolve_operation_conflicts`；`resolve_operation_conflicts` 也不回调 `sync_once` 或显式动作，未发现共享锁重入/自锁路径。
- 所有共享锁均使用 `try_lock`：显式动作为 `FEISHU_WRITE_IN_PROGRESS`，设备同步为 `SYNC_FEISHU_LIFECYCLE_BUSY`；后台调度将后者作为可重试 busy 忽略，不误记确定性失败或自动暂停。

### 3. 真实并发反例：通过

- `r2_barriers_cross_explicit_device_sync_and_lifecycle_production_entries` 是多线程 barrier 测试，穿过生产 `engine::sync_once`、生产 `resolve_operation_conflicts(KeepRemote)` 与生产共用的显式动作协调入口，而不是单独测试锁对象的 `try_lock`。
- 显式动作持锁时，设备同步和 KeepRemote 均在闭包/数据库动作前返回 lifecycle busy；设备同步经生产协调函数持锁时，显式闭包调用次数保持 0。
- 测试释放 barrier 后进入真实 `sync_once` 后续并得到预期 `SYNC_NOT_FOUND`，证明 gate 位于生产编排链而非孤立测试 helper。

### 4. active orphan 缺 inbox 恢复：通过

- `unbind_case` 先在事务内验证本地 case 是否存在；仅当 active link 已孤立时调用 `ensure_orphan_recovery_inbox`。正常非 orphan 缺 inbox 仍由 `case_link_inbox` 返回 `FEISHU_BINDING_NOT_FOUND`，且事务回滚、指纹不变。
- 孤儿恢复在同一事务中完成：合成 `pending_binding / bound_case_id=NULL / auto_bind_suppressed=1` inbox、失效 pending field/entity candidates、dismiss pending conflicts、归档 active link、写入 `previous_case_id=NULL` 的解绑审计并提交。
- 动态反例验证 `PRAGMA foreign_key_check` 为空，飞书 HTTP spy 为 read=0/write=0；恢复后孤儿不再出现在 bound UI 数据中。
- UI 对 active orphan 明示“本地案件已删除 / 绑定异常 / 解除孤立绑定”，该动作直接调用已修复的本地 unbind；pull 已归档 orphan 不再作为 bound 动作残留。

### 5. 主库与 `device_sync_contract` 模块边界：通过

- 主库通过 `lib.rs` 的 `#[cfg(test)] mod feishu_binding_lifecycle_tests` 编译并执行 R2 barrier 测试。
- `device_sync_contract` 通过 `#[path = "../src/device_sync/mod.rs"]` 使用生产设备同步模块；新增 coordinator 为 `pub(crate)`，契约目标成功独立编译并执行 59 项，未重复装载主库专属 barrier 测试，也未出现可见性/模块路径错误。

### 6. 无 schema 扩张、迁移和 sentinel：通过

- `src-tauri/migrations` 差异为 0，目录最高仍为 `0063_device_sync_quarantine_lifecycle.sql`。
- 精确搜索 `0064`、`M64`、`F1_SENTINEL` 为 0；未放宽外键、候选授权或删除设备同步能力。

### 7. 差异范围：通过

- 产品差异只涉及 F1 需要的案件删除/飞书绑定事务、候选授权与 UI、共享设备同步协调器、错误类型、测试 spy 和相应测试。
- `git diff --check` 通过；未发现 F1 以外的产品语义改动。工作树中的 `.agent-work` 文件均为本轮调度、报告和线程状态。

## 二、独立门禁证据

| 门禁 | 结果 |
|---|---|
| `pnpm test:logic` | **123 passed / 0 failed**，44 个 Node 文件；含 4 项 orphan UI/R2 静态契约 |
| `scripts/run-windows-rust-tests.ps1` | **3 个测试可执行目标通过** |
| Rust 主库 | **335 passed / 0 failed / 3 ignored**；含 R2 barrier 与 orphan missing inbox 动态测试 |
| Rust bin | 0 tests，成功 |
| `device_sync_contract` | **59 passed / 0 failed** |
| Rust 总计 | **394 passed / 0 failed**（忽略 3 个需真实飞书/OAuth 的既有 live tests） |
| `git diff --check` | 通过 |
| 迁移差异 / 0064 / sentinel | **0 / 0 / 0** |
| `pnpm validate:source` | 通过：`source=0.8.2`、`published=0.8.2`、license 校验通过 |

说明：首次启动 Windows Rust 包装器时，前一次命令超时留下的清理进程与后一轮共用同名临时 manifest，造成一次 `mt.exe` 环境竞态，测试尚未执行。清除并发后原命令完整重跑，以上 394 项全部通过；该事件不是编译或测试失败。

## 三、问题分级与验收建议

- P0：无。
- P1：无。
- P2：无。
- 验收建议：R2 已关闭前次复审的三个拒绝项，并满足 `.agent-work/31_f1_remediation_acceptance.md` 的接受条件；建议主控将 `V083-F1-R2` 与本独立复审一并接受。最终 `accepted/rejected` 仍由主控决定。
