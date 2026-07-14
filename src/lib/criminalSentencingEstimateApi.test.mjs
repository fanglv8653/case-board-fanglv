import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const apiSource = readFileSync(new URL("./api.ts", import.meta.url), "utf8");
const typeSource = readFileSync(new URL("./types.ts", import.meta.url), "utf8");

test("sentencing estimate API exposes explicit save and case-scoped reads", () => {
  for (const command of [
    "save_criminal_sentencing_estimate",
    "list_criminal_sentencing_estimates",
    "get_criminal_sentencing_estimate",
  ]) {
    assert.match(apiSource, new RegExp(`invoke<[^;]+>\\(\\s*"${command}"`, "s"));
  }
  assert.match(apiSource, /\{ caseId, estimateId \}/);
});

test("save input carries revision and complete immutable snapshots", () => {
  const input = typeSource.match(
    /export interface CriminalSentencingEstimateSaveInput \{(?<body>[\s\S]*?)\n\}/,
  )?.groups?.body;
  assert.ok(input);
  for (const field of [
    "case_id",
    "expected_profile_revision",
    "input_snapshot",
    "output_min_months",
    "output_max_months",
    "output_snapshot",
    "process_snapshot",
    "basis_snapshot",
    "created_source",
  ]) {
    assert.match(input, new RegExp(`\\b${field}:`));
  }
});
