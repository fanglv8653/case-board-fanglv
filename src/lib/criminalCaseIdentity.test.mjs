import assert from "node:assert/strict";
import test from "node:test";

import {
  buildCriminalCaseIdentity,
  criminalPartyTermForIdentityStage,
  mergeCriminalDisplayNameOverride,
  normalizeCriminalProcedureStage,
  resolveCriminalDisplayName,
  resolvePureCriminalCharge,
  selectCriminalStageDate,
} from "./criminalCaseIdentity.ts";

test("案件显示名称和纯罪名使用不同输入源", () => {
  assert.equal(resolvePureCriminalCharge(" 贪污罪、受贿罪 "), "贪污罪、受贿罪");
  assert.equal(resolvePureCriminalCharge(null), null);
  assert.equal(
    resolveCriminalDisplayName({
      displayNameOverride: "杨赛清案（人工名称）",
      suspectOrDefendantName: "杨赛清",
      suspectedCharge: "贪污罪、受贿罪",
      storedName: "20260617杨赛清贪污罪、受贿罪",
    }).value,
    "杨赛清案（人工名称）",
  );
  // 即使案件名含“罪”，没有 suspectedCharge 也不能反向切出纯罪名。
  assert.equal(
    buildCriminalCaseIdentity({ storedName: "杨赛清贪污罪、受贿罪" }).pureCharge,
    null,
  );
});

test("显示名称按人工、当事人加罪名、罪名、原名称顺序回退", () => {
  assert.deepEqual(
    resolveCriminalDisplayName({
      suspectOrDefendantName: "杨赛清",
      suspectedCharge: "贪污罪、受贿罪",
      storedName: "1刑事委托材料",
    }),
    { value: "杨赛清贪污罪、受贿罪", source: "party_and_charge" },
  );
  assert.deepEqual(resolveCriminalDisplayName({ suspectedCharge: "诈骗罪" }), {
    value: "诈骗罪",
    source: "charge",
  });
  assert.deepEqual(resolveCriminalDisplayName({ storedName: "1刑事委托材料" }), {
    value: "1刑事委托材料",
    source: "stored_name",
  });
  assert.deepEqual(resolveCriminalDisplayName({}), {
    value: "未命名刑事案件",
    source: "unknown",
  });
});

test("重识别和飞书候选不能覆盖已有人工 display_name_override", () => {
  assert.equal(
    mergeCriminalDisplayNameOverride("人工案件名", "识别候选名", "recognition"),
    "人工案件名",
  );
  assert.equal(
    mergeCriminalDisplayNameOverride("人工案件名", "飞书案件名", "feishu"),
    "人工案件名",
  );
  assert.equal(mergeCriminalDisplayNameOverride(null, "飞书案件名", "feishu"), "飞书案件名");
  assert.equal(mergeCriminalDisplayNameOverride("旧人工名", "新人工名", "manual"), "新人工名");
  assert.equal(mergeCriminalDisplayNameOverride("旧人工名", " ", "manual"), null);
});

test("刑事程序阶段支持明确中文和稳定英文代码，模糊审判不猜审级", () => {
  for (const stage of ["侦查阶段", "审查逮捕", "investigation_stage"]) {
    assert.equal(normalizeCriminalProcedureStage(stage), "investigation");
  }
  for (const stage of ["审查起诉阶段", "检察院审查起诉", "review_prosecution"]) {
    assert.equal(normalizeCriminalProcedureStage(stage), "prosecution");
  }
  assert.equal(normalizeCriminalProcedureStage("一审阶段"), "first_instance");
  assert.equal(normalizeCriminalProcedureStage("second_instance"), "second_instance");
  assert.equal(normalizeCriminalProcedureStage("法院审判阶段"), "trial_other");
  assert.equal(normalizeCriminalProcedureStage("待确认"), "unknown");
  assert.equal(normalizeCriminalProcedureStage(null), "unknown");
});

test("侦查和审查起诉称犯罪嫌疑人，审判称被告人，未知使用未决组合称谓", () => {
  assert.equal(criminalPartyTermForIdentityStage("侦查"), "犯罪嫌疑人");
  assert.equal(criminalPartyTermForIdentityStage("审查起诉"), "犯罪嫌疑人");
  assert.equal(criminalPartyTermForIdentityStage("一审"), "被告人");
  assert.equal(criminalPartyTermForIdentityStage("二审"), "被告人");
  assert.equal(criminalPartyTermForIdentityStage("法院审判"), "被告人");
  assert.equal(criminalPartyTermForIdentityStage("unknown"), "犯罪嫌疑人/被告人");
});

