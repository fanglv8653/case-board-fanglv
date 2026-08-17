import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  const key = process.argv[index];
  const value = process.argv[index + 1];
  if (!key?.startsWith("--") || value === undefined) {
    throw new Error(`参数必须使用 --name value：${key ?? "<missing>"}`);
  }
  args.set(key.slice(2), value);
}
const mode = args.get("mode") ?? "source";
if (!new Set(["source", "release"]).has(mode)) {
  throw new Error("--mode 只能是 source 或 release");
}

function read(path) {
  return readFileSync(join(root, path), "utf8");
}
function json(path) {
  return JSON.parse(read(path));
}
function requiredMatch(value, pattern, label) {
  const match = value.match(pattern);
  if (!match) throw new Error(`${label} 缺失或格式无效`);
  return match[1];
}
function same(actual, expected, label) {
  if (actual !== expected) throw new Error(`${label}: ${actual} !== ${expected}`);
}

const semver = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
const packageVersion = json("package.json").version;
if (!semver.test(packageVersion)) throw new Error(`package.json 不是严格 SemVer：${packageVersion}`);
const cargoToml = read("src-tauri/Cargo.toml");
const cargoVersion = requiredMatch(
  cargoToml,
  /^version\s*=\s*"([^"]+)"/m,
  "src-tauri/Cargo.toml package version",
);
const tauriConfig = json("src-tauri/tauri.conf.json");
same(cargoVersion, packageVersion, "Cargo/package 版本漂移");
same(tauriConfig.version, packageVersion, "Tauri/package 版本漂移");
if (JSON.stringify(tauriConfig.bundle.externalBin) !== JSON.stringify(["binaries/caseboard-updater-helper"])) {
  throw new Error("Tauri bundle 必须携带独立 caseboard updater helper");
}
if (!cargoToml.includes('name = "caseboard-updater-helper"') || !existsSync(join(root, "src-tauri/src/bin/caseboard-updater-helper.rs"))) {
  throw new Error("独立 caseboard updater helper 源码或 Cargo binary 注册缺失");
}

if (existsSync(join(root, "src-tauri/Cargo.lock"))) {
  throw new Error("检测到陈旧的 src-tauri/Cargo.lock；workspace 只能保留根 Cargo.lock");
}
const rootLock = read("Cargo.lock");
const lockVersion = requiredMatch(
  rootLock,
  /\[\[package\]\]\s*\nname = "caseboard"\s*\nversion = "([^"]+)"/,
  "根 Cargo.lock caseboard version",
);
same(lockVersion, packageVersion, "Cargo.lock/source 版本漂移");

const metadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  }),
);
same(resolve(metadata.workspace_root), root, "Cargo workspace_root 异常");
same(resolve(metadata.target_directory), join(root, "target"), "Cargo target_directory 异常");
const caseboardPackages = metadata.packages.filter((item) => item.name === "caseboard");
if (caseboardPackages.length !== 1) throw new Error("cargo metadata 必须且只能解析一个 caseboard package");
same(caseboardPackages[0].version, packageVersion, "cargo metadata/source 版本漂移");

const license = read("LICENSE").replace(/\r\n/g, "\n");
const heading = "# PolyForm Noncommercial License 1.0.0";
const boundary = license.indexOf(heading);
if (boundary < 0) throw new Error("LICENSE 缺少 PolyForm 英文正文边界");
const englishBody = license.slice(boundary);
const englishHash = createHash("sha256").update(englishBody, "utf8").digest("hex");
same(
  englishHash,
  "c0ea4a896d2c8c394b29f9427589996db826cd501c512279ff0ed3ef48fabbe5",
  "LICENSE 英文正文被修改",
);
const notice = read("NOTICE").replace(/\r\n/g, "\n");
const licenseNotice = requiredMatch(license, /^(Required Notice: .+)$/m, "LICENSE Required Notice");
const noticeNotice = requiredMatch(notice, /^(Required Notice: .+)$/m, "NOTICE Required Notice");
same(noticeNotice, licenseNotice, "LICENSE/NOTICE Required Notice 漂移");
if (!notice.includes("https://github.com/leo123-tto/case-board")) throw new Error("NOTICE 缺少上游项目归属");
if (!notice.includes("https://github.com/Hmbown/CodeWhale") || !notice.includes("MIT License")) {
  throw new Error("NOTICE 缺少 CodeWhale MIT 通知");
}
const copyright = licenseNotice.replace(/^Required Notice: /, "").replace(/ \(https:\/\/lawtools\.top\)$/, "");
same(tauriConfig.bundle.copyright, copyright, "Tauri 安装包版权字段漂移");

