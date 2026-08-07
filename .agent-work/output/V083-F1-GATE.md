# V083-F1-GATE｜飞书孤立绑定只读链路审计

- 审计日期：2026-08-07
- 审计范围：案件删除事务、飞书拉取/预演、解绑/重绑、绑定审计、UI 动作、稳定错误码、网络调用边界
- 审计方式：只读源码与既有事故证据核对；未修改产品代码、测试或迁移，未访问飞书或正式数据库
- Gate 结论：**当前基线不可进入 F1 验收；确认 4 项 P0、4 项 P1。** 旧事故报告描述的失败链仍能由当前代码逐行解释，且进一步发现“解绑后重绑复用同一 link 时，旧候选可能跨案件继续执行”的数据完整性缺口。

## 一、与旧事故报告的核对结论

旧报告记录了四个现场事实：案件已删除但 active link 仍在；字段处理返回 `FEISHU_REVIEW_NOT_FOUND`；拉取把失效案件 ID 回写 inbox 后返回 `FEISHU_DB_PREVIEW_WRITE_FAILED`；解绑审计写失效 `previous_case_id` 导致整个事务回滚（旧报告第 7—24、42—51 行）。当前实现与这些事实完全吻合：

1. `src-tauri/src/db/cases.rs:335-342` 的 `delete_case` 只有一条 `DELETE FROM cases`，没有显式事务，也没有处理飞书 link、inbox、pending 候选或绑定审计。
2. `src-tauri/migrations/0049_feishu_case_management_sync.sql:9-30` 的 `feishu_sync_links.local_entity_id` 是无外键的通用文本列，案件删除不会自动归档 link；同迁移第 68—85 行只会通过 `ON DELETE SET NULL` 清空 inbox 的 `bound_case_id`，不会同步改变 inbox 的 `status='bound'`、`auto_bind_suppressed` 或 link 状态。
3. `src-tauri/src/db/feishu_sync.rs:301-326` 信任 active link 并把其 `local_entity_id` 当成有效 case ID；第 361—379 行又将该失效 ID 写入有外键的 `feishu_sync_inbox.bound_case_id`，于是整个第 290—535 行预演事务失败。
4. `src-tauri/src/db/feishu_sync.rs:976-995` 通过 `JOIN cases` 读取字段处理计划，孤立 link 命不中时返回 `FEISHU_REVIEW_NOT_FOUND`；这证明旧报告所述“事务尚未开始、未写任一端”与实现一致。
5. `src-tauri/src/db/feishu_sync.rs:1560-1599` 的解绑事务最后仍将 `link.local_entity_id` 写入 `previous_case_id`；该列由 `src-tauri/migrations/0051_feishu_manual_binding.sql:7-20` 外键约束到 `cases(id)`，案件已不存在时审计插入失败，前两项 UPDATE 随事务回滚。

因此旧事故不是历史偶发数据，也不是权限故障；代码根因尚未修复。

## 二、端到端调用链

### 1. 案件删除

`src/App.tsx:617-635、641-665、670-709` 的单条、右键和批量删除都调用 `src/lib/api.ts:561-564` 的 `deleteCase`；Tauri 入口位于 `src-tauri/src/lib.rs:271-279`，最终直达 `cases_db::delete_case`。当前链路没有经过 `FEISHU_WRITE_LOCK`（该锁只定义并使用于 `src-tauri/src/lib.rs:4677-4688、4765-4768` 的字段/明细处理），也没有飞书生命周期清理。

删除后实际状态是：

- `cases` 行消失；级联业务行按既有外键处理；
- inbox 的 `bound_case_id` 被外键自动置空，但 `status` 仍可为 `bound`；
- `feishu_sync_links.status` 仍为 `active`；
- `feishu_sync_field_previews` 通过 link 外键继续保留 pending；
- `feishu_sync_entity_previews.case_id` 具有 `ON DELETE CASCADE`（`0062` 第 22—25 行），会被物理删除，无法留下 `superseded` 历史；
- `feishu_sync_conflicts` 继续保持 pending，UI 查询也未排除 archived/orphan link。

### 2. 拉取与本地预演

手动 UI 从 `FeishuSyncPreview.tsx:145-167` 调用 `api.ts:1565-1568`；后台又在 `src/main.tsx:24-25` 启动 `src/lib/feishuAutoPull.ts:19-38`，启动、聚焦、联网及每 30 分钟均可能触发同一命令。

