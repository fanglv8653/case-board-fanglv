# 验收记录

## 基本信息

- task_id: V083-M1-COMPAT36
- reviewer: 04-project-master
- decision: rejected
- reviewed_at: 2026-08-09T22:43:24+08:00

## 结论

- summary: 退回修复：删除全局ignore_missing，显式补入仅v36迁移元数据并保持unknown二次校验；封闭预检至迁移竞态；补STRICT/WITHOUT ROWID/CHECK/UNIQUE/COLLATE负例和完整原始行断言。
