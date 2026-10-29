import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { formatHistoryTimestamp } from "./historyTime.ts";

test("SQLite UTC and legacy explicit offsets display the same Beijing time", () => {
  for (const value of ["2026-10-29 09:10:54", "2026-10-29T09:10:54Z", "2026-10-29T17:10:54+08:00", "2026-10-29T02:10:54-07:00"])
    assert.equal(formatHistoryTimestamp(value), "2026/10/29 17:10:54");
});
test("UTC crossing midnight and leap day are converted without rewriting the input", () => {
  const input = "2024-02-29 20:30:00.123456";
  assert.equal(formatHistoryTimestamp(input), "2024/03/01 04:30:00");
  assert.equal(input, "2024-02-29 20:30:00.123456");
  assert.equal(formatHistoryTimestamp("2026-12-31 16:00:00"), "2027/01/01 00:00:00");
});
test("malformed, absent and impossible dates remain explicitly unverified", () => {
  for (const value of [null, undefined, "", "2026-02-30 12:00:00", "2026-02-29 12:00:00", "2026-10-29", "2026-10-29 24:00:00", "2026-10-29T12:00:00+25:00", "garbage"])
    assert.equal(formatHistoryTimestamp(value), "时间待核");
});
test("display is independent of the host timezone", () => {
  const source = `import { formatHistoryTimestamp } from ${JSON.stringify(new URL("./historyTime.ts", import.meta.url).href)}; console.log(formatHistoryTimestamp('2026-10-29 09:10:54'));`;
  for (const zone of ["UTC", "America/Los_Angeles", "Asia/Shanghai"]) {
    const r = spawnSync(process.execPath, ["--experimental-strip-types", "--input-type=module", "-e", source], { env: { ...process.env, TZ: zone }, encoding: "utf8" });
    assert.equal(r.status, 0, r.stderr);
    assert.equal(r.stdout.trim(), "2026/10/29 17:10:54");
  }
});
