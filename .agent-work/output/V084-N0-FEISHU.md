# V084-N0-FEISHU｜飞书“收件箱”双向同步冻结契约

状态：`submitted_for_review` 前置报告（只读审计；未修改产品代码、迁移、版本、依赖或外部状态）

## 1. 结论

0.8.4 的待办同步应复用现有“读取→本地预演→用户确认→写前复核→执行→审计”控制面，但必须新建独立的待办同步模块和账本，不能直接复用现有案件同步表：现有 `feishu_sync_links.entity_type` 只允许 `case/work_item/stage/contact`，且其唯一键和预演模型均围绕案件绑定设计（`src-tauri/migrations/0049_feishu_case_management_sync.sql:9-30`、`src-tauri/migrations/0062_feishu_entity_change_previews.sql:2-25`）。

冻结决定如下：

1. 案件看板模块名为“待办事项”；飞书交换表名严格为“收件箱”。
2. `case_todos.id` 就是跨端稳定事项 ID；T1 业务表字段由 V084-N0-TODO 冻结，F1 不再改 `todos.rs`。
3. 自动拉取只能读取远端并生成本地候选，不得改写 `case_todos`，也不得产生飞书写入。所有内容写入和删除状态传播均由用户逐项确认或明确触发批量确认。
4. 第一阶段从不调用飞书物理删除 API。删除只同步为软删除状态；远端记录物理消失只生成 `remote_missing` 人工候选。
5. 同步判断采用稳定 ID + 来源 + 同步版本 + 基线哈希 + 内容哈希 + 本地三方基线，不依赖更新时间文本，也不按标题去重。
6. 任一离线、授权不足、结构漂移、重复 ID、元数据不一致或并发变化均失败关闭：保留上次有效基线，不推进版本，不静默覆盖任一端。
7. `item_at`和飞书“事项时间”均可为空。无日期事项可以创建、修改、完成和双向同步；同步层不得为它补当前时间或创建时间。

## 2. 现有能力与缺口

### 2.1 可直接复用的能力

- 凭据边界已正确隔离：App Secret、access token、refresh token 只进入内存和 Windows Credential Manager，不进入 SQLite、设置 DTO、错误文本或日志；OAuth 当前申请 `offline_access bitable:app auth:user.id:read`，并公开安全的 `write_enabled` 状态（`src-tauri/src/feishu_oauth.rs:1-7`、`:20-48`、`:610-632`）。
- 飞书 HTTP 已有稳定的鉴权、权限、表不存在、网络、响应格式分类，并限制单页响应大小；ID 只接受 ASCII 字母、数字、下划线和连字符（`src-tauri/src/feishu.rs:311-370`）。
- 现有读取先枚举并校验字段结构，动态发现关联表且检查回链，结构不符即失败（`src-tauri/src/feishu.rs:455-544`、`:983-1049`）。
- 现有拉取的所有预演写入位于单个 SQLite 事务内，成功后才提交；旧候选先置为 `superseded`，失败不留下半套新预演（`src-tauri/src/db/feishu_sync.rs:462-482`、`:698-731`）。
- 现有人工写回会先检查候选仍为 pending、绑定权威仍有效、本地值未变，再读取远端单条记录并比对预演快照；候选重复处理返回稳定错误（`src-tauri/src/db/feishu_sync.rs:1181-1293`、`:1309-1329`、`:1440-1457`）。
- 自动拉取协调器已具备离线跳过、节流、断开跳过、单飞和失败收敛；它只调用预演读取（`src/lib/feishuAutoPullCore.ts:19-52`）。
- 现有案件明细明确规定远端缺失不映射为本地删除或归档（`src-tauri/src/db/feishu_entities.rs:413-414`）；这一边界直接沿用到待办。

### 2.2 不能复用或尚不存在的部分

