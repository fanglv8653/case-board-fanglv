# 线程任务包

## 任务信息

- task_id: V084-N0-TODO
- title: v0.8.4待办本地模型与案件进展复制契约
- goal: 只读审计现有待办、案件进展和页面导航，冻结兼容迁移、全局待办及人工复制防重契约
- owner_thread: worker-v084-todo
- reviewer: 04-project-master
- input_path: .agent-work/output/V084-更新与发布流程待办.md;.agent-work/53_v084_n0_dispatch_plan.md;.agent-work/54_v084_n0_acceptance_rubric.md;src-tauri/migrations/0024_case_todos.sql;src-tauri/migrations/0027_todo_due_date.sql;src-tauri/src/db/todos.rs;src-tauri/src/db/case_work_items.rs;src/components/TodosCard.tsx;src/components/HomeView.tsx;src/components/ModuleTabs.tsx;src/lib/api.ts
- output_path: .agent-work/output/V084-N0-TODO.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
