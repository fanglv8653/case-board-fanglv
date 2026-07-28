import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const css = readFileSync(new URL("./globals.css", import.meta.url), "utf8");
const moduleTabs = readFileSync(
  new URL("../components/ModuleTabs.tsx", import.meta.url),
  "utf8",
);

test("UI foundation exposes semantic surfaces, statuses and progressive list primitives", () => {
  for (const contract of [
    "--surface-raised",
    "--space-surface",
    "--status-success",
    "--status-warning",
    "--status-danger",
    "--status-info",
    "--status-paused",
    ".surface-card",
    ".surface-interactive",
    ".status-chip",
    ".status-processing",
    ".progressive-list",
    "content-visibility: auto",
    "contain-intrinsic-size",
  ]) {
    assert.match(css, new RegExp(contract.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});

test("UI foundation keeps keyboard and operating-system accessibility preferences", () => {
  assert.match(css, /:focus-visible/);
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)/);
  assert.match(css, /@media \(prefers-reduced-transparency: reduce\)/);
  assert.match(css, /@media \(forced-colors: active\)/);
});

test("top navigation retains horizontal overflow without replacing its structure", () => {
  assert.match(moduleTabs, /horizontal-scroll min-w-0 flex-1 overflow-x-auto/);
  assert.match(moduleTabs, /role="group"/);
  assert.match(moduleTabs, /aria-label="业务模块导航"/);
  assert.match(moduleTabs, /surface-interactive/);
  assert.match(moduleTabs, /getPrivateTopTabs/);
});
