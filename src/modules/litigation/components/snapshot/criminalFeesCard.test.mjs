import assert from "node:assert/strict";
import { isApplicableCriminalFee } from "./criminalFeeModels.ts";

assert.equal(isApplicableCriminalFee("案件受理费"), false);
assert.equal(isApplicableCriminalFee("财产保全费"), false);
assert.equal(isApplicableCriminalFee("律师代理费"), true);
assert.equal(isApplicableCriminalFee("差旅费"), true);
console.log("criminal fee filter assertions passed");
