# 28 V083-F1 飞书孤立绑定修复派发计划

## 冻结结论

- 不新增 `0064`，不修改任何迁移或 migration sentinel。
- 复用现有 `archived link + pending_binding/null/suppressed inbox + superseded field preview + entity preview ON DELETE CASCADE + unbind audit(previous_case_id=NULL)` 语义。
- 仅修本地案件删除、拉取预演、解绑/重绑、候选授权、预演 UI 与稳定错误码；不扩大为飞书业务功能开发。
- 不访问正式数据库、正式飞书 Base、OAuth 凭据、NAS 或真实案件数据。

## 唯一写入任务

由一个产品实现窗口统一修改源码和测试，避免 `cases.rs`、`feishu_sync.rs`、`lib.rs`、前端预演组件发生并行写冲突。迁移哨兵与 Gate 窗口只读，不参与产品代码写入。

## 最优实现顺序

1. 先建立 CE-1 至 CE-8 的失败夹具，并给 Tauri 编排层增加可计数的测试 seam/spy；测试不得连接真实飞书。
2. 抽取事务内复用的本地绑定隔离 helper：归档 active link、恢复 inbox 为 `pending_binding`、清空绑定、设置 `auto_bind_suppressed=1`、失效 pending field/conflict/entity 候选、写安全审计。
3. 将案件删除改为单一 SQLite 事务；多 link 任一缺 inbox 或清理失败均 fail closed 并整体回滚。删除不调用飞书，不改其他案件业务字段。
4. 修复历史孤立解绑；审计 `previous_case_id` 对已不存在案件必须为 `NULL`，并验证 `foreign_key_check` 为空。
5. 修复 bind/unbind/rebind 生命周期：在同一事务失效旧候选与冲突；旧候选不得因 link ID 复用跨案件执行。
6. 所有字段/明细 resolver 在取 token 或任何 HTTP 前校验 pending 状态、active link 与当前案件归属；生命周期命令与显式飞书写命令共享 `FEISHU_WRITE_LOCK` 或提供经过并发测试证明的等价 generation 协议。
7. pull 在逐条写 inbox 前识别 active orphan，逐条本地隔离后继续有效记录；整批提交为 `partial`，稳定码为 `FEISHU_ORPHAN_BINDING`，有效案件预演不得回滚，飞书写调用为 0。
8. 扩展预演 DTO/UI：孤立项显示“本地案件已删除”；只保留本地解绑入口；采用飞书、写回飞书及失效候选动作禁用或不返回。错误展示按稳定码分类，不用中文错误文本判断。
9. 运行定向反例、全量 Rust/Node、Clippy、构建、source gate、diff check；提交实现报告供独立复审。

## 必须闭合的反例

- CE-1：删除 case 时 link/inbox/candidate/audit 与 case 删除同事务，失败前后指纹一致，网络 0。
- CE-2：同批一个 orphan、一个有效绑定，run 为 partial；orphan 被隔离，有效预演仍提交，写网络 0。
- CE-3：历史 orphan 可解绑，审计不写失效 FK，`foreign_key_check` 为空。
- CE-4/CE-5：unbind/rebind 后旧字段和明细候选不可对新/旧案件执行，且在 token/HTTP 前拒绝。
- CE-6：显式飞书写与 delete/bind/unbind/rebind 不得跨绑定生命周期交错提交。
- CE-7：UI 展示删除文案与稳定码，孤立或失效项不提供采用/写回动作。
- CE-8：同一案件多 active link 且任一缺 inbox 时删除整体回滚，不产生半清理现场。

## 实施边界

- 允许对测试可注入性做最小重构，但生产网络行为不变；只有用户明确点击的既有写回动作可写飞书。
- 允许案件删除后由现有 `ON DELETE CASCADE` 清除 entity preview；本轮不要求永久保留其 superseded 历史。
- archived link 的 `local_entity_id` 可作为历史定位线索，binding audit 的受 FK 字段不得保存已删除案件 ID。
- 不 commit、不 push、不 merge；由主控验收后统一提交。
