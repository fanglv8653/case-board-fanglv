import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  CalendarClock,
  CheckCircle2,
  KeyRound,
  Link2,
  Loader2,
  LogOut,
  TableProperties,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import {
  connectFeishuReadonly,
  disconnectFeishuReadonly,
  getFeishuConnectionStatus,
} from "@/lib/api";
import type { FeishuConnectionStatus } from "@/lib/types";
import { cn } from "@/lib/utils";
import { FeishuCalendarTool } from "./FeishuCalendarTool";
import { FeishuSyncPreview } from "./FeishuSyncPreview";

type Tab = "sync" | "connection" | "calendar";

function connectionErrorMessage(error: unknown): string {
  const message = String(error).toUpperCase();
  if (message.includes("FEISHU_OAUTH_CREDENTIAL_STORE")) {
    return "Windows 凭据安全保存失败，请确认当前 Windows 用户凭据库可用后重试。";
  }
  if (message.includes("FEISHU_OAUTH_TOKEN_REJECTED")) {
    return "飞书认证失败，请重新连接并确认应用权限已经发布。";
  }
  if (message.includes("FEISHU_OAUTH_INVALID_TOKEN_RESPONSE")) {
    return "飞书认证响应异常，请稍后重试；如持续出现，请重新连接。";
  }
  if (
    message.includes("FEISHU_OAUTH_INVALID_APP_ID")
    || message.includes("FEISHU_OAUTH_MISSING_APP_SECRET")
    || message.includes("FEISHU_OAUTH_INVALID_CLIENT")
  ) {
    return "App ID 或 App Secret 无效，请核对后重新连接。";
  }
  if (message.includes("FEISHU_OAUTH_MISSING_READONLY_SCOPE")) {
    return "当前应用缺少多维表格只读权限，请在飞书开发者后台补充权限后重新连接。";
  }
  if (message.includes("FEISHU_OAUTH_REAUTHORIZATION_REQUIRED")) {
    return "飞书授权已失效，请重新连接。";
  }
  if (message.includes("FEISHU_OAUTH_ACCESS_DENIED")) {
    return "未完成飞书授权，请在授权页面同意只读权限后重试。";
  }
  if (message.includes("FEISHU_OAUTH_BROWSER_OPEN_FAILED")) {
    return "无法打开飞书授权页面，请检查默认浏览器后重试。";
  }
  if (message.includes("FEISHU_OAUTH_CALLBACK_PORT_UNAVAILABLE")) {
    return "无法启动本地授权回调，请关闭占用端口的程序后重试。";
  }
  if (message.includes("FEISHU_OAUTH_CALLBACK_TIMEOUT")) {
    return "飞书授权等待超时，请重新连接。";
  }
  if (message.includes("FEISHU_OAUTH_NETWORK")) {
    return "连接飞书超时，请检查网络后重试。";
  }
  if (message.includes("FEISHU_OAUTH_IN_PROGRESS")) {
    return "已有一个飞书授权窗口正在等待完成，请先完成或关闭该窗口。";
  }
  return "暂时无法完成飞书连接，请稍后重试。";
}

export function FeishuTool() {
  const [tab, setTab] = useState<Tab>("sync");
  const [connectionStatus, setConnectionStatus] = useState<FeishuConnectionStatus | null>(null);

  const refreshConnectionStatus = useCallback(async () => {
    try {
      const status = await getFeishuConnectionStatus();
      setConnectionStatus(status);
      return status;
    } catch {
      setConnectionStatus(null);
      return null;
    }
  }, []);

  useEffect(() => void refreshConnectionStatus(), [refreshConnectionStatus]);

  return <div className="space-y-5">
    <div role="tablist" aria-label="飞书连接功能" className="inline-flex max-w-full overflow-x-auto rounded-lg border border-border bg-muted/30 p-1">
      <TabButton active={tab === "sync"} onClick={() => setTab("sync")} controls="feishu-sync-panel"><TableProperties />案件同步预览</TabButton>
      <TabButton active={tab === "connection"} onClick={() => setTab("connection")} controls="feishu-connection-panel"><Link2 />只读连接</TabButton>
      <TabButton active={tab === "calendar"} onClick={() => setTab("calendar")} controls="feishu-calendar-panel"><CalendarClock />日历设置</TabButton>
    </div>
    <div id={`feishu-${tab}-panel`} role="tabpanel">
      {tab === "sync" && <FeishuSyncPreview connectionStatus={connectionStatus} onOpenConnection={() => setTab("connection")} onConnectionStatusChange={setConnectionStatus} />}
      {tab === "connection" && <FeishuConnectionPanel status={connectionStatus} onStatusChange={setConnectionStatus} onRefresh={refreshConnectionStatus} />}
      {tab === "calendar" && <div className="mx-auto max-w-3xl"><FeishuCalendarTool /></div>}
    </div>
  </div>;
}

