import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

test("material IPC surface never exposes claim token mutation commands", () => {
  const handlers = read("src-tauri/src/lib.rs");
  for (const forbidden of [
    "claim_next_material_processing_item,",
    "can_execute_material_processing_item,",
    "finish_material_processing_item,",
    "fail_material_processing_item,",
    "import_case_folder,",
    "commit_import_folder,",
    "refresh_case_files,",
  ]) {
    assert.equal(handlers.includes(forbidden), false, forbidden);
  }
});

test("OCR and each LLM chunk are guarded by persistent execution permission", () => {
  const extractor = read("src-tauri/src/ingest/extractor.rs");
  assert.match(extractor, /struct ExtractionExecutionGuard/);
  assert.match(extractor, /guard\.ensure_allowed\(\)\.await/);
  assert.ok(
    extractor.match(/guard\.ensure_allowed\(\)\.await/g)?.length >= 3,
    "expected OCR and both extraction loops to check execution permission",
  );
});

test("all user material entrances route to decisions or remain pending confirmation", () => {
  const pipeline = read("src-tauri/src/ingest/pipeline.rs");
  const lib = read("src-tauri/src/lib.rs");
  const bundle = read("src-tauri/src/case_bundle/mod.rs");
  assert.match(pipeline, /enqueue_decided_documents_and_run/);
  assert.equal(pipeline.includes("extract_cached_text("), false);
  assert.match(pipeline, /trigger_reextract[\s\S]*save_decisions/);
  assert.match(lib, /ingest_court_sms[\s\S]*新增待确认/);
  assert.equal(
    bundle.slice(bundle.indexOf("pub async fn merge_case_bundle"), bundle.indexOf("pub async fn merge_into")).includes("spawn_extraction"),
    false,
  );
});

test("preflight UI provides nested tree, bulk modes, estimates and progressive rendering", () => {
  const dialog = read("src/components/MaterialPreflightDialog.tsx");
  for (const marker of [
    "buildTree",
    "<details",
    "全选识别",
    "全设仅索引",
    "全排除",
    "反选（识别↔排除）",
    "预计：本地解析",
    "PAGE_SIZE",
    'role="dialog"',
    'aria-modal="true"',
  ]) {
    assert.ok(dialog.includes(marker), marker);
  }
});
