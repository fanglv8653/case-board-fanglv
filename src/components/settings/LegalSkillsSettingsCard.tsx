import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Archive,
  BookOpenCheck,
  CheckCircle2,
  Download,
  FileArchive,
  History,
  RotateCcw,
  Trash2,
  FileUp,
  Loader2,
  ShieldCheck,
} from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";

import {
  bindDefaultLegalSkill,
  importLegalSkillPackage,
  importLegalSkillArchive,
  listLegalSkillPackages,
  listLegalSkillVersions,
  previewLegalSkillDiff,
  upgradeLegalSkillPackage,
  rollbackLegalSkillPackage,
  exportLegalSkillPackage,
  deleteLegalSkillPackage,
  setLegalSkillPackageEnabled,
} from "@/lib/api";
import { confirmDialog } from "@/lib/dialog";
import type {
  LegalSkillDiffPreview,
  LegalSkillVersionHistory,
} from "@/lib/types";

interface LegalSkillManifest {
  slug: string;
  title: string;
  version: string;
  description?: string;
  legal_domains: string[];
  task_types: string[];
  requested_tools?: string[];
}

interface LegalSkillPackageRecord {
  id: string;
  slug: string;
  title: string;
  version: string;
  description: string;
  origin: "builtin" | "imported" | string;
  status: "enabled" | "disabled" | "quarantined" | string;
  manifest_json: string;
  package_content_json: string;
  content_hash: string;
  updated_at: string;
}

interface LegalSkillFile {
  relative_path: string;
  content: string;
}

const DOMAIN_LABELS: Record<string, string> = {
  criminal: "刑事",
  civil: "民事",
  enforcement: "执行",
  non_litigation: "合同与非诉",
  legal_research: "法律检索",
};

const TASK_LABELS: Record<string, string> = {
  free_chat: "自由问答",
  compile_legal_basis: "整理法律依据",
  find_similar_cases: "检索类案",
  verify_my_draft: "复核文稿",
  simulate_opposition: "模拟对方观点",
  deep_analysis: "深度分析",
  criminal_deep_analysis: "刑事深度分析",
};

function parseManifest(record: LegalSkillPackageRecord): LegalSkillManifest | null {
  try {
    return JSON.parse(record.manifest_json) as LegalSkillManifest;
  } catch {
    return null;
  }
}

function methodBody(record: LegalSkillPackageRecord): string {
  try {
    const files = JSON.parse(record.package_content_json) as Record<string, string>;
    return files["SKILL.md"] ?? "";
  } catch {
    return "";
  }
}

function normalizeSelectedPath(file: File): string | null {
  const raw = (file.webkitRelativePath || file.name).replace(/\\/g, "/");
  const segments = raw.split("/").filter(Boolean);
  const skillIndex = segments.lastIndexOf("SKILL.md");
  const manifestIndex = segments.lastIndexOf("manifest.json");
  const rootMarker = Math.max(skillIndex, manifestIndex);
  if (rootMarker >= 0) {
    segments.splice(0, rootMarker);
  } else if (segments.length > 1) {
    segments.shift();
  }
  const path = segments.join("/");
  if (path === "SKILL.md" || path === "manifest.json") return path;
  if (/^references\/[^/]+\.(?:json|md|txt)$/i.test(path)) return path;
  return null;
}

async function readImportFiles(fileList: FileList): Promise<LegalSkillFile[]> {
  const accepted: LegalSkillFile[] = [];
  for (const file of Array.from(fileList)) {
    const relativePath = normalizeSelectedPath(file);
    if (!relativePath) continue;
    accepted.push({
      relative_path: relativePath,
      content: await file.text(),
    });
  }
  return accepted;
}

interface ArchivePreview {
  fileName: string;
  bytes: number[];
  manifest: LegalSkillManifest;
  body: string;
  paths: string[];
  compressedBytes: number;
  expandedBytes: number;
}

interface CentralZipEntry {
  name: string;
  flags: number;
  method: number;
  compressedSize: number;
  expandedSize: number;
  externalAttributes: number;
  localOffset: number;
  directory: boolean;
}