Tauri 命令 `src-tauri/src/lib.rs:1703-1755` 先取 OAuth token，再调用 `feishu::fetch_active_case_management_records`（真实只读网络边界），然后进入 `complete_pull_with_entities`。数据库阶段在一个总事务中执行：先 supersede 全部旧字段候选，再逐条处理远端案件，最后处理三类明细并把 run 标为 succeeded（`feishu_sync.rs:277-535`）。

当前没有“孤立 link 预扫描/归档/单条隔离”。任意一条孤立 link 在 inbox upsert 的外键处失败，会回滚同批所有有效案件的新快照、字段候选、明细候选以及 run 完成状态；外层仅在事务外把 run 标为 failed（`lib.rs:1752-1754`）。后台协调器只记录通用 warning，错误细节不进入 UI（`feishuAutoPullCore.ts:36-49`、`feishuAutoPull.ts:23-27`）。

### 3. 预演读取与 UI 动作

`get_preview` 的 bound 查询使用 `LEFT JOIN cases` 并最终回退到 `l.local_entity_id`（`feishu_sync.rs:772-807`），所以孤立绑定显示为 UUID；返回类型 `FeishuSyncLinkPreview`（第 570—579 行；`src/lib/types.ts:1319-1327`）没有 `is_orphan/local_case_exists` 字段。

字段候选查询 `feishu_sync.rs:839-877` 不要求 `l.status='active'` 或 `c.id IS NOT NULL`；明细候选查询第 879—891 行甚至不 join 当前 link。UI 第 325—351 行因此无法可靠识别孤立/已解绑候选，仍渲染“采用飞书”“保留本地并写飞书”等按钮。

错误映射也未闭环：

- 拉取映射 `FeishuSyncPreview.tsx:70-112` 不识别 `FEISHU_ORPHAN_BINDING`；
- 绑定动作映射第 182—203 行不识别 orphan；
- 字段动作映射第 205—236 行不识别 `FEISHU_REVIEW_NOT_FOUND`，最终仍提示“请检查权限或刷新”；
- 最近运行 UI 第 296—306、353 行只显示状态/时间，不显示 `error_code/error_message`；即使后端改为 `partial + FEISHU_ORPHAN_BINDING`，用户也看不到原因。

### 4. 解绑、重绑与审计

`bind_case`（`feishu_sync.rs:1487-1557`）在本地事务中校验 inbox/case/冲突；遇到 archived remote link 时直接复用同一 link ID、改写 `local_entity_id` 并重新激活。`unbind_case`（第 1560—1600 行）归档 link、恢复 inbox 并写审计，但没有 supersede 该 link 的字段候选、明细候选或 pending conflicts。

这使 link ID 同时承担“远端记录身份”和“绑定生命周期身份”，而候选只绑定 link ID：

- 字段候选没有保存生成时的 case ID；重绑后查询会把旧候选解释为新 case 的候选。若旧 snapshot 恰好与新 case 当前值相同，`ensure_field_preview_fresh`（第 997—1034 行）可通过。
- 明细候选保存旧 `case_id`，但 `get_entity_resolution_plan`（第 1237—1248 行）不校验当前 link 是否 active、是否仍绑定该 case，也不限定 `review_status='pending'`；旧候选可继续写回旧 case 或飞书。
- 字段 `resolve_preview_and_audit`（第 1168—1182 行）没有检查 UPDATE 的 `rows_affected`；对已 supersede 的字段执行 dismiss 仍可能插入一条 succeeded 审计。
- “采用飞书”分支在检查 `review_status` 前就获取 token 并读取远端（`lib.rs:4691-4711`）；即使候选已失效，也可能产生不应发生的网络读取。

此外，bind/unbind/delete 均未共享 `FEISHU_WRITE_LOCK`。它们可与“保留本地并写飞书”的网络阶段并发：旧 case 的字段已写到飞书后，link 可在本地被解绑并重绑到新 case，随后旧操作仍把 audit/last_synced_at 落在被复用的 link 上。这不是单纯 UI 过期，而是绑定授权边界缺失。

## 三、必须冻结的事务不变量

### A. 删除案件事务

删除成功后的单一原子状态必须同时满足：

