import assert from "node:assert/strict";

import {
  calculateRiskAgencyCap,
  calculateZhejiangHistoricalReference,
  createPracticeQuoteProfile,
  getRegionFeeRegime,
  PROVINCIAL_REGIONS,
} from "./nationalLawyerFee.ts";

assert.equal(PROVINCIAL_REGIONS.length, 34);
assert.equal(new Set(PROVINCIAL_REGIONS.map((item) => item.code)).size, 34);
for (const code of ["710000", "810000", "820000"]) {
  const regime = getRegionFeeRegime(code);
  assert.equal(regime.status, "unsupported");
  assert.equal(regime.autoOfficialCalculation, false);
}

assert.equal(getRegionFeeRegime("110000").status, "market_pricing");
assert.equal(getRegionFeeRegime("440000").status, "market_pricing");
assert.equal(getRegionFeeRegime("310000").status, "market_pricing");
assert.equal(getRegionFeeRegime("510000").status, "historical_only");
assert.equal(getRegionFeeRegime("320000").status, "conflict_unverified");
assert.equal(getRegionFeeRegime("320000", "320200").regionName, "江苏·无锡");
assert.equal(getRegionFeeRegime("320000", "320200").autoOfficialCalculation, false);
assert.equal(getRegionFeeRegime("330000").status, "market_pricing");
for (const code of ["110000", "440000", "310000", "510000", "320000", "330000"]) {
  const regime = getRegionFeeRegime(code);
  assert.equal(regime.autoOfficialCalculation, false);
  assert.ok(regime.sources.length > 0);
  assert.ok(regime.sources.every((source) => source.url.startsWith("https://")));
}

const zhejiangVectors = [
  [100_000, 6_000, 8_000],
  [500_000, 26_000, 32_000],
  [1_000_000, 46_000, 57_000],
  [5_000_000, 166_000, 217_000],
  [10_000_000, 266_000, 367_000],
  [20_000_000, 366_000, 567_000],
];
for (const [amount, min, max] of zhejiangVectors) {
  const result = calculateZhejiangHistoricalReference({
    matter: "civil",
    propertyAmountYuan: amount,
    historicalReferenceConfirmed: true,
  });
  assert.equal(result.status, "reference_only");
  assert.equal(result.minYuan, min);
  assert.equal(result.maxYuan, max);
}
const small = calculateZhejiangHistoricalReference({
  matter: "civil",
  propertyAmountYuan: 10_000,
  historicalReferenceConfirmed: true,
});
assert.equal(small.minYuan, 600);
assert.equal(small.maxYuan, 800);
assert.equal(small.mayCharge2500, true);

for (const [stage, min, max] of [
  ["investigation", 1_500, 8_000],
  ["prosecution", 1_500, 10_000],
  ["trial_first", 2_500, 25_000],
]) {
  const result = calculateZhejiangHistoricalReference({
    matter: "criminal",
    criminalStage: stage,
    historicalReferenceConfirmed: true,
  });
  assert.equal(result.minYuan, min);
  assert.equal(result.maxYuan, max);
}
assert.equal(
  calculateZhejiangHistoricalReference({
    matter: "criminal",
    criminalStage: "private_prosecution",
    historicalReferenceConfirmed: true,
  }).manualAdjustmentRequired,
  true,
);
const laterStage = calculateZhejiangHistoricalReference({
  matter: "civil",
  propertyAmountYuan: null,
  historicalReferenceConfirmed: true,
  procedureStage: "later_same_firm",
  priorStageStandardYuan: 10_000,
});
assert.equal(laterStage.laterStageCapYuan, 7_000);
assert.equal(laterStage.maxYuan, 7_000);
assert.equal(
  calculateZhejiangHistoricalReference({
    matter: "civil",
    propertyAmountYuan: null,
    historicalReferenceConfirmed: true,
    procedureStage: "later_same_firm",
  }).manualAdjustmentRequired,
  true,
);
const complex = calculateZhejiangHistoricalReference({
  matter: "criminal",
  criminalStage: "trial_first",
  historicalReferenceConfirmed: true,
  complexRequested: true,
  complexQualified: true,
});
assert.equal(complex.complexUpperLimitYuan, 125_000);
assert.equal(complex.maxYuan, 125_000);
assert.equal(
  calculateZhejiangHistoricalReference({
    matter: "criminal",
    criminalStage: "trial_first",
    historicalReferenceConfirmed: true,
    complexRequested: true,
    complexQualified: false,
  }).manualAdjustmentRequired,
  true,
);
assert.throws(() =>
  calculateZhejiangHistoricalReference({
    matter: "civil",
    propertyAmountYuan: 100_000,
    historicalReferenceConfirmed: false,
  }),
);

const riskVectors = [
  [500_000, 90_000], [1_000_000, 180_000], [2_000_000, 330_000],
  [5_000_000, 780_000], [6_000_000, 900_000], [10_000_000, 1_380_000],
  [20_000_000, 2_280_000], [50_000_000, 4_980_000], [60_000_000, 5_580_000],
];
for (const [amount, expected] of riskVectors) {
  const result = calculateRiskAgencyCap(amount, "general_property_civil");
  assert.equal(result.allowed, true);
  assert.equal(result.maximumFeeYuan, expected);
  assert.equal(result.aggregateAllStages, true);
}
for (const category of [
  "criminal", "administrative", "state_compensation", "group_litigation",
  "marriage_inheritance", "social_security", "minimum_living_security", "support",
  "pension_relief", "work_injury", "labor_remuneration",
]) {
  const result = calculateRiskAgencyCap(2_000_000, category);
  assert.equal(result.allowed, false);
  assert.equal(result.error, "RISK_AGENT_PROHIBITED");
}

const quote = createPracticeQuoteProfile({ label: "本所民商事参考", minYuan: 10_000, maxYuan: 20_000 });
assert.equal(quote.source, "law_firm_internal");
assert.match(quote.note, /非官方/);

console.log("national lawyer fee assertions passed");
