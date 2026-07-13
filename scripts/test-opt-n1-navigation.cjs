const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "..");
const read = (relativePath) =>
  fs.readFileSync(path.join(root, relativePath), "utf8");

const app = read("src/App.tsx");
const homeView = read("src/components/HomeView.tsx");
const moduleTabs = read("src/components/ModuleTabs.tsx");

assert.match(homeView, /caseboard:case-list-preferences:v1/);
assert.match(homeView, /return `\$\{CASE_LIST_PREFERENCES_PREFIX\}:\$\{mode\}`/);

for (const mode of ["workspace", "civil", "criminal"]) {
  assert.match(
    app,
    new RegExp(`mode="${mode}"`),
    `App should render HomeView with mode="${mode}"`,
  );
}

assert.match(moduleTabs, /id: "litigation", label: "民事"/);
assert.match(moduleTabs, /setUnderline\(\{ left: 0, width: 0 \}\)/);

assert.match(app, /useState<string>\("workspace"\)/);
assert.match(app, /openCaseFromHome/);
assert.match(app, /previousHomeMode === "workspace"/);
assert.match(app, /targetCase && isCriminalCase\(targetCase\) \? "criminal" : "litigation"/);
assert.doesNotMatch(app, /allCases=\{cases\}/);

assert.match(homeView, /export type HomeViewMode = "workspace" \| "civil" \| "criminal"/);
assert.match(homeView, /readCaseListPreferences/);
assert.match(homeView, /writeCaseListPreferences/);
assert.match(homeView, /catch \{\s*return DEFAULT_CASE_LIST_PREFERENCES;\s*\}/);
assert.match(homeView, /row\.status\.id !== "execution"/);
assert.match(homeView, /!isCriminalCase\(row\.caseData\)/);

console.log("OPT-N1 static navigation checks passed");
