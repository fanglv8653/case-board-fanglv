from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable, List, Sequence


MASTER_ACTORS = {"00-master", "04-project-master"}
MASTER_ONLY_STATUSES = {"dispatched", "accepted", "rejected"}
WORKER_STATUSES = {"inProgress", "submitted_for_review"}
ALL_STATUSES = {"todo", "dispatched", "inProgress", "submitted_for_review", "accepted", "rejected"}
BOARD_HEADERS = [
    "task_id",
    "title",
    "owner_thread",
    "status",
    "input_path",
    "output_path",
    "reviewer",
    "updated_at",
]
BOARD_HEADING = "## 任务看板"
STATUS_HEADING = "## 元数据"


class WorkflowError(RuntimeError):
    pass


@dataclass
class TaskRecord:
    task_id: str
    title: str
    owner_thread: str
    status: str
    input_path: str
    output_path: str
    reviewer: str
    updated_at: str

    @classmethod
    def from_row(cls, row: Dict[str, str]) -> "TaskRecord":
        return cls(
            task_id=row["task_id"],
            title=row["title"],
            owner_thread=row["owner_thread"],
            status=row["status"],
            input_path=row["input_path"],
            output_path=row["output_path"],
            reviewer=row["reviewer"],
            updated_at=row["updated_at"],
        )

    def to_row(self) -> List[str]:
        return [
            self.task_id,
            self.title,
            self.owner_thread,
            self.status,
            self.input_path,
            self.output_path,
            self.reviewer,
            self.updated_at,
        ]


def now_iso() -> str:
    return datetime.now(timezone.utc).astimezone().replace(microsecond=0).isoformat()


def ensure_root(root: Path) -> Path:
    work_root = root / ".agent-work"
    if not work_root.exists():
        raise WorkflowError(f"missing scaffold: {work_root}")
    return work_root


def board_file(root: Path) -> Path:
    return root / ".agent-work" / "09_dispatch_board.md"


def status_file(root: Path) -> Path:
    return root / ".agent-work" / "00_status.md"


def handoff_file(root: Path) -> Path:
    return root / ".agent-work" / "08_handoff.md"


def log_file(root: Path) -> Path:
    return root / ".agent-work" / "06_operation_log.md"


def review_dir(root: Path) -> Path:
    return root / ".agent-work" / "review"


def thread_dir(root: Path, thread_id: str) -> Path:
    return root / ".agent-work" / "threads" / thread_id


def output_path(root: Path, relative_path: str) -> Path:
    return root / relative_path


def template_file(root: Path, name: str) -> Path:
    return root / ".agent-work" / "templates" / name


def read_lines(path: Path) -> List[str]:
    return path.read_text(encoding="utf-8").splitlines()


def write_text(path: Path, text: str) -> None:
    path.write_text(text.rstrip() + "\n", encoding="utf-8")


def find_section(lines: Sequence[str], heading: str) -> tuple[int, int]:
    start = None
    for idx, line in enumerate(lines):
        if line.strip() == heading:
            start = idx
            break
    if start is None:
        raise WorkflowError(f"missing section: {heading}")
    end = len(lines)
    for idx in range(start + 1, len(lines)):
        if lines[idx].startswith("## ") and lines[idx].strip() != heading:
            end = idx
            break
    return start, end


def parse_table(lines: Sequence[str], heading: str) -> List[Dict[str, str]]:
    start, end = find_section(lines, heading)
    table_lines = [line for line in lines[start + 1 : end] if line.strip()]
    if len(table_lines) < 2:
        return []
    header_line = table_lines[0]
    if not header_line.startswith("|"):
        return []
    headers = [cell.strip() for cell in header_line.strip("|").split("|")]
    rows: List[Dict[str, str]] = []
    for line in table_lines[2:]:
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) != len(headers):
            continue
        rows.append(dict(zip(headers, cells)))
    return rows


