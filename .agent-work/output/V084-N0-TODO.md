# V084-N0-TODO｜待办本地模型与案件进展复制冻结契约

- task_id：`V084-N0-TODO`
- 状态：`submitted_for_review`（主控审阅前不代表 accepted）
- 审计方式：仅只读源码、迁移和既有测试；未读取正式数据库、凭据、NAS 或正式飞书 Base，未修改产品代码、迁移、版本或依赖。
- 冻结结论：**演进现有 `case_todos`，不另建平行的全局待办业务表**；`case_todos.id` 继续作为稳定事项 ID；`case_id` 改为可空且案件删除时仅解除关联；删除改为软删除；复制到案件进展由一个后端事务完成，并以 `case_work_items(external_source, external_record_id)` 的现有唯一索引防重。

## 1. 源码事实、现有能力与缺口

| 项目 | 已存在能力 | 缺口/风险 | 依据 |
| --- | --- | --- | --- |
| 旧待办表 | `id` 为文本主键；支持标题、完成状态、完成时间、创建/更新时间 | `case_id TEXT NOT NULL` 且 `ON DELETE CASCADE`，不能保存未关联事项，删案件会删待办 | `src-tauri/migrations/0024_case_todos.sql:5-16` |
| 事项日期 | `due_date` 是可选 `YYYY-MM-DD`，用于首页日历 | 没有内容、事项时间、来源、软删除字段 | `src-tauri/migrations/0027_todo_due_date.sql:1-7` |
| 后端 CRUD | 本地生成 UUID；按案件列出；完成/撤销写 `done_at` | 新增输入强制 `case_id: String`；全局列表使用内连接；删除执行 `DELETE`；没有字段校验和稳定业务错误码 | `src-tauri/src/db/todos.rs:10-38,52-68,70-90,92-127` |
| 首页汇总 | 可跨案件列出未完成项并打钩完成 | `OpenTodoRow.case_id/case_name` 均不可空，未关联事项会被内连接过滤 | `src/lib/api.ts:1408-1454`；`src/components/HomeView.tsx:1827-1922` |
| 案件详情 | 民事快照、刑事案件和执行详情复用 `TodosCard` | 仅能增加、完成/撤销和物理删除；无内容编辑、关联/解除关联和复制 | `src/components/TodosCard.tsx:55-128,167-245` |
| 案件进展 | `case_work_items` 已有 `occurred_at/title/content/source/external_*`、软删除，并按 `occurred_at DESC` 排序 | 现有通用 upsert 不是“从待办复制”的原子命令，且失败文本不是稳定契约 | `src-tauri/migrations/0038_case_work_items.sql:1-27`；`src-tauri/src/db/case_work_items.rs:98-157,168-237` |
| 复制幂等基础 | `case_work_items(external_source, external_record_id)` 已有条件唯一索引 | 当前没有以 `case_todo + todo.id` 写入的实现 | `src-tauri/migrations/0038_case_work_items.sql:25-27` |
| 进展显示 | 刑事案件页实际以 `occurred_at`、标题、内容和来源展示“案件进展” | 来源标签目前只识别 `manual/feishu/材料提取`，需补“待办复制” | `src/modules/litigation/components/criminal/CriminalCasePanel.tsx:1455-1482,1530-1534` |
| 设备同步 | `case_todos` 已登记为 `case_todo`，已有 insert/update/delete dirty trigger | registry 只列旧字段且漏了 `due_date`；重建表会移除旧触发器，迁移内必须重建 | `src-tauri/src/device_sync/registry.rs:290-304`；`src-tauri/migrations/0058_device_sync_core.sql:244-255` |
| UI 主入口 | 顶部模块由 `ModuleTabs` 固定数组及 `App.tsx` 条件挂载 | 尚无“待办事项”模块 | `src/components/ModuleTabs.tsx:32-74,108-117`；`src/App.tsx:1104-1168` |

## 2. 业务表兼容策略（冻结）

### 2.1 选择演进旧表