const changelog = read("CHANGELOG.md");
if (!new RegExp(`^## \\[${packageVersion.replaceAll(".", "\\.")}\\]`, "m").test(changelog)) {
  throw new Error(`CHANGELOG 缺少 ${packageVersion} 标题`);
}
if (/CaseBoard-LPR-Reference\/\d+\.\d+\.\d+/.test(read("src-tauri/src/lpr.rs"))) {
  throw new Error("LPR User-Agent 仍硬编码版本号");
}

const published = json("release/latest.json");
if (!semver.test(published.version)) throw new Error("release/latest.json published version 无效");
console.log(`source gate OK: source=${packageVersion}, published=${published.version}, license=${englishHash}`);

if (mode === "release") {
  const tag = args.get("tag");
  const artifactDirArg = args.get("artifact-dir");
  const baseUrl = args.get("base-url");
  const draftOutputArg = args.get("draft-output");
  if (!tag || !artifactDirArg || !baseUrl || !draftOutputArg) {
    throw new Error("release 模式必须提供 --tag、--artifact-dir、--base-url、--draft-output");
  }
  same(tag, `v${packageVersion}-fanglv`, "发布 tag 与源码版本不一致");
  const artifactDir = resolve(root, artifactDirArg);
  if (!existsSync(artifactDir)) throw new Error(`产物目录不存在：${artifactDir}`);
  const names = readdirSync(artifactDir).sort();
  const installer = `FanglvCaseBoard_${packageVersion}_x64-setup.exe`;
  const signatureName = `${installer}.sig`;
  const expectedNames = [installer, signatureName].sort();
  if (names.length !== 2 || names.some((name, index) => name !== expectedNames[index])) {
    throw new Error(`REL_ASSET_NAME_INVALID：产物必须精确为 ${expectedNames.join(", ")}`);
  }
  if (!/^\d+\.\d+\.\d+$/.test(packageVersion)) {
    throw new Error(`release 模式只允许稳定版本号：${packageVersion}`);
  }
  if (names.some((name) => [...name].some((character) => character.charCodeAt(0) < 0x20 || character.charCodeAt(0) > 0x7e))) {
    throw new Error("REL_ASSET_NAME_INVALID：资产名必须全部为 ASCII");
  }
  const signature = readFileSync(join(artifactDir, signatureName), "utf8").trim();
  const decodedSignature = signature.includes("untrusted comment:")
    ? signature
    : Buffer.from(signature, "base64").toString("utf8");
  if (!decodedSignature.includes("untrusted comment:") || !decodedSignature.includes("trusted comment:")) {
    throw new Error("updater .sig 不是 Tauri/minisign 签名");
  }
  const url = `${baseUrl.replace(/\/$/, "")}/${tag}/${encodeURIComponent(installer)}`;
  const draft = {
    version: packageVersion,
    notes: `方律案件看板 ${packageVersion}。发布前仍须验证最终安装包 updater minisign；该签名不是 Windows Authenticode。`,
    pub_date: new Date().toISOString(),
    platforms: { "windows-x86_64": { signature, url } },
  };
  const draftOutput = resolve(root, draftOutputArg);
  writeFileSync(draftOutput, `${JSON.stringify(draft, null, 2)}\n`, "utf8");
  console.log(`release draft OK: ${relative(root, draftOutput)} <- ${basename(installer)}`);
}