def replace_table(lines: Sequence[str], heading: str, headers: Sequence[str], rows: Iterable[Sequence[str]]) -> str:
    start, end = find_section(lines, heading)
    rendered = [heading, "", "| " + " | ".join(headers) + " |", "| " + " | ".join(["---"] * len(headers)) + " |"]
    for row in rows:
        rendered.append("| " + " | ".join(str(cell) for cell in row) + " |")
    rendered.append("")
    updated = list(lines[:start]) + rendered + list(lines[end:])
    return "\n".join(updated)


def load_board(root: Path) -> Dict[str, TaskRecord]:
    rows = parse_table(read_lines(board_file(root)), BOARD_HEADING)
    return {row["task_id"]: TaskRecord.from_row(row) for row in rows}


def save_board(root: Path, records: Dict[str, TaskRecord]) -> None:
    path = board_file(root)
    rows = [records[key].to_row() for key in sorted(records)]
    write_text(path, replace_table(read_lines(path), BOARD_HEADING, BOARD_HEADERS, rows))


def append_markdown_row(path: Path, row: Sequence[str]) -> None:
    lines = read_lines(path)
    lines.append("| " + " | ".join(row) + " |")
    write_text(path, "\n".join(lines))


def append_log(root: Path, actor: str, action: str, task_id: str, details: str) -> None:
    append_markdown_row(log_file(root), [now_iso(), actor, action, task_id, details])


def append_handoff(root: Path, source: str, target: str, task_id: str, kind: str, message: str, paths: str) -> None:
    append_markdown_row(handoff_file(root), [now_iso(), source, target, task_id, kind, message, paths])


def parse_bullets(path: Path) -> Dict[str, str]:
    values: Dict[str, str] = {}
    for line in read_lines(path):
        stripped = line.strip()
        if stripped.startswith("- ") and ":" in stripped:
            key, value = stripped[2:].split(":", 1)
            values[key.strip()] = value.strip()
    return values


def load_template(root: Path, name: str) -> str:
    path = template_file(root, name)
    if not path.exists():
        raise WorkflowError(f"missing template: {name}")
    return path.read_text(encoding="utf-8")


def render_template(template: str, values: Dict[str, str]) -> str:
    rendered = template
    for key, value in values.items():
        rendered = rendered.replace(f"{{{{{key}}}}}", value)
    return rendered


def write_bullets(path: Path, title: str, heading: str, values: Dict[str, str], footer: Sequence[str] | None = None) -> None:
    lines = [title, "", heading, ""]
    for key, value in values.items():
        lines.append(f"- {key}: {value}")
    if footer:
        lines.extend(["", *footer])
    write_text(path, "\n".join(lines))


def sync_status_summary(root: Path) -> None:
    records = load_board(root)
    counts = {status: 0 for status in ALL_STATUSES}
    for record in records.values():
        counts[record.status] += 1
    path = status_file(root)
    lines = read_lines(path)
    updated: List[str] = []
    replacements = {
        "- last_sync_at:": f"- last_sync_at: {now_iso()}",
        "| total_tasks |": f"| total_tasks | {len(records)} |",
        "| todo_tasks |": f"| todo_tasks | {counts['todo']} |",
        "| dispatched_tasks |": f"| dispatched_tasks | {counts['dispatched']} |",
        "| in_progress_tasks |": f"| in_progress_tasks | {counts['inProgress']} |",
        "| submitted_tasks |": f"| submitted_tasks | {counts['submitted_for_review']} |",
        "| accepted_tasks |": f"| accepted_tasks | {counts['accepted']} |",
        "| rejected_tasks |": f"| rejected_tasks | {counts['rejected']} |",
    }
    for line in lines:
        replaced = False
        for prefix, new_line in replacements.items():
            if line.startswith(prefix):
                updated.append(new_line)
                replaced = True
                break
        if not replaced:
            updated.append(line)
    write_text(path, "\n".join(updated))


