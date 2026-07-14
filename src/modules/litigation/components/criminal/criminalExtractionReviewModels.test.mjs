import assert from "node:assert/strict";

import {
  candidateBatchStatusLabel,
  confidenceLabel,
  formatCandidateFieldValue,
  formatValueJson,
  parseProtectedFieldKeys,
  shouldDefaultAccept,
  valuesAreEqual,
} from "./criminalExtractionReviewModels.ts";

assert.equal(formatValueJson('"诈骗罪"'), "诈骗罪");
assert.equal(formatValueJson('["诈骗罪","合同诈骗罪"]'), "诈骗罪；合同诈骗罪");
assert.equal(
  formatValueJson('[{"stage":"审查起诉","charge":"诈骗罪"}]'),
  "阶段：审查起诉；涉嫌罪名：诈骗罪",
);
assert.equal(formatValueJson(null), "未填写");
assert.equal(formatCandidateFieldValue("restitution_amount", "12000.5"), "¥12,000.50");
assert.equal(valuesAreEqual('"诈骗罪"', '"诈骗罪"'), true);
assert.equal(confidenceLabel(0.86), "高 86%");
assert.equal(confidenceLabel(0.63), "中 63%");
assert.equal(confidenceLabel(0.2), "低 20%");

const baseField = {
  id: "field-1",
  field_key: "suspected_charge",
  value_json: '"诈骗罪"',
  current_value_json: null,
  source_filename: "起诉书.pdf",
  confidence: 0.82,
  review_status: "pending",
};
assert.equal(shouldDefaultAccept(baseField), true, "画像为空且中高置信时应默认接受");
assert.equal(
  shouldDefaultAccept({ ...baseField, current_value_json: '"盗窃罪"' }),
  false,
  "画像非空时必须默认不接受",
);
assert.equal(
  shouldDefaultAccept({ ...baseField, is_user_protected: true }),
  false,
  "人工保护字段不得默认接受",
);
assert.equal(
  shouldDefaultAccept({ ...baseField, has_conflict: true }),
  false,
  "冲突候选不得默认接受",
);
assert.deepEqual(
  [...parseProtectedFieldKeys('{"fields":{"suspected_charge":"诈骗罪"}}').keys],
  ["suspected_charge"],
);
assert.equal(parseProtectedFieldKeys("{损坏").corrupt, true);
assert.equal(parseProtectedFieldKeys('{"fields":[]}').corrupt, true);
assert.equal(
  shouldDefaultAccept({ ...baseField, confidence: 0.42 }),
  false,
  "低置信候选不得默认接受",
);
assert.equal(candidateBatchStatusLabel("pending", "success"), "待确认");
assert.equal(candidateBatchStatusLabel("pending", "partial"), "部分识别失败");
assert.equal(candidateBatchStatusLabel("pending", "failed"), "识别失败");

console.log("criminalExtractionReviewModels assertions passed");