- 当前设置只有案件总表 `feishu_cases_table_id`，没有“收件箱” Table ID（`src-tauri/src/settings.rs:227-239`；前端类型同样只有该字段，`src/lib/types.ts:1257-1267`）。F1 需新增独立 `feishu_todo_inbox_table_id`，不得把“收件箱”误配为案件总表。
- 当前抓取结果固定为案件总表及进展、阶段、联系人三表（`src-tauri/src/feishu.rs:94-106`），没有待办记录 DTO 或“收件箱”字段校验。
- 当前 UI 的分区仅覆盖案件绑定、字段、明细、冲突和运行状态（`src/modules/tools/FeishuSyncPreview.tsx:33-43`），没有待办同步候选。
- 当前 `case_todos` 只含标题、完成状态和日期，`case_id NOT NULL ON DELETE CASCADE`，也无来源、正文、软删除和同步账本（`src-tauri/migrations/0024_case_todos.sql:5-16`、`src-tauri/migrations/0027_todo_due_date.sql:1-7`）。该业务表由 T1/0064 负责兼容演进；F1 不重复处理。
- 现有 `feishu_sync_snapshots.payload_hash` 能证明曾见过某 payload，但没有待办的本地/远端版本向量、基线链路或远端重复事项 ID 约束（`src-tauri/migrations/0049_feishu_case_management_sync.sql:34-46`）。

## 3. 飞书“收件箱”字段映射（冻结）

配置以 `feishu_app_token + feishu_todo_inbox_table_id` 为事实源；读取时还必须从表元数据确认表名恰为“收件箱”。同一名称有多个表、Table ID 不存在或字段类型不符均返回结构错误，不猜测、不自动创建或改列。字段结构仍必须包含“事项时间”日期时间列，但每条记录的该字段允许为空；空值不是 schema 错误。

| 飞书字段 | 类型 | 必填 | 本地映射 / 约束 |
| --- | --- | --- | --- |
| `事项ID` | 单行文本 | 是 | `case_todos.id`；规范小写 UUID；创建后不可变 |
| `标题` | 单行文本 | 是 | `case_todos.title`；空标题拒绝入站 |
| `内容` | 多行文本 | 否 | `case_todos.content`；空值规范为 `null` |
| `事项时间` | 日期时间 | 否 | `case_todos.item_at`；空单元格映射 JSON `null`；旧 `due_date` 仅在 `item_at` 为空时出站投影为 Asia/Shanghai 当日 00:00 |
| `状态` | 单选 | 是 | 仅 `待办` / `已完成`，映射 `done=0/1` |
| `完成时间` | 日期时间 | 否 | `case_todos.done_at`；状态为待办时必须为空 |
| `关联案件` | 双向关联 | 否 | 最多关联一条案件总表记录；通过现有 active `case` link 解出 `case_id`；无关联映射 `NULL` |
| `来源` | 单选 | 是 | 仅 `caseboard` / `feishu` / `hermes`；创建后不可变，映射 `case_todos.source` |
| `同步版本` | 数字（整数） | 是 | 正整数，从 1 起；每次业务 payload 变化加 1，不允许回退 |
| `基线哈希` | 单行文本 | 条件必填 | 本次变化所基于的上一版内容哈希；新建为留空，其余为 64 位小写 SHA-256 |
| `内容哈希` | 单行文本 | 是 | 当前规范业务 payload 的 64 位小写 SHA-256；与实际内容不符即元数据冲突 |
| `删除状态` | 单选 | 是 | 仅 `有效` / `已删除`；后者映射本地 `deleted_at`，绝不触发物理删除 |

`关联案件`不是必填项。若远端关联记录尚未在本机与案件绑定，则事项仍可作为未关联待办进入候选，但必须保留“远端案件待绑定”提示；不得通过案件名称模糊匹配后静默绑定。现有实现也只允许唯一精确案号自动绑定，名称只作推荐（`src-tauri/src/db/feishu_sync.rs:511-540`）。

## 4. 稳定 ID、规范 payload 与版本基线

### 4.1 稳定 ID 与来源

- 新事项由最初创建端生成 UUID v4；`caseboard`、`hermes` 或经用户接纳的 `feishu` 创建端都必须写入 `事项ID`。
- `事项ID` 永不因远端 record_id 变化、软删除、恢复、解除案件关联或“复制到案件进展”而变化。
- `来源`表示最初创建端，不表示最后写入端，创建后不可修改。来源变化直接形成 `source_immutable` 冲突。
- 远端手工新增但缺少合法 `事项ID/来源/版本/哈希` 的记录只进入 `metadata_invalid` 候选；用户点击“接纳为新事项”后才补发技术字段并创建本地事项。

### 4.2 规范 payload

参与哈希的字段固定为：

