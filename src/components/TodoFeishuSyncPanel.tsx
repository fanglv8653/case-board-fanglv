import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, CloudDownload, Loader2, RefreshCw, Save, X } from "lucide-react";

import {
  getSettings,
  getTodoFeishuPreview,
  pullTodoFeishuPreview,
  resolveTodoFeishuPreview,
  saveSettings,
  type TodoFeishuPreview,
  type TodoFeishuPreviewRow,
} from "@/lib/api";
import type { Case, Settings } from "@/lib/types";
import { toast } from "@/components/ui/toast";

const labels: Record<string, string> = {
  create_local: "飞书新增",
  create_remote: "本地新增",
  pull_to_local: "飞书有更新",
  push_to_remote: "本地有更新",
  soft_delete_local: "飞书请求删除",
  remote_missing: "远端记录缺失",
  metadata_invalid: "元数据异常",
  duplicate_id: "重复事项编号",
  conflict: "双方冲突",
};

function parseBaseUrl(value: string): { appToken: string; tableId: string; viewId: string } | null {
  try {
    const url = new URL(value.trim());
    const match = url.pathname.match(/\/base\/([A-Za-z0-9_-]+)/);
    const tableId = url.searchParams.get("table") ?? "";
    const viewId = url.searchParams.get("view") ?? "";
    if (!match || !tableId || !viewId) return null;
    return { appToken: match[1], tableId, viewId };
  } catch {
    return null;
  }
}

function titleFrom(row: TodoFeishuPreviewRow): string {
  for (const raw of [row.remote_payload_json, row.local_payload_json]) {
    if (!raw) continue;
    try {
      const value = JSON.parse(raw) as { title?: string };
      if (value.title) return value.title;
    } catch {
      // 结构异常由稳定错误码展示，不在 UI 猜测修复。
    }
  }
  return row.remote_business_key ?? "无法识别的事项";
}

