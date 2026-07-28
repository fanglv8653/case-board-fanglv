import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  CRIMINAL_STAGE_ORDER,
  classifyCriminalStageFromPath,
  groupCriminalDocuments,
} from "./criminalStage.ts";

const root = "D:\\案件\\示例刑事案件";

function doc(id, relative, stage = null) {
  return {
    id,
    source_path: `${root}\\${relative}`,
    filename: relative.split("\\").at(-1),
    stage,
    is_ai_artifact: false,
  };
}

test("criminal stages have the frozen mutually exclusive display order", () => {
  assert.deepEqual(CRIMINAL_STAGE_ORDER, [
    "委托身份材料",
    "侦查",
    "审查起诉",
    "审判",
    "二审",
  ]);
});

test("full relative directory wins over a conflicting filename", () => {
  assert.equal(
    classifyCriminalStageFromPath(
      `${root}\\02 审 查 起 诉\\逮捕材料.pdf`,
      root,
    ),
    "审查起诉",
  );
  assert.equal(
    classifyCriminalStageFromPath(
      `${root}\\03 审判\\05 第二审\\一审判决书.pdf`,
      root,
    ),
    "二审",
  );
});

test("numbered directories and common synonyms are recognized", () => {
  const samples = [
    ["01-委托 身份 材料\\授权书.pdf", "委托身份材料"],
    ["2_公安侦办\\卷宗.pdf", "侦查"],
    ["03 检察院\\阅卷笔录.pdf", "审查起诉"],
    ["04 第一审\\开庭通知.pdf", "审判"],
    ["05 第二审\\上诉材料.pdf", "二审"],
  ];
  for (const [relative, expected] of samples) {
    assert.equal(
      classifyCriminalStageFromPath(`${root}\\${relative}`, root),
      expected,
      relative,
    );
  }
});

test("filename is fallback only and unknown files remain unclassified", () => {
  assert.equal(
    classifyCriminalStageFromPath(`${root}\\未分类\\取保候审决定书.pdf`, root),
    "侦查",
  );
  assert.equal(
    classifyCriminalStageFromPath(`${root}\\未分类\\量刑建议书.pdf`, root),
    "审查起诉",
  );
  assert.equal(
    classifyCriminalStageFromPath(`${root}\\未分类\\普通说明.pdf`, root),
    null,
  );
});

test("case root and unrelated absolute parent folders never contaminate stage", () => {
  assert.equal(
    classifyCriminalStageFromPath(
      "D:\\审判案件\\当前案件\\未分类\\普通说明.pdf",
      "D:\\审判案件\\当前案件",
    ),
    null,
  );
  assert.equal(
    classifyCriminalStageFromPath(
      "E:\\公安共享资料\\未分类\\普通说明.pdf",
      root,
    ),
    null,
  );
});

test("groups are mutually exclusive and unknown documents count only as unknown", () => {
  const docs = [
    doc("identity", "01 委托\\身份证.pdf"),
    doc("investigation", "02 侦查\\起诉书.pdf"),
    doc("prosecution", "03 审查起诉\\逮捕材料.pdf"),
    doc("trial", "04 审判\\判决书.pdf"),
    doc("appeal", "05 二审\\一审判决书.pdf"),
    doc("unknown", "其他\\普通说明.pdf"),
    { ...doc("artifact", "其他\\案件总览.md"), is_ai_artifact: true },
  ];
  const groups = groupCriminalDocuments(docs, root);

  assert.equal(groups.委托身份材料.length, 1);
  assert.equal(groups.侦查.length, 1);
  assert.equal(groups.审查起诉.length, 1);
  assert.equal(groups.审判.length, 1);
  assert.equal(groups.二审.length, 1);
  assert.deepEqual(groups.未分类.map((item) => item.id), ["unknown"]);

  const classifiedIds = CRIMINAL_STAGE_ORDER.flatMap((stage) =>
    groups[stage].map((item) => item.id),
  );
  assert.equal(new Set(classifiedIds).size, classifiedIds.length);
  assert.equal(classifiedIds.includes("unknown"), false);
  assert.equal(classifiedIds.includes("artifact"), false);
});

test("civil stage grouping remains a separate STAGE_ORDER implementation", () => {
  const source = readFileSync(new URL("./groupByStage.ts", import.meta.url), "utf8");
  assert.match(source, /import \{ type Document, STAGE_ORDER \}/);
  assert.match(source, /export function groupByStage\(docs: Document\[\]\)/);
  assert.doesNotMatch(source, /CRIMINAL_STAGE_ORDER/);
});
