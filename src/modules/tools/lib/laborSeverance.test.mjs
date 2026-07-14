import assert from "node:assert/strict";

import { calculateSeverance } from "./laborSeverance.ts";

const common = {
  scenario: "notice",
  startDate: "2020-01-01",
  endDate: "2022-07-01",
  avgMonthlyWage: 10_000,
  lastMonthWage: 8_000,
  localAvgWage: 0,
  withAnnualLeave: false,
  annualLeaveDays: 0,
  annualLeaveBase: 0,
  annualLeaveMode: "supplement",
};
const notice = calculateSeverance(common);
assert.ok(notice);
assert.equal(notice.severanceMonths, 3);
assert.equal(notice.economicComp, 30_000);
assert.equal(notice.noticePay, 8_000, "+1必须按上一个月工资");
assert.equal(notice.primaryAmount, 38_000);

const highIncome = calculateSeverance({
  ...common,
  startDate: "2000-01-01",
  endDate: "2020-01-01",
  avgMonthlyWage: 30_000,
  lastMonthWage: 30_000,
  localAvgWage: 5_000,
});
assert.ok(highIncome);
assert.equal(highIncome.base, 15_000);
assert.equal(highIncome.severanceMonths, 12);
assert.equal(highIncome.economicComp, 180_000);
assert.equal(highIncome.noticePay, 30_000, "三倍社平和十二年封顶不得套用到+1");
assert.equal(highIncome.primaryAmount, 210_000);

const economic = calculateSeverance({ ...common, scenario: "economic" });
assert.ok(economic);
assert.equal(economic.noticePay, null, "非第四十条代通知金路线不得产生+1");
assert.equal(economic.primaryAmount, 30_000);

const illegal = calculateSeverance({ ...common, scenario: "illegal" });
assert.ok(illegal);
assert.equal(illegal.noticePay, null, "违法解除2N不得混入代通知金");
assert.equal(illegal.primaryAmount, 60_000);

console.log("laborSeverance assertions passed");
