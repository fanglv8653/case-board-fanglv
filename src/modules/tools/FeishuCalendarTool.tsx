/**
 * 飞书日历配置(整合外部贡献 PR #9,gcheng-001;2026-06-17)。
 *
 * 在这里打开「飞书日历」并填配置;开启且连接成功后,首页会用飞书月历替代本地「日程日历」卡。
 * 日历功能继续复用本机 lark-cli 登录态；案件受控同步使用相邻的「案件同步连接」，两者互不依赖。
 *
 * 依赖(诚实标明,装不上属正常):
 *   1. 本机装好飞书官方 `lark-cli` 并授权日历只读 scope;
 *   2. (可选)飞书"案件池"多维表格,用于点日历事件反查并导入本地案件目录。
 */
import { useEffect, useState } from "react";
import {
  CalendarClock,
  Loader2,
  Save,
  CheckCircle2,
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  ExternalLink,
} from "lucide-react";

import {
  finishFeishuCalendarAuthorization,
  getSettings,
  openUrl,
  saveSettings,
  startFeishuCalendarAuthorization,
  testFeishuCalendarConnection,
} from "@/lib/api";
import type { FeishuCalendarAuthorization, FeishuCalendarDiagnostic, Settings } from "@/lib/types";
import { toast } from "@/components/ui/toast";