```text
事项ID, 标题, 内容, 事项时间或null, 状态, 完成时间, 关联案件的远端案件record_id或null, 来源, 删除状态
```

不参与哈希：飞书 record_id、同步版本、基线哈希、内容哈希、远端 modified time、本地 `created_at/updated_at`、显示用案件名称。`due_date`不作为独立字段参与哈希；它只在 `item_at IS NULL AND due_date IS NOT NULL` 时生成“事项时间”的兼容投影。

序列化规则固定为 UTF-8 JSON、键按上述固定顺序、无多余空白、换行统一 LF；`null` 与空字符串不混用。事项时间按以下顺序得到规范值：

1. `item_at`非空：规范为 RFC3339 时间；
2. `item_at`为空但旧 `due_date`非空：按 Asia/Shanghai 当日 00:00 生成 RFC3339 兼容值；
3. 两者均为空：写入 JSON 字面量 `null`，不得写 `""`、Unix epoch、当前时间或 `created_at`。

远端空“事项时间”同样规范为 JSON `null`。SHA-256 对序列化字节计算并输出 64 位小写十六进制。

### 4.3 日期兼容与双向同步

- 本地出站只计算上述规范事项时间，不为预演改写业务表。旧记录若只有 `due_date`，远端看到的是该日 00:00；下一轮比较仍使用同一规范值，因此 `L==R`，本地 `item_at`继续保持 `NULL`，不会因为回读而变成一个新时间。
- 远端有时间且用户确认采用飞书时，写入规范后的 `item_at`，同时把 Asia/Shanghai 日期部分投影到 `due_date`，供 0.8.3 日历和旧 API 使用。
- 远端事项时间为空且用户确认采用飞书时，同时清空本地 `item_at`和 `due_date`。只清 `item_at`但留下旧 `due_date`会让下一轮又投影出日期，属于禁止状态。
- 本地从有日期改为无日期时，T1 命令层必须同时清空 `item_at/due_date`；显式同步把飞书“事项时间”清空。无日期事项在两端往返后仍为 `null`。
- 本地从无日期改为有日期时，以 `item_at`为准并更新 `due_date`日期投影；显式同步写入飞书日期时间。事项时间变化才提升同步版本，单纯重复计算兼容投影不提升版本。
- “复制到案件进展”不把同步规范时间当作强制必填。进展发生时间按 `item_at` → `due_date`的 Asia/Shanghai 当日 00:00 → `created_at` 顺序回退；该回退只用于生成案件进展，不回写待办 `item_at/due_date`，也不参与待办同步哈希。

### 4.4 三方判断

本地账本保存上次双方接受的 `base_hash`；每次预演计算本地当前哈希 `L`、远端当前哈希 `R`，并读取远端声明基线 `RB`：

| 条件 | 分类 | 行为 |
| --- | --- | --- |
| `L == R` | `noop` | 不写任一端；可推进本地观察到的版本/modified time |
| 新远端 ID 且元数据完整 | `create_local` | 用户确认后新建本地；允许 `case_id=NULL` |
| 新本地 ID 且远端无同 ID | `create_remote` | 用户确认后创建远端 |
| `L == base && R != base && RB == base` | `pull_to_local` | 远端单边变化，用户确认后应用本地 |
| `L != base && R == base` | `push_to_remote` | 本地单边变化，用户确认后写远端 |
| `L != base && R != base && L != R` | `conflict` | 必须选择本地、飞书、保留两份或暂不处理 |
| 声明 `内容哈希 != R`、版本回退、`RB` 不在已知链路 | `metadata_conflict` | 失败关闭，不更新业务表或基线 |

成功写入时版本为 `max(local_seen_version, remote_version)+1`；`基线哈希`写本次变化前的共同基线，`内容哈希`写新 payload 哈希。若同版本出现不同内容哈希，直接冲突，不能以更新时间或来源决定胜负。

## 5. 本地 F1 账本（冻结到 0065+，不进入 T1/0064）

F1 使用独立表，建议名称及职责固定如下：

