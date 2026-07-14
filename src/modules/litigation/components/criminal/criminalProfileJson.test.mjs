import assert from "node:assert/strict";

import {
  resolveStructuredListJson,
  structuredListToText,
  textToStructuredListJson,
} from "./criminalProfileJson.ts";

const chargeRaw = '[{"stage":"起诉","charge":"诈骗罪"}]';
const chargeText = structuredListToText(chargeRaw, "charge");
assert.equal(chargeText, "诈骗罪");
assert.equal(
  resolveStructuredListJson({
    rawJson: chargeRaw,
    initialText: chargeText,
    currentText: chargeText,
    key: "charge",
  }),
  chargeRaw,
  "未编辑罪名历史时必须原样保留对象数组及 stage 元数据",
);

const coDefendantsRaw = '[{"name":"测试同案犯"}]';
const coDefendantsText = structuredListToText(coDefendantsRaw, "name");
assert.equal(coDefendantsText, "测试同案犯");
assert.equal(
  resolveStructuredListJson({
    rawJson: coDefendantsRaw,
    initialText: coDefendantsText,
    currentText: coDefendantsText,
    key: "name",
  }),
  coDefendantsRaw,
  "未编辑同案犯时必须原样保留对象数组",
);

assert.deepEqual(JSON.parse(textToStructuredListJson("诈骗罪\n盗窃罪", "charge")), [
  { charge: "诈骗罪" },
  { charge: "盗窃罪" },
]);
assert.deepEqual(JSON.parse(textToStructuredListJson("张三\n李四", "name")), [
  { name: "张三" },
  { name: "李四" },
]);
assert.deepEqual(JSON.parse(textToStructuredListJson("诈骗罪,掩饰隐瞒犯罪所得罪", "charge")), [
  { charge: "诈骗罪,掩饰隐瞒犯罪所得罪" },
]);

console.log("criminalProfileJson assertions passed");
