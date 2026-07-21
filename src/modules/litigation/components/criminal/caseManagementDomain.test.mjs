import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const panel = readFileSync(new URL("./CriminalCasePanel.tsx", import.meta.url), "utf8");
const caseView = readFileSync(new URL("../CaseView.tsx", import.meta.url), "utf8");

assert.match(caseView, /domain === "criminal" \|\| domain === "civil"/);
assert.match(caseView, /<CriminalCasePanel[\s\S]*?domain=\{domain\}/);
assert.match(panel, /domain\?: "criminal" \| "civil"/);
assert.match(panel, /managementTab === "todo" && isCriminal/);
assert.match(panel, /isCriminal \? listCriminalDeadlineItems\(caseId\) : Promise\.resolve\(\[\]\)/);
assert.match(panel, /function newStageForm\(caseId: string, domain: "criminal" \| "civil"/);
assert.match(panel, /item\s*\?\s*\{\s*\.\.\.item,/s, "编辑进展必须保留旧记录的飞书外部标识和原始载荷");

console.log("case management domain separation tests passed");
