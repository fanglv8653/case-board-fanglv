import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const serviceSource = readFileSync(
  new URL("./criminalNotifications.ts", import.meta.url),
  "utf8",
);
const settingsSource = readFileSync(
  new URL("../components/SettingsModal.tsx", import.meta.url),
  "utf8",
);
const mainSource = readFileSync(new URL("../main.tsx", import.meta.url), "utf8");

test("自动提醒仅由本机开关启用，启动入口不请求权限", () => {
  assert.match(serviceSource, /caseboard:criminal-notifications-enabled/);
  assert.match(serviceSource, /isEnabled: isCriminalNotificationEnabled/);
  const runtime = serviceSource.match(
    /export function startCriminalNotificationRuntime\(\) \{(?<body>[\s\S]*?)\n\}/,
  )?.groups?.body;
  assert.ok(runtime);
  assert.doesNotMatch(runtime, /requestPermission/);
  assert.match(mainSource, /startCriminalNotificationRuntime\(\)/);
});

test("设置页提供显式开关、运行期说明和测试提醒", () => {
  assert.match(settingsSource, /aria-label="刑事案件 Windows 自动提醒"/);
  assert.match(settingsSource, /仅在案件看板运行期间扫描和发送/);
  assert.match(settingsSource, /发送测试提醒/);
  assert.match(settingsSource, /不代表用户已经阅读/);
});

test("运行时按启动、聚焦、60秒和设置变更触发扫描", () => {
  assert.match(serviceSource, /const SCAN_INTERVAL_MS = 60_000/);
  assert.match(serviceSource, /window\.addEventListener\("focus", scan\)/);
  assert.match(
    serviceSource,
    /window\.addEventListener\(CRIMINAL_NOTIFICATION_SETTINGS_EVENT, scan\)/,
  );
  assert.match(serviceSource, /window\.setInterval\(scan, SCAN_INTERVAL_MS\)/);
});

