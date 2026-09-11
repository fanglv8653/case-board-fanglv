import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const backend = readFileSync(new URL("../../../src-tauri/src/lib.rs", import.meta.url), "utf8");
const calendar = readFileSync(new URL("../../../src-tauri/src/feishu.rs", import.meta.url), "utf8");
const ui = readFileSync(new URL("./FeishuCalendarTool.tsx", import.meta.url), "utf8");

test("calendar board fetch and connection test share one backend connection path", () => {
  const calls = backend.match(/feishu::calendar_connection\(/g) ?? [];
  assert.ok(calls.length >= 3, "fetch, test, and authorization completion must use the same path");
  assert.match(backend, /async fn fetch_feishu_calendar[\s\S]*?feishu::calendar_connection/);
  assert.match(backend, /async fn test_feishu_calendar_connection[\s\S]*?feishu::calendar_connection/);
});

test("calendar connection checks identity, scope, and a real request without returning tokens", () => {
  assert.match(calendar, /auth", "status", "--verify", "--json"/);
  assert.match(
    calendar,
    /"auth",\s*"check",\s*"--scope",\s*FEISHU_CALENDAR_READ_SCOPE/,
  );
  assert.match(calendar, /match fetch_calendar_events\(bin, start, end\)\.await/);
  const diagnostic = calendar.slice(
    calendar.indexOf("pub struct FeishuCalendarDiagnostic"),
    calendar.indexOf("pub struct FeishuCalendarAuthorization"),
  );
  assert.doesNotMatch(diagnostic, /access_token|refresh_token/i);
});

test("failed connection exposes an in-app device authorization recovery action", () => {
  assert.match(ui, /startFeishuCalendarAuthorization/);
  assert.match(ui, /finishFeishuCalendarAuthorization/);
  assert.match(ui, /重新授权日历/);
  assert.match(ui, /我已授权，完成连接/);
});
