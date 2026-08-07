# 22 V083-S1 派发计划

## 阶段目标

让真实形状设备同步基线可原子导入；确定性失败不得无限重试、重复新增隔离或被报告为成功。

## 串行边界

- 前置：`V083-M1` 已 accepted，提交 `47b0508`。
- 本阶段只设一个源码实现线程 `worker-s1`，避免迁移 0063、错误码和事务语义被并行改写。
- 实现提交主控验收后，再派独立只读审计；未 accepted 不进入 F1。

## 必达实现

1. 解密和完整性校验通过后，先构建事件/包内实体依赖图，再进入业务写事务。
2. `case.judge_id → contact.id` 与 `contact.case_id → case.id` 采用同一事务两阶段导入：案件首写安全延后 `judge_id`，联系人落库后补写并复验；不得关闭外键。
3. 导出端依赖感知排序和原子分组：案件与其被引用联系人不得被默认 500 条边界拆成必败包；超过容量按依赖闭包分包，无法安全分包时在写 NAS 事件前失败。
4. 依赖既不在当前闭包也不在接收端时，整包零部分写入并返回 `SYNC_PACKAGE_DEPENDENCY_MISSING`。
5. 新增且只新增 `0063_device_sync_quarantine_lifecycle.sql`：活动/已解决状态、首次/最近时间、重试次数、解决时间、最后错误码；同一 `group_id + source_path + reason_code` 的 active 隔离只更新计数。
6. 确定性包错误首次隔离后自动暂停组并返回 `SYNC_GROUP_AUTO_PAUSED`；后台调度不得继续重试，用户明确恢复后才可重放。
7. 修复后的成功重放把对应 active 隔离标记 resolved，历史记录保留，不物理删除；界面隔离数只统计 active。
8. `sync_once`、审计与 UI 对隔离/自动暂停显示失败或已暂停，不得记 `succeeded`；最近尝试时间与最近业务成功时间分离，失败不得推进成功时间。
9. 隔离详情和错误只允许安全元数据，不得展示业务正文、密钥、Token、解密载荷或绝对敏感路径。

## 允许修改范围

- `src-tauri/src/device_sync/**`
- `src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql`
- 为设备同步状态/错误显示所需的最小前端文件：
  - `src/components/settings/DeviceSyncSettingsCard.tsx`
  - `src/lib/api.ts`
  - `src/lib/types.ts`
- 如新增 Tauri 命令确属必需，可最小修改 `src-tauri/src/lib.rs`，但必须先在报告解释原因。
- 本线程自己的 `.agent-work/threads/worker-s1/**`、`.agent-work/output/V083-S1.md`。

禁止修改 M1 迁移安全文件、飞书模块、案件业务 UI、依赖、版本、发布配置和其他迁移编号。禁止接触正式数据库、NAS 同步目录/组、成员密钥、飞书或凭据。

## 最低测试矩阵

- N0 的 5 个失败夹具改为目标语义：循环闭合、跨 500 边界依赖闭包、contact 先到缺依赖、同一活动隔离去重、隔离不记成功。
- 500/501/1000/1001 边界；多案件共享/不共享联系人；包事务中途失败全回滚。
- 自动暂停后后台不再重试；明确恢复后可重放并 resolved；重复重放幂等。
- `quick_check=ok`、`foreign_key_check` 为空；active 隔离计数准确；历史 resolved 保留。
- 成功路径推进 `last_success_at`；失败只推进最近尝试时间。
- 全部使用内存/临时合成库和临时目录，不写正式 NAS。

## 交付

- 报告必须列出文件、事务不变量、错误码、测试命令/实测计数、未运行项、残余风险和范围声明。
- worker 不 commit、不 push；状态到 `submitted_for_review` 后由主控统一验收。
