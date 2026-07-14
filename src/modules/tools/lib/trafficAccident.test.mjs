import assert from "node:assert/strict";

import {
  calculateDependentCompensation,
  calculateTraffic,
} from "./trafficAccident.ts";

const deathTwoSole = calculateDependentCompensation(10_000, 1, [
  { years: 10, supporters: 1 },
  { years: 5, supporters: 1 },
]);
assert.equal(deathTwoSole.uncapped, 150_000);
assert.equal(deathTwoSole.capped, 100_000);
assert.equal(deathTwoSole.capApplied, true);

const deathTwoShared = calculateDependentCompensation(10_000, 1, [
  { years: 10, supporters: 2 },
  { years: 5, supporters: 2 },
]);
assert.equal(deathTwoShared.uncapped, 75_000);
assert.equal(deathTwoShared.capped, 75_000);
assert.equal(deathTwoShared.capApplied, false);

const disabledThree = calculateDependentCompensation(10_000, 0.5, [
  { years: 10, supporters: 1 },
  { years: 10, supporters: 1 },
  { years: 10, supporters: 1 },
]);
assert.equal(disabledThree.uncapped, 150_000);
assert.equal(disabledThree.capped, 100_000);
assert.equal(disabledThree.capApplied, true);

const fractional = calculateDependentCompensation(10_000, 1, [
  { years: 1.5, supporters: 1 },
  { years: 0.5, supporters: 1 },
]);
assert.equal(fractional.uncapped, 20_000);
assert.equal(fractional.capped, 15_000);
assert.equal(fractional.capApplied, true);

assert.deepEqual(calculateDependentCompensation(10_000, 1, [{ years: 3, supporters: 2 }]), {
  uncapped: 15_000,
  capped: 15_000,
  capApplied: false,
});
assert.deepEqual(calculateDependentCompensation(10_000, 1, [{ years: 0, supporters: 1 }]), {
  uncapped: 0,
  capped: 0,
  capApplied: false,
});
assert.deepEqual(calculateDependentCompensation(10_000, 0, [{ years: 10, supporters: 1 }]), {
  uncapped: 0,
  capped: 0,
  capApplied: false,
});

const baseInput = {
  perCapitaIncome: 0,
  perCapitaConsumption: 10_000,
  avgMonthlyWage: 0,
  victimAge: 30,
  responsibilityPct: 100,
  isDisability: false,
  disabilityLevel: 1,
  isDeath: false,
  dependents: [{ years: 10, supporters: 1 }],
  medical: 0,
  followUp: 0,
  rehab: 0,
  lostWork: 0,
  nursing: 0,
  transport: 0,
  lodging: 0,
  mealSubsidy: 0,
  nutrition: 0,
  assistiveDevice: 0,
  appraisal: 0,
  propertyLoss: 0,
  useFuneralAuto: false,
  funeral: 0,
  mentalClaim: 0,
  jqxPaid: 0,
  syxPaid: 0,
};
assert.equal(calculateTraffic(baseInput).dependentComp, 0, "非伤残且非死亡不计算被扶养人生活费");

console.log("trafficAccident assertions passed");
