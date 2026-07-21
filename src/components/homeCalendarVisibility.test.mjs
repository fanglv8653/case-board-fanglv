import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { shouldMountFullCalendar } from "./homeCalendarVisibility.ts";

test("full calendar is mounted only by the workspace home", () => {
  assert.equal(shouldMountFullCalendar("workspace"), true);
  assert.equal(shouldMountFullCalendar("civil"), false);
  assert.equal(shouldMountFullCalendar("criminal"), false);
});

test("both calendar implementations share the workspace gate while key dates remain", () => {
  const source = readFileSync(new URL("./HomeView.tsx", import.meta.url), "utf8");
  assert.match(source, /const showFullCalendar = shouldMountFullCalendar\(mode\)/);
  assert.match(source, /showFullCalendar && cases\.length > 0 && feishuEnabled/);
  assert.match(
    source,
    /showFullCalendar && cases\.length > 0 && !feishuEnabled && calendarEnabled/,
  );
  assert.match(source, /<ImportantDates events=\{upcomingEvents\} onPickCase=\{onPickCase\} \/>/);
});