def validate_transition(actor: str, current: str, target: str, owner_thread: str) -> None:
    if target not in ALL_STATUSES:
        raise WorkflowError(f"invalid target status: {target}")
    if target in MASTER_ONLY_STATUSES and actor not in MASTER_ACTORS:
        raise WorkflowError(f"actor {actor} cannot set status to {target}")
    if target in WORKER_STATUSES:
        if actor in MASTER_ACTORS:
            raise WorkflowError(f"master actor {actor} cannot set worker status {target}")
        if actor != owner_thread:
            raise WorkflowError(f"actor {actor} does not own thread {owner_thread}")
    allowed = {
        "todo": {"dispatched"},
        "dispatched": {"inProgress"},
        "inProgress": {"submitted_for_review"},
        "submitted_for_review": {"accepted", "rejected", "inProgress"},
        "rejected": {"inProgress", "submitted_for_review"},
        "accepted": set(),
    }
    if target not in allowed[current]:
        raise WorkflowError(f"invalid transition: {current} -> {target}")


def status_payload_for_task(task: TaskRecord) -> Dict[str, str]:
    return {
        "task_id": task.task_id,
        "thread_id": task.owner_thread,
        "role": "worker",
        "status": task.status,
        "updated_at": task.updated_at,
        "deliverable_path": task.output_path,
        "last_submission": "-",
    }


def read_thread_meta(root: Path, thread_id: str) -> Dict[str, str]:
    path = thread_dir(root, thread_id) / "thread_meta.md"
    if not path.exists():
        raise WorkflowError(f"missing thread meta: {thread_id}")
    return parse_bullets(path)


def write_thread_meta(root: Path, thread_id: str, values: Dict[str, str]) -> None:
    path = thread_dir(root, thread_id) / "thread_meta.md"
    footer = [
        "## 说明",
        "",
        "- `codex_thread_id` 用于登记真实 Codex 线程。",
        "- `codex_thread_title` 与 `codex_thread_state` 只做本地事实源同步。",
    ]
    write_bullets(path, "# 线程元数据", STATUS_HEADING, values, footer)


def write_thread_status(root: Path, task: TaskRecord, last_submission: str = "-") -> None:
    path = thread_dir(root, task.owner_thread) / "status.md"
    current = status_payload_for_task(task)
    if path.exists():
        current.update(parse_bullets(path))
    current.update(
        {
            "task_id": task.task_id,
            "thread_id": task.owner_thread,
            "role": "worker",
            "status": task.status,
            "updated_at": task.updated_at,
            "deliverable_path": task.output_path,
            "last_submission": last_submission,
        }
    )
    footer = [
        "## 最近动作",
        "",
        "- 说明：执行窗口只能修改本线程目录、交付物和通知记录。",
        "- 说明：最终 accepted 或 rejected 只能由主控写入。",
    ]
    write_bullets(path, "# 线程状态", STATUS_HEADING, current, footer)


def create_thread_package(root: Path, record: TaskRecord, goal: str) -> None:
    directory = thread_dir(root, record.owner_thread)
    (directory / "deliverable").mkdir(parents=True, exist_ok=True)
    meta = {
        "thread_id": record.owner_thread,
        "role": "worker",
        "task_id": record.task_id,
        "title": record.title,
        "reviewer": record.reviewer,
        "owner_scope": str(directory.relative_to(root)),
        "codex_thread_id": "pending",
        "codex_thread_title": "pending",
        "codex_thread_state": "unbound",
    }
    write_thread_meta(root, record.owner_thread, meta)
    task_lines = [
        "# 线程任务包",
        "",
        "## 任务信息",
        "",
        f"- task_id: {record.task_id}",
        f"- title: {record.title}",
        f"- goal: {goal}",
        f"- owner_thread: {record.owner_thread}",
        f"- reviewer: {record.reviewer}",
        f"- input_path: {record.input_path}",
        f"- output_path: {record.output_path}",
        "",
        "## 允许操作",
        "",
        "1. 只修改本线程目录及指定输出路径。",
        "2. 更新本线程的状态、备注和交付物。",
        "3. 向 `08_handoff.md` 写入提交通知。",
        "",
        "## 禁止操作",
        "",
        "1. 修改 `00_status.md`。",
        "2. 直接写入 `09_dispatch_board.md` 的主控字段。",
        "3. 覆盖其他线程目录。",
        "4. 绕过主控直接宣称任务通过。",
    ]
    write_text(directory / "task.md", "\n".join(task_lines))
    write_thread_status(root, record)
    write_text(directory / "notes.md", "# 线程备注\n\n- 仅记录本线程的过程说明。\n")