1. `todo_feishu_sync_links`：`item_id`、`app_token/table_id/record_id`、本地/远端已见版本、`base_payload_hash`、最近本地/远端哈希、remote modified time、`status(active/conflict/remote_missing/archived)`、最近同步时间。唯一约束：`(app_token,table_id,item_id)` 与 `(app_token,table_id,record_id)`。
2. `todo_feishu_sync_runs`：一次只读预演或人工同步批次的状态、计数和稳定错误码；不得复用带 `active_case_filter` 语义的旧 runs 表。
3. `todo_feishu_sync_previews`：每次 run 每个 stable item 一条候选，保存 base/local/remote payload、哈希、版本、change kind 和 `pending/applied_local/applied_remote/kept_both/dismissed/superseded`。
4. `todo_feishu_sync_conflicts`：保存冲突类型、三方 payload/哈希/版本和解决结果；同一 item 只允许一个 pending 冲突。
5. `todo_feishu_sync_operation_audits`：保存显式动作 ID、方向、开始/成功/失败/不确定、错误码和前后哈希，不保存 token 或外部响应正文。

拉取建立 run 后，在一个本地事务中 supersede 旧 pending 候选、upsert 本轮候选/冲突/链接观察值并完成 run；任一步失败整轮回滚并保留上一轮有效预演。该模式与现有预演事务一致（`src-tauri/src/db/feishu_sync.rs:475-482`、`:726-731`）。

## 6. 冲突、删除、去重与防循环

### 6.1 冲突处理

- 选择“采用飞书”：写前重读本地和远端，二者都仍与候选快照一致才在单个本地事务中更新 `case_todos`、链接基线、候选和审计。
- 选择“保留本地并写飞书”：先验证读写授权，重读远端并核对 record_id、版本、内容哈希；写后立即回读验证。验证成功后才提交本地基线。
- 选择“保留两份”：原事项保留一端版本；另一份生成新 UUID，`来源`沿用其原始来源，建立新的 link，不能让两个本地事项共用同一稳定 ID。
- “暂不处理”只关闭本轮候选；下轮仍有差异时重新出现。

### 6.2 删除与恢复

- 本地删除是 `deleted_at` 软删除，出站仅把 `删除状态`改为`已删除`；远端记录仍保留。
- 远端 `删除状态=已删除`只生成 `soft_delete_local` 候选，用户确认后才设置本地 `deleted_at`。
- 远端 record_id 物理消失生成 `remote_missing` 冲突，不自动删除本地、不自动重建远端。用户只能明确选择“保留本地并重建远端”或“确认远端已删除并软删除本地”。
- 恢复同理为显式动作；恢复不分配新事项 ID。
- F1 不实现、也不得调用飞书 DELETE records 接口。

### 6.3 去重和防环

- 远端同一 `事项ID` 出现两条及以上记录：全部隔离为 `FEISHU_TODO_DUPLICATE_ID`，不得任取其一。
- 标题、内容、时间相同但事项 ID 不同：视为不同事项，不做内容去重；UI 可提示但不得自动合并。
- 创建远端前先按 `事项ID`精确查询：0 条才 POST，1 条转绑定/更新，2 条以上冲突。POST 超时或响应不确定时必须重新查询该 ID，禁止盲目重试创建。
- 每次成功同步更新本地 base；下轮 `L==R`直接 noop，不写技术字段，从而切断回声循环。
- 所有人工动作带唯一 `action_id`，审计表对该 ID 唯一；重复点击返回已有结果或 `ALREADY_RESOLVED`，不能再次发 HTTP 写请求。

## 7. 授权、离线与失败原子性

### 7.1 授权

- 继续使用现有 OAuth 与 Windows Credential Manager；不新增 token 存储。当前 scope 已含 `bitable:app`，`write_enabled`来自已保存 scope（`src-tauri/src/feishu_oauth.rs:23-25`、`:622-632`）。
- 自动拉取与手动“刷新预演”只读；`write_enabled=false`时仍可读预演，但所有远端写按钮禁用。现有 UI 已采用相同门禁（`src/modules/tools/FeishuSyncPreview.tsx:220-249`、`:257-280`）。
- 写操作在取得候选之后仍须再次检查 `connection_status.write_enabled` 和有效 access token。权限不足不得先改本地基线。

### 7.2 离线及结构漂移

- `navigator.onLine=false`时自动拉取不尝试网络；现有协调器已返回 `offline`（`src/lib/feishuAutoPullCore.ts:26-34`）。
- 网络超时/断线：只标记 run/action failed 或 uncertain，保留上次有效预演与基线；不把“远端无响应”解释为远端删除。
- 表名、字段名、字段类型、单选值、关联目标任一漂移：整轮 `FEISHU_TODO_SCHEMA_MISMATCH`，不得部分解析或自动补列。
- 读取分页或响应缺记录：整轮失败。现有批量读取已要求返回集合与请求集合完全一致（`src-tauri/src/feishu.rs:755-802`）。

