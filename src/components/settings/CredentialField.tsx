import { useState } from "react";
import { CheckCircle2, Loader2, Save, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { confirmDialog } from "@/lib/dialog";
import { deleteCredential, setCredential } from "@/lib/api";
import type { CredentialStatus } from "@/lib/types";

interface Props {
  label: string;
  locator: string;
  status?: CredentialStatus;
  placeholder?: string;
  onStatusChange?: (status: CredentialStatus) => void;
}

export function CredentialField({
  label,
  locator,
  status,
  placeholder = "输入新凭据（不会回显已保存值）",
  onStatusChange,
}: Props) {
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function save() {
    if (!draft.trim()) return;
    setBusy(true);
    setError("");
    try {
      const next = await setCredential(locator, draft.trim());
      setDraft("");
      onStatusChange?.(next);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (
      !(await confirmDialog(
        `确定删除“${label}”的已保存凭据吗？删除后相关在线功能将立即停用。`,
      ))
    ) {
      return;
    }
    setBusy(true);
    setError("");
    try {
      const next = await deleteCredential(locator);
      setDraft("");
      onStatusChange?.(next);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <label className="text-sm font-medium">{label}</label>
        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
          {status?.configured ? (
            <>
              <CheckCircle2 className="h-3.5 w-3.5 text-emerald-600" />
              已安全保存
            </>
          ) : (
            "未配置"
          )}
        </span>
      </div>
      <div className="flex gap-2">
        <input
          type="password"
          autoComplete="new-password"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder={placeholder}
          className="min-w-0 flex-1 rounded-md border bg-background px-3 py-2 text-sm"
        />
        <Button type="button" variant="outline" disabled={busy || !draft.trim()} onClick={save}>
          {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
          保存
        </Button>
        {status?.configured && (
          <Button type="button" variant="outline" disabled={busy} onClick={remove}>
            <Trash2 className="h-4 w-4" />
            删除
          </Button>
        )}
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}