def get_task_record(root: Path, task_id: str) -> TaskRecord:
    records = load_board(root)
    record = records.get(task_id)
    if record is None:
        raise WorkflowError(f"missing task: {task_id}")
    return record


def build_prompt_context(root: Path, record: TaskRecord) -> Dict[str, str]:
    meta = read_thread_meta(root, record.owner_thread)
    return {
        "task_id": record.task_id,
        "title": record.title,
        "goal": parse_task_goal(root, record.owner_thread),
        "owner_thread": record.owner_thread,
        "reviewer": record.reviewer,
        "status": record.status,
        "input_path": record.input_path,
        "output_path": record.output_path,
        "thread_dir": str(thread_dir(root, record.owner_thread).relative_to(root)),
        "task_file": str((thread_dir(root, record.owner_thread) / "task.md").relative_to(root)),
        "status_file": str((thread_dir(root, record.owner_thread) / "status.md").relative_to(root)),
        "notes_file": str((thread_dir(root, record.owner_thread) / "notes.md").relative_to(root)),
        "review_file": str((review_dir(root) / f"{record.task_id}.md").relative_to(root)),
        "codex_thread_id": meta.get("codex_thread_id", "pending"),
        "codex_thread_title": meta.get("codex_thread_title", "pending"),
        "codex_thread_state": meta.get("codex_thread_state", "unbound"),
    }


def parse_task_goal(root: Path, thread_id: str) -> str:
    task_path = thread_dir(root, thread_id) / "task.md"
    values = parse_bullets(task_path)
    return values.get("goal", "-")


