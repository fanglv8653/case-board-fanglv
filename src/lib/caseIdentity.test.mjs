import assert from "node:assert/strict";
import test from "node:test";

import {
  caseMatchesSearch,
  formatRecognitionFailure,
  formatRecognitionFailureList,
  getCaseDisplayName,
} from "./caseIdentity.ts";

const base = {
  name: "1刑事委托材料",
  cause: null,
  agg_cause: "贪污罪、受贿罪",
  agg_plaintiffs: "[]",
  agg_defendants: '["杨赛清"]',
  user_overrides_json: null,
  legal_domain: "criminal",
  domain_source: "inferred",
  display_name_override: null,
};

test("刑事案件显示当事人姓名加罪名，而不是文件夹名", () => {
  assert.equal(getCaseDisplayName(base), "杨赛清贪污罪、受贿罪");
});

test("人工显示名称优先，空白人工值回退自动名称", () => {
  assert.equal(
    getCaseDisplayName({ ...base, display_name_override: "杨赛清案（自定义）" }),
    "杨赛清案（自定义）",
  );
  assert.equal(
    getCaseDisplayName({ ...base, display_name_override: "  " }),
    "杨赛清贪污罪、受贿罪",
  );
});

test("缺少当事人时回退案由，缺少案由时回退文件夹名", () => {
  assert.equal(getCaseDisplayName({ ...base, agg_defendants: "[]" }), "贪污罪、受贿罪");
  assert.equal(
    getCaseDisplayName({ ...base, agg_defendants: "[]", agg_cause: null }),
    "1刑事委托材料",
  );
});

test("搜索同时命中统一显示名、自定义名、案由和文件夹名", () => {
  assert.equal(caseMatchesSearch(base, "杨赛清贪污罪"), true);
  assert.equal(caseMatchesSearch(base, "受贿"), true);
  assert.equal(caseMatchesSearch(base, "刑事委托材料"), true);
  assert.equal(caseMatchesSearch(base, "不存在"), false);
});

test("识别失败按稳定错误前缀转换为可操作提示", () => {
  assert.match(formatRecognitionFailure("DOMAIN_MISMATCH: civil"), /案件领域不符/);
  assert.match(formatRecognitionFailure("MATERIAL_UNREADABLE: missing"), /材料不可读取/);
  assert.match(formatRecognitionFailure("RECOGNITION_ENGINE_FAILED: timeout"), /识别引擎运行失败/);
  assert.equal(formatRecognitionFailure("legacy failure"), "legacy failure");
});

test("识别报告的逐项错误与顶层异常使用同一分层规则", () => {
  const message = formatRecognitionFailureList([
    "MATERIAL_UNREADABLE: missing.pdf",
    "RECOGNITION_ENGINE_FAILED: timeout",
  ]);
  assert.match(message, /案件材料不可读取/);
  assert.match(message, /识别引擎运行失败/);
  assert.doesNotMatch(message, /MATERIAL_UNREADABLE|RECOGNITION_ENGINE_FAILED/);
});
