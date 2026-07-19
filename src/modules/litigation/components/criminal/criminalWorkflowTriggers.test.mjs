import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  appliedCandidateTriggerFields,
  buildCriminalWorkflowTriggerEvents,
  stableCriminalWorkflowEventId,
} from "./criminalWorkflowTriggers.ts";

const fields = {
  detention_date: "2026/07/01 09:00",
  arrest_review_received_date: "2026-07-04",
  arrest_date: "2026-07-08",
  transfer_for_prosecution_date: "2026-08-01",
  prosecution_received_date: "2026-08-02",
  first_instance_accepted_date: "2026-09-01",
  judgment_received_date: "2026-10-01",
  second_instance_accepted_date: "2026-10-10",
  guilty_plea_status: " 已签署具结书 ",
};

const events = buildCriminalWorkflowTriggerEvents("case-1", fields);
assert.deepEqual(events.map((event) => event.eventCode), [
  "detention_confirmed",
  "arrest_review_request_confirmed",
  "arrest_confirmed",
  "prosecution_transfer_confirmed",
  "court_acceptance_confirmed",
  "first_instance_judgment_received",
  "second_instance_procedure_confirmed",
  "plea_process_confirmed",
]);
assert.equal(events[0].normalizedValue, "2026-07-01");
assert.equal(events[3].normalizedValue, "2026-08-01", "人工保存同时存在两个起诉日期时优先移送日期");
assert.equal(
  stableCriminalWorkflowEventId("case-1", "detention_confirmed", "2026-07-01"),
  stableCriminalWorkflowEventId("case-1", "detention_confirmed", "2026-07-01"),
  "相同案件、事件和规范值必须生成稳定 event_id",
);
assert.notEqual(
  stableCriminalWorkflowEventId("case-1", "detention_confirmed", "2026-07-01"),
  stableCriminalWorkflowEventId("case-1", "detention_confirmed", "2026-07-02"),
);

assert.equal(buildCriminalWorkflowTriggerEvents("case-1", { guilty_plea_status: "未确认" }).length, 0);
assert.equal(buildCriminalWorkflowTriggerEvents("case-1", { guilty_plea_status: "考虑中" }).length, 0);
assert.equal(buildCriminalWorkflowTriggerEvents("case-1", { detention_date: "2026-02-30" }).length, 0);

const batch = {
  fields: [
    { field_key: "arrest_date", value_json: '"2026-07-08"' },
    { field_key: "suspected_charge", value_json: '"诈骗罪"' },
    { field_key: "prosecution_received_date", value_json: '"2026-08-02"' },
  ],
};
const appliedValues = appliedCandidateTriggerFields(batch, ["prosecution_received_date", "suspected_charge"]);
const appliedEvents = buildCriminalWorkflowTriggerEvents(
  "case-1",
  appliedValues,
  new Set(["prosecution_received_date", "suspected_charge"]),
);
assert.deepEqual(appliedEvents.map((event) => event.eventCode), ["prosecution_transfer_confirmed"]);
assert.equal(appliedEvents[0].normalizedValue, "2026-08-02");
assert.deepEqual(appliedCandidateTriggerFields(batch, []), {}, "未 applied 的候选必须零映射");
assert.deepEqual(
  buildCriminalWorkflowTriggerEvents("case-1", appliedCandidateTriggerFields(batch, []), new Set()),
  [],
  "预览、待确认或拒绝且未进入 applied_fields 的候选不得触发",
);

const workflowPanelSource = readFileSync(new URL("./CriminalWorkflowPanel.tsx", import.meta.url), "utf8");
assert.match(workflowPanelSource, /if \(!workflowRow\)[\s\S]*refreshCriminalWorkflow\([\s\S]*event_code: "case_created"/);
assert.match(workflowPanelSource, /event_id: `case_created:\$\{caseId\}`/);

const casePanelSource = readFileSync(new URL("./CriminalCasePanel.tsx", import.meta.url), "utf8");
assert.match(casePanelSource, /result\.applied_fields[\s\S]*type: "accepted_extraction_candidate"/);
assert.match(casePanelSource, /buildCriminalWorkflowTriggerEvents\(caseId, savedInput\)/);
const rejectBody = casePanelSource.match(/const rejectCandidateBatch[\s\S]*?\n  };/)?.[0] ?? "";
assert.doesNotMatch(rejectBody, /refreshCriminalWorkflow/, "拒绝候选不得触发流程事件");

console.log("criminalWorkflowTriggers assertions passed");
