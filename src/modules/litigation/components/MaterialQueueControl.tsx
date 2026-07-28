import { useCallback, useEffect, useMemo, useState } from "react";
import {
  cancelMaterialProcessingBatch,
  getMaterialProcessingBatch,
  ignoreFailedMaterialItems,
  listMaterialProcessingBatches,
  pauseMaterialProcessingBatch,
  resumeMaterialBatchExecution,
  startMaterialBatchExecution,
} from "@/lib/api";
import type { MaterialBatchDetail, MaterialProcessingItem } from "@/lib/types";

export function MaterialQueueControl({ caseId }: { caseId: string }) {
  const [detail, setDetail] = useState<MaterialBatchDetail | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const batches = await listMaterialProcessingBatches(caseId);
      if (!batches.length) {
        setDetail(null);
        return;
      }
      setDetail(await getMaterialProcessingBatch(batches[0].id));
      setError(null);
    } catch (reason) {
      setError(`读取材料队列失败：${reason}`);
    }
  }, [caseId]);

  useEffect(() => {
    void reload();
  }, [reload]);
  useEffect(() => {
    if (!detail || !["queued", "running"].includes(detail.batch.status)) return;
    const timer = window.setInterval(() => void reload(), 2000);
    return () => window.clearInterval(timer);
  }, [detail?.batch.status, reload]);

  const counts = useMemo(() => {
    const result: Record<string, number> = {};
    detail?.items.forEach((item) => (result[item.status] = (result[item.status] ?? 0) + 1));
    return result;
  }, [detail]);
  const failures = useMemo(() => {
    const result = new Map<string, MaterialProcessingItem[]>();
    detail?.items
      .filter((item) => item.status === "failed")
      .forEach((item) => {
        const category = item.errorCategory ?? "unknown";
        result.set(category, [...(result.get(category) ?? []), item]);
      });
    return [...result.entries()];
  }, [detail]);

  if (!detail) {
    return error ? (
      <p role="alert" className="mb-3 rounded border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">{error}</p>
    ) : null;
  }
  const act = async (action: () => Promise<MaterialBatchDetail>) => {
    setBusy(true);
    try {
      setDetail(await action());
      setError(null);
    } catch (reason) {
      setError(`队列操作失败：${reason}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="mb-3 rounded-lg border bg-white px-4 py-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <strong className="text-sm">材料识别队列</strong>
          <span className="ml-2 text-xs text-slate-500">
            {detail.batch.status} · 待处理 {counts.queued ?? 0} · 处理中 {counts.running ?? 0} ·
            成功 {counts.completed ?? 0} · 失败 {counts.failed ?? 0}
          </span>
        </div>
        <div className="flex gap-2">
          {detail.batch.status === "queued" && (
            <button
              className="rounded border px-2 py-1 text-xs"
              disabled={busy}
              onClick={() => void act(() => startMaterialBatchExecution(detail.batch.id))}
            >
              开始
            </button>
          )}
          {detail.batch.status === "running" && (
            <button
              className="rounded border px-2 py-1 text-xs"
              disabled={busy}
              onClick={() => void act(() => pauseMaterialProcessingBatch(detail.batch.id))}
            >
              暂停
            </button>
          )}
          {detail.batch.status === "paused" && (
            <button
              className="rounded border px-2 py-1 text-xs"
              disabled={busy}
              onClick={() => void act(() => resumeMaterialBatchExecution(detail.batch.id))}
            >
              恢复为待处理
            </button>
          )}
          {["queued", "running", "paused", "recovery_required"].includes(
            detail.batch.status,
          ) && (
            <button
              className="rounded border border-red-200 px-2 py-1 text-xs text-red-700"
              disabled={busy}
              onClick={() => void act(() => cancelMaterialProcessingBatch(detail.batch.id))}
            >
              取消剩余
            </button>
          )}
        </div>
      </div>
      {error && <p role="alert" className="mt-2 text-xs text-red-700">{error}</p>}
      {failures.map(([category, items]) => (
        <details key={category} className="mt-2 rounded bg-red-50 px-3 py-2 text-xs">
          <summary className="cursor-pointer text-red-800">
            {category}（{items.length}）
          </summary>
          <ul className="my-2 list-disc pl-5 text-red-700">
            {items.map((item) => (
              <li key={item.id}>
                {item.sourcePath.split(/[\\/]/).pop()}：{item.errorSummary ?? "未提供摘要"}
              </li>
            ))}
          </ul>
          <button
            className="rounded border border-red-200 bg-white px-2 py-1"
            disabled={busy}
            onClick={() =>
              void act(() => ignoreFailedMaterialItems(detail.batch.id, category))
            }
          >
            批量忽略此类失败
          </button>
        </details>
      ))}
    </section>
  );
}
