import { readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");

function collect(dir, matches, output = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (["node_modules", "target", "dist"].includes(entry.name)) continue;
    const path = join(dir, entry.name);
    if (entry.isDirectory()) collect(path, matches, output);
    else if (matches(entry.name)) output.push(relative(root, path));
  }
  return output;
}

const tests = [
  ...collect(join(root, "src"), (name) => name.endsWith(".test.mjs")),
  ...collect(join(root, "scripts"), (name) => /^test-.*\.cjs$/.test(name)),
].sort();

if (tests.length === 0) throw new Error("未找到 Node 纯逻辑/UI 契约测试");
console.log(`Running ${tests.length} Node logic/contract test files`);
const result = spawnSync(
  process.execPath,
  ["--experimental-strip-types", "--test", ...tests],
  { cwd: root, stdio: "inherit" },
);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
