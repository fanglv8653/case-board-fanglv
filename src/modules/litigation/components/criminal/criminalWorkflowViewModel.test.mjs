import assert from "node:assert/strict";

import {
  allowedCriminalTaskActions,
  bucketCriminalTasks,
  classifyCriminalTask,
  groupCriminalTasksByStage,
} from "./criminalWorkflowViewModel.ts";

const task = (overrides = {}) => ({
  id: "task-1",
  workflow_id: "workflow-1",
  case_id: "case-1",
  template_node_id: "node-1",
  node_code: "meeting",
  title: "会见",
  stage_code: "prosecution_review",
  stage_sort: 40,
  node_sort: 10,
  task_type: "meeting",
  applicability_status: "applicable",
  status: "unscheduled",
  occurrence_key: "default",
  occurrence_no: 1,
  trigger_event: "prosecution_transfer_confirmed",
  trigger_event_id: "event-1",
  trigger_source_type: "manual_confirmed",
  trigger_source_ref_id: null,
  planned_at: null,
  original_planned_at: null,
  started_at: null,
  completed_at: null,
  deferred_at: null,
  ignored_at: null,
  reopened_at: null,
  result: null,
  next_action: null,
  duration_minutes: null,
  disposition_reason: null,
  client_feedback_recorded: false,
  time_nature: "unscheduled",
  deadline_item_id: null,
  work_item_id: null,
  assigned_to: null,
  created_at: "2026-07-18T00:00:00+08:00",
  updated_at: "2026-07-18T00:00:00+08:00",
  ...overrides,
});

const now = new Date("2026-07-18T12:00:00+08:00");
assert.equal(classifyCriminalTask(task({ planned_at: "2026-07-17T09:00:00+08:00" }), now), "overdue");
assert.equal(classifyCriminalTask(task({ planned_at: "2026-07-18T18:00:00+08:00" }), now), "today");
assert.equal(classifyCriminalTask(task({ planned_at: "2026-07-25T09:00:00+08:00" }), now), "next_seven_days");
assert.equal(classifyCriminalTask(task({ status: "pending_confirmation", applicability_status: "pending_confirmation" }), now), "pending_confirmation");
assert.equal(classifyCriminalTask(task(), now), "unscheduled");
assert.equal(classifyCriminalTask(task({ node_code: "common_client_feedback", task_type: "client_feedback", title: "委托人反馈" }), now), "pending_feedback");
assert.equal(classifyCriminalTask(task({ status: "completed" }), now), "later");
assert.equal(classifyCriminalTask(task({ status: "ignored" }), now), "later");

const buckets = bucketCriminalTasks([
  task({ id: "late", planned_at: "2026-07-17T09:00:00+08:00" }),
  task({ id: "today", planned_at: "2026-07-18T09:00:00+08:00" }),
  task({ id: "unscheduled" }),
], now);
assert.equal(buckets.find((bucket) => bucket.key === "overdue").tasks.length, 1);
assert.equal(buckets.find((bucket) => bucket.key === "today").tasks.length, 1);
assert.equal(buckets.find((bucket) => bucket.key === "unscheduled").tasks.length, 1);

const stages = groupCriminalTasksByStage([
  task({ id: "p2", node_sort: 20 }),
  task({ id: "intake", stage_code: "engagement_intake", stage_sort: 10, node_sort: 10 }),
  task({ id: "p1", node_sort: 5 }),
]);
assert.deepEqual(stages.map((group) => group.stageCode), ["engagement_intake", "prosecution_review"]);
assert.deepEqual(stages[1].tasks.map((row) => row.id), ["p1", "p2"]);

assert.deepEqual(
  allowedCriminalTaskActions(task({ status: "pending_confirmation", applicability_status: "pending_confirmation" })),
  ["confirm_applicable", "not_applicable"],
);
assert.deepEqual(allowedCriminalTaskActions(task({ status: "completed" })), ["reopen"]);
assert.deepEqual(allowedCriminalTaskActions(task({ status: "not_applicable" })), []);
assert.ok(allowedCriminalTaskActions(task({ status: "in_progress" })).includes("complete"));
assert.ok(!allowedCriminalTaskActions(task({ status: "in_progress" })).includes("start"));

console.log("criminalWorkflowViewModel assertions passed");