冻结为迁移 `0064_global_todos.sql` 重建 `case_todos`，而不是新增 `todos`/`global_todos`：

1. 现有案件内待办、首页汇总、Tauri 命令和设备同步都已经以 `case_todos`/`case_todo` 为事实源；另建表会制造双写、双 ID 和历史数据归属问题。
2. 保留全部旧 `id`，因此旧待办的稳定事项 ID 不变，后续飞书同步和案件进展防重都直接使用该 ID。
3. SQLite 不能直接移除 `case_id NOT NULL` 并把外键从 `CASCADE` 改为 `SET NULL`，所以必须在单一 SQLx 迁移事务中“建新表 → 全量复制 → 删除旧表 → 改名 → 重建索引/触发器”。禁止在应用启动后另跑非事务补丁。

### 2.2 冻结字段

`case_todos` 的 v0.8.4 业务字段如下；同步版本、基线哈希、冲突、远端 record ID **不放入此表**，由 F1 的独立同步账本管理。

| 字段 | 约束/语义 |
| --- | --- |
| `id TEXT PRIMARY KEY NOT NULL` | 稳定事项 ID；本地创建用 UUID；同步、改名、关联、完成、软删均不得更换 |
| `case_id TEXT NULL` | 可暂不关联案件；外键 `REFERENCES cases(id) ON DELETE SET NULL` |
| `title TEXT NOT NULL` | trim 后非空，命令层与 DB `CHECK` 双校验 |
| `content TEXT NOT NULL DEFAULT ''` | 事项正文；迁移旧行回填空串 |
| `item_at TEXT NULL` | 事项时间；新输入使用 ISO 本地时间 `YYYY-MM-DDTHH:mm`，允许旧事项为空 |
| `due_date TEXT NULL` | 0.8.3 API/首页日历兼容投影；旧行原值保留。仅传旧 `due_date` 时令 `item_at = due_date + 'T00:00'`；传新 `item_at` 时同一命令把 `due_date` 更新为其日期部分；清空 `item_at` 同时清空 `due_date` |
| `done INTEGER NOT NULL DEFAULT 0` | 仅 `0/1`；完成时写 `done_at`，撤销完成时清空 |
| `done_at TEXT NULL` | 完成时间 |
| `source TEXT NOT NULL DEFAULT 'caseboard'` | 仅 `caseboard/feishu/hermes`；创建后不可变。Hermes 仍只能经飞书同步层进入，不得直写 SQLite |
| `deleted_at TEXT NULL` | 第一阶段删除标记；业务列表默认过滤，回收站显式查询 |
| `created_at/updated_at TEXT NOT NULL` | 旧值原样保留；任何业务变更刷新 `updated_at` |

迁移数据映射固定为：旧 `id/case_id/title/done/done_at/due_date/created_at/updated_at` 原样复制；`content=''`；`item_at = due_date || 'T00:00'`（无 `due_date` 则 `NULL`）；`source='caseboard'`；`deleted_at=NULL`。迁移前后须断言行数与 ID 集合完全一致。

迁移须重建：

- `idx_case_todos_case_done`：改为只索引 `deleted_at IS NULL` 的 `case_id, done`；
- `idx_case_todos_item_at` 与兼容的 `idx_case_todos_due`；
- 三个 `device_sync_todos_*` trigger（重建表会删除旧 trigger）；
- `source` 不可变 trigger（仅允许插入时确定来源，禁止普通 UPDATE 改来源）；
- `src-tauri/src/device_sync/registry.rs` 的 `case_todo` 白名单补齐 `content/item_at/due_date/source/deleted_at`。

## 3. 本地命令与兼容契约（冻结）

### 3.1 新全局命令

