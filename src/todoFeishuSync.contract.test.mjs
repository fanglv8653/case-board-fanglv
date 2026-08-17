import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

test("todo inbox binding is independent from case-table configuration", () => {
  const settings = read("src-tauri/src/settings.rs");
  const commands = read("src-tauri/src/lib.rs");
  assert.match(settings, /feishu_todo_inbox_app_token/);
  assert.match(settings, /feishu_todo_inbox_table_id/);
  assert.match(settings, /feishu_todo_inbox_view_id/);
  assert.match(commands, /FEISHU_TODO_CONFIG_INVALID/);
  assert.equal(commands.includes("feishu_cases_table_id, \"收件箱"), false);
});

test("0065 keeps synchronization metadata out of case_todos and the live Base", () => {
  const migration = read("src-tauri/migrations/0065_todo_feishu_sync.sql");
  for (const table of [
    "todo_feishu_sync_links",
    "todo_feishu_sync_runs",
    "todo_feishu_sync_previews",
    "todo_feishu_sync_conflicts",
    "todo_feishu_sync_operation_audits",
  ]) assert.match(migration, new RegExp(`CREATE TABLE ${table}`));
  assert.equal(migration.includes("ALTER TABLE case_todos"), false);
  assert.match(migration, /UNIQUE\(app_token, table_id, remote_business_key\)/);
  assert.match(migration, /UNIQUE\(app_token, table_id, record_id\)/);
});

test("automatic path is read-only and remote writes require explicit resolution with post-read", () => {
  const commands = read("src-tauri/src/lib.rs");
  const feishu = read("src-tauri/src/feishu.rs");
  const panel = read("src/components/TodoFeishuSyncPanel.tsx");

  const pull = commands.slice(commands.indexOf("async fn pull_todo_feishu_preview"), commands.indexOf("struct ResolveTodoFeishuPreviewInput"));
  assert.match(pull, /fetch_todo_inbox_records/);
  assert.equal(pull.includes("update_bitable_record_fields"), false);
  assert.equal(pull.includes("create_bitable_record"), false);
  assert.match(commands, /write_enabled/);
  assert.match(commands, /FEISHU_TODO_WRITE_UNCERTAIN/);
  assert.ok((commands.match(/fetch_bitable_record/g) ?? []).length >= 3);
  assert.equal(feishu.includes("delete_bitable"), false);
  assert.match(panel, /采用飞书/);
  assert.match(panel, /写入飞书/);
});

test("audited 22-field schema and duplicate identifiers fail closed", () => {
  const feishu = read("src-tauri/src/feishu.rs");
  const ledger = read("src-tauri/src/db/todo_feishu_sync.rs");
  for (const field of ["事项", "事项编号", "原始内容", "类型", "状态", "优先级", "截止时间", "提醒时间", "关联案件", "删除请求时间", "删除原因", "来源消息ID", "外部归档路径"]) {
    assert.ok(feishu.includes(`"${field}"`));
  }
  assert.match(feishu, /Table ID 必须指向“📥收件箱”/);
  assert.match(feishu, /View ID 必须指向“收件箱”视图/);
  assert.match(ledger, /FEISHU_TODO_DUPLICATE_ID/);
  assert.match(ledger, /source_counts/);
  assert.match(ledger, /remote_missing/);
});
