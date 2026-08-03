import assert from "node:assert/strict";
import test from "node:test";

import {
  CRIMINAL_STAGE_LIST,
  normalizeCriminalStageId,
  resolveCriminalCaseStatus,
} from "./criminalCaseStatus.ts";

test("刑事状态列表不包含民事状态", () => {
  const labels = CRIMINAL_STAGE_LIST.map((item) => item.label);
  for (const forbidden of ["仲裁中", "已调解", "上诉期", "待开庭"]) {
    assert.equal(labels.includes(forbidden), false);
  }
});
test("历史刑事阶段归一到稳定主序列", () => {
  assert.equal(normalizeCriminalStageId("侦查阶段"), "侦查");
  assert.equal(normalizeCriminalStageId("审查逮捕"), "审查逮捕");
  assert.equal(normalizeCriminalStageId("检察院审查起诉"), "审查起诉");
  assert.equal(normalizeCriminalStageId("second_instance"), "上诉及二审");
  assert.equal(normalizeCriminalStageId("未知自由文本"), "待确认");
});

test("刑事卡片只从刑事画像解析阶段", () => {
  assert.equal(resolveCriminalCaseStatus({ current_stage: "一审阶段" }).id, "一审");
  assert.equal(resolveCriminalCaseStatus(null).id, "待确认");
});
