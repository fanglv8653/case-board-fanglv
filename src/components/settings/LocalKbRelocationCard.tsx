import { useState } from "react";
import { FolderOpen, RefreshCw } from "lucide-react";

export type LocalKbRelocationMode = "switch_existing" | "migrate_current";

export interface LocalKbRelocationProgress {
  phase: string;
  completed: number;
  total: number | null;
  message: string;
}

export interface LocalKbRelocationResult {
  target_path: string;
  backup_path: string | null;
  recovery_path: string | null;
  index_rebuild_required: boolean;
}

export interface LocalKbRelocationCardProps {
  currentPath: string;
  onPickDirectory: (mode: LocalKbRelocationMode) => Promise<string | null>;
  onConfirm: (mode: LocalKbRelocationMode, targetPath: string) => Promise<boolean>;
  onSwitchExisting: (
    targetPath: string,
    onProgress: (progress: LocalKbRelocationProgress) => void,
  ) => Promise<LocalKbRelocationResult>;
  onMigrateCurrent: (
    targetPath: string,
    onProgress: (progress: LocalKbRelocationProgress) => void,
  ) => Promise<LocalKbRelocationResult>;
  onRebuildSemanticIndex: (
    targetPath: string,
    onProgress: (progress: LocalKbRelocationProgress) => void,
  ) => Promise<void>;
}

type OperationState =
  | "idle"
  | "selecting"
  | "running"
  | "rebuild_required"
  | "rebuilding"
  | "complete"
  | "error";

function relocationErrorMessage(cause: unknown): string {
  if (cause && typeof cause === "object") {
    const error = cause as { message?: unknown; recovery_path?: unknown; recoveryPath?: unknown };
    const message = typeof error.message === "string" ? error.message : JSON.stringify(cause);
    const recovery = error.recovery_path ?? error.recoveryPath;
    return typeof recovery === "string" && recovery
      ? `${message}；恢复路径：${recovery}`
      : message;
  }
  return String(cause);
}

export function LocalKbRelocationCard({
  currentPath,
  onPickDirectory,
  onConfirm,
  onSwitchExisting,
  onMigrateCurrent,
  onRebuildSemanticIndex,
}: LocalKbRelocationCardProps) {
  const [state, setState] = useState<OperationState>("idle");
  const [progress, setProgress] = useState<LocalKbRelocationProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<LocalKbRelocationResult | null>(null);

  async function rebuildIndex(relocation: LocalKbRelocationResult) {
    setError(null);
    setState("rebuild_required");
    setProgress({
      phase: "index_rebuild_required",
      completed: 0,
      total: null,
      message: "目录已处理，必须重建语义索引后才算完成。",
    });
    try {
      setState("rebuilding");
      await onRebuildSemanticIndex(relocation.target_path, setProgress);
      setState("complete");
      setProgress({
        phase: "complete",
        completed: 1,
        total: 1,
        message: "目录处理和语义索引重建均已完成。",
      });
    } catch (cause) {
      setState("rebuild_required");
      setError(`语义索引重建失败：${relocationErrorMessage(cause)}`);
    }
  }

  async function run(mode: LocalKbRelocationMode) {
    if (!["idle", "complete", "error", "rebuild_required"].includes(state)) return;
    setState("selecting");
    setError(null);
    setProgress(null);
    setResult(null);
    try {
      const target = await onPickDirectory(mode);
      if (!target) {
        setState("idle");
        return;
      }
      if (!(await onConfirm(mode, target))) {
        setState("idle");
        return;
      }
      setState("running");
      const relocation =
        mode === "switch_existing"
          ? await onSwitchExisting(target, setProgress)
          : await onMigrateCurrent(target, setProgress);
      setResult(relocation);
      if (relocation.index_rebuild_required) {
        await rebuildIndex(relocation);
      } else {
        setState("complete");
      }
    } catch (cause) {
      setState("error");
      setError(relocationErrorMessage(cause));
    }
  }

  const busy = state === "selecting" || state === "running" || state === "rebuilding";
  const percent =
    progress?.total && progress.total > 0
      ? Math.min(100, Math.round((progress.completed / progress.total) * 100))
      : null;

  return (
    <section className="rounded-lg border border-border bg-card p-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground">本地知识库目录</h3>
        <p className="mt-1 text-xs text-muted-foreground">当前绝对路径</p>
        <code className="mt-1 block break-all rounded-md bg-muted/50 p-2 text-xs">{currentPath}</code>
      </div>

      <div className="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => run("switch_existing")}
          className="inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs disabled:opacity-50"
        >
          <FolderOpen className="size-3.5" />
          切换已有库
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => run("migrate_current")}
          className="inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs disabled:opacity-50"
        >
          <RefreshCw className="size-3.5" />
          迁移当前库
        </button>
      </div>

      {progress && (
        <div className="mt-3 rounded-md bg-muted/40 p-3 text-xs">
          <p>{progress.message}</p>
          <p className="mt-1 text-muted-foreground">
            阶段：{progress.phase}
            {percent === null ? "" : ` · ${percent}%`}
          </p>
        </div>
      )}

      {state === "rebuild_required" && result && (
        <div className="mt-3 rounded-md border border-amber-300 bg-amber-50 p-3 text-xs text-amber-800">
          <p>语义索引尚未完成，当前不能宣称迁移闭环。</p>
          <button
            type="button"
            onClick={() => rebuildIndex(result)}
            className="mt-2 rounded-md border border-amber-400 px-2.5 py-1"
          >
            重新执行语义索引重建
          </button>
        </div>
      )}

      {result?.backup_path && (
        <p className="mt-3 break-all text-xs text-muted-foreground">
          旧目录回退备份：{result.backup_path}
        </p>
      )}
      {result?.recovery_path && (
        <p className="mt-1 break-all text-xs text-muted-foreground">
          恢复路径：{result.recovery_path}
        </p>
      )}
      {error && (
        <p role="alert" className="mt-3 text-xs text-red-600">
          {error}
        </p>
      )}
      {state === "complete" && (
        <p className="mt-3 text-xs text-emerald-700">
          操作已完成{result?.index_rebuild_required ? "，语义索引已重建" : ""}。
        </p>
      )}
    </section>
  );
}
