import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import type {
  MaterialDecisionInput,
  MaterialDisposition,
  MaterialPreflight,
  MaterialPreflightItem,
} from "@/lib/types";

const choices: { value: MaterialDisposition; label: string }[] = [
  { value: "recognize", label: "识别" },
  { value: "index_only", label: "仅索引" },
  { value: "excluded", label: "排除" },
];
const PAGE_SIZE = 150;
type Tree = { name: string; path: string; files: MaterialPreflightItem[]; dirs: Tree[] };

function buildTree(items: MaterialPreflightItem[]): Tree {
  const root: Tree = { name: "根目录", path: "", files: [], dirs: [] };
  for (const item of items) {
    const parts = item.relativePath.split("/");
    parts.pop();
    let node = root;
    for (const part of parts) {
      let child = node.dirs.find((candidate) => candidate.name === part);
      if (!child) {
        child = {
          name: part,
          path: node.path ? `${node.path}/${part}` : part,
          files: [],
          dirs: [],
        };
        node.dirs.push(child);
      }
      node = child;
    }
    node.files.push(item);
  }
  return root;
}
function descendants(node: Tree): MaterialPreflightItem[] {
  return [...node.files, ...node.dirs.flatMap(descendants)];
}
function formatSize(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function MaterialPreflightDialog({
  preflight,
  busy,
  onCancel,
  onConfirm,
}: {
  preflight: MaterialPreflight;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (decisions: MaterialDecisionInput[], startProcessing: boolean) => void;
}) {
  const [decisions, setDecisions] = useState<Record<string, MaterialDisposition>>(() =>
    Object.fromEntries(
      preflight.items.map((item) => [item.sourcePath, item.defaultDisposition]),
    ),
  );
  const [visibleLimit, setVisibleLimit] = useState(PAGE_SIZE);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const tree = useMemo(() => buildTree(preflight.items), [preflight]);
  const visiblePaths = useMemo(
    () => new Set(preflight.items.slice(0, visibleLimit).map((item) => item.sourcePath)),
    [preflight.items, visibleLimit],
  );
  const counts = useMemo(() => {
    const result: Record<MaterialDisposition, number> = {
      recognize: 0,
      index_only: 0,
      excluded: 0,
    };
    Object.values(decisions).forEach((value) => result[value]++);
    return result;
  }, [decisions]);
  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && !busy && onCancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onCancel]);

  const setItems = (items: MaterialPreflightItem[], value: MaterialDisposition) =>
    setDecisions((current) => {
      const next = { ...current };
      items.forEach((item) => (next[item.sourcePath] = value));
      return next;
    });
  const setAll = (value: MaterialDisposition) => setItems(preflight.items, value);
  const invert = () =>
    setDecisions((current) =>
      Object.fromEntries(
        Object.entries(current).map(([path, value]) => [
          path,
          value === "recognize"
            ? "excluded"
            : value === "excluded"
              ? "recognize"
              : "index_only",
        ]),
      ),
    );
  const submit = (startProcessing: boolean) =>
    onConfirm(
      preflight.items.map((item) => ({
        sourcePath: item.sourcePath,
        disposition: decisions[item.sourcePath],
      })),
      startProcessing,
    );

  const renderNode = (node: Tree, depth = 0): ReactNode => {
    const files = node.files.filter((item) => visiblePaths.has(item.sourcePath));
    const children = node.dirs
      .map((child) => renderNode(child, depth + 1))
      .filter(Boolean);
    if (!files.length && !children.length) return null;
    return (
      <details key={node.path || "root"} open={depth < 2} className="border-l">
        <summary className="sticky top-0 flex cursor-pointer items-center justify-between gap-2 bg-slate-50 px-3 py-2 text-sm">
          <span className="min-w-0 truncate">📁 {node.name}</span>
          <span className="flex shrink-0 gap-1" onClick={(event) => event.preventDefault()}>
            {choices.map((choice) => (
              <button
                key={choice.value}
                className="rounded border bg-white px-2 py-1 text-xs"
                onClick={() => setItems(descendants(node), choice.value)}
              >
                全部{choice.label}
              </button>
            ))}
          </span>
        </summary>
        <div className="pl-3">
          {children}
          {files.map((item) => (
            <div
              key={item.sourcePath}
              className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-t px-3 py-2"
            >
              <div className="min-w-0">
                <div className="truncate text-sm" title={item.relativePath}>
                  {item.filename}
                  {!item.isExisting && preflight.mode === "refresh" && (
                    <span className="ml-2 rounded bg-amber-100 px-1.5 py-0.5 text-xs text-amber-800">
                      新增待确认
                    </span>
                  )}
                </div>
                <div className="text-xs text-slate-400">
                  {formatSize(item.sizeBytes)}
                  {item.stage ? ` · ${item.stage}` : ""}
                  {item.category ? ` · ${item.category}` : ""}
                </div>
              </div>
              <div className="flex gap-1">
                {choices.map((choice) => (
                  <button
                    key={choice.value}
                    className={`rounded px-2.5 py-1 text-xs ${
                      decisions[item.sourcePath] === choice.value
                        ? "bg-slate-900 text-white"
                        : "border bg-white"
                    }`}
                    onClick={() => setItems([item], choice.value)}
                  >
                    {choice.label}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      </details>
    );
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/35 p-6">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="material-preflight-title"
        className="flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-xl bg-white shadow-2xl"
      >
        <div className="border-b px-6 py-4">
          <h2 id="material-preflight-title" className="text-lg font-semibold">材料导入预检</h2>
          <p className="mt-1 text-sm text-slate-500">
            本次仅读取文件名、路径和大小；确认前不会建案、写库或调用 OCR/AI。
          </p>
          {preflight.largeCriminalBatch && (
            <p className="mt-2 rounded-md bg-amber-50 px-3 py-2 text-sm text-amber-800">
              刑事材料共 {preflight.totalFiles} 份，已按大批量保护策略默认设为“仅索引”。
            </p>
          )}
        </div>
        <div className="border-b bg-slate-50 px-6 py-3 text-sm">
          <div className="flex flex-wrap items-center gap-4">
            <span>{preflight.totalFiles} 份 · {formatSize(preflight.totalSizeBytes)}</span>
            <span className="text-emerald-700">识别 {counts.recognize}</span>
            <span className="text-blue-700">仅索引 {counts.index_only}</span>
            <span className="text-slate-500">排除 {counts.excluded}</span>
            <span className="text-slate-600">
              预计：本地解析 {counts.recognize} 份；OCR 最多 {counts.recognize} 次；
              LLM 字段抽取 {counts.recognize} 份
            </span>
          </div>
          <div className="mt-2 flex flex-wrap gap-2">
            <button className="rounded border bg-white px-2 py-1 text-xs" onClick={() => setAll("recognize")}>全选识别</button>
            <button className="rounded border bg-white px-2 py-1 text-xs" onClick={() => setAll("index_only")}>全设仅索引</button>
            <button className="rounded border bg-white px-2 py-1 text-xs" onClick={() => setAll("excluded")}>全排除</button>
            <button className="rounded border bg-white px-2 py-1 text-xs" onClick={invert} title="识别与排除互换，仅索引保持不变">反选（识别↔排除）</button>
          </div>
        </div>
        <div className="flex-1 overflow-auto px-6 py-3">{renderNode(tree)}</div>
        {visibleLimit < preflight.items.length && (
          <button
            className="border-t py-2 text-sm text-blue-700"
            onClick={() => setVisibleLimit((value) => value + PAGE_SIZE)}
          >
            再显示 {Math.min(PAGE_SIZE, preflight.items.length - visibleLimit)} 份
          </button>
        )}
        <div className="flex justify-end gap-2 border-t px-6 py-4">
          <button ref={cancelRef} className="rounded border px-4 py-2 text-sm" disabled={busy} onClick={onCancel}>取消</button>
          <button className="rounded border px-4 py-2 text-sm" disabled={busy} onClick={() => submit(false)}>仅保存决策（不联网）</button>
          <button className="rounded bg-slate-900 px-4 py-2 text-sm text-white disabled:opacity-50" disabled={busy || counts.recognize === 0} onClick={() => submit(true)}>
            确认并开始识别
          </button>
        </div>
      </div>
    </div>
  );
}
