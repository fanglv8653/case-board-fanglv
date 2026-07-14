import assert from "node:assert/strict";

import {
  needsApplicabilityOverrideReason,
  resolveDeadlineStageId,
} from "./criminalTimelineRules.ts";

const stages = [
  { id: "investigation", major_stage: "侦查阶段", stage_label: "侦查" },
  { id: "prosecution", major_stage: "审查起诉", stage_label: "起诉" },
];

assert.equal(
  resolveDeadlineStageId(
    { stage_item_id: "investigation", major_stage: "审查起诉" },
    stages,
  ),
  "investigation",
  "有效 stage_item_id 必须优先于大阶段匹配",
);
assert.equal(
  resolveDeadlineStageId({ stage_item_id: null, major_stage: " 审查 起诉 " }, stages),
  "prosecution",
  "无显式关联时应规范化 major_stage 并唯一匹配",
);
assert.equal(
  resolveDeadlineStageId(
    { major_stage: "审查起诉" },
    [...stages, { id: "prosecution-2", major_stage: "审查起诉", stage_label: "二次退补" }],
  ),
  null,
  "大阶段多匹配时必须保持未归类",
);
assert.equal(
  resolveDeadlineStageId(
    { major_stage: "一审" },
    [{ id: "trial", major_stage: null, stage_label: "一审" }],
  ),
  "trial",
  "major_stage 无匹配时可按唯一 stage_label 回退",
);

const autoDeadline = {
  source_type: "auto",
  applicability_status: "needs_confirmation",
};
assert.equal(
  needsApplicabilityOverrideReason(autoDeadline, {
    applicability_status: "confirmed",
    override_reason: "",
  }),
  true,
  "自动期限改变适用性且无原因时必须阻止保存",
);
assert.equal(
  needsApplicabilityOverrideReason(autoDeadline, {
    applicability_status: "confirmed",
    override_reason: "已核对案件事实",
  }),
  false,
);
assert.equal(
  needsApplicabilityOverrideReason(
    { source_type: "manual", applicability_status: "confirmed" },
    { applicability_status: "not_applicable", override_reason: "" },
  ),
  false,
  "人工期限不强制修正原因",
);

console.log("criminalTimelineRules assertions passed");
