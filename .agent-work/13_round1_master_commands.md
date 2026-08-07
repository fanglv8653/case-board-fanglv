# 13 第一轮主控命令

```powershell
$py='C:\Users\William Feng\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
& $py scripts\agent_workflow.py report-markdown --root .
& $py scripts\agent_workflow.py audit --root .
& $py scripts\agent_workflow.py dispatch --root . --task-id <TASK> --actor 04-project-master
& $py scripts\agent_workflow.py prepare-review-prompt --root . --task-id <TASK>
& $py scripts\agent_workflow.py review --root . --task-id <TASK> --actor 04-project-master --decision accepted --summary '<EVIDENCE>'
```
