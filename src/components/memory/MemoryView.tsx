import { useEffect, useState } from "react";
import { BookLock, Database, ShieldCheck, UserRoundCog } from "lucide-react";

import { CaseMemoryPanel } from "@/components/memory/CaseMemoryPanel";
import { Button } from "@/components/ui/button";
import {
  confirmUserMemoryPreference,
  createUserMemoryPreference,
  listUserMemoryPreferences,
} from "@/lib/api";
import { getCaseDisplayName } from "@/lib/caseIdentity";
import { MEMORY_CAPABILITIES } from "@/lib/memoryCapabilities";
import type {
  Case,
  MemoryInjectionMode,
  UserMemoryPreference,
} from "@/lib/types";

type Section = "rules" | "preferences" | "case";
const inputClass =
  "w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20";

const RULES = [
  ["规则优先级", "系统安全规则高于 Skills、记忆和用户偏好；记忆不能覆盖系统约束。"],
  ["案件隔离", "案件记忆只属于所选案件，不跨案件检索、展示或注入。"],
  ["人工确认", "AI 只能提出候选项；草稿、候选项和未确认修订均不可启用。"],
  ["逐轮门禁", "默认仅归档、不注入。每一轮必须重新选择、生成预览并人工确认。"],
  ["证据边界", "记忆用于工作连续性，不替代原始材料、证据核验和律师专业判断。"],
] as const;

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function MemoryView({
  cases,
  initialCaseId,
}: {
  cases: Case[];
  initialCaseId?: string | null;
}) {
  const [section, setSection] = useState<Section>("rules");
  const [caseId, setCaseId] = useState(
    initialCaseId && cases.some((item) => item.id === initialCaseId)
      ? initialCaseId
      : "",
  );
  const [preferences, setPreferences] = useState<UserMemoryPreference[]>([]);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [mode, setMode] = useState<MemoryInjectionMode>("archive_only");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function loadPreferences() {
    try {
      setPreferences(await listUserMemoryPreferences());
    } catch (e) {
      setError(errorText(e));
    }
  }

  useEffect(() => {
    void loadPreferences();
  }, []);

  async function createPreference() {
    if (!title.trim() || !content.trim()) return;
    setBusy(true);
    setError("");
    try {
      await createUserMemoryPreference({
        title: title.trim(),
        content: content.trim(),
        injection_mode: mode,
      });
      setTitle("");
      setContent("");
      await loadPreferences();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  const selectedCase = cases.find((item) => item.id === caseId);

  return (
    <div className="h-full overflow-auto bg-background">
      <main className="mx-auto max-w-6xl space-y-5 px-5 py-6 lg:px-8">
        <header>
          <div className="flex items-center gap-2">
            <Database className="size-6" />
            <h1 className="text-2xl font-semibold">记忆</h1>
          </div>
          <p className="mt-2 text-sm text-muted-foreground">
            管理可审计、可停用、按案件隔离的工作记忆。任何记忆默认都不会静默注入 AI。
          </p>
          <div className="mt-3 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3 text-sm">
            <strong>人工门禁：</strong>启用不等于注入；每一轮都要重新选择、预览并确认。
          </div>
        </header>

        <nav className="flex flex-wrap gap-2">
          {([
            ["rules", "系统规则"],
            ["preferences", "全局偏好"],
            ["case", "按案件查看"],
          ] as const).map(([id, label]) => (
            <Button key={id} variant={section === id ? "default" : "outline"} onClick={() => setSection(id)}>
              {label}
            </Button>
          ))}
        </nav>

        {error && <div className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive">{error}</div>}

        {section === "rules" && (
          <section className="space-y-4">
            <div className="grid gap-3 md:grid-cols-2">
              {RULES.map(([title, content], index) => {
                const Icon = index === 0 ? ShieldCheck : BookLock;
                return (
                  <article key={title} className="rounded-xl border border-border bg-card p-4 shadow-sm">
                    <div className="flex items-center gap-2 font-semibold"><Icon className="size-4" />{title}</div>
                    <p className="mt-2 text-sm text-muted-foreground">{content}</p>
                  </article>
                );
              })}
            </div>
            <article className="rounded-xl border border-border bg-card p-4 shadow-sm">
              <h2 className="font-semibold">AI 入口记忆支持范围</h2>
              <p className="mt-1 text-xs text-muted-foreground">
                未明确支持的入口不会静默读取或注入记忆。
              </p>
              <div className="mt-3 divide-y divide-border">
                {MEMORY_CAPABILITIES.map((entry) => (
                  <div key={entry.id} className="flex flex-wrap items-start justify-between gap-3 py-3">
                    <div>
                      <div className="text-sm font-medium">{entry.label}</div>
                      <div className="mt-1 text-xs text-muted-foreground">{entry.behavior}</div>
                    </div>
                    <span
                      className={
                        entry.supported
                          ? "rounded-full bg-emerald-500/10 px-2 py-1 text-xs text-emerald-700"
                          : "rounded-full bg-muted px-2 py-1 text-xs text-muted-foreground"
                      }
                    >
                      {entry.supported ? "支持逐轮人工注入" : "不支持记忆"}
                    </span>
                  </div>
                ))}
              </div>
            </article>
          </section>
        )}

        {section === "preferences" && (
          <section className="space-y-4">
            <div className="rounded-xl border border-border bg-card p-4 shadow-sm">
              <div className="flex items-center gap-2 font-semibold"><UserRoundCog className="size-4" />新建全局偏好草稿</div>
              <p className="mt-1 text-xs text-muted-foreground">偏好应描述长期稳定的工作方式，不应写入具体案件事实。</p>
              <input className={`${inputClass} mt-3`} value={title} onChange={(e) => setTitle(e.target.value)} placeholder="偏好标题" />
              <textarea className={`${inputClass} mt-3 min-h-24`} value={content} onChange={(e) => setContent(e.target.value)} placeholder="例如：输出先列结论，再列依据与风险" />
              <select className={`${inputClass} mt-3`} value={mode} onChange={(e) => setMode(e.target.value as MemoryInjectionMode)}>
                <option value="archive_only">仅归档（默认，不注入）</option>
                <option value="manual_each_turn">允许逐轮人工选择</option>
              </select>
              <Button className="mt-3" disabled={busy || !title.trim() || !content.trim()} onClick={() => void createPreference()}>创建草稿</Button>
            </div>
            <div className="space-y-3">
              {preferences.filter((item) => item.status !== "deleted").map((item) => (
                <article key={item.id} className="rounded-xl border border-border bg-card p-4 shadow-sm">
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div>
                      <div className="font-medium">{item.title}</div>
                      <div className="mt-1 text-xs text-muted-foreground">
                        {item.status} · {item.injection_mode === "archive_only" ? "仅归档" : "逐轮人工选择"} · 修订 {item.current_revision_no}
                      </div>
                    </div>
                    {item.status !== "active" && (
                      <Button size="sm" disabled={busy} onClick={async () => {
                        setBusy(true);
                        setError("");
                        try {
                          await confirmUserMemoryPreference(item.id, item.current_revision_no);
                          await loadPreferences();
                        } catch (e) {
                          setError(errorText(e));
                        } finally {
                          setBusy(false);
                        }
                      }}>确认启用</Button>
                    )}
                  </div>
                  <p className="mt-2 whitespace-pre-wrap text-sm">{item.content}</p>
                </article>
              ))}
              {!preferences.length && <p className="text-sm text-muted-foreground">暂无全局偏好。</p>}
            </div>
          </section>
        )}

        {section === "case" && (
          <section className="space-y-4">
            <div className="rounded-xl border border-border bg-card p-4 shadow-sm">
              <label className="text-sm font-medium" htmlFor="memory-case">选择案件</label>
              <select id="memory-case" className={`${inputClass} mt-2`} value={caseId} onChange={(e) => setCaseId(e.target.value)}>
                <option value="">请明确选择一个案件</option>
                {cases.map((item) => <option key={item.id} value={item.id}>{getCaseDisplayName(item)}</option>)}
              </select>
            </div>
            {selectedCase ? (
              <CaseMemoryPanel key={selectedCase.id} caseId={selectedCase.id} caseName={getCaseDisplayName(selectedCase)} />
            ) : (
              <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-muted-foreground">
                选择案件后才会读取该案件的记忆，避免误看或误操作其他案件。
              </div>
            )}
          </section>
        )}
      </main>
    </div>
  );
}
