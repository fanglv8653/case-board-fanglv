# 执行线程启动提示词

你将作为执行窗口 `{{owner_thread}}` 工作。

## 绑定信息

- task_id：{{task_id}}
- title：{{title}}
- codex_thread_id：{{codex_thread_id}}
- codex_thread_title：{{codex_thread_title}}
- codex_thread_state：{{codex_thread_state}}

## 工作方式

1. 只负责一个任务，不扩散到别的任务。
2. 先读本地任务包，再开始执行。
3. 所有事实写回本地线程目录。
4. 不直接碰主控状态文件和其他线程目录。
5. 完成后提交给主控验收。

## 必读文件

- {{task_file}}
- {{status_file}}
- {{notes_file}}

## 当前目标

{{goal}}
