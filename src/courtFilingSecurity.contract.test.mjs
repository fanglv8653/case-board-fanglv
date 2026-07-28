import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

test("court filing CLI exposes stdin credentials only", () => {
  const cli = read("standalone/court_filing_cli/cli.py");
  assert.ok(cli.includes("--credentials-stdin"));
  assert.equal(cli.includes('add_argument("--account"'), false);
  assert.equal(cli.includes('add_argument("--password"'), false);
  assert.equal(cli.includes('add_argument("--cookie-dir"'), false);
});

test("court filing cookie persistence is encrypted-envelope-only and fail closed", () => {
  const cookies = read("standalone/court_filing_cli/cookie_service.py");
  assert.ok(cookies.includes("CookiePersistenceDisabled"));
  assert.ok(cookies.includes("self.codec is None"));
  assert.ok(cookies.includes("拒绝加载明文 Cookie"));
  assert.equal(cookies.includes("write_text(json.dumps"), false);
});

test("court filing UI never hydrates or saves plaintext credentials", () => {
  const tool = read("src/modules/tools/CourtFilingTool.tsx");
  assert.equal(tool.includes("setAccount(s.court_filing_account"), false);
  assert.equal(tool.includes("setPassword(s.court_filing_password"), false);
  assert.ok(tool.includes('setCredential("court-filing/zxfw/account"'));
  assert.ok(tool.includes('setCredential("court-filing/zxfw/password"'));
  assert.ok(tool.includes("court_filing_account: null"));
  assert.ok(tool.includes("court_filing_password: null"));
});

test("court filing job persistence hardcodes account to NULL", () => {
  const database = read("src-tauri/src/db/court_filing.rs");
  const migration = read(
    "src-tauri/migrations/0054_scrub_court_filing_credentials.sql",
  );
  assert.ok(database.includes(".bind(Option::<String>::None)"));
  assert.equal(database.includes(".bind(&j.cookie_account)"), false);
  assert.match(
    migration,
    /UPDATE\s+court_filing_jobs[\s\S]+SET\s+cookie_account\s*=\s*NULL/i,
  );
});

test("court filing backend resolves vault credentials and spawns with stdin only", () => {
  const backend = read("src-tauri/src/lib.rs");
  const start = backend.slice(
    backend.indexOf("async fn start_court_filing"),
    backend.indexOf("async fn submit_captcha_answer"),
  );
  assert.ok(start.includes("StaticCredential::CourtFilingAccount"));
  assert.ok(start.includes("StaticCredential::CourtFilingPassword"));
  assert.ok(start.includes("spawn_with_stdin_credentials"));
  assert.ok(start.includes("redactor.redact(&line)"));
  assert.equal(start.includes(".court_filing_account"), false);
  assert.equal(start.includes(".court_filing_password"), false);
  assert.equal(start.includes("--account"), false);
  assert.equal(start.includes("--password"), false);
  assert.equal(start.includes("--cookie-dir"), false);
});

test("legacy court settings migrate atomically and are removed from JSON", () => {
  const settings = read("src-tauri/src/settings.rs");
  assert.ok(settings.includes("CourtFilingAccount => settings.court_filing_account"));
  assert.ok(settings.includes("CourtFilingPassword => settings.court_filing_password"));
  assert.ok(settings.includes("replace_verified_with(backend, locator, value)"));
  assert.ok(settings.includes("atomic_write_settings(path, &sanitized, original)"));
  assert.ok(settings.includes('"court_filing_account"'));
  assert.ok(settings.includes('"court_filing_password"'));
  assert.ok(settings.includes('"court_filing_cookie_dir"'));
});
