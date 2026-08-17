import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

test("0064 evolves case_todos without creating a competing todo table", () => {
  const migration = read("src-tauri/migrations/0064_global_todos.sql");
  for (const field of [
    "content",
    "kind",
    "priority",
    "tags_json",
    "next_action",
    "status",
    "due_at",
    "remind_at",
    "source",
    "delete_requested_at",
    "deleted_at",
  ]) {
    assert.match(migration, new RegExp(`\\b${field}\\b`));
  }
  assert.match(migration, /FOREIGN KEY \(case_id\) REFERENCES cases\(id\) ON DELETE SET NULL/);
  assert.match(migration, /due_date \|\| 'T00:00:00\+08:00'/);
  assert.equal(migration.includes("CREATE TABLE todos"), false);
});

test("todo deletion is soft and copy-to-progress is idempotent", () => {
  const backend = read("src-tauri/src/db/todos.rs");
  assert.match(backend, /SET status = 'deleted', deleted_at = datetime\('now'\)/);
  assert.equal(backend.includes("DELETE FROM case_todos"), false);
  assert.match(backend, /external_source = 'case_todo'/);
  assert.match(backend, /external_record_id/);
  assert.match(backend, /INSERT INTO case_work_items/);
  assert.match(backend, /ON CONFLICT DO NOTHING/);
  assert.ok(backend.indexOf(".due_at") < backend.indexOf(".remind_at"));
  assert.match(backend, /TODO_PROGRESS_TARGET_CONFLICT/);
  assert.match(backend, /TODO_PROGRESS_ALREADY_EXISTS_DELETED/);
});

test("top-level todo board exposes unbound items, recovery and explicit progress copy", () => {
  const tabs = read("src/components/ModuleTabs.tsx");
  const app = read("src/App.tsx");
  const board = read("src/components/TodoBoard.tsx");
  const api = read("src/lib/api.ts");

  assert.match(tabs, /id: "todos", label: "待办事项"/);
  assert.match(app, /activeModule === "todos" && <TodoBoard cases=\{cases\}/);
  assert.match(board, /不关联案件/);
  assert.match(board, /复制到案件进展/);
  assert.match(board, /restoreTodo/);
  assert.match(api, /invoke<Todo\[]>\("list_global_todos"/);
  assert.match(api, /invoke<CopyTodoResult>\("copy_todo_to_case_progress"/);
});