function readCentralDirectory(bytes: Uint8Array): CentralZipEntry[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let eocd = -1;
  for (let offset = bytes.length - 22; offset >= Math.max(0, bytes.length - 65_557); offset -= 1) {
    if (view.getUint32(offset, true) === 0x06054b50) {
      eocd = offset;
      break;
    }
  }
  if (eocd < 0) throw new Error("无法读取 ZIP 中央目录。");
  const entryCount = view.getUint16(eocd + 10, true);
  const centralOffset = view.getUint32(eocd + 16, true);
  if (entryCount > 23) throw new Error("ZIP 条目不能超过 23 个。");
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const entries: CentralZipEntry[] = [];
  let offset = centralOffset;
  for (let index = 0; index < entryCount; index += 1) {
    if (view.getUint32(offset, true) !== 0x02014b50) throw new Error("ZIP 中央目录损坏。");
    const flags = view.getUint16(offset + 8, true);
    const method = view.getUint16(offset + 10, true);
    const compressedSize = view.getUint32(offset + 20, true);
    const expandedSize = view.getUint32(offset + 24, true);
    const nameLength = view.getUint16(offset + 28, true);
    const extraLength = view.getUint16(offset + 30, true);
    const commentLength = view.getUint16(offset + 32, true);
    const externalAttributes = view.getUint32(offset + 38, true);
    const localOffset = view.getUint32(offset + 42, true);
    const name = decoder.decode(bytes.slice(offset + 46, offset + 46 + nameLength)).replace(/\\/g, "/");
    const unixMode = externalAttributes >>> 16;
    if ((unixMode & 0o170000) === 0o120000) throw new Error(`拒绝符号链接：${name}`);
    entries.push({
      name,
      flags,
      method,
      compressedSize,
      expandedSize,
      externalAttributes,
      localOffset,
      directory: name.endsWith("/"),
    });
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return entries;
}

async function readZipEntry(bytes: Uint8Array, entry: CentralZipEntry): Promise<Uint8Array> {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(entry.localOffset, true) !== 0x04034b50) {
    throw new Error(`ZIP 本地条目损坏：${entry.name}`);
  }
  if ((entry.flags & 1) !== 0) throw new Error(`不接受加密 ZIP 条目：${entry.name}`);
  const nameLength = view.getUint16(entry.localOffset + 26, true);
  const extraLength = view.getUint16(entry.localOffset + 28, true);
  const start = entry.localOffset + 30 + nameLength + extraLength;
  const compressed = bytes.slice(start, start + entry.compressedSize);
  if (entry.method === 0) return compressed;
  if (entry.method !== 8) throw new Error(`不支持的 ZIP 压缩方法：${entry.name}`);
  const stream = new Blob([compressed]).stream().pipeThrough(
    new DecompressionStream("deflate-raw" as never),
  );
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function preflightArchive(file: File): Promise<ArchivePreview> {
  if (!file.name.toLowerCase().endsWith(".fanglv-skill.zip")) {
    throw new Error("压缩方法包必须使用 .fanglv-skill.zip 扩展名。");
  }
  if (file.size < 1 || file.size > 1024 * 1024) {
    throw new Error("压缩包大小必须在 1 B 至 1 MB 之间。");
  }
  const buffer = await file.arrayBuffer();
  const archiveBytes = new Uint8Array(buffer);
  const entries = readCentralDirectory(archiveBytes);
  const paths: string[] = [];
  let expandedBytes = 0;
  let manifestText = "";
  let body = "";
  for (const entry of entries) {
    const name = entry.name;
    if (entry.directory) {
      if (name !== "references/") throw new Error(`不允许的目录项：${name}`);
      continue;
    }
    if (
      name.includes("../") ||
      name.startsWith("/") ||
      /^[A-Za-z]:/.test(name) ||
      (!["SKILL.md", "manifest.json"].includes(name) &&
        !/^references\/[^/]+\.(?:md|json|txt)$/i.test(name))
    ) {
      throw new Error(`不安全或不受支持的 ZIP 路径：${name}`);
    }
    expandedBytes += entry.expandedSize;
    if (expandedBytes > 512 * 1024) throw new Error("解包后总大小不能超过 512 KB。");
    const bytes = await readZipEntry(archiveBytes, entry);
    if (bytes.byteLength !== entry.expandedSize) throw new Error(`ZIP 条目长度不一致：${name}`);
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    paths.push(name);
    if (name === "manifest.json") manifestText = text;
    if (name === "SKILL.md") body = text;
  }
  if (!manifestText || !body.trim()) {
    throw new Error("压缩包根目录必须同时包含 manifest.json 和 SKILL.md。");
  }
  const manifest = JSON.parse(manifestText) as LegalSkillManifest;
  if (!manifest.slug || !manifest.title || !manifest.version) {
    throw new Error("manifest.json 缺少 slug、title 或 version。");
  }
  return {
    fileName: file.name,
    bytes: Array.from(archiveBytes),
    manifest,
    body,
    paths,
    compressedBytes: file.size,
    expandedBytes,
  };
}

function compareVersions(left: string, right: string): number {
  const a = left.split(/[.-]/).map((part) => Number.parseInt(part, 10) || 0);
  const b = right.split(/[.-]/).map((part) => Number.parseInt(part, 10) || 0);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    if ((a[index] ?? 0) !== (b[index] ?? 0)) {
      return (a[index] ?? 0) - (b[index] ?? 0);
    }
  }
  return 0;
}

