import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./FeishuTool.tsx", import.meta.url), "utf8");
const functionSource = source.match(
  /function connectionErrorMessage\(error: unknown\): string \{[\s\S]*?\n\}\n\nexport function FeishuTool/,
)?.[0]
  .replace("function connectionErrorMessage(error: unknown): string", "function connectionErrorMessage(error)")
  .replace(/\n\nexport function FeishuTool$/, "");

assert.ok(functionSource, "应能读取 FeishuTool 中的连接错误映射函数");
const connectionErrorMessage = Function(`${functionSource}; return connectionErrorMessage;`)();

test("系统凭据库错误不会误报为 App ID 或 App Secret 无效", () => {
  const result = connectionErrorMessage("FEISHU_OAUTH_CREDENTIAL_STORE: 凭据安全保存失败");
  assert.equal(result, "Windows 凭据安全保存失败，请确认当前 Windows 用户凭据库可用后重试。");
  assert.doesNotMatch(result, /App ID|App Secret/);
  assert.doesNotMatch(functionSource, /includes\(["']凭据["']\)/);
});

test("飞书拒绝 token 时显示认证失败而不是输入错误", () => {
  const result = connectionErrorMessage("FEISHU_OAUTH_TOKEN_REJECTED: 飞书拒绝 token");
  assert.equal(result, "飞书认证失败，请重新连接并确认应用权限已经发布。");
  assert.doesNotMatch(result, /App ID|App Secret/);
});

test("飞书 token 响应异常使用独立提示", () => {
  const result = connectionErrorMessage("FEISHU_OAUTH_INVALID_TOKEN_RESPONSE: invalid json");
  assert.equal(result, "飞书认证响应异常，请稍后重试；如持续出现，请重新连接。");
  assert.doesNotMatch(result, /App ID|App Secret/);
});

test("只有明确客户端输入错误才提示核对 App ID 或 App Secret", () => {
  for (const code of [
    "FEISHU_OAUTH_INVALID_APP_ID",
    "FEISHU_OAUTH_MISSING_APP_SECRET",
    "FEISHU_OAUTH_INVALID_CLIENT",
  ]) {
    assert.match(connectionErrorMessage(`${code}: rejected`), /App ID 或 App Secret 无效/);
  }
  assert.doesNotMatch(connectionErrorMessage("某个未知凭据错误"), /App ID|App Secret/);
});
