import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");
const read = (path) => readFileSync(resolve(root, path), "utf8");
const EXPECTED_UPDATER_PUBKEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ1MkRGODBFOTM2RTA0MjIKUldRaUJHNlREdmd0UmVEWm1tU1NkTUY2R3JHQ0lmc2xEK09oN1JiTWdzMmdpRkJiaVk3L0JXRHYK";
const EXPECTED_UPDATER_PUBKEY_SHA256 =
  "a9a2c4e0dda49d42f02effdd6b0d2f862689bd58164c6ed00bc68a2065664c38";

test("new builds restore discovery through the main branch version marker", () => {
  const backend = read("src-tauri/src/update.rs");
  const marker = JSON.parse(read("release/version.json"));

  assert.ok(
    backend.includes(
      "https://raw.githubusercontent.com/fanglv8653/case-board-fanglv/main/release/version.json",
    ),
  );
  assert.equal(backend.includes("PRIVATE_UPDATE_CHECK_DISABLED"), false);
  assert.equal(marker.version, "0.8.3");
  assert.equal(
    marker.download_url,
    "https://github.com/fanglv8653/case-board-fanglv/releases/tag/v0.8.3-fanglv",
  );
});

test("automatic installation remains on the signed Tauri updater path", () => {
  const updater = read("src/lib/updater.ts");
  const config = JSON.parse(read("src-tauri/tauri.conf.json"));
  const updaterConfig = config.plugins.updater;

  assert.ok(updater.includes('from "@tauri-apps/plugin-updater"'));
  assert.ok(updater.includes("return await check()"));
  assert.ok(updater.includes("await update.downloadAndInstall("));
  assert.equal(updater.includes("fetch("), false);
  assert.equal(updater.includes("writeFile"), false);
  assert.equal(updater.includes("Command.create"), false);

  assert.equal(updaterConfig.pubkey, EXPECTED_UPDATER_PUBKEY);
  assert.equal(
    createHash("sha256").update(updaterConfig.pubkey, "utf8").digest("hex"),
    EXPECTED_UPDATER_PUBKEY_SHA256,
  );
  assert.deepEqual(updaterConfig.endpoints, [
    "https://raw.githubusercontent.com/fanglv8653/case-board-fanglv/main/release/latest.json",
  ]);
  assert.equal(updaterConfig.windows.installMode, "passive");
});

test("manual download is fallback only and never becomes an unsigned in-app installer", () => {
  const dialog = read("src/components/UpdateAvailableDialog.tsx");
  const updater = read("src/lib/updater.ts");

  assert.ok(dialog.includes("await openUrl(url)"));
  assert.ok(dialog.includes("const update = await checkAppUpdate()"));
  assert.ok(dialog.includes("await downloadInstallRelaunch(update"));
  assert.equal(updater.includes("download_url"), false);
});
