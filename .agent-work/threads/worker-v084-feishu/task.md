# 线程任务包

## 任务信息

- task_id: V084-N0-FEISHU
- title: v0.8.4飞书收件箱双向同步契约
- goal: 只读审计现有飞书受控同步，冻结待办收件箱字段、版本基线、冲突删除去重和授权边界
- owner_thread: worker-v084-feishu
- reviewer: 04-project-master
- input_path: .agent-work/output/V084-更新与发布流程待办.md;.agent-work/53_v084_n0_dispatch_plan.md;.agent-work/54_v084_n0_acceptance_rubric.md;src-tauri/src/db/feishu_sync.rs;src-tauri/src/db/feishu_entities.rs;src-tauri/src/feishu.rs;src/modules/tools/FeishuSyncPreview.tsx;src/lib/feishuAutoPullCore.ts;src/lib/api.ts
- output_path: .agent-work/output/V084-N0-FEISHU.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
