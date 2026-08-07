# 线程任务包｜V083-N0-GATE

## 目标

独立只读审计 N0 验收门禁、测试覆盖和跨任务冲突，输出可供主控逐项复核的矩阵。

## 必读

- `agent-work/output/V083-20260803_下一轮待开发计划.md`
- `agent-work/tasks/V083-N0_开发前准备任务包.md`
- `.agent-work/01_project_brief.md`
- `.agent-work/02_repo_snapshot.md`
- `.agent-work/05_acceptance_gates.md`
- `.agent-work/10_round1_dispatch_plan.md`
- `.agent-work/11_round1_acceptance_rubric.md`
- `scripts/run-windows-rust-tests.ps1`
- `scripts/release-gate.mjs`

## 允许写入

- `.agent-work/threads/worker-gate/`；
- `.agent-work/output/V083-N0-GATE.md`。

## 禁止

- 不改产品源码、测试源码、迁移、计划、看板或其他线程；
- 不运行生产构建、不读正式数据、不提交 Git；
- 不自批 MIG/SYNC，只指出缺口和复验要求。

## 交付

按 MIG/SYNC/综合三部分列出：必须证据、可接受跳过、拒绝条件、命令、潜在重叠文件、M1/S1 前置输入和最优后续顺序。
