import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, Eye, RefreshCw, ShieldAlert } from "lucide-react";

import { Button } from "@/components/ui/button";
import { confirmDialog } from "@/lib/dialog";
import {
  acceptMemoryCandidate,
  confirmCaseMemory,
  confirmMemoryInjection,
  createCaseMemoryDraft,
  listCaseMemories,
  listMemoryCandidates,
  listUserMemoryPreferences,
  previewMemoryInjection,
  rejectMemoryCandidate,
  reviseCaseMemory,
  setCaseMemoryStatus,
} from "@/lib/api";
import { saveConfirmedMemoryInjection } from "@/lib/memoryInjection";
import type {
  CaseMemory,
  MemoryCandidate,
  MemoryInjectionMode,
  MemoryInjectionPreview,
  MemoryType,
  MemoryVerificationStatus,
  UserMemoryPreference,
} from "@/lib/types";

const MEMORY_TYPE_LABELS: Record<MemoryType, string> = {
  fact: "案件事实",
  procedure: "程序进展",
  strategy: "办案策略",
  client_instruction: "委托人指示",
  risk_warning: "风险提示",
};

const inputClass =
  "w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20";
const cardClass = "rounded-xl border border-border bg-card p-4 shadow-sm";

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function CaseMemoryPanel({
  caseId,
  caseName,
}: {
  caseId: string;
  caseName: string;
}) {
  const [memories, setMemories] = useState<CaseMemory[]>([]);
  const [candidates, setCandidates] = useState<MemoryCandidate[]>([]);
  const [preferences, setPreferences] = useState<UserMemoryPreference[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const requestVersion = useRef(0);

  const [memoryType, setMemoryType] = useState<MemoryType>("fact");
  const [verification, setVerification] =
    useState<MemoryVerificationStatus>("unverified");
  const [injectionMode, setInjectionMode] =
    useState<MemoryInjectionMode>("archive_only");
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editContent, setEditContent] = useState("");
  const [editReason, setEditReason] = useState("");

  const [selectedMemoryIds, setSelectedMemoryIds] = useState<string[]>([]);
  const [selectedPreferenceIds, setSelectedPreferenceIds] = useState<string[]>([]);
  const [taskType, setTaskType] = useState("");
  const [preview, setPreview] = useState<MemoryInjectionPreview | null>(null);
  const [previewConfirmed, setPreviewConfirmed] = useState(false);

  const refresh = useCallback(async () => {
    const version = ++requestVersion.current;
    setLoading(true);
    setError("");
    try {
      const [nextMemories, nextCandidates, nextPreferences] = await Promise.all([
        listCaseMemories(caseId),
        listMemoryCandidates(caseId),
        listUserMemoryPreferences(),
      ]);
      if (version !== requestVersion.current) return;
      setMemories(nextMemories);
      setCandidates(nextCandidates);
      setPreferences(nextPreferences);
    } catch (e) {
      if (version === requestVersion.current) setError(errorText(e));
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, [caseId]);

  useEffect(() => {
    setSelectedMemoryIds([]);
    setSelectedPreferenceIds([]);
    setPreview(null);
    setPreviewConfirmed(false);
    void refresh();
    return () => {
      requestVersion.current += 1;
    };
  }, [refresh]);

  const selectableMemories = useMemo(
    () =>
      memories.filter(
        (item) =>
          item.status === "active" &&
          item.active_revision_no === item.current_revision_no &&
          item.injection_mode === "manual_each_turn",
      ),
    [memories],
  );
  const selectablePreferences = useMemo(
    () =>
      preferences.filter(
        (item) =>
          item.status === "active" && item.injection_mode === "manual_each_turn",
      ),
    [preferences],
  );

  function invalidatePreview() {
    setPreview(null);
    setPreviewConfirmed(false);
  }

  async function runAction(key: string, action: () => Promise<unknown>, message: string) {
    setBusy(key);
    setError("");
    setNotice("");
    try {
      await action();
      setNotice(message);
      await refresh();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }

  async function createDraft() {
    if (!title.trim() || !content.trim()) return;
    await runAction(
      "create",
      () =>
        createCaseMemoryDraft(caseId, {
          memory_type: memoryType,
          title: title.trim(),
          content: content.trim(),
          verification_status: verification,
          injection_mode: injectionMode,
          change_reason: "用户在记忆界面创建草稿",
          source: {
            source_type: "manual_assertion",
            locator: "memory-ui",
            verification_status: verification,
          },
        }),
      "记忆草稿已创建，尚未启用。",
    );
    setTitle("");
    setContent("");
  }

  async function saveRevision(item: CaseMemory) {
    if (!editTitle.trim() || !editContent.trim() || !editReason.trim()) return;
    await runAction(
      `revise-${item.id}`,
      () =>
        reviseCaseMemory(caseId, item.id, {
          expected_revision: item.current_revision_no,
          title: editTitle.trim(),
          content: editContent.trim(),
          change_reason: editReason.trim(),
          source: {
            source_type: "manual_assertion",
            locator: "memory-ui-revision",
            verification_status: "unverified",
          },
        }),
      "新修订已保存，但必须再次人工确认后才能参与逐轮注入。",
    );
    setEditingId(null);
  }

  async function makePreview() {
    setBusy("preview");
    setError("");
    setNotice("");
    try {
      const next = await previewMemoryInjection(
        caseId,
        taskType.trim() || null,
        selectedMemoryIds,
        selectedPreferenceIds,
      );
      setPreview(next);
      setPreviewConfirmed(false);
      setNotice("已生成本轮注入预览，仍未注入任何 AI 请求。");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return <div className={cardClass}>正在读取“{caseName}”的记忆…</div>;
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{caseName}</h2>
          <p className="text-sm text-muted-foreground">
            案件记忆与其他案件隔离；草稿、候选项和未确认修订均不会参与注入。
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={() => void refresh()}>
          <RefreshCw />刷新
        </Button>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive">
          {error}
        </div>
      )}
      {notice && (
        <div className="rounded-md border border-emerald-500/30 bg-emerald-500/5 p-3 text-sm">
          {notice}
        </div>
      )}

      <section className={cardClass}>
        <h3 className="font-semibold">新建案件记忆草稿</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          新建后仅为草稿。即使选择“逐轮人工注入”，也必须先确认启用，并在具体一轮中再次选择、预览和确认。
        </p>
        <div className="mt-3 grid gap-3 md:grid-cols-3">
          <select className={inputClass} value={memoryType} onChange={(e) => setMemoryType(e.target.value as MemoryType)}>
            {Object.entries(MEMORY_TYPE_LABELS).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select>
          <select className={inputClass} value={verification} onChange={(e) => setVerification(e.target.value as MemoryVerificationStatus)}>
            <option value="unverified">未核验</option>
            <option value="disputed">有争议</option>
            <option value="stale">可能过时</option>
          </select>
          <select className={inputClass} value={injectionMode} onChange={(e) => setInjectionMode(e.target.value as MemoryInjectionMode)}>
            <option value="archive_only">仅归档（默认，不注入）</option>
            <option value="manual_each_turn">允许逐轮人工选择</option>
          </select>
        </div>
        <input className={`${inputClass} mt-3`} value={title} onChange={(e) => setTitle(e.target.value)} placeholder="记忆标题" />
        <textarea className={`${inputClass} mt-3 min-h-24 resize-y`} value={content} onChange={(e) => setContent(e.target.value)} placeholder="仅写入经你判断值得保留的内容" />
        <Button className="mt-3" disabled={!title.trim() || !content.trim() || busy === "create"} onClick={() => void createDraft()}>
          创建草稿
        </Button>
      </section>

      <section className={cardClass}>
        <h3 className="font-semibold">候选记忆</h3>
        <p className="mt-1 text-xs text-muted-foreground">AI 只能提出候选项；接受后仍是草稿，不会直接启用。</p>
        <div className="mt-3 space-y-3">
          {candidates.filter((item) => item.status === "pending").map((item) => (
            <div key={item.id} className="rounded-lg border border-border p-3">
              <div className="font-medium">{item.proposed_title}</div>
              <div className="mt-1 whitespace-pre-wrap text-sm text-muted-foreground">{item.proposed_content}</div>
              <div className="mt-3 flex gap-2">
                <Button size="sm" disabled={busy !== null} onClick={() => void runAction(
                  `accept-${item.id}`,
                  () => acceptMemoryCandidate(caseId, item.id, {
                    title: item.proposed_title,
                    content: item.proposed_content,
                    memory_type: item.proposed_type,
                    verification_status: "unverified",
                  }),
                  "候选项已接受为草稿，尚未启用。",
                )}>接受为草稿</Button>
                <Button variant="outline" size="sm" disabled={busy !== null} onClick={async () => {
                  const ok = await confirmDialog("拒绝后该候选项将不再显示为待处理，是否继续？", { danger: true, okLabel: "拒绝" });
                  if (ok) void runAction(`reject-${item.id}`, () => rejectMemoryCandidate(caseId, item.id, "用户在记忆界面拒绝"), "候选项已拒绝。");
                }}>拒绝</Button>
              </div>
            </div>
          ))}
          {!candidates.some((item) => item.status === "pending") && <p className="text-sm text-muted-foreground">暂无待处理候选项。</p>}
        </div>
      </section>

      <section className={cardClass}>
        <h3 className="font-semibold">案件记忆</h3>
        <div className="mt-3 space-y-3">
          {memories.filter((item) => item.status !== "deleted").map((item) => {
            const revisionPending = item.active_revision_no !== item.current_revision_no;
            return (
              <article key={item.id} className="rounded-lg border border-border p-3">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div>
                    <div className="font-medium">{item.title}</div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      {MEMORY_TYPE_LABELS[item.memory_type]} · {item.status} · {item.verification_status} ·
                      {item.injection_mode === "archive_only" ? " 仅归档" : " 逐轮人工选择"} · 修订 {item.current_revision_no}
                    </div>
                  </div>
                  {revisionPending && <span className="rounded-full bg-amber-500/10 px-2 py-1 text-xs text-amber-700">待人工确认</span>}
                </div>
                <div className="mt-2 whitespace-pre-wrap text-sm">{item.content}</div>
                {editingId === item.id ? (
                  <div className="mt-3 space-y-2 rounded-lg bg-muted/40 p-3">
                    <input className={inputClass} value={editTitle} onChange={(e) => setEditTitle(e.target.value)} />
                    <textarea className={`${inputClass} min-h-24`} value={editContent} onChange={(e) => setEditContent(e.target.value)} />
                    <input className={inputClass} value={editReason} onChange={(e) => setEditReason(e.target.value)} placeholder="必填：本次修订原因" />
                    <div className="flex gap-2">
                      <Button size="sm" disabled={!editReason.trim() || busy !== null} onClick={() => void saveRevision(item)}>保存为待确认修订</Button>
                      <Button variant="ghost" size="sm" onClick={() => setEditingId(null)}>取消</Button>
                    </div>
                  </div>
                ) : (
                  <div className="mt-3 flex flex-wrap gap-2">
                    {(item.status === "draft" || revisionPending || item.status === "disabled") && (
                      <Button size="sm" disabled={busy !== null} onClick={() => void runAction(
                        `confirm-${item.id}`,
                        () => confirmCaseMemory(caseId, item.id, item.current_revision_no),
                        "该修订已由用户确认启用。",
                      )}><CheckCircle2 />确认启用</Button>
                    )}
                    <Button variant="outline" size="sm" onClick={() => {
                      setEditingId(item.id);
                      setEditTitle(item.title);
                      setEditContent(item.content);
                      setEditReason("");
                    }}>修订</Button>
                    {item.status === "active" && (
                      <Button variant="outline" size="sm" disabled={busy !== null} onClick={async () => {
                        const ok = await confirmDialog("停用后该记忆不会出现在新的注入预览中，是否继续？", { danger: true, okLabel: "停用" });
                        if (ok) void runAction(`disable-${item.id}`, () => setCaseMemoryStatus(caseId, item.id, "disabled", "用户在记忆界面停用"), "记忆已停用。");
                      }}>停用</Button>
                    )}
                    <Button variant="outline" size="sm" disabled={busy !== null} onClick={async () => {
                      const ok = await confirmDialog(
                        "删除采用可审计软删除；删除后不会再显示或参与注入。是否继续？",
                        { danger: true, okLabel: "删除" },
                      );
                      if (ok) {
                        void runAction(
                          `delete-${item.id}`,
                          () => setCaseMemoryStatus(caseId, item.id, "deleted", "用户在记忆界面软删除"),
                          "记忆已软删除并保留审计记录。",
                        );
                      }
                    }}>删除</Button>
                  </div>
                )}
              </article>
            );
          })}
          {!memories.length && <p className="text-sm text-muted-foreground">暂无案件记忆。</p>}
        </div>
      </section>

      <section className={`${cardClass} border-amber-500/40`}>
        <div className="flex items-start gap-2">
          <ShieldAlert className="mt-0.5 size-5 shrink-0 text-amber-600" />
          <div>
            <h3 className="font-semibold">本轮注入预览</h3>
            <p className="mt-1 text-xs text-muted-foreground">
              默认没有任何选项。勾选只用于生成预览；预览确认也不会自动发送给模型。
            </p>
          </div>
        </div>
        <input className={`${inputClass} mt-3`} value={taskType} onChange={(e) => { setTaskType(e.target.value); invalidatePreview(); }} placeholder="本轮任务类型（可选，例如：庭前准备）" />
        <div className="mt-3 grid gap-4 md:grid-cols-2">
          <div>
            <div className="text-sm font-medium">案件记忆</div>
            {selectableMemories.map((item) => (
              <label key={item.id} className="mt-2 flex items-start gap-2 text-sm">
                <input type="checkbox" className="mt-1" checked={selectedMemoryIds.includes(item.id)} onChange={(e) => {
                  setSelectedMemoryIds((current) => e.target.checked ? [...current, item.id] : current.filter((id) => id !== item.id));
                  invalidatePreview();
                }} />
                <span>{item.title}</span>
              </label>
            ))}
            {!selectableMemories.length && <p className="mt-2 text-xs text-muted-foreground">没有已确认且允许逐轮选择的案件记忆。</p>}
          </div>
          <div>
            <div className="text-sm font-medium">全局偏好</div>
            {selectablePreferences.map((item) => (
              <label key={item.id} className="mt-2 flex items-start gap-2 text-sm">
                <input type="checkbox" className="mt-1" checked={selectedPreferenceIds.includes(item.id)} onChange={(e) => {
                  setSelectedPreferenceIds((current) => e.target.checked ? [...current, item.id] : current.filter((id) => id !== item.id));
                  invalidatePreview();
                }} />
                <span>{item.title}</span>
              </label>
            ))}
            {!selectablePreferences.length && <p className="mt-2 text-xs text-muted-foreground">没有已确认且允许逐轮选择的全局偏好。</p>}
          </div>
        </div>
        <Button className="mt-4" variant="outline" disabled={busy !== null || (!selectedMemoryIds.length && !selectedPreferenceIds.length)} onClick={() => void makePreview()}>
          <Eye />生成预览（不注入）
        </Button>
        {preview && (
          <div className="mt-4 rounded-lg border border-border bg-muted/30 p-3">
            <div className="text-xs text-muted-foreground">
              案件记忆 {preview.case_used_chars} 字 · 全局偏好 {preview.preference_used_chars} 字
            </div>
            <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap text-xs">{preview.prompt_markdown}</pre>
            <Button className="mt-3" size="sm" disabled={busy !== null || previewConfirmed} onClick={async () => {
              setBusy("confirm-preview");
              setError("");
              try {
                await confirmMemoryInjection(caseId, preview.id, preview.preview_sha256);
                saveConfirmedMemoryInjection(caseId, {
                  runId: preview.id,
                  previewSha256: preview.preview_sha256,
                });
                setPreviewConfirmed(true);
                setNotice("本轮预览已确认；下一次向本案 AI 发送消息时使用，且仅使用一次。");
              } catch (e) {
                setError(errorText(e));
              } finally {
                setBusy(null);
              }
            }}>{previewConfirmed ? "本轮预览已确认" : "确认本轮预览（仍不自动发送）"}</Button>
          </div>
        )}
      </section>
    </div>
  );
}
