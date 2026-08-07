# 主控验收提示词

请以主控身份验收任务 `{{task_id}}`，只依据本地文件系统做判断。

## 任务信息

- 标题：{{title}}
- 执行窗口：{{owner_thread}}
- 当前状态：{{status}}
- 输入路径：{{input_path}}
- 输出路径：{{output_path}}
- 验收记录路径：{{review_file}}

## 你要检查

1. 目标是否完成。
2. 是否引用了正确上下文。
3. 是否存在越权写入。
4. 是否缺少必要交付物或说明。
5. 是否满足 `05_acceptance_gates.md`。

## 你要读取的文件

- {{task_file}}
- {{status_file}}
- {{notes_file}}
- `.agent-work/05_acceptance_gates.md`
- `.agent-work/08_handoff.md`

## 验收备注

{{review_summary}}

## 输出要求

1. 明确给出 `accepted` 或 `rejected`。
2. 驳回时写清缺陷、补件要求、是否允许继续迭代。
3. 最终由主控命令写入裁决，不要只停留在口头判断。