1. 目标 case 不存在；其每一条 `entity_type='case' AND status='active'` link 均已归档，任何一条失败则整个删除回滚。
2. 每条 link 对应 inbox 均为 `pending_binding`、`bound_case_id=NULL`、`resolved_at=NULL`、`auto_bind_suppressed=1`；禁止自动重绑。
3. link 下所有 pending 字段/明细候选均不可再执行；pending conflicts 不得继续显示为可处理。若要求保留明细候选的 superseded 历史，现有 `0062` 的 `case_id ON DELETE CASCADE` 必须迁移调整；否则应明确接受物理级联删除而不能声称已保留 superseded 记录。
4. 写一条本地 binding audit；已删除/即将删除的 ID 不得进入受 FK 约束的 `previous_case_id`。若只写 NULL，审计仍能追踪 inbox、动作、状态和时间，但不能保留旧 case ID；若验收要求保留旧 ID，则 `0064` 必须增加无外键历史快照列。
5. 不修改飞书记录，不修改其他案件业务字段；事务失败前后 case/link/inbox/candidates/audit 指纹一致。
6. 多 link、缺 inbox、重复删除均须有明确策略：建议缺 inbox 时 fail closed，不允许“案件删了但 link 清理了一半”。

### B. 历史孤立解绑事务

成功后必须同时满足：link archived；inbox pending/null/suppressed；全部旧候选失效；审计不引用失效 FK；`foreign_key_check` 为空。孤立 case 已不存在不能阻止本地解绑。

### C. 重绑事务与候选授权

重绑前必须先失效该 remote link 的全部旧 pending 候选和冲突；重绑与候选失效必须在同一事务。所有 resolver 在任何 token/HTTP 调用前必须验证：候选 pending、link active、link 当前 case 与候选生成时 case/binding generation 一致。仅依赖字段值 freshness 不足以证明绑定身份未变化。

### D. 拉取隔离

进入逐条 inbox upsert 前，以 `active case link LEFT JOIN cases` 找出 orphan；对每条 orphan 执行与安全解绑等价的本地隔离，再继续有效记录。run 应提交为 `partial`，保存 `error_code='FEISHU_ORPHAN_BINDING'` 和安全计数；不得返回 Err 使前端误以为新预演全部未提交，也不得标记 succeeded 隐藏隔离事实。

## 四、网络调用断言点

当前真实网络读取入口为 `src-tauri/src/lib.rs:1736-1741、4700-4709、4733-4742、4781-4790、4818-4827`；真实飞书写入入口仅为第 4744—4751、4829—4836 行，底层 HTTP 写端点在 `src-tauri/src/feishu.rs:849-889`。本地 delete/bind/unbind DB 函数本身只接收 `SqlitePool`，但现有测试没有可计数的网络 client seam，因此尚不能给出动态“0 次”证据。

验收应在可注入的 Feishu client/spy 上断言：

| 场景 | 预期读网络 | 预期写网络 |
|---|---:|---:|
| delete case / orphan cleanup / unbind / bind / rebind | 0 | 0 |
| 已失效候选再次点击（含解绑后重绑） | 0，必须在取 token 前拒绝 | 0 |
| pull preview（含一个 orphan＋一个有效绑定） | 仅既定只读拉取 | 0 |
| “采用飞书” | 1 次写前复读 | 0 |
| “保留本地并写飞书” | 1 次复读 | 1 次明确写入 |

仅用源码正则证明“DB 模块没有 reqwest”不等于端到端网络零调用；需要让 Tauri 编排层也通过 spy 执行。

## 五、可击穿当前实现的合成反例