- `create_todo(input)`：`case_id` 可空；调用方不能伪造 `source`，本地 UI 固定写 `caseboard`。
- `list_global_todos(filter)`：支持 `open/completed/deleted/all`、可选案件和关键字；默认不含软删；排序为 `done ASC, item_at ASC NULLS LAST, updated_at DESC`。
- `update_todo(id, patch)`：可改标题、内容、事项时间；完成状态走同一事务维护 `done_at`。
- `set_todo_case(id, case_id)`：`Some` 表示关联，`None` 表示解除；关联前验证案件存在。
- `soft_delete_todo(id)`、`restore_todo(id)`：都只更新本行，不物理删除；软删后的事项不能修改、完成或复制，必须先人工恢复。
- `copy_todo_to_case_progress(id, target_case_id?)`：见第 4 节。

### 3.2 旧命令继续可用

保留 `add_todo/list_todos/list_open_todos/update_todo/delete_todo` 的命令名，避免现有案件详情与首页立即断裂：

- `add_todo` 仍接受旧 `{case_id,title,due_date}`，内部转新建命令；
- `list_todos(case_id)` 仅返回该案未软删事项；
- `list_open_todos()` 改为 `LEFT JOIN cases`，返回 `case_id/case_name` 可空，未关联分组显示“未关联案件”；
- `delete_todo` 名称暂保留但语义改为软删除，不再执行 SQL `DELETE`；
- 前端 `Todo.case_id`、`OpenTodoRow.case_id/case_name` 改为 nullable，并补 `content/item_at/source/deleted_at`。

完成与删除是正交状态：完成不会删除；软删也不自动改 `done/done_at`。案件删除仅把 `case_id` 置空。关联/解除关联不改 `source`、稳定 ID 或已复制进展。

## 4. “一键复制到案件进展”事务契约（冻结）

### 4.1 目标选择

1. 已关联事项：目标案件固定为 `todo.case_id`；若调用方传入不同案件，失败关闭。
2. 未关联事项：必须由用户在 UI 选择案件后传 `target_case_id`；后端再次验证案件存在。
3. 软删事项不得复制；是否完成不影响人工复制。

### 4.2 单事务步骤

同一个数据库事务内依次执行，任一步失败全部回滚：

1. 查询未软删待办；不存在或已软删即失败。
2. 解析并验证目标案件。
3. 计算进展时间 `effective_item_at = COALESCE(todo.item_at, todo.due_date || 'T00:00', todo.created_at)`。
4. 以新 UUID 写一条 `case_work_items`：
   - `case_id = target_case_id`
   - `occurred_at = effective_item_at`（现有列表按该字段排序，因此会按事项时间自动穿插；依据 `case_work_items.rs:140-157`）
   - `work_type = 'todo'`
   - `title/content` 为复制当时快照
   - `source = 'case_todo:' || todo.source`
   - `external_source = 'case_todo'`
   - `external_record_id = todo.id`
   - `confirmation_status = 'confirmed'`
   - `raw_payload_json` 保存 `source_item_id/source/item_at/copied_at` 与标题、内容快照
5. 依靠现有 `idx_case_work_items_external_record` 唯一约束防并发重复；提交后返回 `CopyTodoResult`。

返回结果固定为 `{work_item_id, case_id, created, outcome_code}`：首次为 `created=true/TODO_PROGRESS_CREATED`；重复点击或并发竞争返回同一 `work_item_id` 和 `created=false/TODO_PROGRESS_ALREADY_EXISTS`，不能生成第二条。若既有幂等记录的案件与当前目标不一致，失败关闭为 `TODO_PROGRESS_LINK_CONFLICT`。若既有进展已经软删，也不得悄悄恢复或另建，返回 `created=false/TODO_PROGRESS_ALREADY_EXISTS_DELETED`，交由用户在进展侧处理。

复制完成后两边完全解耦：修改、完成、解除关联、软删或恢复待办，均不得 UPDATE/DELETE 已生成的 `case_work_items`；进展侧修改/软删也不得回写待办。

## 5. 稳定错误码（冻结）

后端应定义 `TodoError`，Tauri 边界至少保留稳定 `code`（可用结构化 `{code,message}`；若为字符串，必须以 `CODE: ` 开头），UI 不得靠中文全文分支。

