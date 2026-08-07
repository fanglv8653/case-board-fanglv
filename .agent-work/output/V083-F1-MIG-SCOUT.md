# V083-F1-MIG-SCOUT：F1 是否需要 0064 迁移只读判定

状态：`submitted_for_review`

## 一、结论

建议冻结：**F1 不新增 `0064`，不修改现有 schema。**

0049、0051、0061、0062 已能完整表达 F1 的安全终态：

- 案件 link：`status='archived'`；
- inbox：`status='pending_binding'`、`bound_case_id=NULL`、`resolved_at=NULL`、`auto_bind_suppressed=1`；
- 字段候选：`review_status='superseded'`；
- 明细候选：删除案件时由现有 `ON DELETE CASCADE` 清除，因此不会保留可操作的 pending 候选；
- 解绑/孤立修复审计：复用 `action='unbind'`，对已删除案件写 `previous_case_id=NULL`；
- 历史 active 孤立绑定：由查询动态识别并事务性修复，不把“orphan”保存为新的长期业务状态。

没有正确性所必需的新列、CHECK、FK 或索引。为占号创建空迁移反而增加谱系与升级风险。

## 二、DDL 依据

### 0049：link 与 inbox 已有完整状态

`0049_feishu_case_management_sync.sql`：

- 第 9—30 行：`feishu_sync_links` 的 `status` 已限定为 `pending/active/archived`；远端身份和本地身份分别有唯一约束；
- 第 31—32 行：`idx_feishu_sync_links_local(entity_type, local_entity_id, status)` 支持按案件查找 active link；
- 第 68—86 行：`feishu_sync_inbox.status` 已限定为 `pending_binding/bound/ignored/archived`，`bound_case_id` 可空；
- 第 84 行：`bound_case_id -> cases(id) ON DELETE SET NULL`；
- 第 85 行：`UNIQUE(app_token, table_id, record_id)` 支持按远端身份原位恢复同一 inbox；
- 第 87—88 行：现有 inbox 状态索引支持 pending 列表。

因此，`archived link + pending_binding inbox` 已能表示“远端记录仍存在，但不再绑定本地案件”。

### 0051：禁止自动重绑与安全审计已存在

`0051_feishu_manual_binding.sql`：

- 第 4—5 行：`auto_bind_suppressed INTEGER NOT NULL DEFAULT 0 CHECK(IN (0,1))`；
- 第 7—20 行：binding audit 已允许 `unbind`；`previous_case_id`、`next_case_id` 均可空；
- 第 18—19 行：两个案件 FK 均为 `ON DELETE SET NULL`。

故历史审计引用案件时，删除案件会自动归零；在案件已不存在的修复路径中，直接写 `previous_case_id=NULL` 不触发 FK，也符合既定验收要求。

### 0061/0062：旧候选可失效

`0061_feishu_controlled_bidirectional.sql` 第 4—7 行允许字段候选进入 `superseded`；第 31—32 行已有 review 状态索引。

`0062_feishu_entity_change_previews.sql`：

- 第 18—20 行允许明细候选进入 `superseded`；
- 第 24 行的 `case_id -> cases(id) ON DELETE CASCADE` 表明案件删除后该类候选会直接删除；
- 第 28—29 行已有 pending/review 状态索引。

这里必须区分两类候选：字段候选只关联 link，link 会归档保留，所以需显式 `superseded`；明细候选直接关联 case，删除 case 后级联消失。若验收只要求“删除后不再可操作”，现有结构已满足。只有未来明确要求“删除案件后仍永久保留每条明细候选并显示 superseded 历史”时，才需重建 0062 表并改变 FK；这不是当前 F1 要求，也不建议在热修复中扩大。

0052 与 0060 只分别定义入站明细业务表的外部状态、案件阶段领域门禁；它们不限制上述孤立绑定修复，也不需要变化。

## 三、源码事实与缺口

### 当前删除会留下 active orphan

`src-tauri/src/db/cases.rs` 第 335—341 行仅执行：

```sql
DELETE FROM cases WHERE id = ?;
```

link 的 `local_entity_id` 没有案件 FK，因此 link 不会随案件删除，形成 active orphan。

### 当前拉取会整批失败

`src-tauri/src/db/feishu_sync.rs`：

- 第 301—326 行只凭 active link 取得 `local_entity_id`，未确认案件仍存在；
- 第 361—379 行随后把该失效 ID 写入 inbox 的 `bound_case_id`，触发 FK；
- 即使绕过该处，第 411—433 行读取本地案件也会失败；
- 明细预演在 `feishu_entities.rs` 第 224—237 行同样把所有 active link 直接映射为本地案件，后续读取已删除案件会失败。

这都是实现顺序缺口，不是 schema 表达能力不足。

### 当前孤立解绑审计会 FK 失败

`feishu_sync.rs` 第 1560—1600 行的 `unbind_case` 会把 link 中的失效 `local_entity_id` 原样写入 `previous_case_id`（第 1593—1595 行）。当案件已删除时，这违反 0051 FK。修复只需先判断案件是否存在并绑定 `NULL`，无需迁移。

### 当前 UI 查询可动态识别 orphan

`get_preview` 已对 link 使用 `LEFT JOIN cases`（第 772—803 行），但当前以 `local_entity_id` 兜底显示 UUID。查询可直接增加 `c.id IS NULL AS is_orphaned`，生成稳定 `FEISHU_ORPHAN_BINDING` 状态并禁用采用/写回按钮，不需要持久化新列。

