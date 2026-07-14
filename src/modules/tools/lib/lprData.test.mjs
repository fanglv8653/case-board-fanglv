import assert from "node:assert/strict";

import {
  applyCachedLprPoints,
  BUILTIN_LPR_DATA,
  getLprForDate,
  LPR_DATA,
  mergeLprPoints,
} from "./lprData.ts";
import {
  calcFiveStage,
  calculateInterestByPeriod,
  calculateInterestSegments,
} from "./interestCalc.ts";

assert.equal(getLprForDate("2024-10-20", "1y"), 3.35);
assert.equal(getLprForDate("2024-10-21", "1y"), 3.1);
assert.equal(getLprForDate("2025-10-20", "1y"), 3.0);
assert.equal(getLprForDate("2025-10-20", "5y+"), 3.5);

const merged = mergeLprPoints(
  [
    { date: "2024-10-21", lpr1y: 3.1, lpr5y: 3.6 },
    { date: "2024-09-20", lpr1y: 3.35, lpr5y: 3.85 },
  ],
  [
    {
      publication_date: "2024-10-21",
      lpr_1y: 3.05,
      lpr_5y: 3.55,
    },
    {
      publication_date: "2024-11-20",
      lpr_1y: 3.0,
      lpr_5y: 3.5,
    },
    {
      publication_date: "2024-11-20",
      lpr_1y: 3.0,
      lpr_5y: 3.5,
    },
  ],
);
assert.deepEqual(merged.map((point) => point.date), [
  "2024-09-20",
  "2024-10-21",
  "2024-11-20",
]);
assert.equal(merged[1].lpr1y, 3.05, "官方缓存必须覆盖同日基线");
assert.throws(
  () =>
    mergeLprPoints([], [
      { publication_date: "2026-07-20", lpr_1y: 2.9, lpr_5y: 3.4 },
      { publication_date: "2026-07-20", lpr_1y: 3.0, lpr_5y: 3.5 },
    ]),
  /同一发布日期存在冲突值/,
);
assert.throws(
  () =>
    mergeLprPoints([], [
      { publication_date: "2026-07-20", lpr_1y: 20, lpr_5y: 3.5 },
    ]),
  /无效利率/,
);

const beforeRefresh = calculateInterestByPeriod(
  100_000,
  "2026-07-20",
  "2026-07-21",
  "lpr",
  0,
  "1y",
);
const runtimeReference = LPR_DATA;
applyCachedLprPoints([
  { publication_date: "2026-07-20", lpr_1y: 2.9, lpr_5y: 3.4 },
]);
assert.equal(LPR_DATA, runtimeReference, "运行时更新必须保持同一数组引用");
const afterRefresh = calculateInterestByPeriod(
  100_000,
  "2026-07-20",
  "2026-07-21",
  "lpr",
  0,
  "1y",
);
assert.notEqual(afterRefresh, beforeRefresh, "刷新成功后计算必须即时使用新点");
assert.equal(afterRefresh, 7.95);

const snapshotBeforeFailure = structuredClone(LPR_DATA);
assert.throws(() =>
  applyCachedLprPoints([
    { publication_date: "2026-07-20", lpr_1y: 2.9, lpr_5y: 3.4 },
    { publication_date: "2026-07-20", lpr_1y: 3.0, lpr_5y: 3.5 },
  ]),
);
assert.deepEqual(LPR_DATA, snapshotBeforeFailure, "失败不得清空或改写旧运行数据");
applyCachedLprPoints([]);
assert.deepEqual(LPR_DATA, BUILTIN_LPR_DATA);

const fixed = calculateInterestByPeriod(
  100_000,
  "2026-01-01",
  "2026-01-02",
  "custom",
  10,
  "1y",
);
assert.equal(fixed, 27.4, "固定利率公式不得回归");

const lprBase = calculateInterestByPeriod(
  100_000,
  "2025-10-20",
  "2025-10-21",
  "lpr",
  0,
  "1y",
  1,
);
const lprDouble = calculateInterestByPeriod(
  100_000,
  "2025-10-20",
  "2025-10-21",
  "lpr",
  0,
  "1y",
  2,
);
assert.equal(lprDouble, lprBase * 2, "LPR倍数不得回归");

const hybrid = calculateInterestSegments(
  100_000,
  "2020-08-19",
  "2020-08-21",
  "hybrid",
  24,
  "1y",
  4,
);
assert.deepEqual(hybrid.map((segment) => segment.rateType), ["hybrid", "hybrid"]);
assert.equal(hybrid[0].rate, 24);
assert.equal(hybrid[1].rate, 15.4);

const execution = calcFiveStage(
  {
    id: 1,
    name: "回归案件",
    principal: 10_000,
    rate: 10,
    rateType: "custom",
    lprTerm: "1y",
    lprMultiplier: 1,
    startDate: "2026-01-01",
    endDate: "2026-01-02",
    litigationFee: 0,
    lawyerFee: 0,
    otherFee: 0,
  },
  [],
  true,
);
assert.equal(execution.accumulatedInterest, 10_000 * 0.1 / 365);
assert.equal(execution.accumulatedDelayed, 1.75);
assert.equal(execution.total, 10_000 + 10_000 * 0.1 / 365 + 1.75);

console.log("lprData and interest regression assertions passed");