def create_task(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    records = load_board(root)
    if args.task_id in records:
        raise WorkflowError(f"task already exists: {args.task_id}")
    record = TaskRecord(
        task_id=args.task_id,
        title=args.title,
        owner_thread=args.thread,
        status="todo",
        input_path=args.input,
        output_path=args.output,
        reviewer=args.reviewer,
        updated_at=now_iso(),
    )
    records[record.task_id] = record
    save_board(root, records)
    create_thread_package(root, record, args.goal)
    append_log(root, "04-project-master", "create_task", record.task_id, f"created task {record.title}")
    sync_status_summary(root)


def mutate_status(args: argparse.Namespace, target_status: str, action: str, handoff_kind: str, handoff_message: str) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    records = load_board(root)
    record = records.get(args.task_id)
    if record is None:
        raise WorkflowError(f"missing task: {args.task_id}")
    actor = args.actor.strip()
    validate_transition(actor, record.status, target_status, record.owner_thread)
    record.status = target_status
    record.updated_at = now_iso()
    records[record.task_id] = record
    save_board(root, records)
    summary = getattr(args, "summary", "-")
    write_thread_status(root, record, last_submission=summary)
    append_log(root, actor, action, record.task_id, summary)
    target = record.reviewer if actor == record.owner_thread else record.owner_thread
    append_handoff(
        root,
        actor,
        target,
        record.task_id,
        handoff_kind,
        handoff_message.format(task_id=record.task_id),
        str(thread_dir(root, record.owner_thread).relative_to(root)),
    )
    sync_status_summary(root)


def dispatch(args: argparse.Namespace) -> None:
    mutate_status(args, "dispatched", "dispatch", "dispatch", "master dispatched task {task_id}")


def batch_dispatch(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    actor = args.actor.strip()
    if actor not in MASTER_ACTORS:
        raise WorkflowError(f"actor {actor} cannot batch dispatch")
    records = load_board(root)
    todo_records = [record for record in sorted(records.values(), key=lambda item: item.task_id) if record.status == "todo"]
    if args.status_filter:
        todo_records = [record for record in todo_records if record.status == args.status_filter]
    if args.task_ids:
        selected = set(args.task_ids.split(","))
        todo_records = [record for record in todo_records if record.task_id in selected]
    if args.limit is not None:
        todo_records = todo_records[: args.limit]
    dispatched: List[str] = []
    for record in todo_records:
        validate_transition(actor, record.status, "dispatched", record.owner_thread)
        record.status = "dispatched"
        record.updated_at = now_iso()
        records[record.task_id] = record
        write_thread_status(root, record, last_submission="-")
        append_log(root, actor, "dispatch", record.task_id, "batch dispatch")
        append_handoff(root, actor, record.owner_thread, record.task_id, "dispatch", f"master dispatched task {record.task_id}", str(thread_dir(root, record.owner_thread).relative_to(root)))
        dispatched.append(record.task_id)
    save_board(root, records)
    sync_status_summary(root)
    print(json.dumps({"dispatched": dispatched, "count": len(dispatched)}, ensure_ascii=False))


def start(args: argparse.Namespace) -> None:
    mutate_status(args, "inProgress", "start", "ack", "worker accepted task {task_id}")


def submit(args: argparse.Namespace) -> None:
    mutate_status(
        args,
        "submitted_for_review",
        "submit",
        "review_request",
        "task {task_id} is ready for review; please read local files",
    )


def review(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    decision = args.decision.strip()
    if decision not in {"accepted", "rejected"}:
        raise WorkflowError("review decision must be accepted or rejected")
    records = load_board(root)
    record = records.get(args.task_id)
    if record is None:
        raise WorkflowError(f"missing task: {args.task_id}")
    actor = args.actor.strip()
    validate_transition(actor, record.status, decision, record.owner_thread)
    record.status = decision
    record.updated_at = now_iso()
    records[record.task_id] = record
    save_board(root, records)
    review_path = review_dir(root) / f"{record.task_id}.md"
    review_path.parent.mkdir(parents=True, exist_ok=True)
    review_lines = [
        "# 验收记录",
        "",
        "## 基本信息",
        "",
        f"- task_id: {record.task_id}",
        f"- reviewer: {actor}",
        f"- decision: {decision}",
        f"- reviewed_at: {record.updated_at}",
        "",
        "## 结论",
        "",
        f"- summary: {args.summary}",
    ]
    write_text(review_path, "\n".join(review_lines))
    if decision == "accepted":
        final_output = output_path(root, record.output_path)
        final_output.parent.mkdir(parents=True, exist_ok=True)
        if not final_output.exists():
            write_text(
                final_output,
                "\n".join(
                    [
                        f"# {record.task_id} 交付物",
                        "",
                        f"- title: {record.title}",
                        f"- accepted_by: {actor}",
                        f"- accepted_at: {record.updated_at}",
                        "",
                        "此文件由主控在验收通过时创建，用于占位最终产物。",
                    ]
                ),
            )
    write_thread_status(root, record, last_submission=args.summary)
    append_log(root, actor, "review", record.task_id, f"{decision}: {args.summary}")
    append_handoff(
        root,
        actor,
        record.owner_thread,
        record.task_id,
        decision,
        f"master set {decision}; read .agent-work/review/{record.task_id}.md",
        f".agent-work/review/{record.task_id}.md",
    )
    sync_status_summary(root)


def register_thread(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    values = read_thread_meta(root, args.thread)
    values["codex_thread_id"] = args.codex_thread_id
    values["codex_thread_title"] = args.title
    values["codex_thread_state"] = args.state
    write_thread_meta(root, args.thread, values)
    append_log(root, args.actor, "register_thread", values.get("task_id", "-"), f"bound {args.thread} -> {args.codex_thread_id}")


def sync_thread_state(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    values = read_thread_meta(root, args.thread)
    values["codex_thread_state"] = args.state
    if args.title:
        values["codex_thread_title"] = args.title
    write_thread_meta(root, args.thread, values)
    append_log(root, args.actor, "sync_thread_state", values.get("task_id", "-"), f"{args.thread} -> {args.state}")


def recover(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    status_path = thread_dir(root, args.thread) / "status.md"
    values = parse_bullets(status_path)
    task_id = values.get("task_id")
    if not task_id:
        raise WorkflowError(f"thread status missing task_id: {args.thread}")
    if values.get("status") not in ALL_STATUSES:
        raise WorkflowError(f"thread status invalid: {values.get('status')}")
    records = load_board(root)
    record = records.get(task_id)
    if record is None:
        raise WorkflowError(f"board missing task from thread: {task_id}")
    record.status = values["status"]
    record.updated_at = now_iso()
    records[task_id] = record
    save_board(root, records)
    append_log(root, "00-master", "recover", task_id, f"recovered board from thread {args.thread}")
    sync_status_summary(root)


def report(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    records = sorted(load_board(root).values(), key=lambda item: item.task_id)
    counts = {status: 0 for status in ALL_STATUSES}
    ready: List[Dict[str, str]] = []
    active: List[Dict[str, str]] = []
    for record in records:
        counts[record.status] += 1
        if record.status == "submitted_for_review":
            ready.append({"task_id": record.task_id, "thread": record.owner_thread, "output_path": record.output_path})
        if record.status in {"dispatched", "inProgress"}:
            active.append({"task_id": record.task_id, "thread": record.owner_thread, "status": record.status})
    payload = {
        "generated_at": now_iso(),
        "counts": counts,
        "review_queue": ready,
        "active_queue": active,
    }
    print(json.dumps(payload, ensure_ascii=False, indent=2))


def report_markdown(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    records = sorted(load_board(root).values(), key=lambda item: item.task_id)
    lines = [
        "# 主控日报视图",
        "",
        f"- generated_at: {now_iso()}",
        "",
        "## 待验收",
        "",
    ]
    ready = [record for record in records if record.status == "submitted_for_review"]
    if not ready:
        lines.append("- 无")
    else:
        for record in ready:
            lines.append(f"- {record.task_id} | {record.title} | {record.owner_thread} | {record.output_path}")
    lines.extend(["", "## 执行中", ""])
    active = [record for record in records if record.status in {"dispatched", "inProgress"}]
    if not active:
        lines.append("- 无")
    else:
        for record in active:
            lines.append(f"- {record.task_id} | {record.title} | {record.owner_thread} | {record.status}")
    lines.extend(["", "## 待派发", ""])
    queued = [record for record in records if record.status == "todo"]
    if not queued:
        lines.append("- 无")
    else:
        for record in queued:
            lines.append(f"- {record.task_id} | {record.title} | {record.owner_thread}")
    print("\n".join(lines))


def prepare_dispatch_prompt(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    record = get_task_record(root, args.task_id)
    context = build_prompt_context(root, record)
    context["extra_instructions"] = args.extra_instructions or "无额外要求。"
    template = load_template(root, "worker_dispatch_prompt.md")
    print(render_template(template, context))


def prepare_review_prompt(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    record = get_task_record(root, args.task_id)
    context = build_prompt_context(root, record)
    context["review_summary"] = args.review_summary or "请严格按本地事实源进行验收。"
    template = load_template(root, "master_review_prompt.md")
    print(render_template(template, context))


def prepare_thread_bootstrap(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    record = get_task_record(root, args.task_id)
    context = build_prompt_context(root, record)
    template = load_template(root, "thread_bootstrap_prompt.md")
    print(render_template(template, context))


def audit(args: argparse.Namespace) -> None:
    root = Path(args.root).resolve()
    ensure_root(root)
    records = load_board(root)
    issues: List[str] = []
    for task in records.values():
        thread_status_path = thread_dir(root, task.owner_thread) / "status.md"
        if not thread_status_path.exists():
            issues.append(f"{task.task_id}: missing thread status")
            continue
        values = parse_bullets(thread_status_path)
        thread_status = values.get("status")
        if task.status in {"accepted", "rejected"}:
            if thread_status not in {task.status, "submitted_for_review"}:
                issues.append(f"{task.task_id}: board={task.status}, thread={thread_status}")
        elif thread_status != task.status:
            issues.append(f"{task.task_id}: board={task.status}, thread={thread_status}")
        if ".." in task.output_path:
            issues.append(f"{task.task_id}: invalid output path")
        meta = read_thread_meta(root, task.owner_thread)
        if meta.get("task_id") != task.task_id:
            issues.append(f"{task.task_id}: thread meta task mismatch")
    sync_status_summary(root)
    if issues:
        raise WorkflowError("audit failed:\n" + "\n".join(issues))
    print(f"audit ok: {len(records)} task(s)")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="multi-window workflow helper")
    subparsers = parser.add_subparsers(dest="command", required=True)

    def add_root_argument(subparser: argparse.ArgumentParser) -> None:
        subparser.add_argument("--root", default=".")

    create = subparsers.add_parser("create-task")
    add_root_argument(create)
    create.add_argument("--task-id", required=True)
    create.add_argument("--title", required=True)
    create.add_argument("--thread", required=True)
    create.add_argument("--goal", required=True)
    create.add_argument("--input", required=True)
    create.add_argument("--output", required=True)
    create.add_argument("--reviewer", default="04-project-master")
    create.set_defaults(func=create_task)

    for name, func in [("dispatch", dispatch), ("start", start), ("submit", submit)]:
        command = subparsers.add_parser(name)
        add_root_argument(command)
        command.add_argument("--task-id", required=True)
        command.add_argument("--actor", required=True)
        if name == "submit":
            command.add_argument("--summary", required=True)
        else:
            command.add_argument("--summary", default="-")
        command.set_defaults(func=func)

    batch = subparsers.add_parser("batch-dispatch")
    add_root_argument(batch)
    batch.add_argument("--actor", required=True)
    batch.add_argument("--limit", type=int)
    batch.add_argument("--task-ids")
    batch.add_argument("--status-filter", default="todo")
    batch.set_defaults(func=batch_dispatch)

    review_command = subparsers.add_parser("review")
    add_root_argument(review_command)
    review_command.add_argument("--task-id", required=True)
    review_command.add_argument("--actor", required=True)
    review_command.add_argument("--decision", required=True)
    review_command.add_argument("--summary", required=True)
    review_command.set_defaults(func=review)

    register = subparsers.add_parser("register-thread")
    add_root_argument(register)
    register.add_argument("--thread", required=True)
    register.add_argument("--codex-thread-id", required=True)
    register.add_argument("--title", required=True)
    register.add_argument("--state", default="bound")
    register.add_argument("--actor", default="04-project-master")
    register.set_defaults(func=register_thread)

    sync = subparsers.add_parser("sync-thread-state")
    add_root_argument(sync)
    sync.add_argument("--thread", required=True)
    sync.add_argument("--state", required=True)
    sync.add_argument("--title")
    sync.add_argument("--actor", default="04-project-master")
    sync.set_defaults(func=sync_thread_state)

    recover_command = subparsers.add_parser("recover")
    add_root_argument(recover_command)
    recover_command.add_argument("--thread", required=True)
    recover_command.set_defaults(func=recover)

    report_command = subparsers.add_parser("report")
    add_root_argument(report_command)
    report_command.set_defaults(func=report)

    report_md_command = subparsers.add_parser("report-markdown")
    add_root_argument(report_md_command)
    report_md_command.set_defaults(func=report_markdown)

    dispatch_prompt = subparsers.add_parser("prepare-dispatch-prompt")
    add_root_argument(dispatch_prompt)
    dispatch_prompt.add_argument("--task-id", required=True)
    dispatch_prompt.add_argument("--extra-instructions")
    dispatch_prompt.set_defaults(func=prepare_dispatch_prompt)

    review_prompt = subparsers.add_parser("prepare-review-prompt")
    add_root_argument(review_prompt)
    review_prompt.add_argument("--task-id", required=True)
    review_prompt.add_argument("--review-summary")
    review_prompt.set_defaults(func=prepare_review_prompt)

    bootstrap_prompt = subparsers.add_parser("prepare-thread-bootstrap")
    add_root_argument(bootstrap_prompt)
    bootstrap_prompt.add_argument("--task-id", required=True)
    bootstrap_prompt.set_defaults(func=prepare_thread_bootstrap)

    audit_command = subparsers.add_parser("audit")
    add_root_argument(audit_command)
    audit_command.set_defaults(func=audit)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        args.func(args)
    except WorkflowError as exc:
        print(f"ERROR: {exc}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