function todayISO(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function FeishuCalendarTool() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [larkPath, setLarkPath] = useState("");
  const [appToken, setAppToken] = useState("");
  const [tableId, setTableId] = useState("");
  const [poolOpen, setPoolOpen] = useState(false);

  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testOk, setTestOk] = useState<boolean | null>(null);
  const [testMsg, setTestMsg] = useState<string | null>(null);
  const [diagnostic, setDiagnostic] = useState<FeishuCalendarDiagnostic | null>(null);
  const [authorization, setAuthorization] = useState<FeishuCalendarAuthorization | null>(null);
  const [authorizing, setAuthorizing] = useState(false);

  useEffect(() => {
    getSettings()
      .then((s) => {
        setSettings(s);
        setEnabled(s.feishu_enabled === true);
        setLarkPath(s.feishu_lark_cli_path ?? "");
        setAppToken(s.feishu_app_token ?? "");
        setTableId(s.feishu_cases_table_id ?? "");
        if ((s.feishu_app_token ?? "").trim() || (s.feishu_cases_table_id ?? "").trim()) {
          setPoolOpen(true);
        }
      })
      .catch(() => {});
  }, []);

  const markDirty = () => setDirty(true);

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      const next: Settings = {
        ...settings,
        feishu_enabled: enabled,
        feishu_lark_cli_path: larkPath.trim() || null,
        feishu_app_token: appToken.trim() || null,
        feishu_cases_table_id: tableId.trim() || null,
      };
      await saveSettings(next);
      setSettings(next);
      setDirty(false);
      toast("飞书日历配置已保存", "info");
    } catch (e) {
      toast(`保存失败:${e}`, "error");
    } finally {
      setSaving(false);
    }
  };

  // 测试连接:先存当前配置,再拉今天的飞书日历(透传真实错误,守坑#8)。
  const handleTest = async () => {
    setTesting(true);
    setTestOk(null);
    setTestMsg(null);
    try {
      if (settings) {
        const next: Settings = {
          ...settings,
          feishu_enabled: true,
          feishu_lark_cli_path: larkPath.trim() || null,
          feishu_app_token: appToken.trim() || null,
          feishu_cases_table_id: tableId.trim() || null,
        };
        await saveSettings(next);
        setSettings(next);
        setEnabled(true);
        setDirty(false);
      }
      const today = todayISO();
      const result = await testFeishuCalendarConnection(today, today);
      setDiagnostic(result);
      setTestOk(result.real_request_ok);
      setTestMsg(result.real_request_ok ? `连接成功 · 今天有 ${result.event_count ?? 0} 个日程` : result.message);
    } catch (e) {
      setTestOk(false);
      setTestMsg(String(e));
    } finally {
      setTesting(false);
    }
  };

  const handleAuthorize = async () => {
    setAuthorizing(true);
    try {
      const result = await startFeishuCalendarAuthorization();
      setAuthorization(result);
      await openUrl(result.verification_url);
      setTestOk(null);
      setTestMsg("授权页面已打开。完成授权后，请返回这里点击“我已授权，完成连接”。");
    } catch (error) {
      setTestOk(false);
      setTestMsg(`启动授权失败：${String(error)}`);
    } finally {
      setAuthorizing(false);
    }
  };

  const handleFinishAuthorization = async () => {
    if (!authorization) return;
    setAuthorizing(true);
    try {
      const result = await finishFeishuCalendarAuthorization(authorization.device_code);
      setDiagnostic(result);
      setTestOk(result.real_request_ok);
      setTestMsg(result.real_request_ok ? `授权完成 · 今天有 ${result.event_count ?? 0} 个日程` : result.message);
      if (result.real_request_ok) setAuthorization(null);
    } catch (error) {
      setTestOk(false);
      setTestMsg(`授权收尾失败：${String(error)}`);
    } finally {
      setAuthorizing(false);
    }
  };

  return (
    <div className="space-y-5">
      {/* 标题 */}
      <div className="flex items-center gap-2">
        <CalendarClock className="size-5 text-foreground" />
        <h3 className="text-base font-semibold text-foreground">飞书日历</h3>
      </div>

      {/* 依赖说明(淡蓝,诚实标明门槛) */}
      <div className="rounded-lg bg-sky-50 px-4 py-3 text-sm text-slate-700">
        <p className="font-medium text-slate-800">用前先准备:</p>
        <p className="mt-1 text-[13px] leading-relaxed text-slate-600">这里仅配置首页飞书日历。案件管理数据请在「案件同步连接」中授权，不需要开启日历。</p>
        <ol className="mt-1.5 list-decimal space-y-1 pl-5 text-[13px] leading-relaxed">
          <li>
            本机安装飞书官方 <code className="rounded bg-white px-1">lark-cli</code>，并为当前用户授权
            <code className="rounded bg-white px-1">calendar:calendar.event:read</code>。CaseBoard 只复用它的登录态，不保存你的飞书 token。
          </li>
          <li>
            macOS 自动找 Homebrew 路径;<b>Windows / Linux</b> 需把 lark-cli 加入系统 PATH,
            或在下方填它的<b>完整路径</b>。
          </li>
          <li>
            日历数据来自你<b>飞书日历应用</b>里的日程。开启并测试成功后,首页会用飞书月历
            替代本地「日程日历」卡片。
          </li>
        </ol>
      </div>

      {/* 总开关 */}
      <label className="flex cursor-pointer items-center gap-3 rounded-lg border border-border bg-card px-4 py-3">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => {
            setEnabled(e.target.checked);
            markDirty();
          }}
          className="size-4"
        />
        <div>
          <p className="text-sm font-medium text-foreground">启用飞书日历</p>
          <p className="text-xs text-muted-foreground">开启后首页显示飞书月历(蓝点=飞书日程 / 黄点=案件节点)</p>
        </div>
      </label>

      {/* lark-cli 路径(可选) */}
      <div className="space-y-1.5">
        <label className="text-sm font-medium text-foreground">
          lark-cli 路径 <span className="text-muted-foreground">(可选)</span>
        </label>
        <input
          type="text"
          value={larkPath}
          onChange={(e) => {
            setLarkPath(e.target.value);
            markDirty();
          }}
          placeholder="留空 = 自动查找。Windows 示例:C:\\Tools\\lark-cli.exe"
          className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-foreground/40"
        />
      </div>

      {/* 飞书案件总表配置 */}
      <div className="rounded-lg border border-border">
        <button
          type="button"
          onClick={() => setPoolOpen((v) => !v)}
          className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm font-medium text-foreground"
        >
          {poolOpen ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
          案件管理多维表格
        </button>
        {poolOpen && (
          <div className="space-y-3 border-t border-border px-4 py-3">
            <p className="text-xs text-muted-foreground">
              用于案件同步预演。App Token 填多维表格的 base 标识；Table ID 必须填写
              “🚩案件总表”的表标识，不要填写进度表、阶段表或当前视图的其他表标识。
            </p>
            <div className="space-y-1.5">
              <label className="text-sm text-foreground">App Token</label>
              <input
                type="text"
                value={appToken}
                onChange={(e) => {
                  setAppToken(e.target.value);
                  markDirty();
                }}
                placeholder="bascn... / 多维表格 URL 里的 app_token"
                className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-foreground/40"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-sm text-foreground">案件总表 Table ID</label>
              <input
                type="text"
                value={tableId}
                onChange={(e) => {
                  setTableId(e.target.value);
                  markDirty();
                }}
                placeholder="tbl... / 请选择“🚩案件总表”的 table_id"
                className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-foreground/40"
              />
            </div>
          </div>
        )}
      </div>

      {/* 测试结果 */}
      {testMsg && (
        <div
          className={
            testOk
              ? "flex items-start gap-2 rounded-md bg-emerald-50 px-3 py-2 text-sm text-emerald-700"
              : "flex items-start gap-2 rounded-md bg-red-50 px-3 py-2 text-sm text-red-700"
          }
        >
          {testOk ? (
            <CheckCircle2 className="mt-0.5 size-4 shrink-0" />
          ) : (
            <AlertTriangle className="mt-0.5 size-4 shrink-0" />
          )}
          <span className="break-all">{testMsg}</span>
        </div>
      )}

      {diagnostic && (
        <div className="grid gap-2 rounded-lg border border-border bg-muted/20 p-3 text-xs md:grid-cols-2">
          <p><span className="text-muted-foreground">CLI：</span>{diagnostic.cli_path}</p>
          <p><span className="text-muted-foreground">版本：</span>{diagnostic.cli_version || "未识别"}</p>
          <p><span className="text-muted-foreground">身份：</span>{diagnostic.identity || "未识别"} {diagnostic.app_id_masked ? `· ${diagnostic.app_id_masked}` : ""}</p>
          <p><span className="text-muted-foreground">用户授权：</span>{diagnostic.user_available && diagnostic.user_verified ? "有效" : "无效"}</p>
          <p><span className="text-muted-foreground">日历权限：</span>{diagnostic.scope_granted ? "已授予" : "缺失"}</p>
          <p><span className="text-muted-foreground">真实请求：</span>{diagnostic.real_request_ok ? "成功" : "失败"}</p>
        </div>
      )}

      {authorization && (
        <div className="rounded-lg border border-sky-300 bg-sky-50 p-3 text-sm text-slate-700">
          <p>请在飞书授权页确认日历只读权限{authorization.user_code ? `，验证码：${authorization.user_code}` : ""}。</p>
          <div className="mt-2 flex flex-wrap gap-2">
            <button type="button" onClick={() => void openUrl(authorization.verification_url)} className="inline-flex items-center gap-1 rounded-md border bg-white px-3 py-1.5"><ExternalLink className="size-3.5" />重新打开授权页</button>
            <button type="button" disabled={authorizing} onClick={() => void handleFinishAuthorization()} className="rounded-md bg-slate-900 px-3 py-1.5 text-white disabled:opacity-50">我已授权，完成连接</button>
          </div>
        </div>
      )}

      {/* 操作按钮 */}
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={handleTest}
          disabled={testing || saving}
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-2 text-sm font-medium text-foreground transition-colors hover:bg-accent disabled:opacity-50"
        >
          {testing ? <Loader2 className="size-4 animate-spin" /> : <CalendarClock className="size-4" />}
          测试连接
        </button>
        {testOk === false && (
          <button
            type="button"
            onClick={() => void handleAuthorize()}
            disabled={authorizing || testing || saving}
            className="inline-flex items-center gap-1.5 rounded-md border border-sky-300 bg-sky-50 px-3 py-2 text-sm font-medium text-sky-800 disabled:opacity-50"
          >
            {authorizing ? <Loader2 className="size-4 animate-spin" /> : <ExternalLink className="size-4" />}
            重新授权日历
          </button>
        )}
        <button
          type="button"
          onClick={handleSave}
          disabled={saving || testing || !dirty}
          className="inline-flex items-center gap-1.5 rounded-md bg-foreground px-3 py-2 text-sm font-medium text-background transition-colors hover:bg-foreground/90 disabled:opacity-50"
        >
          {saving ? <Loader2 className="size-4 animate-spin" /> : <Save className="size-4" />}
          保存配置
        </button>
        {dirty && <span className="text-xs text-muted-foreground">有未保存改动</span>}
      </div>
    </div>
  );
}