export function TodoFeishuSyncPanel({ cases, onApplied }: { cases: Case[]; onApplied: () => void }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [url, setUrl] = useState("");
  const [preview, setPreview] = useState<TodoFeishuPreview | null>(null);
  const [caseChoices, setCaseChoices] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [acting, setActing] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setPreview(await getTodoFeishuPreview());
    } catch {
      setPreview({ rows: [], recent_runs: [] });
    }
  }, []);

  useEffect(() => {
    void Promise.all([getSettings(), getTodoFeishuPreview().catch(() => null)]).then(([next, data]) => {
      setSettings(next);
      setPreview(data ?? { rows: [], recent_runs: [] });
      if (next.feishu_todo_inbox_app_token && next.feishu_todo_inbox_table_id && next.feishu_todo_inbox_view_id) {
        setUrl(`https://open.feishu.cn/base/${next.feishu_todo_inbox_app_token}?table=${next.feishu_todo_inbox_table_id}&view=${next.feishu_todo_inbox_view_id}`);
      }
    });
  }, []);

  const save = async () => {
    const parsed = parseBaseUrl(url);
    if (!settings || !parsed) {
      toast("请粘贴包含 table 和 view 参数的完整飞书 Base URL", "error");
      return;
    }
    setSaving(true);
    try {
      const next: Settings = {
        ...settings,
        feishu_todo_inbox_app_token: parsed.appToken,
        feishu_todo_inbox_table_id: parsed.tableId,
        feishu_todo_inbox_view_id: parsed.viewId,
      };
      await saveSettings(next);
      setSettings(next);
      toast("收件箱绑定已保存；尚未读取或写入任何事项", "success");
    } catch (error) {
      toast(`保存失败：${String(error)}`, "error");
    } finally {
      setSaving(false);
    }
  };

  const pull = async () => {
    setPulling(true);
    try {
      const result = await pullTodoFeishuPreview();
      await reload();
      toast(`只读预演完成：读取 ${result.remote_count} 条，生成 ${result.preview_count} 条候选`, "success");
    } catch (error) {
      toast(`预演失败，已保留上次结果：${String(error)}`, "error");
    } finally {
      setPulling(false);
    }
  };

  const resolve = async (row: TodoFeishuPreviewRow, resolution: "local" | "feishu" | "keep_both" | "dismiss") => {
    setActing(row.id);
    try {
      await resolveTodoFeishuPreview({
        preview_id: row.id,
        resolution,
        case_id: caseChoices[row.id] || null,
        action_id: crypto.randomUUID(),
      });
      await reload();
      if (resolution === "feishu" || resolution === "keep_both") onApplied();
      toast(resolution === "feishu" ? "已采用飞书事项" : resolution === "keep_both" ? "已保留本地事项，并将飞书版本另存为新事项" : resolution === "local" ? "已将本地版本写入飞书并回读确认" : "已暂不处理；下次有差异仍会提示", "success");
    } catch (error) {
      toast(`处理失败：${String(error)}`, "error");
    } finally {
      setActing(null);
    }
  };

  const rows = preview?.rows ?? [];
  const latest = preview?.recent_runs[0];
  return (
    <section className="space-y-3 rounded-xl border border-sky-200 bg-sky-50/40 p-4 dark:border-sky-900 dark:bg-sky-950/20">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="font-medium">飞书“收件箱”同步</h2>
          <p className="mt-1 text-xs text-muted-foreground">自动操作只读取并生成预演；逐条确认后才写入本地。不会物理删除飞书记录。</p>
        </div>
        <button type="button" onClick={() => void pull()} disabled={pulling || !parseBaseUrl(url)} className="inline-flex items-center gap-2 rounded-md border border-border bg-card px-3 py-2 text-sm disabled:opacity-50">
          {pulling ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}只读预演
        </button>
      </div>
      <div className="flex gap-2">
        <input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="粘贴完整飞书 Base URL（含 table、view）" className="min-w-0 flex-1 rounded-md border border-border bg-background px-3 py-2 text-sm" />
        <button type="button" onClick={() => void save()} disabled={saving || !parseBaseUrl(url)} className="inline-flex items-center gap-2 rounded-md bg-sky-600 px-3 py-2 text-sm text-white disabled:opacity-50"><Save className="size-4" />保存绑定</button>
      </div>
      {latest && <p className="text-xs text-muted-foreground">最近预演：{latest.status} · 远端 {latest.remote_count} · 候选 {latest.preview_count} · 冲突 {latest.conflict_count}{latest.error_code ? ` · ${latest.error_code}` : ""}</p>}
      {rows.length > 0 && <div className="space-y-2 border-t border-sky-200 pt-3 dark:border-sky-900">
        {rows.map((row) => {
          const canPull = (["create_local", "pull_to_local", "soft_delete_local", "conflict"].includes(row.change_kind) && Boolean(row.record_id)) || row.change_kind === "remote_missing";
          const canPush = ["create_remote", "push_to_remote", "conflict", "remote_missing"].includes(row.change_kind) && Boolean(row.item_id);
          return <div key={row.id} className="rounded-lg border border-border bg-card p-3">
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div><p className="text-sm font-medium">{titleFrom(row)}</p><p className="mt-1 text-xs text-muted-foreground">{labels[row.change_kind] ?? row.change_kind}{row.remote_business_key ? ` · ${row.remote_business_key}` : ""}{row.case_hint ? ` · 案件线索：${row.case_hint}` : ""}</p>{row.error_code && <p className="mt-1 flex items-center gap-1 text-xs text-amber-700"><AlertTriangle className="size-3" />{row.error_code}</p>}</div>
              <div className="flex gap-1">
                {canPull && <button type="button" disabled={acting === row.id} onClick={() => void resolve(row, "feishu")} className="inline-flex items-center gap-1 rounded bg-sky-600 px-2.5 py-1.5 text-xs text-white disabled:opacity-50"><CloudDownload className="size-3.5" />{row.change_kind === "remote_missing" ? "确认远端已删除" : "采用飞书"}</button>}
                {canPush && <button type="button" disabled={acting === row.id} onClick={() => void resolve(row, "local")} className="inline-flex items-center gap-1 rounded bg-emerald-600 px-2.5 py-1.5 text-xs text-white disabled:opacity-50">写入飞书</button>}
                {row.change_kind === "conflict" && <button type="button" disabled={acting === row.id} onClick={() => void resolve(row, "keep_both")} className="rounded border border-border px-2.5 py-1.5 text-xs disabled:opacity-50">保留两份</button>}
                <button type="button" disabled={acting === row.id} onClick={() => void resolve(row, "dismiss")} className="inline-flex items-center gap-1 rounded border border-border px-2.5 py-1.5 text-xs disabled:opacity-50"><X className="size-3.5" />暂不处理</button>
              </div>
            </div>
            {canPull && row.case_hint && <label className="mt-2 flex items-center gap-2 text-xs"><span className="text-muted-foreground">确认关联案件</span><select value={caseChoices[row.id] ?? ""} onChange={(event) => setCaseChoices((current) => ({ ...current, [row.id]: event.target.value }))} className="rounded border border-border bg-background px-2 py-1"><option value="">暂不关联</option>{cases.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>}
          </div>;
        })}
      </div>}
    </section>
  );
}