export function LegalSkillsSettingsCard() {
  const directoryInputRef = useRef<HTMLInputElement | null>(null);
  const archiveInputRef = useRef<HTMLInputElement | null>(null);
  const [packages, setPackages] = useState<LegalSkillPackageRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [binding, setBinding] = useState<Record<string, { domain: string; task: string }>>({});
  const [pendingImport, setPendingImport] = useState<{
    files: LegalSkillFile[];
    manifest: LegalSkillManifest;
    body: string;
  } | null>(null);
  const [pendingArchive, setPendingArchive] = useState<ArchivePreview | null>(null);
  const [historyFor, setHistoryFor] = useState<LegalSkillPackageRecord | null>(null);
  const [history, setHistory] = useState<LegalSkillVersionHistory | null>(null);
  const [targetSkillId, setTargetSkillId] = useState("");
  const [diffPreview, setDiffPreview] = useState<LegalSkillDiffPreview | null>(null);

  async function reload() {
    const next = (await listLegalSkillPackages()) as LegalSkillPackageRecord[];
    setPackages(next);
  }

  useEffect(() => {
    let active = true;
    void listLegalSkillPackages()
      .then((next) => {
        if (active) setPackages(next as LegalSkillPackageRecord[]);
      })
      .catch((reason) => {
        if (active) setError(String(reason));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const packageManifests = useMemo(
    () => new Map(packages.map((record) => [record.id, parseManifest(record)])),
    [packages],
  );

  async function togglePackage(record: LegalSkillPackageRecord) {
    if (record.status === "quarantined") return;
    setBusyId(record.id);
    setError(null);
    setNotice(null);
    try {
      await setLegalSkillPackageEnabled(record.id, record.status !== "enabled");
      await reload();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  }

  async function bindPackage(record: LegalSkillPackageRecord) {
    const manifest = packageManifests.get(record.id);
    const selected = binding[record.id] ?? {
      domain: manifest?.legal_domains[0] ?? "",
      task: manifest?.task_types[0] ?? "",
    };
    if (!selected.domain || !selected.task) return;
    setBusyId(record.id);
    setError(null);
    setNotice(null);
    try {
      await bindDefaultLegalSkill(record.id, selected.domain, selected.task);
      setNotice(
        `已将“${record.title}”设为${DOMAIN_LABELS[selected.domain] ?? selected.domain} / ${
          TASK_LABELS[selected.task] ?? selected.task
        }的默认方法包。`,
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  }

  async function importFiles(fileList: FileList | null) {
    if (!fileList?.length) return;
    setImporting(true);
    setError(null);
    setNotice(null);
    try {
      const files = await readImportFiles(fileList);
      const paths = new Set(files.map((file) => file.relative_path));
      if (!paths.has("SKILL.md") || !paths.has("manifest.json")) {
        throw new Error("所选内容必须同时包含根目录 SKILL.md 与 manifest.json。");
      }
      const manifestFile = files.find((file) => file.relative_path === "manifest.json");
      const body = files.find((file) => file.relative_path === "SKILL.md")?.content ?? "";
      const manifest = JSON.parse(manifestFile?.content ?? "") as LegalSkillManifest;
      if (!manifest.slug || !manifest.title || !manifest.version || !body.trim()) {
        throw new Error("manifest 或 SKILL.md 缺少必要内容。");
      }
      setPendingImport({ files, manifest, body });
    } catch (reason) {
      setError(String(reason));
    } finally {
      setImporting(false);
      if (directoryInputRef.current) directoryInputRef.current.value = "";
    }
  }

  async function confirmImport() {
    if (!pendingImport) return;
    setImporting(true);
    setError(null);
    try {
      await importLegalSkillPackage(pendingImport.files);
      await reload();
      setNotice(`方法包已导入，共读取 ${pendingImport.files.length} 个纯文本/JSON 文件。`);
      setPendingImport(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setImporting(false);
    }
  }

  async function selectArchive(fileList: FileList | null) {
    const file = fileList?.[0];
    if (!file) return;
    setImporting(true);
    setError(null);
    setNotice(null);
    try {
      setPendingArchive(await preflightArchive(file));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setImporting(false);
      if (archiveInputRef.current) archiveInputRef.current.value = "";
    }
  }

  async function confirmArchiveImport() {
    if (!pendingArchive) return;
    setImporting(true);
    setError(null);
    try {
      const result = await importLegalSkillArchive(
        pendingArchive.fileName,
        pendingArchive.bytes,
      );
      await reload();
      setNotice(
        `${result.package.title} v${result.package.version} 已安全导入为停用版本；如需替换当前版本，请先查看差异。`,
      );
      setPendingArchive(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setImporting(false);
    }
  }

  async function openHistory(record: LegalSkillPackageRecord) {
    setBusyId(record.id);
    setError(null);
    try {
      const next = await listLegalSkillVersions(record.slug);
      const current = next.packages.find(
        (item) => item.origin === "imported" && item.status === "enabled",
      );
      if (!current) {
        throw new Error("版本升级或回滚必须从当前已启用的导入版本开始；请先启用一个版本。");
      }
      setHistoryFor(current);
      setHistory(next);
      setTargetSkillId(
        next.packages.find(
          (item) =>
            item.id !== current.id &&
            !["quarantined", "deleted"].includes(item.status),
        )?.id ?? "",
      );
      setDiffPreview(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  }

  async function previewSelectedVersion() {
    if (!historyFor || !targetSkillId) return;
    setBusyId(historyFor.id);
    setError(null);
    try {
      setDiffPreview(await previewLegalSkillDiff(historyFor.id, targetSkillId));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  }

  async function switchVersion() {
    if (!historyFor || !diffPreview) return;
    const target = history?.packages.find((item) => item.id === diffPreview.to_skill_id);
    if (!target) return;
    const isUpgrade = compareVersions(target.version, historyFor.version) > 0;
    const ok = await confirmDialog(
      `${isUpgrade ? "升级" : "回滚"}会切换当前启用版本，但保留版本历史。已查看 ${diffPreview.files.length} 个文件差异，是否继续？`,
      { danger: !isUpgrade, okLabel: isUpgrade ? "确认升级" : "确认回滚" },
    );
    if (!ok) return;
    setBusyId(historyFor.id);
    setError(null);
    try {
      if (isUpgrade) {
        await upgradeLegalSkillPackage(historyFor.id, target.id);
      } else {
        await rollbackLegalSkillPackage(historyFor.id, target.id);
      }
      await reload();
      setNotice(`已${isUpgrade ? "升级" : "回滚"}到 v${target.version}。`);
      setHistoryFor(null);
      setHistory(null);
      setDiffPreview(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  }

  async function exportPackage(record: LegalSkillPackageRecord) {
    setBusyId(record.id);
    setError(null);
    try {
      const archive = await exportLegalSkillPackage(record.id);
      const destination = await save({
        defaultPath: archive.file_name,
        filters: [{ name: "方律方法包", extensions: ["fanglv-skill.zip"] }],
      });
      if (typeof destination !== "string") return;
      await writeFile(destination, Uint8Array.from(archive.bytes));
      setNotice(`已导出到 ${destination}`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  }

  async function deletePackage(record: LegalSkillPackageRecord) {
    if (record.origin === "builtin") {
      setError("内置方法包不可删除；如暂不使用，请关闭“已启用”开关。");
      return;
    }
    const ok = await confirmDialog(
      `删除“${record.title}”v${record.version}？后端会保留审计快照，但该导入版本将从可用列表移除。`,
      { danger: true, okLabel: "确认删除" },
    );
    if (!ok) return;
    setBusyId(record.id);
    setError(null);
    try {
      await deleteLegalSkillPackage(record.id);
      await reload();
      setNotice("导入方法包已删除。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <section className="rounded-lg border border-border bg-card p-4" aria-labelledby="legal-skills-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 id="legal-skills-title" className="text-sm font-semibold text-foreground">
            全局法律 Skills
          </h3>
          <p className="mt-1 text-xs text-muted-foreground">
            管理方律法律方法包，并按法律领域与任务类型选择默认方法。
          </p>
        </div>
        <div>
          <input
            ref={(element) => {
              directoryInputRef.current = element;
              element?.setAttribute("webkitdirectory", "");
            }}
            type="file"
            multiple
            accept=".md,.json,.txt"
            className="sr-only"
            onChange={(event) => void importFiles(event.currentTarget.files)}
          />
          <input
            ref={archiveInputRef}
            type="file"
            accept=".zip,.fanglv-skill.zip"
            className="sr-only"
            onChange={(event) => void selectArchive(event.currentTarget.files)}
          />
          <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => directoryInputRef.current?.click()}
            disabled={importing}
            className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-muted disabled:opacity-50"
          >
            {importing ? (
              <Loader2 className="size-3.5 animate-spin" aria-hidden="true" />
            ) : (
              <FileUp className="size-3.5" aria-hidden="true" />
            )}
            {importing ? "导入中…" : "选择方法包目录"}
          </button>
          <button
            type="button"
            onClick={() => archiveInputRef.current?.click()}
            disabled={importing}
            className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-muted disabled:opacity-50"
          >
            <FileArchive className="size-3.5" aria-hidden="true" />
            选择 .fanglv-skill.zip
          </button>
          </div>
        </div>
      </div>

      <div className="mt-3 flex items-start gap-2 rounded-md border border-sky-200 bg-sky-50/60 p-3 text-xs text-sky-900">
        <ShieldCheck className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
        <p>
          方法包只提供办案思路、检查清单和表达规则，不授予任何工具权限。即使 manifest
          声明了工具，实际可用工具仍由系统白名单和当前场景独立控制。
        </p>
      </div>

      {pendingImport && (
        <div className="mt-3 rounded-md border border-amber-300 bg-amber-50/60 p-3 text-xs">
          <div className="font-semibold text-foreground">导入前预览（尚未写入）</div>
          <div className="mt-2 grid gap-1 text-muted-foreground md:grid-cols-2">
            <span>名称：{pendingImport.manifest.title}</span>
            <span>版本：{pendingImport.manifest.version}</span>
            <span className="font-mono">slug：{pendingImport.manifest.slug}</span>
            <span>文件：{pendingImport.files.length} 个</span>
            <span>领域：{pendingImport.manifest.legal_domains.join("、")}</span>
            <span>任务：{pendingImport.manifest.task_types.join("、")}</span>
          </div>
          <div className="mt-2">
            <strong>声明请求的工具（不是授权）：</strong>
            {pendingImport.manifest.requested_tools?.join("、") || "无"}
          </div>
          <details className="mt-2 rounded border border-border bg-background/70 p-2">
            <summary className="cursor-pointer font-medium">查看 SKILL.md 正文与文件清单</summary>
            <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap text-xs">
              {pendingImport.body}
            </pre>
            <ul className="mt-2 list-disc pl-5 text-muted-foreground">
              {pendingImport.files.map((file) => (
                <li key={file.relative_path}>{file.relative_path}</li>
              ))}
            </ul>
          </details>
          <p className="mt-2 text-amber-800">
            后端仍会重新校验路径、大小、工具声明与内容哈希；脚本和越界文件将被拒绝。
          </p>
          <div className="mt-3 flex gap-2">
            <button
              type="button"
              onClick={() => void confirmImport()}
              disabled={importing}
              className="rounded-md bg-foreground px-3 py-1.5 text-background disabled:opacity-50"
            >
              {importing ? "导入中…" : "确认导入"}
            </button>
            <button
              type="button"
              onClick={() => setPendingImport(null)}
              disabled={importing}
              className="rounded-md border border-border bg-background px-3 py-1.5"
            >
              取消
            </button>
          </div>
        </div>
      )}

      {pendingArchive && (
        <div className="mt-3 rounded-md border border-amber-300 bg-amber-50/60 p-3 text-xs">
          <div className="flex items-center gap-2 font-semibold text-foreground">
            <Archive className="size-4" />压缩包安全预检（尚未写入）
          </div>
          <div className="mt-2 grid gap-1 text-muted-foreground md:grid-cols-2">
            <span>名称：{pendingArchive.manifest.title}</span>
            <span>版本：{pendingArchive.manifest.version}</span>
            <span className="font-mono">slug：{pendingArchive.manifest.slug}</span>
            <span>文件：{pendingArchive.paths.length} 个</span>
            <span>压缩：{pendingArchive.compressedBytes} B</span>
            <span>解包：{pendingArchive.expandedBytes} B</span>
          </div>
          <p className="mt-2 text-amber-800">
            客户端已检查扩展名、中央目录、条目数量、路径穿越、文件类型和解包大小；后端仍会重新执行 CRC 等完整的失败关闭校验。
          </p>
          <details className="mt-2 rounded border border-border bg-background/70 p-2">
            <summary className="cursor-pointer font-medium">查看 SKILL.md 与文件清单</summary>
            <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap">{pendingArchive.body}</pre>
            <ul className="mt-2 list-disc pl-5 text-muted-foreground">
              {pendingArchive.paths.map((path) => <li key={path}>{path}</li>)}
            </ul>
          </details>
          <div className="mt-3 flex gap-2">
            <button type="button" onClick={() => void confirmArchiveImport()} disabled={importing} className="rounded-md bg-foreground px-3 py-1.5 text-background disabled:opacity-50">
              确认导入为停用版本
            </button>
            <button type="button" onClick={() => setPendingArchive(null)} disabled={importing} className="rounded-md border border-border bg-background px-3 py-1.5">
              取消
            </button>
          </div>
        </div>
      )}

      {historyFor && history && (
        <div className="mt-3 rounded-md border border-border bg-muted/20 p-3 text-xs">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2 font-semibold"><History className="size-4" />{historyFor.title} 版本历史</div>
            <button type="button" onClick={() => { setHistoryFor(null); setHistory(null); setDiffPreview(null); }} className="rounded border border-border bg-background px-2 py-1">关闭</button>
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <select value={targetSkillId} onChange={(event) => { setTargetSkillId(event.target.value); setDiffPreview(null); }} className="min-w-52 rounded-md border border-border bg-background px-2 py-1.5">
              <option value="">选择目标版本</option>
              {history.packages.filter(
                (item) =>
                  item.id !== historyFor.id &&
                  !["quarantined", "deleted"].includes(item.status),
              ).map((item) => (
                <option key={item.id} value={item.id}>v{item.version} · {item.origin} · {item.status}</option>
              ))}
            </select>
            <button type="button" disabled={!targetSkillId || busyId === historyFor.id} onClick={() => void previewSelectedVersion()} className="rounded-md border border-border bg-background px-3 py-1.5 disabled:opacity-50">
              预览差异
            </button>
          </div>
          {history.revisions.length > 0 && (
            <details className="mt-2">
              <summary className="cursor-pointer text-muted-foreground">审计历史 {history.revisions.length} 条</summary>
              <ul className="mt-1 space-y-1 text-muted-foreground">
                {history.revisions.map((revision) => <li key={revision.id}>v{revision.version} · {revision.revision_action} · {revision.created_at}</li>)}
              </ul>
            </details>
          )}
          {diffPreview && (
            <div className="mt-3 rounded-md border border-amber-300 bg-amber-50/60 p-3">
              <div className="font-semibold">v{diffPreview.from_version} → v{diffPreview.to_version}</div>
              <p className="mt-1 text-muted-foreground">文件差异 {diffPreview.files.length} 项；执行前必须在此查看并再次确认。</p>
              <div className="mt-2 max-h-72 space-y-2 overflow-auto">
                {diffPreview.files.map((file) => (
                  <details key={file.path} className="rounded border border-border bg-background p-2">
                    <summary className="cursor-pointer font-mono">{file.change} · {file.path}</summary>
                    <div className="mt-2 grid gap-2 md:grid-cols-2">
                      <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded bg-red-50 p-2">{file.before ?? "（无）"}</pre>
                      <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded bg-emerald-50 p-2">{file.after ?? "（无）"}</pre>
                    </div>
                  </details>
                ))}
              </div>
              <button type="button" onClick={() => void switchVersion()} disabled={busyId === historyFor.id} className="mt-3 inline-flex items-center gap-1 rounded-md bg-foreground px-3 py-1.5 text-background disabled:opacity-50">
                <RotateCcw className="size-3.5" />
                {compareVersions(diffPreview.to_version, diffPreview.from_version) > 0 ? "确认升级" : "确认回滚"}
              </button>
            </div>
          )}
        </div>
      )}

      {loading ? (
        <p className="mt-4 text-xs text-muted-foreground">正在读取法律方法包…</p>
      ) : packages.length === 0 ? (
        <p className="mt-4 text-xs text-muted-foreground">暂无已注册的方法包。</p>
      ) : (
        <div className="mt-4 space-y-3">
          {packages.map((record) => {
            const manifest = packageManifests.get(record.id);
            const selected = binding[record.id] ?? {
              domain: manifest?.legal_domains[0] ?? "",
              task: manifest?.task_types[0] ?? "",
            };
            const allowedTasks =
              selected.domain && manifest?.legal_domains.includes(selected.domain)
                ? manifest.task_types
                : [];
            return (
              <article key={record.id} className="rounded-md border border-border bg-background/60 p-3">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <BookOpenCheck className="size-4 text-emerald-700" aria-hidden="true" />
                      <h4 className="text-sm font-medium text-foreground">{record.title}</h4>
                      <span className="rounded-full bg-muted px-2 py-0.5 text-label text-muted-foreground">
                        {record.origin === "builtin" ? "内置" : "导入"} · v{record.version}
                      </span>
                      {record.status === "quarantined" && (
                        <span className="rounded-full bg-red-50 px-2 py-0.5 text-label text-red-700">
                          已隔离
                        </span>
                      )}
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {record.description || record.slug}
                    </p>
                    <p className="mt-1 font-mono text-label text-muted-foreground">
                      {record.slug} · {record.content_hash.slice(0, 12)}
                    </p>
                    {manifest && (
                      <details className="mt-2 rounded border border-border bg-muted/20 p-2 text-xs">
                        <summary className="cursor-pointer">查看方法正文与安全声明</summary>
                        <p className="mt-2 text-muted-foreground">
                          声明请求的工具（不是授权）：
                          {manifest.requested_tools?.join("、") || "无"}
                        </p>
                        <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap">
                          {methodBody(record) || "未读取到 SKILL.md 正文"}
                        </pre>
                      </details>
                    )}
                  </div>
                  <div className="flex flex-wrap items-center justify-end gap-2">
                  {record.origin === "imported" && <button
                    type="button"
                    onClick={() => void openHistory(record)}
                    disabled={busyId === record.id}
                    className="inline-flex items-center gap-1 rounded-md border border-border bg-background px-2 py-1 text-xs hover:bg-muted disabled:opacity-50"
                  >
                    <History className="size-3.5" />版本
                  </button>}
                  <button
                    type="button"
                    onClick={() => void exportPackage(record)}
                    disabled={busyId === record.id}
                    className="inline-flex items-center gap-1 rounded-md border border-border bg-background px-2 py-1 text-xs hover:bg-muted disabled:opacity-50"
                  >
                    <Download className="size-3.5" />导出
                  </button>
                  {record.origin === "imported" && (
                    <button
                      type="button"
                      onClick={() => void deletePackage(record)}
                      disabled={busyId === record.id}
                      className="inline-flex items-center gap-1 rounded-md border border-red-200 bg-red-50 px-2 py-1 text-xs text-red-700 hover:bg-red-100 disabled:opacity-50"
                    >
                      <Trash2 className="size-3.5" />删除
                    </button>
                  )}
                  <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
                    <input
                      type="checkbox"
                      checked={record.status === "enabled"}
                      disabled={busyId === record.id || record.status === "quarantined"}
                      onChange={() => void togglePackage(record)}
                      className="size-4 rounded border-border"
                    />
                    {record.status === "enabled" ? "已启用" : "未启用"}
                  </label>
                  </div>
                </div>

                {manifest ? (
                  <div className="mt-3 grid gap-2 border-t border-border pt-3 md:grid-cols-[1fr_1fr_auto]">
                    <label className="space-y-1 text-label text-muted-foreground">
                      <span>法律领域</span>
                      <select
                        value={selected.domain}
                        onChange={(event) =>
                          setBinding((current) => ({
                            ...current,
                            [record.id]: {
                              domain: event.target.value,
                              task: manifest.task_types[0] ?? "",
                            },
                          }))
                        }
                        className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground"
                      >
                        {manifest.legal_domains.map((domain) => (
                          <option key={domain} value={domain}>
                            {DOMAIN_LABELS[domain] ?? domain}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="space-y-1 text-label text-muted-foreground">
                      <span>任务类型</span>
                      <select
                        value={selected.task}
                        onChange={(event) =>
                          setBinding((current) => ({
                            ...current,
                            [record.id]: { ...selected, task: event.target.value },
                          }))
                        }
                        className="w-full rounded-md border border-border bg-background px-2 py-1.5 text-xs text-foreground"
                      >
                        {allowedTasks.map((task) => (
                          <option key={task} value={task}>
                            {TASK_LABELS[task] ?? task}
                          </option>
                        ))}
                      </select>
                    </label>
                    <button
                      type="button"
                      onClick={() => void bindPackage(record)}
                      disabled={
                        busyId === record.id ||
                        record.status !== "enabled" ||
                        !selected.domain ||
                        !selected.task
                      }
                      className="self-end rounded-md border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {busyId === record.id ? "保存中…" : "设为默认"}
                    </button>
                  </div>
                ) : (
                  <p className="mt-3 flex items-center gap-1.5 text-xs text-amber-700">
                    <AlertTriangle className="size-3.5" aria-hidden="true" />
                    已保存的 manifest 无法解析，不能设置默认绑定。
                  </p>
                )}
              </article>
            );
          })}
        </div>
      )}

      {notice && (
        <p className="mt-3 flex items-start gap-2 text-xs text-emerald-700">
          <CheckCircle2 className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          {notice}
        </p>
      )}
      {error && (
        <p role="alert" className="mt-3 flex items-start gap-2 text-xs text-destructive">
          <AlertTriangle className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          {error}
        </p>
      )}

      <p className="mt-3 text-label leading-5 text-muted-foreground">
        导入仅接收 UTF-8 纯文本/JSON：根目录 SKILL.md、manifest.json，以及 references/
        下的 .md/.json/.txt。脚本、二进制文件和目录越界路径不会进入后端。
      </p>
    </section>
  );
}
