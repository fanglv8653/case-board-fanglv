import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./FeishuSyncPreview.tsx", import.meta.url), "utf8")
  .replace(/\r\n/g, "\n");
const functionSource = source.match(
  /function pullErrorMessage\(error: unknown\): string \{[\s\S]*?\n\}\n\nexport function FeishuSyncPreview/,
)?.[0]
  .replace("function pullErrorMessage(error: unknown): string", "function pullErrorMessage(error)")
  .replace(/\n\nexport function FeishuSyncPreview$/, "");

assert.ok(functionSource, "应能读取同步预演错误映射函数");
const pullErrorMessage = Function(`${functionSource}; return pullErrorMessage;`)();

test("App Token 缺失属于配置错误而不是授权失效", () => {
  const result = pullErrorMessage("FEISHU_CONFIG_INVALID: 请先填写多维表格 App Token");
  assert.equal(result, "请先在“日历设置—案件管理多维表格”填写 App Token 和案件总表 Table ID。");
  assert.doesNotMatch(result, /连接未建立|授权.*失效/);
});

test("Table ID 缺失属于配置错误而不是授权失效", () => {
  const result = pullErrorMessage("FEISHU_CONFIG_INVALID: 请先填写案件总表 Table ID");
  assert.equal(result, "请先在“日历设置—案件管理多维表格”填写 App Token 和案件总表 Table ID。");
  assert.doesNotMatch(result, /连接未建立|授权.*失效/);
});

test("只有稳定授权错误码进入重新连接提示", () => {
  assert.match(pullErrorMessage("FEISHU_AUTH_REQUIRED: 请先连接"), /连接未建立或已失效/);
  assert.match(
    pullErrorMessage("FEISHU_OAUTH_REAUTHORIZATION_REQUIRED: expired"),
    /连接未建立或已失效/,
  );
});

test("映射不再用 token 或 auth 普通子串抢先分类", () => {
  assert.doesNotMatch(functionSource, /includes\(["']token["']\)/i);
  assert.doesNotMatch(functionSource, /includes\(["']auth["']\)/i);
  assert.match(pullErrorMessage("FEISHU_TABLE_NOT_FOUND: missing"), /找不到飞书案件总表/);
});

test("配置到进度表时明确提示选择案件总表", () => {
  const result = pullErrorMessage("FEISHU_TABLE_SCHEMA_MISMATCH: missing case fields");
  assert.match(result, /当前 Table ID 不是案件总表/);
  assert.match(result, /案件名称.*☑状态/);
});