function FeishuConnectionPanel({
  status,
  onStatusChange,
  onRefresh,
}: {
  status: FeishuConnectionStatus | null;
  onStatusChange: (status: FeishuConnectionStatus) => void;
  onRefresh: () => Promise<FeishuConnectionStatus | null>;
}) {
  const [appId, setAppId] = useState(status?.app_id ?? "");
  const [appSecret, setAppSecret] = useState("");
  const [loading, setLoading] = useState(false);
  const [checking, setChecking] = useState(status === null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (status?.app_id) setAppId(status.app_id);
  }, [status?.app_id]);

  useEffect(() => {
    if (!checking) return;
    void onRefresh().finally(() => setChecking(false));
  }, [checking, onRefresh]);

  const handleConnect = async (event: React.FormEvent) => {
    event.preventDefault();
    const input = { app_id: appId.trim(), app_secret: appSecret };
    if (!input.app_id || !input.app_secret) {
      setError("请填写 App ID 和 App Secret。");
      return;
    }
    setAppSecret("");
    setLoading(true);
    setError(null);
    try {
      const next = await connectFeishuReadonly(input);
      onStatusChange(next);
      toast("飞书只读连接已建立", "info");
    } catch (connectError) {
      setError(connectionErrorMessage(connectError));
    } finally {
      setLoading(false);
    }
  };

  const handleDisconnect = async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await disconnectFeishuReadonly();
      onStatusChange(next);
      toast("飞书只读连接已断开", "info");
    } catch (disconnectError) {
      setError(connectionErrorMessage(disconnectError));
    } finally {
      setLoading(false);
    }
  };

  const needsReauthorization = Boolean(status?.app_id) && status?.reauthorization_required === true;
  const connected = status?.connected === true && !needsReauthorization;

  return <div className="mx-auto max-w-3xl space-y-5" aria-busy={loading || checking}>
    <section className="rounded-xl border border-border bg-card p-5" aria-labelledby="feishu-connection-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <KeyRound className="size-5 text-foreground" />
            <h3 id="feishu-connection-title" className="text-base font-semibold text-foreground">飞书案件只读连接</h3>
          </div>
          <p className="mt-2 text-sm leading-relaxed text-muted-foreground">只申请身份识别和多维表格只读权限。案件看板不会写入飞书；App Secret 提交后会立即从页面清空，不写入项目设置或案件数据库。</p>
        </div>
        <div role="status" aria-live="polite" className={cn("inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium", connected ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/35 dark:text-emerald-300" : "bg-muted text-muted-foreground")}>
          {checking ? <Loader2 className="size-3.5 animate-spin" /> : connected ? <CheckCircle2 className="size-3.5" /> : <AlertTriangle className="size-3.5" />}
          {checking ? "正在检查连接" : connected ? "已连接 · 只读权限" : needsReauthorization ? "授权已失效" : "未连接"}
        </div>
      </div>

      {error && <div role="alert" className="mt-4 flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive"><AlertTriangle className="mt-0.5 size-4 shrink-0" /><span>{error}</span></div>}

      {connected ? <div className="mt-5 space-y-4">
        <dl className="grid gap-3 rounded-lg bg-muted/25 p-4 text-sm sm:grid-cols-2">
          <div><dt className="text-xs text-muted-foreground">App ID</dt><dd className="mt-1 font-medium text-foreground">{status?.app_id || "—"}</dd></div>
          <div><dt className="text-xs text-muted-foreground">授权范围</dt><dd className="mt-1 font-medium text-foreground">多维表格只读</dd></div>
        </dl>
        <Button type="button" variant="outline" onClick={handleDisconnect} disabled={loading}>
          {loading ? <Loader2 className="animate-spin" /> : <LogOut />}断开只读连接
        </Button>
      </div> : <form className="mt-5 space-y-4" onSubmit={handleConnect}>
        <div className="space-y-1.5">
          <label htmlFor="feishu-oauth-app-id" className="text-sm font-medium text-foreground">App ID</label>
          <input id="feishu-oauth-app-id" type="text" value={appId} onChange={(event) => setAppId(event.target.value)} disabled={loading} autoComplete="off" spellCheck={false} placeholder="cli_..." className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-foreground/40 disabled:opacity-60" />
        </div>
        <div className="space-y-1.5">
          <label htmlFor="feishu-oauth-app-secret" className="text-sm font-medium text-foreground">App Secret</label>
          <input id="feishu-oauth-app-secret" type="password" value={appSecret} onChange={(event) => setAppSecret(event.target.value)} disabled={loading} autoComplete="off" spellCheck={false} placeholder="仅用于本次连接" className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-foreground/40 disabled:opacity-60" />
        </div>
        <Button type="submit" disabled={loading || checking}>
          {loading ? <Loader2 className="animate-spin" /> : <Link2 />}{loading ? "等待浏览器授权…" : needsReauthorization ? "重新连接飞书" : "连接飞书"}
        </Button>
      </form>}
    </section>
  </div>;
}

function TabButton({ active, onClick, controls, children }: { active: boolean; onClick: () => void; controls: string; children: React.ReactNode }) {
  return <button type="button" role="tab" aria-selected={active} aria-controls={controls} onClick={onClick} className={cn("inline-flex shrink-0 items-center gap-2 rounded-md px-3 py-2 text-sm text-muted-foreground [&_svg]:size-4", active && "bg-card font-medium text-foreground shadow-sm")}>{children}</button>;
}
