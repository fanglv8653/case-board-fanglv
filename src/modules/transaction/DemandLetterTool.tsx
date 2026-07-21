import { useEffect, useMemo, useState } from "react";
import { save as dialogSave } from "@tauri-apps/plugin-dialog";
import { CalendarPlus, Download, Loader2, WandSparkles } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  exportDemandLetterDocx,
  generateDemandLetter,
  listCases,
  upsertCaseStageItem,
  type DemandLetterDraft,
  type DemandLetterInput,
} from "@/lib/api";
import type { Case } from "@/lib/types";

type ExportMode = "draft" | "final";

const initialInput: DemandLetterInput = {
  letter_type: "履行催告函",
  sender: "",
  recipient: "",
  relationship: "",
  facts: "",
  demands: "",
  deadline: "",
  tone: "克制、专业",
  evidence_note: "",
  legal_basis_note: "",
};

function formatError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

export function DemandLetterTool() {
  const [input, setInput] = useState(initialInput);
  const [draft, setDraft] = useState<DemandLetterDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [factsConfirmed, setFactsConfirmed] = useState(false);
  const [sourcesVerified, setSourcesVerified] = useState(false);
  const [lawyerConfirmed, setLawyerConfirmed] = useState(false);
  const [exportMode, setExportMode] = useState<ExportMode>("draft");
  const [cases, setCases] = useState<Case[]>([]);
  const [caseId, setCaseId] = useState("");
  const [reminderDate, setReminderDate] = useState("");

  const formalReady = factsConfirmed && sourcesVerified && lawyerConfirmed;
  const selectedCase = useMemo(() => cases.find((item) => item.id === caseId), [cases, caseId]);

  useEffect(() => {
    listCases()
      .then((items) => setCases(items))
      .catch((reason) => console.warn("load local cases for demand reminder failed", reason));
  }, []);

  function patch<K extends keyof DemandLetterInput>(key: K, value: DemandLetterInput[K]) {
    setInput((current) => ({ ...current, [key]: value }));
  }

  async function generate() {
    setError(null);
    setMessage("");
    setBusy(true);
    setDraft(null);
    setFactsConfirmed(false);
    setSourcesVerified(false);
    setLawyerConfirmed(false);
    setExportMode("draft");
    try {
      setDraft(await generateDemandLetter(input));
    } catch (reason) {
      setError(formatError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function exportDocx() {
    if (!draft) return;
    setError(null);
    const suffix = exportMode === "final" ? "正式稿" : "工作稿";
    const path = await dialogSave({
      defaultPath: `${draft.title || "律师函"}_${suffix}.docx`,
      filters: [{ name: "Word 文档", extensions: ["docx"] }],
    });
    if (typeof path !== "string" || !path.trim()) return;
    setBusy(true);
    try {
      await exportDemandLetterDocx(
        draft.title,
        draft.draft_md,
        exportMode,
        factsConfirmed,
        sourcesVerified,
        lawyerConfirmed,
        path,
      );
      setMessage(`律师函${suffix}已导出。应用不会自动发送函件。`);
    } catch (reason) {
      setError(formatError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function createReminder() {
    if (!caseId || !reminderDate || !draft) return;
    setError(null);
    setBusy(true);
    try {
      await upsertCaseStageItem({
        case_id: caseId,
        domain: selectedCase?.legal_domain || "other",
        major_stage: "律师函跟进",
        stage_label: `${draft.title || "律师函"}回复/履行期限`,
        status: "pending",
        due_at: reminderDate,
        reminder_at: reminderDate,
        source: "local",
        notes: "由非诉律师函工具人工创建，仅保存到本地案件；未写入飞书。",
      });
      setMessage("本地案件提醒已创建，未写入飞书。");
    } catch (reason) {
      setError(formatError(reason));
    } finally {
      setBusy(false);
    }
  }

  const fieldClass =
    "w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-sky-400";

  return (
    <div className="space-y-5">
      <section className="rounded-lg border border-sky-200/70 bg-sky-50/50 p-4 text-sm leading-relaxed dark:border-sky-900/40 dark:bg-sky-950/15">
        这里只生成非诉律师函工作稿。系统不会自动发送，也不会写入飞书；事实、法源和最终措辞必须由执业律师复核。
      </section>

      <section className="space-y-4 rounded-lg border border-border bg-card p-4">
        <div>
          <h3 className="text-sm font-medium">核心信息</h3>
          <p className="mt-1 text-xs text-muted-foreground">先填写四项必填内容，其余信息可按需补充。</p>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="space-y-1 text-xs text-muted-foreground">
            <span>函件类型</span>
            <select value={input.letter_type} onChange={(e) => patch("letter_type", e.target.value)} className={fieldClass}>
              <option>履行催告函</option>
              <option>催款函</option>
              <option>停止侵权函</option>
              <option>回复函</option>
              <option>证据保全提示函</option>
              <option>其他律师函</option>
            </select>
          </label>
          <label className="space-y-1 text-xs text-muted-foreground">
            <span>语气</span>
            <select value={input.tone} onChange={(e) => patch("tone", e.target.value)} className={fieldClass}>
              <option>克制、专业</option>
              <option>明确、坚定</option>
              <option>保留协商空间</option>
            </select>
          </label>
          <label className="space-y-1 text-xs text-muted-foreground">
            <span>发函方 *</span>
            <input value={input.sender} onChange={(e) => patch("sender", e.target.value)} className={fieldClass} />
          </label>
          <label className="space-y-1 text-xs text-muted-foreground">
            <span>收函方 *</span>
            <input value={input.recipient} onChange={(e) => patch("recipient", e.target.value)} className={fieldClass} />
          </label>
          <label className="space-y-1 text-xs text-muted-foreground sm:col-span-2">
            <span>基本事实 *</span>
            <textarea rows={4} value={input.facts} onChange={(e) => patch("facts", e.target.value)} className={fieldClass} placeholder="按时间顺序写明已确认事实；不确定内容请明确标注。" />
          </label>
          <label className="space-y-1 text-xs text-muted-foreground sm:col-span-2">
            <span>具体要求 *</span>
            <textarea rows={3} value={input.demands} onChange={(e) => patch("demands", e.target.value)} className={fieldClass} placeholder="例如支付金额、履行事项、停止行为或书面回复。" />
          </label>
        </div>

        <details className="rounded-md border border-border bg-muted/15 px-3 py-2">
          <summary className="cursor-pointer text-xs font-medium">补充信息（可选）</summary>
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            <input value={input.relationship} onChange={(e) => patch("relationship", e.target.value)} className={fieldClass} placeholder="双方关系/合同背景" />
            <input value={input.deadline} onChange={(e) => patch("deadline", e.target.value)} className={fieldClass} placeholder="履行期限，如收到后 7 日内" />
            <textarea rows={2} value={input.evidence_note} onChange={(e) => patch("evidence_note", e.target.value)} className={`${fieldClass} sm:col-span-2`} placeholder="证据和附件说明" />
            <textarea rows={2} value={input.legal_basis_note} onChange={(e) => patch("legal_basis_note", e.target.value)} className={`${fieldClass} sm:col-span-2`} placeholder="已经人工核验的法律依据；未核验可留空" />
          </div>
        </details>

        <Button onClick={generate} disabled={busy}>
          {busy ? <Loader2 className="size-4 animate-spin" /> : <WandSparkles className="size-4" />}
          生成工作稿
        </Button>
      </section>

      {error && <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">{error}</div>}
      {message && <div className="rounded-md border border-emerald-200 bg-emerald-50/60 px-3 py-2 text-xs text-emerald-700 dark:border-emerald-900/40 dark:bg-emerald-950/20 dark:text-emerald-300">{message}</div>}

      {draft && (
        <>
          <section className="space-y-3 rounded-lg border border-border bg-card p-4">
            <div className="flex items-center justify-between gap-3">
              <h3 className="text-sm font-semibold">{draft.title}</h3>
              <span className="rounded bg-amber-50 px-2 py-1 text-caption text-amber-700 dark:bg-amber-950/30 dark:text-amber-300">待律师复核</span>
            </div>
            {draft.missing_items.length > 0 && <p className="text-xs text-amber-700 dark:text-amber-300">待补充：{draft.missing_items.join("；")}</p>}
            {draft.risk_notes.length > 0 && <p className="text-xs text-red-700 dark:text-red-300">发出前注意：{draft.risk_notes.join("；")}</p>}
            <pre className="max-h-[520px] overflow-auto whitespace-pre-wrap rounded-md bg-muted/30 p-4 font-sans text-sm leading-7 text-foreground">{draft.draft_md}</pre>
          </section>

          <section className="space-y-3 rounded-lg border border-border bg-card p-4">
            <h3 className="text-sm font-medium">律师审核与导出</h3>
            <div className="grid gap-2 text-xs sm:grid-cols-3">
              <label className="flex items-center gap-2"><input type="checkbox" checked={factsConfirmed} onChange={(e) => setFactsConfirmed(e.target.checked)} />事实材料已核对</label>
              <label className="flex items-center gap-2"><input type="checkbox" checked={sourcesVerified} onChange={(e) => setSourcesVerified(e.target.checked)} />法源有效性已核验</label>
              <label className="flex items-center gap-2"><input type="checkbox" checked={lawyerConfirmed} onChange={(e) => setLawyerConfirmed(e.target.checked)} />执业律师已审核</label>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" variant={exportMode === "draft" ? "default" : "outline"} onClick={() => setExportMode("draft")}>工作稿</Button>
              <Button size="sm" variant={exportMode === "final" ? "default" : "outline"} disabled={!formalReady} onClick={() => setExportMode("final")}>正式稿</Button>
              <Button size="sm" variant="outline" onClick={exportDocx} disabled={busy}>
                <Download className="size-3.5" />导出 {exportMode === "final" ? "正式稿" : "工作稿"} DOCX
              </Button>
            </div>
          </section>

          <section className="space-y-3 rounded-lg border border-border bg-card p-4">
            <div>
              <h3 className="text-sm font-medium">可选：创建本地案件提醒</h3>
              <p className="mt-1 text-xs text-muted-foreground">只有点击创建后才写本地案件阶段记录，不写飞书。</p>
            </div>
            <div className="grid gap-2 sm:grid-cols-[1fr_180px_auto]">
              <select value={caseId} onChange={(e) => setCaseId(e.target.value)} className={fieldClass}>
                <option value="">选择本地案件</option>
                {cases.map((item) => <option key={item.id} value={item.id}>{item.display_name_override || item.name}</option>)}
              </select>
              <input type="date" value={reminderDate} onChange={(e) => setReminderDate(e.target.value)} className={fieldClass} />
              <Button size="sm" variant="outline" onClick={createReminder} disabled={!caseId || !reminderDate || busy}>
                <CalendarPlus className="size-3.5" />创建提醒
              </Button>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