### 7.3 并发写与网络不确定

现有写回采用“写前重新读取并核对快照”的乐观控制（`src-tauri/src/lib.rs:4777-4793`、`:4878-4894`），但远端 API 调用本身未显示条件更新参数，因此仍存在“最后一次重读之后、写入之前”的竞态窗口。0.8.4 冻结以下保守边界：

1. 不启用后台自动远端写；同一进程内按 `app_token/table_id/item_id` 串行人工写。
2. 写前重读并核对版本、内容哈希、record_id；写后再次回读并核对期望 payload、版本和哈希。
3. 写请求超时、返回无法解析或写后回读不一致均为 `write_uncertain`：不推进本地基线，立即生成冲突并要求刷新，不自动重试。
4. F1 实现前须核对飞书官方接口是否支持 ETag、revision 或条件更新；若支持则必须使用。若不支持，以上前后双检和“仅显式人工写”是 RC 可接受的最低边界，报告中必须保留该残余竞态说明。

## 8. 稳定错误码

| 错误码 | 含义 / 必须行为 |
| --- | --- |
| `FEISHU_TODO_CONFIG_INVALID` | 未配置“收件箱” Table ID；不访问网络 |
| `FEISHU_TODO_SCHEMA_MISMATCH` | 表名、字段、类型、选项或关联目标漂移；整轮失败 |
| `FEISHU_AUTH_REQUIRED` | 未连接或 token 失效；不改本地 |
| `FEISHU_PERMISSION_DENIED` | 无读权限；整轮失败 |
| `FEISHU_OAUTH_MISSING_READWRITE_SCOPE` | 无写权限；允许只读预演，禁止远端写 |
| `FEISHU_NETWORK_TIMEOUT` / `FEISHU_NETWORK_ERROR` | 网络失败；保留旧预演和基线 |
| `FEISHU_TODO_METADATA_INVALID` | ID、来源、版本或哈希无效；隔离记录 |
| `FEISHU_TODO_DUPLICATE_ID` | 同一稳定 ID 对应多条远端记录；全部隔离 |
| `FEISHU_TODO_CONFLICT` | 双端同时变化、不可变来源变化或版本/链路冲突；人工处理 |
| `FEISHU_TODO_STALE` | 候选后本地或远端已变化；不执行写操作 |
| `FEISHU_TODO_REMOTE_MISSING` | 已链接 record_id 物理消失；只生成人工候选 |
| `FEISHU_TODO_ALREADY_RESOLVED` | 重复处理同一候选；不得再次发 HTTP |
| `FEISHU_TODO_WRITE_UNCERTAIN` | 写请求结果或写后验证不确定；不推进基线、不自动重试 |
| `FEISHU_TODO_DB_WRITE_FAILED` | 本地预演/提交失败；事务回滚 |

错误消息只能包含固定文案和安全 ID；不得拼接 token、请求头、OAuth code/state、App Secret 或未经裁剪的飞书响应正文。现有 OAuth 错误类型已遵循这一规则（`src-tauri/src/feishu_oauth.rs:50-105`）。

## 9. 确定性测试矩阵（F1/RC 硬门禁）

