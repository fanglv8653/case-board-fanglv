import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const component = readFileSync(new URL("./FeishuSyncPreview.tsx", import.meta.url), "utf8");
const types = readFileSync(new URL("../../lib/types.ts", import.meta.url), "utf8");
const rust = readFileSync(
  new URL("../../../src-tauri/src/db/feishu_sync.rs", import.meta.url),
  "utf8",
);

test("CE7 active orphan has explicit wording and only the successful unbind action", () => {
  assert.match(types, /is_orphaned:\s*boolean/);
  assert.match(types, /error_code:\s*string \| null/);
  assert.match(component, /item\.is_orphaned \? "本地案件已删除"/);
  assert.match(component, /item\.is_orphaned \? "绑定异常"/);
  assert.match(component, /item\.is_orphaned \? "解除孤立绑定" : "解除绑定"/);
  assert.match(component, /data\?\.bound_cases\.map/);
  assert.match(component, /FEISHU_ORPHAN_BINDING/);
});

test("CE7 pull-archived orphan cannot reappear as a stale bound action", () => {
  const boundQuery = rust.slice(
    rust.indexOf("let bound_cases ="),
    rust.indexOf("let pending_rows ="),
  );
  assert.match(boundQuery, /l\.status = 'active'/);
  assert.match(boundQuery, /c\.id IS NULL AS is_orphaned/);
  assert.match(boundQuery, /FEISHU_ORPHAN_BINDING/);
  assert.doesNotMatch(boundQuery, /l\.status = 'archived'/);
});

test("R2 active orphan missing inbox has a local recovery path while normal links fail closed", () => {
  const unbind = rust.slice(
    rust.indexOf("pub async fn unbind_case"),
    rust.indexOf("async fn change_inbox_status"),
  );
  assert.match(unbind, /if !case_exists \{\s*ensure_orphan_recovery_inbox/);
  assert.match(unbind, /let inbox = case_link_inbox/);
  assert.ok(
    unbind.indexOf("if !case_exists") < unbind.indexOf("let inbox = case_link_inbox"),
    "only an orphan may synthesize its recovery inbox before the required inbox lookup",
  );
  assert.match(component, /item\.is_orphaned \? "解除孤立绑定" : "解除绑定"/);
});

test("CE7 partial pull and stale-review failures remain visible and non-networking", () => {
  assert.match(types, /status:\s*"succeeded" \| "partial" \| string/);
  assert.match(types, /orphan_count:\s*number/);
  assert.match(component, /result\.status === "partial"/);
  assert.match(component, /稳定错误码：\$\{errorCode\}/);
  assert.match(component, /code\.includes\("FEISHU_ORPHAN_BINDING"\)/);
  assert.match(component, /code\.includes\("FEISHU_REVIEW_NOT_FOUND"\)/);
  assert.match(component, /本地案件已删除，未访问飞书；请解除孤立绑定/);
  assert.match(component, /找不到该待复核字段，未访问飞书/);
  assert.match(component, /找不到该待复核明细，未访问飞书/);
});
