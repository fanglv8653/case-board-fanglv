import assert from "node:assert/strict";

import { calculateDivorceFee } from "./litigationFee.ts";

assert.equal(calculateDivorceFee(0, false), 200, "无财产分割按当前200元估算");
assert.equal(calculateDivorceFee(20, true), 200, "20万元以内不另行交纳");
assert.equal(calculateDivorceFee(30, true), 700, "30万元应对超出10万元部分按0.5%计收");
assert.equal(calculateDivorceFee(30, false), 200, "未勾选财产分割时不累加财产部分");

console.log("litigationFee assertions passed");
