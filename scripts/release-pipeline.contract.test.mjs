import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

test("Windows release bundles the dedicated updater helper", () => {
  const config = JSON.parse(read("src-tauri/tauri.conf.json"));
  const workflow = read(".github/workflows/build-windows.yml");
  assert.deepEqual(config.bundle.externalBin, ["binaries/caseboard-updater-helper"]);
  assert.ok(workflow.includes("cargo build --release --locked --bin caseboard-updater-helper"));
  assert.ok(workflow.includes("caseboard-updater-helper-x86_64-pc-windows-msvc.exe"));
  assert.ok(workflow.includes("[IO.File]::WriteAllBytes($target, [byte[]]@())"));
  assert.ok(workflow.includes("staged updater helper is unexpectedly small"));
  assert.ok(workflow.includes("UPD_METADATA_INVALID"));
  assert.ok(
    workflow.indexOf("- name: Build and stage dedicated updater helper") <
      workflow.indexOf("- name: Rust check and tests"),
  );
});

test("release facts use an exact ASCII pair staged from a verified tag", () => {
  const workflow = read(".github/workflows/build-windows.yml");
  const gate = read("scripts/release-gate.mjs");
  assert.ok(workflow.includes("persist-credentials: false"));
  assert.ok(workflow.includes('"FanglvCaseBoard_${version}_x64-setup.exe"'));
  assert.ok(gate.includes("REL_ASSET_NAME_INVALID"));
  assert.ok(gate.includes("names.length !== 2"));
});

test("publication converges a draft before one paired manifest commit", () => {
  const publisher = read("scripts/publish-release-resumable.ps1");
  assert.ok(publisher.includes("'--draft'"));
  assert.ok(publisher.includes("'--draft=false'"));
  assert.ok(publisher.includes("release/latest.json", "release/version.json"));
  assert.ok(publisher.includes("release/version.json"));
  assert.ok(publisher.includes("chore: publish $expectedVersion release manifests"));
  assert.ok(publisher.includes("回读公开清单对"));
  assert.equal(publisher.includes("--force"), false);
});
