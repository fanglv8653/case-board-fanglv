# V083-N0-SYNC｜设备同步确定性失败夹具报告

日期：2026-08-07
状态：主控已验收为 `accepted`

## 1. 结论

已在纯内存构造库范围内建立 5 个 v0.8.3 专项夹具，不修改导出、导入、隔离、审计等生产行为。

本轮得到一个需要更正原高置信推断的关键事实：

- `cases.judge_id` 外键是 `DEFERRABLE INITIALLY DEFERRED`，因此 `case → contact` 在同一事件事务中可以成功；
- 确定性失败发生在循环引用被拆到不同事件时：首包只有带非空 `judge_id` 的 case，在提交时因引用的 contact 不在当前包/接收端而失败；
- 若 contact 先于 case 单独应用，`contacts.case_id` 的非延迟外键会在语句阶段直接失败。

因此 S1 不能只调整事件内顺序；必须建立依赖闭包，保证循环两端不跨 500 条分包边界，或在写 NAS 事件前失败关闭。

## 2. 修改文件

- `src-tauri/src/device_sync/engine.rs`
  - 仅新增 `#[cfg(test)]` 辅助：读取真实单事件上限，以及调用现有私有 quarantine/audit 函数。
  - 没有修改生产常量、SQL 或任何运行时分支。
- `src-tauri/src/device_sync/mod.rs`
  - 仅新增 `#[cfg(test)] mod v083_failure_tests;` 声明。
- `src-tauri/src/device_sync/v083_failure_tests.rs`
  - 新增 5 个只使用合成 ID/文本与 `:memory:` SQLite 的专项单元夹具。
  - 不再使用 integration test 的 `#[path = "../src/device_sync/mod.rs"]`，避免重复引入并运行设备同步模块内既有单元测试。
- `.agent-work/output/V083-N0-SYNC.md`
  - 本报告。

## 3. 五个夹具及当前行为

1. `cyclic_case_then_contact_succeeds_when_both_are_in_one_transaction`
   - 按现有 `case → contact` 顺序在一个事务内应用循环两端。
   - 当前预期：提交成功，case/contact 各1行，`foreign_key_check` 为0。
   - 证明：“case 先行”不是单独根因；同包可由延迟外键闭环。
2. `case_package_commit_failure_rolls_back_every_operation`
   - 首包包含带 `judge_id` 的 case 和一条无关 calendar 变更，但不含被引用 contact。
   - 当前预期：两个 `apply_incoming` 语句先成功，事务提交返回 SQLite 787；case、calendar、applied_operations、entity_revisions 全0行。
   - 证明：跨包缺依赖在 commit 阶段失败，现有事件事务能保持零部分写入。
3. `contact_before_case_is_rejected_without_partial_write`
   - 空接收端首先应用带 `case_id` 的 contact。
   - 当前预期：返回现有 `SYNC_DATABASE`，回滚后 contact 与 applied_operations 均0行。
   - 证明：反向依赖不能用简单的“contact 先行”解决。
4. `repeated_package_quarantine_is_duplicated_and_audit_can_still_say_succeeded`
   - 对同一合成 `group_id + source_path + reason_code` 连续调用现有 quarantine 两次，再写入现有 `sync_once/succeeded` 审计且 `quarantined=2`。
   - 当前预期：隔离表新增2行，审计表允许 `succeeded` 1行。
   - 证明：重复隔离和成功语义缺口是可执行、可断言的现状。
5. `default_windows_split_case_contact_dependency_at_501_and_1001`
   - 直接读取生产常量500与 registry 中 `case < contact` 顺序，用合成实体序列建模500/501/1000/1001。
   - 当前预期：包数分别为1/2/2/3；序列末尾循环两端在500与1000时同包，在501与1001时被拆包。
   - 证明：固定500条窗口存在确定性依赖切断边界。

最窄定向命令的预期计数：`5 passed / 0 failed / 0 ignored`。

## 4. 验证状态

- 首次版本使用 integration test `#[path = "../src/device_sync/mod.rs"]`。主控实际执行 `cargo test --test device_sync_v083_failure_fixtures -j 1` 得到 `15 passed`：其中只有5项本任务夹具，另外10项为被重复引入的既有模块单元测试。该版本因计数不精确被退回。
- 本返工已删除该 integration 文件，转为 `device_sync::v083_failure_tests` 专用 `#[cfg(test)]` 单元模块。
- 返工后静态检查：新单元测试文件经 `rustfmt --edition 2021 --config skip_children=true,newline_style=Native` 处理；指定文件 `git diff --check` 退出码0；静态计数专用模块中 `#[test]/#[tokio::test]` 共5项。
- 本返工按主控要求不运行 Cargo；不报告返工后的 Cargo 通过。

建议主控串行执行：

```powershell
$env:PATH = 'C:\Users\William Feng\.cargo\bin;' + $env:PATH
$env:CARGO_INCREMENTAL = '0'
Set-Location 'D:\CodexWorkspace\008案件看板应用\case-board-v0.8.3-dev\src-tauri'
cargo test --lib device_sync::v083_failure_tests -j 1
```

## 5. S1 实现不变量

1. 始终保持 `PRAGMA foreign_keys=ON`，不用关闭外键规避失败。
2. 事件中任一操作/最终引用校验失败时，业务表、revision、applied_operation、member sequence 一并回滚。
3. 导出前建立 case/contact 依赖闭包；不允许500条边界切断循环两端。依赖闭包超容量且无安全拆分方案时，在写入 NAS 前失败关闭。
4. 被引用实体既不在当前依赖闭包、也不在接收端时，结构化返回 `SYNC_PACKAGE_DEPENDENCY_MISSING`，不将普通 `SYNC_DATABASE` 或中文文案作程序分类依据。
5. 同一活动隔离键只保留1条，更新 `retry_count/last_seen_at/last_error_code`；历史已解决记录保留，“隔离数”只计活动记录。
6. 确定性失败第一次隔离后自动暂停，返回 `SYNC_PACKAGE_QUARANTINED` / `SYNC_GROUP_AUTO_PAUSED`，不再由后台无限重试。
7. 只要本轮有活动隔离，`sync_once` 与审计均不得写 `succeeded`，也不得推进“最近业务同步成功时间”。

## 6. 遗留风险

- 返工后的5个专用单元夹具尚待主控执行上述最窄 Cargo 命令；预期只收集5项，尤其需要确认 SQLite 延迟外键在 sqlx `Transaction::commit` 失败后连接回收与回滚时序符合断言。
- 本夹具证明的501/1001边界是基于当前500常量和 registry 顺序的确定性模型；S1 还需对真实 outbox 查询、依赖感知分包和两事件重放增加实现级回归。
- 本 worker 未解密现场事件，因此只确认“源码/数据库形状可稳定导致同类 SQLite 787”，不把某一具体现场操作写成已验证事实。

## 7. 安全声明

本轮未读取或修改正式 SQLite、NAS 目录、同步组、成员密钥、飞书 Base、凭据或业务正文；未创建迁移，未修改生产路径，未提交 Git。
