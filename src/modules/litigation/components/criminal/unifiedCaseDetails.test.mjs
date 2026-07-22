import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const caseView = readFileSync(new URL("../CaseView.tsx", import.meta.url), "utf8");
const snapshot = readFileSync(
  new URL("../snapshot/CaseSnapshotView.tsx", import.meta.url),
  "utf8",
);
const panel = readFileSync(new URL("./CriminalCasePanel.tsx", import.meta.url), "utf8");

test("header and in-tab basic information share the canonical case display name", () => {
  assert.match(caseView, /selectedCase \? getCaseDisplayName\(selectedCase\) : "—"/);
  assert.match(snapshot, /label="案件名称" value=\{getCaseDisplayName\(caseData\)\}/);
  assert.match(caseView, /basicInformation=\{[\s\S]*?contentMode="basic"/);
  assert.match(caseView, /contentMode="supplemental"/);
});

test("case information, todo and contacts are consolidated without losing their stores", () => {
  assert.match(panel, /\{basicInformation\}/);
  assert.doesNotMatch(panel, /ManagementOverview/);
  assert.match(panel, /<TodosCard caseId=\{caseId\} \/>/);
  assert.match(panel, /<CriminalWorkflowPanel/);
  assert.match(panel, /listCaseAgencyContacts\(caseId\)/);
  assert.match(panel, /材料抽取的待确认联系人/);
  assert.match(panel, /confirmLegacyContact\(candidate\)/);
});

test("criminal identity fields remain separate and stage-aware", () => {
  assert.match(panel, /buildCriminalCaseIdentity/);
  assert.match(panel, /criminalIdentity\.pureCharge/);
  assert.match(panel, /criminalIdentity\.partyNameLabel/);
  assert.match(panel, /criminalIdentity\.stageDate\.label/);
  assert.match(panel, /value=\{profileForm\.client_name \?\? ""\}/);
  assert.match(panel, /criminalIdentity\.prosecutionAuthority \|\| "待核实"/);
  assert.match(panel, /label="当前承办 \/ 审判机关"/);
  assert.doesNotMatch(panel, /client_name[^\n]*(plaintiffs|defendants|prosecutionAgency)/);
});

test("legacy snapshot keeps editable basics only inside the information tab", () => {
  assert.match(snapshot, /contentMode\?: "full" \| "basic" \| "supplemental"/);
  assert.match(snapshot, /contentMode !== "supplemental" \? \[TITLES\.BASIC\]/);
  assert.match(snapshot, /contentMode !== "basic" \? \[TITLES\.FEE, TITLES\.TIMELINE\]/);
  assert.match(snapshot, /!isCriminal && \([\s\S]*?label="案由"/);
  assert.match(snapshot, /!isCriminal && <FactRow label="案件类型"/);
  assert.match(snapshot, /label="案件状态"[\s\S]*?edit\("agg_status_text"\)/);
});
