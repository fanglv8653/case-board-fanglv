import { useCallback, useEffect, useMemo, useState } from "react";
import { ArchiveRestore, CheckCircle2, ClipboardCopy, Plus, Trash2 } from "lucide-react";

import {
  addTodo,
  copyTodoToCaseProgress,
  deleteTodo,
  listGlobalTodos,
  restoreTodo,
  setTodoCase,
  updateTodo,
  type Todo,
} from "@/lib/api";
import type { Case } from "@/lib/types";
import { confirmDialog } from "@/lib/dialog";
import { toast } from "@/components/ui/toast";

type FilterState = "open" | "completed" | "deleted" | "all";

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const value = error as { code?: string; message?: string };
    return [value.code, value.message].filter(Boolean).join("：") || "未知错误";
  }
  return String(error);
}

function dateTimeLocal(value: string): string {
  return value ? new Date(value).toISOString() : "";
}

export function TodoBoard({ cases }: { cases: Case[] }) {
  const [rows, setRows] = useState<Todo[]>([]);
  const [state, setState] = useState<FilterState>("open");
  const [caseFilter, setCaseFilter] = useState("");
  const [query, setQuery] = useState("");
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [kind, setKind] = useState<Todo["kind"]>("todo");
  const [priority, setPriority] = useState<Todo["priority"]>("unjudged");
  const [caseId, setCaseId] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [busy, setBusy] = useState(false);

  const caseNames = useMemo(() => new Map(cases.map((item) => [item.id, item.name])), [cases]);

  const reload = useCallback(async () => {
    try {
      setRows(await listGlobalTodos({ state, case_id: caseFilter || null, query: query.trim() || null }));
    } catch (error) {
      toast(`读取待办失败：${errorMessage(error)}`, "error");
    }
  }, [caseFilter, query, state]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const create = async () => {
    if (!title.trim() || busy) return;
    setBusy(true);
    try {
      await addTodo({
        title,
        content,
        kind,
        priority,
        case_id: caseId || null,
        due_at: dueAt ? dateTimeLocal(dueAt) : null,
      });
      setTitle("");
      setContent("");
      setDueAt("");
      await reload();
      toast("待办已添加", "success");
    } catch (error) {
      toast(`添加失败：${errorMessage(error)}`, "error");
    } finally {
      setBusy(false);
    }
  };

  const mutate = async (action: () => Promise<unknown>, success: string) => {
    try {
      await action();
      await reload();
      toast(success, "success");
    } catch (error) {
      toast(errorMessage(error), "error");
    }
  };

  const copyToProgress = async (todo: Todo) => {
    const target = todo.case_id || caseId;
    if (!target) {
      toast("未关联事项需先在上方选择案件，再复制到案件进展", "info");
      return;
    }
    try {
      const result = await copyTodoToCaseProgress(todo.id, target);
      toast(result.created ? "已复制到案件进展" : "该事项已复制过，无需重复操作", "success");
    } catch (error) {
      toast(`复制失败：${errorMessage(error)}`, "error");
    }
  };

  return (
    <div className="h-full overflow-auto bg-background px-5 py-6 lg:px-8">
      <div className="mx-auto max-w-6xl space-y-5">
        <div>
          <h1 className="text-xl font-semibold">待办事项</h1>
          <p className="mt-1 text-sm text-muted-foreground">统一管理想法、任务、提醒、参考资料和备忘；可选择关联案件。</p>
        </div>

        <section className="space-y-3 rounded-xl border border-border bg-card p-4">
          <div className="grid gap-3 md:grid-cols-2">
            <input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="标题（必填）" className="rounded-md border border-border bg-background px-3 py-2 text-sm" />
            <input value={content} onChange={(event) => setContent(event.target.value)} placeholder="内容（可选）" className="rounded-md border border-border bg-background px-3 py-2 text-sm" />
            <select value={kind} onChange={(event) => setKind(event.target.value as Todo["kind"])} className="rounded-md border border-border bg-background px-3 py-2 text-sm">
              <option value="todo">待办</option><option value="reminder">提醒</option><option value="idea">想法</option><option value="reference">参考</option><option value="memo">备忘</option>
            </select>
            <select value={priority} onChange={(event) => setPriority(event.target.value as Todo["priority"])} className="rounded-md border border-border bg-background px-3 py-2 text-sm">
              <option value="unjudged">未判断优先级</option><option value="high">高</option><option value="medium">中</option><option value="low">低</option>
            </select>
            <select value={caseId} onChange={(event) => setCaseId(event.target.value)} className="rounded-md border border-border bg-background px-3 py-2 text-sm">
              <option value="">不关联案件</option>{cases.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
            <input type="datetime-local" value={dueAt} onChange={(event) => setDueAt(event.target.value)} className="rounded-md border border-border bg-background px-3 py-2 text-sm" title="事项时间" />
          </div>
          <button type="button" disabled={!title.trim() || busy} onClick={() => void create()} className="inline-flex items-center gap-2 rounded-md bg-sky-600 px-4 py-2 text-sm font-medium text-white disabled:opacity-50">
            <Plus className="size-4" />添加事项
          </button>
        </section>

        <section className="space-y-3">
          <div className="flex flex-wrap gap-2">
            <select value={state} onChange={(event) => setState(event.target.value as FilterState)} className="rounded-md border border-border bg-card px-3 py-2 text-sm">
              <option value="open">进行中</option><option value="completed">已完成</option><option value="deleted">回收站</option><option value="all">全部</option>
            </select>
            <select value={caseFilter} onChange={(event) => setCaseFilter(event.target.value)} className="rounded-md border border-border bg-card px-3 py-2 text-sm">
              <option value="">全部案件</option>{cases.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索标题、内容、下一步" className="min-w-64 flex-1 rounded-md border border-border bg-card px-3 py-2 text-sm" />
          </div>

          {rows.length === 0 ? <div className="rounded-xl border border-dashed border-border p-10 text-center text-sm text-muted-foreground">暂无符合条件的事项</div> : (
            <div className="space-y-2">{rows.map((todo) => (
              <article key={todo.id} className="rounded-xl border border-border bg-card p-4">
                <div className="flex items-start gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <h2 className={todo.status === "completed" ? "font-medium text-muted-foreground line-through" : "font-medium"}>{todo.title}</h2>
                      <span className="rounded bg-muted px-2 py-0.5 text-xs">{todo.kind}</span>
                      <span className="rounded bg-muted px-2 py-0.5 text-xs">{todo.priority}</span>
                      <span className="text-xs text-muted-foreground">{todo.case_id ? caseNames.get(todo.case_id) ?? "案件已删除" : "未关联案件"}</span>
                    </div>
                    {todo.content && <p className="mt-2 whitespace-pre-wrap text-sm text-muted-foreground">{todo.content}</p>}
                    {todo.due_at && <p className="mt-2 text-xs text-muted-foreground">事项时间：{new Date(todo.due_at).toLocaleString()}</p>}
                  </div>
                  <div className="flex shrink-0 gap-1">
                    {todo.deleted_at ? (
                      <button type="button" title="恢复" onClick={() => void mutate(() => restoreTodo(todo.id), "事项已恢复")} className="rounded p-2 hover:bg-muted"><ArchiveRestore className="size-4" /></button>
                    ) : (
                      <>
                        <button type="button" title={todo.status === "completed" ? "重新打开" : "完成"} onClick={() => void mutate(() => updateTodo(todo.id, { done: todo.status === "completed" ? 0 : 1 }), todo.status === "completed" ? "事项已重新打开" : "事项已完成")} className="rounded p-2 hover:bg-muted"><CheckCircle2 className="size-4" /></button>
                        <button type="button" title="复制到案件进展" onClick={() => void copyToProgress(todo)} className="rounded p-2 hover:bg-muted"><ClipboardCopy className="size-4" /></button>
                        <button type="button" title="删除到回收站" onClick={() => void (async () => { if (await confirmDialog(`将“${todo.title}”移入回收站？`, { okLabel: "删除", danger: true })) await mutate(() => deleteTodo(todo.id), "事项已移入回收站"); })()} className="rounded p-2 text-destructive hover:bg-destructive/10"><Trash2 className="size-4" /></button>
                      </>
                    )}
                  </div>
                </div>
                {!todo.deleted_at && <div className="mt-3 flex items-center gap-2 border-t border-border pt-3 text-xs"><span className="text-muted-foreground">关联案件</span><select value={todo.case_id ?? ""} onChange={(event) => void mutate(() => setTodoCase(todo.id, event.target.value || null), "案件关联已更新")} className="rounded border border-border bg-background px-2 py-1"><option value="">不关联</option>{cases.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></div>}
              </article>
            ))}</div>
          )}
        </section>
      </div>
    </div>
  );
}