| 编号 | 合成前置 | 动作 | 当前结果 | 应有断言 |
|---|---|---|---|---|
| CE-1 | case A + active link L + bound inbox I + pending field/entity | 调 `delete_case(A)` | A 删除；L 仍 active；I 仅 FK 清空 case、状态仍 bound；字段候选残留 | 单事务归档 L、恢复 I、抑制自动绑定、候选失效、网络 0 |
| CE-2 | orphan L 指向不存在 A；同批另有有效 case B | 完成一次 pull | L 的 A 写入 inbox 触发 FK；B 的新预演随总事务回滚 | run partial；L 隔离；B 候选提交；写网络 0 |
| CE-3 | orphan L + inbox I | `unbind_case(L)` | audit.previous_case_id=A 触发 FK；link/inbox 更新全部回滚 | link archived、I pending/suppressed、audit 可追踪、FK check 空 |
| CE-4 | A 与 L 已有 pending 字段 P，普通 unbind 后把 L rebind 到 B；A/B 对应字段值恰好相同 | 点击 P“采用飞书” | P 被解释为 B 的候选，freshness 可通过并更新 B | P 在 unbind/rebind 时 superseded；resolver 在网络前拒绝 |
| CE-5 | A 与 L 有 pending 明细 E；unbind/rebind L 到 B，A 仍存在 | 点击 E“采用飞书” | plan 不校验 L 当前归属，仍可把远端明细写入 A | resolver 校验 active link + 当前 case/generation；网络 0 |
| CE-6 | “保留本地并写飞书”已完成远端复读、尚未 HTTP 写；并发执行 unbind/rebind | 两动作交错 | 旧 A 值可能写飞书，随后 audit/last_synced_at 记到已绑定 B 的 L | 生命周期动作与写动作同锁/同 generation；不得跨绑定提交 |
| CE-7 | archived/orphan L 仍有 pending field/conflict；最新成功 run 即该 run | 只打开预演 UI | 字段按钮仍显示；orphan 以 UUID 显示；错误落入“检查权限” | 明确 orphan 标志与文案；只留解除；其他操作 disabled/不返回 |
| CE-8 | 两条 active link 同指向待删 case，第二条缺 inbox | 删除 case | 若实现逐条非原子清理，可能第一条归档、case 仍在或随后被删、第二条残留 | 缺任一关联即整体回滚，前后指纹一致 |

## 六、缺陷分级与开发门禁

### P0（必须先修）

1. **案件删除生命周期非原子**：`delete_case` 留下 active orphan，是旧事故根因。
2. **孤立 link 击穿整批拉取**：一个坏绑定回滚其他有效案件预演，后台会持续重复失败。
3. **历史孤立无法解绑**：审计外键使安全恢复入口本身回滚。
4. **解绑/重绑缺少候选授权边界及并发隔离**：旧字段/明细候选可能跨案件执行；bind/unbind/delete 未与显式飞书写操作共享锁或 binding generation。

### P1（F1 验收前修）

1. **UI 模型与动作门禁缺失**：无 orphan 标志，UUID 兜底，字段/明细/冲突查询未排除失效绑定。
2. **稳定错误码未端到端呈现**：未映射 `FEISHU_ORPHAN_BINDING`、`FEISHU_REVIEW_NOT_FOUND`，partial run 错误码不可见；错误提示仍可能误导为权限问题。
3. **审计与候选保留策略未决**：NULL audit 无旧 case ID；实体候选因 `ON DELETE CASCADE` 无法保留 superseded 历史。是否创建 `0064` 必须在实现前冻结。
4. **测试证据不足**：现有 `feishu_sync.rs:2026-2080` 只覆盖“case 仍存在”的 bind→unbind→ignore→restore 快乐路径；没有 delete/orphan/pull-continue/rebind-stale/concurrency/network-spy 反例。前端现有测试只覆盖部分 pull 错误映射。

## 七、建议的最小实施顺序

1. 先冻结 `0064` 决策及审计语义：若需保留旧 case ID 或实体候选 superseded 历史则先迁移；若选 NULL/级联删除，必须在验收说明中明确降级边界。
2. 建立 CE-1—CE-8 合成失败夹具和网络 spy；当前代码应先稳定失败。
3. 抽取单一“本地解除/归档绑定”事务 helper，由 delete、历史 orphan 隔离和人工 unbind 复用；helper 同步失效字段、明细、冲突。
4. 给候选增加绑定身份校验（case snapshot 或 binding generation），所有 resolver 在 token/网络前 fail closed；生命周期动作纳入同一写锁或等价 generation 协议。
5. pull 前隔离 orphan，提交有效项并将 run 记为 partial + `FEISHU_ORPHAN_BINDING`。
6. 最后扩展返回类型/UI/error mapper，显示“本地案件已删除”，只保留解除入口；完成网络零写、FK、quick_check、审计和重绑回归。

## 八、审计边界

- 未读取或修改正式 SQLite、NAS、飞书 Base、OAuth 凭据或案件正文。
- 未运行真实飞书请求；“旧事故现场事实”来自指定的 v0.8.2 事故报告，本次仅以当前源码验证其机制仍成立。
- 未修改产品源码、测试、迁移、`00_status.md` 或调度板。
- 本报告是 F1 开发前 Gate，不代表实现已完成或任务已验收。
