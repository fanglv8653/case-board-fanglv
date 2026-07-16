import assert from "node:assert/strict";

import {
  calculateDivorceFee,
  calculatePropertyFee,
} from "./litigationFee.ts";

assert.equal(calculatePropertyFee(1), 50, "1万元及以下按件收取50元");
assert.equal(calculatePropertyFee(10), 2300, "10万元应覆盖首档累进金额");
assert.equal(calculatePropertyFee(20), 4300, "20万元应覆盖前两档累进金额");
assert.equal(calculatePropertyFee(50), 8800, "50万元应覆盖前三档累进金额");
assert.equal(calculatePropertyFee(100), 13800, "100万元应覆盖前四档累进金额");

assert.equal(calculateDivorceFee(0, false), 200, "无财产分割按当前200元估算");
assert.equal(calculateDivorceFee(20, true), 200, "20万元以内不另行交纳");
assert.equal(calculateDivorceFee(30, true), 700, "30万元应对超出10万元部分按0.5%计收");
assert.equal(calculateDivorceFee(30, false), 200, "未勾选财产分割时不累加财产部分");

console.log("litigationFee assertions passed");