test("阶段日期只能选择本阶段专属字段", () => {
  const dates = {
    detention_date: "2026-01-01",
    prosecution_received_date: "2026-02-02",
    first_instance_accepted_date: "2026-03-03",
    second_instance_accepted_date: "2026-04-04",
    filed_at: "1999-09-09",
  };
  assert.deepEqual(selectCriminalStageDate("侦查", dates), {
    field: "detention_date",
    label: "拘留日期",
    value: "2026-01-01",
    displayValue: "2026-01-01",
    status: "provided",
  });
  assert.deepEqual(selectCriminalStageDate("审查起诉", dates), {
    field: "prosecution_received_date",
    label: "审查起诉收案日期",
    value: "2026-02-02",
    displayValue: "2026-02-02",
    status: "provided",
  });
  assert.deepEqual(selectCriminalStageDate("一审", dates), {
    field: "first_instance_accepted_date",
    label: "一审受理日期",
    value: "2026-03-03",
    displayValue: "2026-03-03",
    status: "provided",
  });
  assert.deepEqual(selectCriminalStageDate("二审", dates), {
    field: "second_instance_accepted_date",
    label: "二审受理日期",
    value: "2026-04-04",
    displayValue: "2026-04-04",
    status: "provided",
  });
});

test("专属阶段日期缺失时返回可编辑 field key 和待核实，不回退 filed_at", () => {
  assert.deepEqual(
    selectCriminalStageDate("侦查", {
      detention_date: null,
      prosecution_received_date: "2026-02-02",
      filed_at: "2026-01-15",
    }),
    {
      field: "detention_date",
      label: "拘留日期",
      value: null,
      displayValue: "待核实",
      status: "missing",
    },
  );
});

test("审级无法确定或阶段未知时不借用其他阶段日期", () => {
  assert.equal(
    selectCriminalStageDate("法院审判", {
      first_instance_accepted_date: "2026-03-03",
      filed_at: "2026-01-15",
    }),
    null,
  );
  assert.equal(selectCriminalStageDate("待确认", { filed_at: "2026-01-15" }), null);
});

test("公诉机关、犯罪嫌疑人或被告人、委托人保持独立且不互相补位", () => {
  const identity = buildCriminalCaseIdentity({
    currentStage: "审查起诉",
    prosecutionAuthority: "某市人民检察院",
    suspectOrDefendantName: "张某",
    clientName: "李某",
    suspectedCharge: "诈骗罪",
    prosecution_received_date: "2026-05-06",
  });
  assert.equal(identity.prosecutionAuthority, "某市人民检察院");
  assert.equal(identity.suspectOrDefendantName, "张某");
  assert.equal(identity.clientName, "李某");
  assert.equal(identity.partyNameLabel, "犯罪嫌疑人姓名");

  const onlyClient = buildCriminalCaseIdentity({ clientName: "母亲李某" });
  assert.equal(onlyClient.clientName, "母亲李某");
  assert.equal(onlyClient.suspectOrDefendantName, null);
  assert.equal(onlyClient.prosecutionAuthority, null);

  const onlyAuthority = buildCriminalCaseIdentity({ prosecutionAuthority: "某检察院" });
  assert.equal(onlyAuthority.prosecutionAuthority, "某检察院");
  assert.equal(onlyAuthority.suspectOrDefendantName, null);
  assert.equal(onlyAuthority.clientName, null);
});

test("构建身份对象不修改输入，unknown 不生成阶段日期或专属字段", () => {
  const input = Object.freeze({
    currentStage: "待确认",
    storedName: "原文件夹名",
    filed_at: "2026-01-01",
  });
  const before = structuredClone(input);
  const identity = buildCriminalCaseIdentity(input);
  assert.deepEqual(input, before);
  assert.equal(identity.stage, "unknown");
  assert.equal(identity.partyTerm, "犯罪嫌疑人/被告人");
  assert.equal(identity.stageDate, null);
  assert.equal(identity.pureCharge, null);
  assert.equal(identity.prosecutionAuthority, null);
  assert.equal(identity.suspectOrDefendantName, null);
  assert.equal(identity.clientName, null);
});