| 错误码 | 条件 |
| --- | --- |
| `TODO_NOT_FOUND` | 稳定 ID 不存在 |
| `TODO_DELETED` | 对软删事项执行修改、完成、关联或复制 |
| `TODO_TITLE_REQUIRED` | 标题 trim 后为空 |
| `TODO_ITEM_AT_INVALID` | 非允许的 ISO 本地日期/时间 |
| `TODO_CASE_NOT_FOUND` | 关联或复制目标案件不存在 |
| `TODO_TARGET_CASE_REQUIRED` | 未关联事项复制时未选案件 |
| `TODO_TARGET_CASE_MISMATCH` | 已关联事项被要求复制到其他案件 |
| `TODO_SOURCE_INVALID` | 内部同步层传入非三值来源 |
| `TODO_PROGRESS_LINK_CONFLICT` | 幂等键已占用但目标/实体语义不一致 |
| `TODO_WRITE_FAILED` | 事务写入或提交失败；对外不暴露 SQL/业务数据 |

`TODO_PROGRESS_CREATED`、`TODO_PROGRESS_ALREADY_EXISTS`、`TODO_PROGRESS_ALREADY_EXISTS_DELETED` 是结果码，不作为异常。

## 6. UI 入口与交互边界（冻结）

1. 顶部模块导航在“工作”组新增 `id='todos'`、标签 **“待办事项”**，不得把案件看板模块称为“收件箱”；“收件箱”仅指飞书表。
2. 新建 `src/modules/todos/TodoModule.tsx`（允许拆分同目录组件/纯逻辑）：提供新增、查看、编辑、完成/撤销、关联/解除关联、软删/恢复、来源与冲突提示、复制状态。
3. 未关联事项显示“未关联案件”；点击“一键复制到案件进展”时打开案件选择器。已关联事项直接显示确认，不允许改投其他案件。
4. 复制后显示目标案件和“已复制”状态；重复点击只展示既有结果。
5. 保留 `TodosCard` 作为案件详情快捷入口，复用同一后端模型；保留首页 `TodoSummary`，但其未关联分组不可调用 `onPickCase(null)`，应引导进入“待办事项”。
6. `HomeView` 的日历仍读取兼容 `due_date`；v0.8.4 不借本任务重构独立日历或滴答同步。
7. 浅色/深色主题、空状态、保存中、软删、已完成、同步冲突、复制成功/已存在/失败均须覆盖；不得继续用静默 `.catch(() => {})` 隐藏主模块加载错误。

## 7. 确定性测试与失败原子性

### 7.1 Rust/SQLite

1. **迁移兼容**：从只应用到 0063 的内存/隔离副本建立含“未完成、已完成、有/无 due_date”的旧行；应用 0064 后比较行数、ID 集合和旧字段，断言新字段回填、外键 `SET NULL`、索引及三个 device-sync trigger 存在。
2. **案件删除**：删除案件后待办仍在、`case_id IS NULL`；不得出现级联删除。
3. **全局 CRUD**：覆盖新增未关联、关联、解除、修改正文/时间、完成、撤销、软删、恢复；默认列表不含软删，回收站仅含软删。
4. **兼容命令**：旧 `due_date` 输入正确投影 `item_at`；旧按案列表与首页列表不丢历史行；未关联项不会被 JOIN 过滤。
5. **复制字段**：逐列断言标题、内容、`occurred_at`、真实来源快照、源事项 ID、目标案件和确认状态。
6. **串行/并发防重**：同一事项连续两次及两个并发任务复制，`case_work_items` 最终均只有一行并返回同一 ID。
7. **时间排序**：先复制晚事项、后复制早事项，列表仍按 `occurred_at` 排列。
8. **失败注入**：用 BEFORE INSERT trigger 强制 `case_work_items` 写失败，断言零进展、待办字段不变、事务可再次成功；目标案件不存在同样零写入。
9. **复制后解耦**：复制后修改/完成/软删待办，进展快照逐字段不变；软删进展后再次复制不新建、不恢复。
10. **设备同步**：registry 序列化含新增业务字段，nullable `case_id` 可导出/导入；软删产生 upsert 事件而不是物理 tombstone。

