import assert from "node:assert/strict";
import test from "node:test";

import { buildSentencingPrefill } from "./prefill.ts";

test("唯一精确罪名才自动预填", () => {
  const result = buildSentencingPrefill({
    caseId: "case-1",
    profileRevision: 7,
    suspectedCharge: " 诈骗罪 ",
  });
  assert.equal(result.crimeName, "诈骗罪");
  assert.deepEqual(result.crimeCandidates, ["诈骗罪"]);
  assert.equal(result.expectedProfileRevision, 7);
});

test("模糊和未知罪名不匹配", () => {
  assert.equal(buildSentencingPrefill({
    caseId: "case-1",
    profileRevision: 1,
    suspectedCharge: "涉嫌诈骗犯罪",
  }).crimeName, null);
  assert.equal(buildSentencingPrefill({
    caseId: "case-1",
    profileRevision: 1,
    suspectedCharge: "非法经营罪",
  }).crimeName, null);
});

test("多个精确候选要求人工确认", () => {
  const result = buildSentencingPrefill({
    caseId: "case-1",
    profileRevision: 3,
    suspectedCharge: "诈骗罪、盗窃罪",
  });
  assert.equal(result.crimeName, null);
  assert.equal(result.requiresCrimeConfirmation, true);
  assert.deepEqual(result.crimeCandidates, ["诈骗罪", "盗窃罪"]);
});

test("案件危险字段始终留空且无案件上下文可独立使用", () => {
  const result = buildSentencingPrefill({
    caseId: "case-1",
    profileRevision: 9,
    suspectedCharge: "诈骗罪",
    chargeHistoryJson: JSON.stringify([{ charge: "诈骗罪" }]),
    restitutionAmount: 120000,
    detentionDate: "2026-01-01",
    court: "某法院",
    notes: "自首并退赔",
  });
  assert.equal(result.amount, null);
  assert.equal(result.crimeDate, null);
  assert.equal(result.areaType, null);
  assert.equal(result.factTier, null);
  assert.deepEqual(result.factors, {});
  assert.equal(buildSentencingPrefill().caseId, null);
});