## 四、建议实现 SQL（无需迁移）

### A. 案件删除事务

以下步骤必须和 `DELETE FROM cases` 位于同一个 SQLite 事务；先查询该案件全部 active case link，对每条 link 执行：

```sql
SELECT id, app_token, table_id, record_id
FROM feishu_sync_links
WHERE entity_type='case'
  AND local_entity_id=?1
  AND status='active';

SELECT id, status
FROM feishu_sync_inbox
WHERE app_token=?1 AND table_id=?2 AND record_id=?3;

UPDATE feishu_sync_field_previews
SET review_status='superseded', resolved_at=datetime('now')
WHERE link_id=?1 AND review_status='pending';

INSERT INTO feishu_sync_binding_audits (
    id, inbox_id, action, previous_status, next_status,
    previous_case_id, next_case_id
)
VALUES (?1, ?2, 'unbind', ?3, 'pending_binding', NULL, NULL);

UPDATE feishu_sync_inbox
SET status='pending_binding',
    bound_case_id=NULL,
    resolved_at=NULL,
    auto_bind_suppressed=1,
    updated_at=datetime('now')
WHERE id=?1;

UPDATE feishu_sync_links
SET status='archived', updated_at=datetime('now')
WHERE id=?1 AND status='active';

DELETE FROM cases WHERE id=?1;
```

说明：

- audit 的 `previous_case_id` 必须显式为 `NULL`；已删除案件 ID 仍保留在 archived link 的 `local_entity_id`，不丢失定位线索；
- 0062 明细候选随最后一步删除 case 自动级联清理；
- 任一步失败均回滚，不能出现“case 已删但 link 仍 active”的新现场；
- 全流程只写本地 SQLite，不调用飞书网络。

### B. 历史孤立绑定扫描与修复

在每次拉取事务进入业务预演前识别：

```sql
SELECT l.id, l.local_entity_id, l.app_token, l.table_id, l.record_id
FROM feishu_sync_links l
LEFT JOIN cases c
  ON l.entity_type='case' AND c.id=l.local_entity_id
WHERE l.entity_type='case'
  AND l.status='active'
  AND c.id IS NULL;
```

逐条复用 A 中的字段候选失效、inbox pending/suppressed、link archived 和 `previous_case_id=NULL` 审计步骤，再继续处理其他有效记录。若历史 link 没有 inbox，则当前远端记录 upsert inbox 时必须显式保留 `auto_bind_suppressed=1`，不能让默认值 0 再次自动绑定。

### C. 孤立解绑

`unbind_case` 读取 link 后先执行：

```sql
SELECT EXISTS(SELECT 1 FROM cases WHERE id=?1);
```

审计参数使用：

```text
previous_case_id = CASE WHEN exists THEN link.local_entity_id ELSE NULL END
```

其余 archive/pending/suppressed SQL 可复用现有实现。这样历史孤立解绑不再触发 FK。

## 五、索引判定

不需要新索引：

- 按指定案件找 active link：0049 的 `(entity_type, local_entity_id, status)`；
- 按远端记录找 link：0049 的 `UNIQUE(app_token, table_id, record_id, slot_key)`；
- 按远端记录找 inbox：0049 的 `UNIQUE(app_token, table_id, record_id)`；
- pending inbox：`idx_feishu_sync_inbox_status`；
- pending field/entity preview：0061/0062 的 review-status 索引。

历史 orphan 扫描是本地 active case link 的有界一致性检查，不构成新增专用索引的充分理由。若未来真实数据量证明该扫描成为热点，再以查询计划和基准决定 `(entity_type,status,local_entity_id)`，不应在本轮预先扩 schema。

## 六、migration sentinel 判定

**无需新增 M64 sentinel。** 当前生产预检已经检查：

- M49 link/inbox 表及 status 列；
- M51 binding audit 表、`auto_bind_suppressed`；
- M61/M62 两类 preview 的 `review_status`；
- inbox `bound_case_id ON DELETE SET NULL`；
- binding audit `previous_case_id ON DELETE SET NULL`；
- entity preview `case_id ON DELETE CASCADE`；
- inbox 和 entity-preview 状态索引。

`migration_safety.rs` 当前未对 `binding_audits.next_case_id` 的 SET NULL FK 设置独立 sentinel。它不影响本轮 `previous_case_id=NULL` 修复，也不是新增 0064 的理由；若主控后续希望对 0051 做对称加固，可在既有版本下补 `M51.fk.binding_audit.next_case_id`，但应作为 M1 sentinel 完整性增强单独验收。

## 七、风险边界与触发重新评估的条件

本轮没有读取正式数据库、飞书 Base、凭据或业务正文；没有运行写入型数据库夹具；没有修改产品代码或迁移。

只有以下任一范围变化才重新评估 0064：

1. 要求持久化独立 `orphaned` 状态，而不是立即修复为 archived/pending；
2. 要求审计必须区分 `manual_unbind` 与 `case_deleted/orphan_repair`，且不接受现有 `unbind + archived link` 证据；
3. 要求案件删除后永久保留 0062 明细候选行，而非安全级联删除；
4. 真实规模与查询计划证明现有索引导致不可接受的 orphan 扫描性能。

在当前冻结验收下，上述条件均不成立。
