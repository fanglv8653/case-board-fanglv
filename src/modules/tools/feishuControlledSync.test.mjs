import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

test("飞书刷新只生成主档和明细候选，不直接改写或归档业务明细", () => {
  const entities = read("../../../src-tauri/src/db/feishu_entities.rs");
  const sync = read("../../../src-tauri/src/db/feishu_sync.rs");
  assert.match(entities, /pub async fn preview_management_records/);
  assert.doesNotMatch(entities, /INSERT INTO case_work_items/);
  assert.doesNotMatch(entities, /UPDATE case_work_items SET/);
  assert.doesNotMatch(entities, /deleted_at=datetime\('now'\)/);
  assert.match(sync, /preview_management_records/);
  assert.match(sync, /review_status='superseded'/);
});

test("案件字段和管理明细都提供逐项双向决定，写回必须有读写授权", () => {
  const ui = read("./FeishuSyncPreview.tsx");
  const backend = read("../../../src-tauri/src/lib.rs");
  const oauth = read("../../../src-tauri/src/feishu_oauth.rs");
  assert.match(ui, /采用飞书/);
  assert.match(ui, /保留本地并写飞书/);
  assert.match(ui, /明细复核/);
  assert.match(backend, /resolve_feishu_sync_field/);
  assert.match(backend, /resolve_feishu_sync_entity/);
  assert.match(backend, /status\.write_enabled/);
  assert.match(backend, /fetch_bitable_record/);
  assert.match(backend, /ensure_remote_field_snapshot/);
  assert.match(backend, /ensure_remote_entity_snapshot/);
  assert.match(oauth, /bitable:app/);
});

test("远端缺失不自动映射为本地删除，阶段写入受领域触发器保护", () => {
  const entities = read("../../../src-tauri/src/db/feishu_entities.rs");
  const migration = read("../../../src-tauri/migrations/0060_case_domain_isolation.sql");
  assert.match(entities, /远端缺失不映射为本地删除或归档/);
  assert.match(migration, /case_stage_items_domain_guard_insert/);
  assert.match(migration, /case_stage_items_domain_guard_update/);
  assert.match(migration, /CASE_STAGE_DOMAIN_MISMATCH/);
});
