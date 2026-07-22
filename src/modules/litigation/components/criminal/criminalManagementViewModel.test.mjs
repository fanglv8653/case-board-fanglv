import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import vm from "node:vm";

const source = readFileSync(new URL("./criminalManagementViewModel.ts", import.meta.url), "utf8");
const js = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const module = { exports: {} };
vm.runInNewContext(js, { module, exports: module.exports });
const {
  MANAGEMENT_TABS,
  buildLegacyContactCandidates,
  isManagementTab,
  parseManagementPartyNames,
  resolveProsecutionAgency,
} = module.exports;

assert.deepEqual(
  Array.from(MANAGEMENT_TABS, (item) => item.label),
  ["案件概览", "进展记录", "待办提醒", "案件通讯录"],
);
assert.equal(isManagementTab("progress"), true);
assert.equal(isManagementTab("overview"), true);
assert.equal(isManagementTab("todo"), true);
assert.equal(isManagementTab("work"), false);
assert.equal(isManagementTab("materials"), false);
assert.equal(isManagementTab("drafting"), false);

assert.equal(
  resolveProsecutionAgency([
    { agency_type: "法院", agency_name: "甲法院" },
    { agency_type: "检察机关", agency_name: "乙检察院" },
  ]),
  "乙检察院",
);
assert.equal(
  resolveProsecutionAgency([], ["佛山市三水区人民检察院"]),
  "佛山市三水区人民检察院（待核实）",
);
assert.deepEqual(
  Array.from(parseManagementPartyNames('["张三","李四"]')),
  ["张三", "李四"],
);

const pendingContacts = buildLegacyContactCandidates(
  {
    fallbackAgencyName: "乙检察院",
    courtContactsJson: '[{"name":"王检察官","role":"承办检察官","phone":"123"}]',
    partyContactsJson: '[{"name":"张三","role":"犯罪嫌疑人","phone":"456"}]',
  },
  [{ agency_name: "乙检察院", contact_name: "王检察官", phone: "123" }],
);
assert.equal(pendingContacts.length, 1, "已进入正式通讯录的联系人不应再作为待确认来源");
assert.equal(pendingContacts[0].contactName, "张三");
assert.equal(pendingContacts[0].sourceLabel, "材料聚合的当事人联系人");

console.log("criminal management view model tests passed");