### 7.2 前端

- 为纯 view-model 增加 `.test.mjs`：状态过滤、未关联分组、事项时间/日期投影、复制按钮目标选择、结果码映射、软删/恢复状态。
- 更新既有 UI 契约测试，确认三个案件详情仍挂载 `TodosCard`。
- 实现阶段门禁命令：`pnpm test:logic`、`pnpm build`、`powershell.exe -ExecutionPolicy Bypass -File scripts/run-windows-rust-tests.ps1`。仓库是 `pnpm-lock.yaml`，不得混用 npm。

## 8. 后续任务文件范围与依赖（冻结）

为避免并行覆盖，按以下顺序与独占范围拆分；共享聚合文件只允许集成任务串行修改。

| 后续任务 | 独占文件/范围 | 依赖与禁止项 |
| --- | --- | --- |
| `V084-T1-TODO-DB` | `src-tauri/migrations/0064_global_todos.sql`；`src-tauri/src/db/todos.rs`；`src-tauri/src/device_sync/registry.rs` 的 `case_todo` policy；该模块 Rust 测试 | 先完成；不改飞书同步文件、更新器、版本 |
| `V084-T1-TODO-UI` | `src/modules/todos/**`；`src/components/TodosCard.tsx`；`src/components/HomeView.tsx` 的 todo 专属区块；`src/lib/api.ts` todo 区块；`src/lib/types.ts` todo 类型；相应 `.test.mjs` | 依赖 DB 契约；不改飞书设置/同步 UI |
| `V084-F1-TODO-FEISHU` | 从 `0065_*` 起的独立 todo 同步账本迁移；新 `src-tauri/src/db/todo_feishu_sync.rs` 及专属测试；飞书“收件箱”专属 UI/映射文件 | 依赖 0064；不得把 sync version/hash/conflict 写回 `case_todos`，不得修改 `todos.rs` |
| `V084-TODO-INTEGRATION` | `src-tauri/src/lib.rs` 命令声明/注册；`src-tauri/src/db/migration_safety.rs`；`migration_lineage_tests.rs`；`src/components/ModuleTabs.tsx`；`src/App.tsx`；必要的 `db/mod.rs` 导出 | 等 T1 与 F1 均提交后串行集成；同时登记 M64/M65 schema sentinels，避免并行冲突 |

若主控改为单线程实现，也仍应遵守迁移号：本地业务表固定 `0064`，飞书同步账本从 `0065` 起；不得让两个线程同时编辑 `lib.rs`、`migration_safety.rs`、`ModuleTabs.tsx` 或 `App.tsx`。

## 9. 与 F1 的已对齐边界

- `stable_item_id = case_todos.id`。
- `source` 三值及 `item_at`、`deleted_at` 属业务表；`due_date` 仅作旧 UI/API/首页日历兼容。
- 同步版本、基线哈希、本地/远端内容哈希、冲突、远端 record ID、预览和审计属于 F1 独立账本（0065+）。
- 飞书“收件箱”业务字段为标题、内容、事项时间、状态、完成时间、关联案件（可空）、来源；技术字段为事项 ID、同步版本、基线哈希、内容哈希、删除状态。
- Hermes/NAS 只能通过飞书接口与案件看板同步层交换，禁止直写 SQLite。

## 10. 主控审阅要点

本报告无待补的产品选择。建议主控在 N0 合并契约中直接采纳：演进 `case_todos`、`item_at` 为可空事项时间、`due_date` 为兼容投影、复制到 `case_work_items` 使用 `external_source='case_todo' + external_record_id=todo.id` 单事务防重，以及 T1/F1 业务表与同步账本分层。主控如改动任一字段名或迁移号，应先同步修改 F1 字段映射，不能在实现线程各自漂移。