1. 字段契约：正确表通过；“事项时间”列存在但单元格为空可以通过；错表名、缺字段、错类型、非法单选、多案件关联均整轮失败且业务表零写入。
2. 新增双向：分别覆盖有事项时间和无事项时间的本地新建→显式创建远端、Hermes/飞书规范新建→显式创建本地；无时间必须保持 `item_at/due_date/remote field=null`，未关联案件正常进入。
3. 修改/完成/恢复/关联/解除关联：逐项覆盖两个方向；`source`和 stable ID 始终不变。
4. 三方判定：noop、local-only、remote-only、同版本异哈希、双方同时改、陈旧 `RB`、内容哈希伪造全部固定分类。
5. 删除：本地软删除、远端软删除、远端物理缺失、恢复；断言 HTTP DELETE 调用数恒为 0。
6. 去重：同 ID 两远端记录隔离；POST 超时后精确查询只绑定现有记录，不重复创建；重复 action ID 写调用数最多 1。
7. 防环：local→remote→pull 和 remote→local→pull 后第二轮均 noop、HTTP 写调用数 0、版本不增长。
8. 并发：候选后本地变化、远端变化、远端 record_id 替换、写后回读不一致均失败关闭；base/version 不推进。
9. 离线/授权：offline、token 过期、只读 scope、403、404、429、超时、非 JSON、分页不完整；断言旧预演保留且 `case_todos`不变。
10. 事务：在 supersede、candidate、conflict、audit、link、commit 各点注入失败，断言无半套本地状态。
11. 日期兼容：旧记录 `item_at=NULL,due_date=YYYY-MM-DD` 出站为 Asia/Shanghai 00:00，回读 noop 后 `item_at`仍为 NULL；远端有时间入站同步 `item_at+due_date`；远端清空时间入站同时清空两列；本地清空两列出站得到远端 null；重复两轮哈希与版本不漂移。
12. 复制时间回退：分别断言 `item_at`优先、仅 `due_date`时采用 Asia/Shanghai 00:00、两者都空时采用 `created_at`；复制不得回写待办时间字段或改变同步哈希。
13. 安全：SQLite、settings、DTO、日志和错误均搜索不到 App Secret/token；正式飞书 Base 和正式数据库不参与自动测试。

测试分层：Rust fixture/HTTP spy 覆盖状态机与零写入，Node 测试覆盖离线协调器和 UI 门禁，隔离结构副本只做 RC 的最小真实 create/read/update/read；现有代码已具备 HTTP 读写计数 spy（`src-tauri/src/feishu.rs:31-70`）及隔离副本 live test 模式（`src-tauri/src/feishu.rs:1582-1633`），不得指向正式 Base。

## 10. 后续非重叠文件范围与依赖

### T1（先行依赖，V084-N0-TODO 已确认）

- 独占 `src-tauri/migrations/0064_*.sql`、`src-tauri/src/db/todos.rs`、待办 CRUD 命令和类型、待办页面/入口。
- 业务字段：`id`（stable item ID）、`case_id NULL`、`title`、`content`、`item_at NULL`、`done/done_at`、`source(caseboard|feishu|hermes，不可变)`、`deleted_at`、`created_at/updated_at`；保留 `due_date`为 0.8.3 兼容投影。无日期时 `item_at/due_date`都为 NULL。
- 不增加飞书 record/version/hash/conflict 字段。

### F1（在 T1 通过后开始）

- 独占 `src-tauri/migrations/0065_*.sql` 起的待办同步表。
- 新增 `src-tauri/src/db/todo_feishu_sync.rs` 及其 Rust fixture tests；不修改 `todos.rs`。
- `src-tauri/src/feishu.rs`只新增“收件箱”schema/read/create/update适配器，不改变现有案件四表行为。
- `src-tauri/src/settings.rs`与 `src/lib/types.ts`新增 `feishu_todo_inbox_table_id`及待办同步 DTO；`src/lib/api.ts`新增独立命令封装。
- UI 新增独立 `TodoFeishuSyncPreview.tsx`，不向现有 `FeishuSyncPreview.tsx`硬塞待办状态机；由待办模块提供入口，飞书工具页可仅增加跳转/摘要。
- 自动预演可复用 `feishuAutoPullCore.ts`的离线/节流/单飞机制，但调用独立 todo pull preview；不得自动调用 resolve/push。

### RC

- 所有单元/集成测试使用临时 SQLite 和 HTTP fake；真实验证只使用结构副本，并显式标注 app/table/record 皆为隔离资产。
- 正式飞书 Base、正式 SQLite、NAS/Hermes 生产实例和本机凭据不在 N0/F1 自动测试范围。

## 11. 待主控合并时确认

1. 将 T1 的 nullable `item_at`、`source`、`deleted_at`命名原样纳入 N0 总契约，F1 不再重复演进业务表；“事项时间”是必须存在但允许空值的飞书字段。
2. 接受“0.8.4 不启用任何后台远端写；远端写仅用户明确动作”的安全策略。
3. 将“官方 API 条件更新能力核验”列为 F1 实现门禁；若无 CAS，RC 必须保留前后双检的残余竞态说明，不得宣称绝对并发原子。
