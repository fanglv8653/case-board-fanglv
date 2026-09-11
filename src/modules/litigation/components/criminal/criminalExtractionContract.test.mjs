import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const backend = readFileSync(
  new URL("../../../../../src-tauri/src/db/criminal_extraction_candidates.rs", import.meta.url),
  "utf8",
);
const schema = readFileSync(new URL("../../../../../src-tauri/src/llm/mod.rs", import.meta.url), "utf8");
const prompt = readFileSync(
  new URL("../../../../../src-tauri/src/llm/prompts.rs", import.meta.url),
  "utf8",
);
const labels = readFileSync(new URL("./criminalExtractionReviewModels.ts", import.meta.url), "utf8");

const STRING_FIELDS = [
  "current_stage",
  "procedure_type",
  "suspected_charge",
  "suspect_or_defendant_name",
  "victim_name",
  "detention_center",
  "coercive_measure_type",
  "guilty_plea_status",
  "sentencing_recommendation",
  "sentence_term",
  "restitution_status",
  "victim_forgiveness",
  "surrender_status",
  "meritorious_service_status",
];
const DATE_FIELDS = [
  "detention_date",
  "arrest_request_date",
  "arrest_review_received_date",
  "arrest_decision_date",
  "arrest_date",
  "bail_start_date",
  "residential_surveillance_start_date",
  "transfer_for_prosecution_date",
  "prosecution_received_date",
  "first_instance_accepted_date",
  "second_instance_accepted_date",
  "judgment_received_date",
  "ruling_received_date",
  "supplementary_investigation_1_date",
  "supplementary_investigation_2_date",
  "judgment_effective_date",
  "death_penalty_review_start_date",
];

test("every recognized criminal profile field is validated, applied, and labeled", () => {
  for (const key of [
    ...STRING_FIELDS,
    ...DATE_FIELDS,
    "charge_history_json",
    "co_defendants_json",
  ]) {
    assert.match(backend, new RegExp(`"${key}"`), `${key} missing in backend contract`);
    assert.match(backend, new RegExp(`"${key}" => update!\\("${key}"\\)`), `${key} is not applied to its profile column`);
    assert.match(labels, new RegExp(`\\b${key}:\\s*"`), `${key} has no user-facing label`);
  }
  assert.match(backend, /SET restitution_amount=\?/);
  assert.match(labels, /\brestitution_amount:\s*"/);
});

test("co-defendants stay connected across prompt, schema, candidate persistence, and UI", () => {
  assert.match(prompt, /criminal\.co_defendants/);
  assert.match(schema, /pub co_defendants:\s*Vec<CriminalCoDefendantExtraction>/);
  assert.match(backend, /key:\s*"co_defendants_json"\.into\(\)/);
  assert.match(backend, /"co_defendants_json"\s*=>\s*update!\("co_defendants_json"\)/);
  assert.match(labels, /\bco_defendants_json:\s*"同案人员"/);
});

test("document type is diagnostic metadata and never mutates the profile", () => {
  assert.match(backend, /let document_type\s*=\s*criminal\.document_type\.value\.clone\(\)/);
  assert.doesNotMatch(backend, /"document_type"\s*=>\s*update!/);
});
