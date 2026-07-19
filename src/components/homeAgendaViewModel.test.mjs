import assert from "node:assert/strict";

import {
  AGENDA_SOURCE_META,
  agendaDotClass,
  classifyCriminalSummaryTask,
  criminalDeadlineRowsToAgenda,
  criminalSummaryRowsToAgenda,
  dateKeyFromTimestamp,
  dedupeAgendaEvents,
  summarizeCriminalTaskRows,
} from "./homeAgendaViewModel.ts";

const row = (overrides = {}) => ({
  case_id: "case-1",
  case_name: "合成刑事案件",
  task_id: "task-1",
  title: "首次会见",
  stage_code: "detention_arrest_review",
  task_type: "meeting",
  status: "pending",
  applicability_status: "applicable",
  planned_at: null,
  client_feedback_required: false,
  ...overrides,
});

const now = new Date("2026-07-18T12:00:00+08:00");
assert.equal(classifyCriminalSummaryTask(row({ planned_at: "2026-07-17T09:00:00+08:00" }), now), "overdue");
assert.equal(classifyCriminalSummaryTask(row({ planned_at: "2026-07-18T09:00:00+08:00" }), now), "today");
assert.equal(classifyCriminalSummaryTask(row({ planned_at: "2026-07-25T09:00:00+08:00" }), now), "next_seven_days");
assert.equal(classifyCriminalSummaryTask(row({ status: "pending_confirmation", applicability_status: "pending_confirmation" }), now), "pending_confirmation");
assert.equal(classifyCriminalSummaryTask(row(), now), "unscheduled");
assert.equal(classifyCriminalSummaryTask(row({ title: "向家属反馈", client_feedback_required: true }), now), "pending_feedback");

const stats = summarizeCriminalTaskRows([
  row({ task_id: "late", planned_at: "2026-07-17T09:00:00+08:00" }),
  row({ task_id: "today", planned_at: "2026-07-18T09:00:00+08:00" }),
  row({ task_id: "week", planned_at: "2026-07-20T09:00:00+08:00" }),
  row({ task_id: "confirm", status: "pending_confirmation", applicability_status: "pending_confirmation" }),
  row({ task_id: "schedule" }),
  row({ task_id: "feedback", title: "向家属反馈", client_feedback_required: true }),
  row({ task_id: "hidden", status: "ignored" }),
], now);
assert.deepEqual(stats, {
  overdue: 1,
  today: 1,
  next_seven_days: 1,
  pending_confirmation: 1,
  unscheduled: 1,
  pending_feedback: 1,
});

assert.equal(dateKeyFromTimestamp("2026-07-21T09:30:00+08:00"), "2026-07-21");
assert.equal(dateKeyFromTimestamp("2026-02-30T09:30:00+08:00"), null);
assert.equal(dateKeyFromTimestamp("not-a-date"), null);
assert.deepEqual(criminalSummaryRowsToAgenda([
  row({ task_id: "planned", planned_at: "2026-07-21T09:30:00+08:00" }),
  row({ task_id: "hidden", planned_at: "2026-07-22T09:30:00+08:00", status: "completed" }),
], now), [{
  kind: "sop",
  date: "2026-07-21",
  daysFromNow: 3,
  type: "首次会见",
  note: "detention_arrest_review",
  caseName: "合成刑事案件",
  caseId: "case-1",
  id: "planned",
}]);

assert.deepEqual(criminalDeadlineRowsToAgenda([{
  deadline_id: "deadline-1",
  case_id: "case-1",
  case_name: "合成刑事案件",
  title: "审查逮捕期限",
  rule_code: "DETENTION_ARREST_REVIEW",
  deadline_at: "2026-07-19T23:59:59+08:00",
  status: "pending",
  applicability_status: "confirmed",
}], now), [{
  kind: "deadline",
  date: "2026-07-19",
  daysFromNow: 1,
  type: "审查逮捕期限",
  note: "DETENTION_ARREST_REVIEW",
  caseName: "合成刑事案件",
  caseId: "case-1",
  id: "deadline-1",
}]);

assert.equal(agendaDotClass("deadline"), "bg-orange-500");
assert.equal(AGENDA_SOURCE_META.sop.label, "SOP 任务");
assert.notEqual(agendaDotClass("deadline"), agendaDotClass("sop"));
assert.equal(new Set(Object.values(AGENDA_SOURCE_META).map((meta) => meta.dotClass)).size, 6);

const duplicateBase = {
  date: "2026-07-19",
  daysFromNow: 1,
  type: "审查逮捕期限",
  note: null,
  caseName: "合成刑事案件",
  caseId: "case-1",
};
assert.deepEqual(dedupeAgendaEvents([
  { ...duplicateBase, kind: "deadline" },
  { ...duplicateBase, kind: "deadline", id: "deadline-1" },
  { ...duplicateBase, kind: "sop", id: "task-1" },
  { ...duplicateBase, kind: "sop", id: "task-2" },
  { ...duplicateBase, kind: "todo", id: "todo-1" },
  { ...duplicateBase, kind: "todo", id: "todo-2" },
]), [
  { ...duplicateBase, kind: "deadline", id: "deadline-1" },
  { ...duplicateBase, kind: "sop", id: "task-1" },
  { ...duplicateBase, kind: "sop", id: "task-2" },
  { ...duplicateBase, kind: "todo", id: "todo-1" },
  { ...duplicateBase, kind: "todo", id: "todo-2" },
]);

console.log("homeAgendaViewModel assertions passed");
