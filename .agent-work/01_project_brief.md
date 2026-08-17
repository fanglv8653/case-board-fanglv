# 01 项目简报

## 目标

发布 v0.8.4，完成：

1. Windows 应用内更新后可靠退出、安装、重启及新版本首次启动成功提示；
2. Release 资产与 `release/version.json`、`release/latest.json` 的最终原子收敛及 ASCII 资产命名；
3. 新增全局“待办事项”板块，允许事项暂未关联案件；
4. 通过飞书多维表格“收件箱”与 Hermes 进行受控双向交换；
5. 人工一键复制到案件进展，按事项时间排序并通过源事项 ID 防重。

## 非目标

- Hermes 或 NAS 直接读写案件看板 SQLite；
- 自动判断案件归属或自动写入案件进展；
- 第一阶段传播物理删除；
- 未经用户确认直接改写冲突数据；
- 读取或修改正式数据库、正式飞书 Base、NAS 同步目录或凭据；
- RC 验收前更新公开清单或创建正式 Release。

## 权威计划

- `.agent-work/output/V084-更新与发布流程待办.md`
- `.agent-work/53_v084_n0_dispatch_plan.md`
- `.agent-work/54_v084_n0_acceptance_rubric.md`
