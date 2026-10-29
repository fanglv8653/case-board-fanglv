import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const source = new URL("../src/modules/tools/sentencing/", import.meta.url);
const files = ["engine.ts", "firstBatch.ts", "firstBatchCalculation.ts", "prefill.ts", "FirstBatchFields.tsx", "SentencingCalculator.tsx", "historyTime.ts"];
const payload = files.map(file => file + "\n" + readFileSync(new URL(file, source), "utf8").replace(/\r\n/g, "\n")).join("\n");
const implementationHash = "sha256:" + createHash("sha256").update(payload).digest("hex");
const dataPath = fileURLToPath(new URL("data.ts", source));
const original = readFileSync(dataPath, "utf8");
if ((original.match(/implementationHash: "/g) ?? []).length !== 1 || (original.match(/contentHash: "/g) ?? []).length !== 1) throw new Error("Expected exactly one implementation/content hash");
const check = process.argv.includes("--check");
if (!check) writeFileSync(dataPath, original.replace(/implementationHash: "[^"]+"/, `implementationHash: "${implementationHash}"`));
const { SENTENCING_DATA, sentencingRulesetHashPayload } = await import(new URL("data.ts?hash-check", source).href);
const contentHash = "sha256:" + createHash("sha256").update(sentencingRulesetHashPayload()).digest("hex");
if (check) {
 if (SENTENCING_DATA.ruleset.implementationHash !== implementationHash || SENTENCING_DATA.ruleset.contentHash !== contentHash) throw new Error("Sentencing source/content fingerprint mismatch. Review changes, then update explicitly.");
} else {
 const updated = readFileSync(dataPath, "utf8").replace(/contentHash: "[^"]+"/, `contentHash: "${contentHash}"`);
 writeFileSync(dataPath, updated);
}
console.log(JSON.stringify({ check, implementationHash, contentHash, files }));
