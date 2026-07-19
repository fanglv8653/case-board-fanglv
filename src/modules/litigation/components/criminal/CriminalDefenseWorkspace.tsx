import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  BookOpenCheck,
  Bot,
  CheckCircle2,
  ChevronRight,
  FileSearch,
  FileText,
  Link2,
  Loader2,
  RefreshCw,
  Scale,
  ShieldCheck,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import type { Document } from "@/lib/types";
import { cn } from "@/lib/utils";
import { save } from "@tauri-apps/plugin-dialog";
import { asWorkspaceError, loadCriminalWorkspace, workspaceCommands } from "./criminalWorkspaceApi";
import type {
  CriminalAnalysisFinding,
  CriminalDraftDocument,
  CriminalEvidenceItem,
  CriminalIssue,
  CriminalReviewNote,
  FindingType,
  ReviewDecision,
  ReviewStatus,
  WorkspaceCitation,
  WorkspaceRecordBase,
} from "./criminalWorkspaceTypes";
import {
  FINDING_LABELS,
  WORKSPACE_ZONES,
  availableProviders,
  canConfirmFinding,
  citationLocation,
  confirmedSelectionIds,
  reviewLabel,
  type WorkspaceZone,
} from "./criminalWorkspaceViewModel";

const DOCUMENT_TYPES = [
  ["defense_statement", "辩护词"],
  ["evidence_objection", "质证意见"],
  ["hearing_questions", "庭审发问提纲"],
  ["first_meeting_record", "首次会见笔录"],
  ["followup_meeting_record", "后续会见笔录"],
  ["bail_application", "取保候审申请书"],
  ["non_arrest_opinion", "不批准逮捕法律意见书"],
  ["custody_necessity_application", "羁押必要性审查申请书"],
  ["prosecution_legal_opinion", "审查起诉法律意见书"],
  ["sentencing_opinion", "量刑意见"],
  ["criminal_appeal", "刑事上诉状"],
  ["evidence_list", "证据目录"],
  ["other", "其他文书"],
] as const;

const EMPTY_CAPABILITIES = {
  manual: true as const,
  native_llm: { available: false, reason: "正在探测" },
  codex: { available: false, experimental: true, reason: "正在探测" },
};
type EvidenceForm = { name: string; evidence_type: string; proof_purpose: string; source_description: string; authenticity: string; legality: string; relevance: string; admissibility: string; probative_force: string; corroboration: string; exclusion_clue: string; reasonable_doubt: string };
const EMPTY_EVIDENCE_FORM: EvidenceForm = { name: "", evidence_type: "", proof_purpose: "", source_description: "", authenticity: "", legality: "", relevance: "", admissibility: "", probative_force: "", corroboration: "", exclusion_clue: "", reasonable_doubt: "" };
type AssessmentKey = "authenticity" | "legality" | "relevance" | "admissibility" | "probative_force" | "corroboration" | "exclusion_clue" | "reasonable_doubt";
type AssessmentStatus = "supported" | "doubtful" | "adverse" | "not_reviewed";
const EMPTY_ASSESSMENT_STATUSES: Record<AssessmentKey, AssessmentStatus> = { authenticity: "not_reviewed", legality: "not_reviewed", relevance: "not_reviewed", admissibility: "not_reviewed", probative_force: "not_reviewed", corroboration: "not_reviewed", exclusion_clue: "not_reviewed", reasonable_doubt: "not_reviewed" };

export function CriminalDefenseWorkspace({
  caseId,
  documents,
  onOpenDocument,
  onRecognizeMaterials,
  recognizingMaterials,
  taskPanel,
}: {
  caseId: string;
  documents: Document[];
  onOpenDocument?: (document: Document, page?: number | null) => void;
  onRecognizeMaterials: () => Promise<void>;
  recognizingMaterials: boolean;
  taskPanel: ReactNode;
}) {
  const [zone, setZone] = useState<WorkspaceZone>("materials");
  const [bundle, setBundle] = useState<Awaited<ReturnType<typeof loadCriminalWorkspace>> | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [noteForm, setNoteForm] = useState<{ title: string; content: string; note_type: "general" | "fact" | "question" | "contradiction" | "todo"; document_id: string }>({ title: "", content: "", note_type: "general", document_id: "" });
  const [evidenceForm, setEvidenceForm] = useState<EvidenceForm>(EMPTY_EVIDENCE_FORM);
  const [evidenceAssessmentStatuses, setEvidenceAssessmentStatuses] = useState(EMPTY_ASSESSMENT_STATUSES);
  const [issueForm, setIssueForm] = useState<{ neutral_title: string; issue_type: string; description: string; position: "neutral" | "prosecution" | "defense" }>({ neutral_title: "", issue_type: "element", description: "", position: "neutral" });
  const [findingForm, setFindingForm] = useState({ title: "", content: "", finding_type: "material_fact" as FindingType });
  const [draftForm, setDraftForm] = useState({ draft_id: "", title: "", document_type: "defense_statement", content: "" });
  const [provider, setProvider] = useState<"manual" | "native_llm" | "codex">("manual");
  const [selectedDocumentIds, setSelectedDocumentIds] = useState<Set<string>>(new Set());
  const [selectedEvidenceIds, setSelectedEvidenceIds] = useState<Set<string>>(new Set());
  const [selectedIssueIds, setSelectedIssueIds] = useState<Set<string>>(new Set());
  const [selectedFindingIds, setSelectedFindingIds] = useState<Set<string>>(new Set());
  const [citationTarget, setCitationTarget] = useState<{ owner_type: "review_note" | "evidence" | "finding" | "draft_version"; owner_id: string; label: string } | null>(null);
  const [citationForm, setCitationForm] = useState({ citation_kind: "material" as "material" | "legal" | "user_statement", document_id: "", page_start: "", page_end: "", locator: "", excerpt: "", legal_title: "", legal_article: "", legal_url: "", verification_status: "unchecked" as "unchecked" | "verified" | "cannot_verify" });

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      setBundle(await loadCriminalWorkspace(caseId));
    } catch (cause) {
      const error = asWorkspaceError(cause);
      toast(`五区工作台加载失败：${error.message}`, "error");
    } finally {
      setLoading(false);
    }
  }, [caseId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const summary = bundle?.summary;
  const zoneBadges = useMemo(() => ({
    materials: countPending(bundle?.review_notes),
    evidence: countPending(bundle?.evidence_items) + countPending(bundle?.issues),
    analysis: countPending(bundle?.findings),
    drafting: (bundle?.drafts ?? []).reduce((sum, draft) => sum + (draft.versions ?? []).filter((v) => v.status === "pending_review").length, 0),
    tasks: summary?.open_tasks ?? 0,
  }), [bundle, summary]);

  const runAction = async (key: string, action: () => Promise<unknown>, success: string) => {
    setBusy(key);
    try {
      await action();
      toast(success, "success");
      await reload();
    } catch (cause) {
      const error = asWorkspaceError(cause);
      const message = error.code === "REVISION_CONFLICT"
        ? "该记录已在其他窗口更新，已重新加载，请核对后重试。"
        : error.message;
      toast(`${success.replace(/^已/, "").replace(/成功$/, "")}失败：${message}`, "error");
      if (error.code === "REVISION_CONFLICT") await reload();
    } finally {
      setBusy(null);
    }
  };

  const review = (kind: "note" | "evidence" | "issue" | "finding", row: WorkspaceRecordBase, decision: ReviewDecision) => {
    const command = kind === "note" ? workspaceCommands.reviewReviewNote
      : kind === "evidence" ? workspaceCommands.reviewEvidence
        : kind === "issue" ? workspaceCommands.reviewIssue
          : workspaceCommands.reviewFinding;
    return runAction(`review-${row.id}`, () => command(row.id, decision, row.revision), decision === "confirm" ? "已由律师确认" : decision === "reject" ? "已拒绝并保留记录" : "已重新打开复核");
  };

  const createNote = () => runAction("create-note", async () => {
    if (!noteForm.title.trim() || !noteForm.content.trim()) throw new Error("请填写笔记标题和内容");
    await workspaceCommands.upsertReviewNote({ case_id: caseId, ...noteForm, document_id: noteForm.document_id || null, author_type: "user", review_status: "draft" });
    setNoteForm({ title: "", content: "", note_type: "general", document_id: "" });
  }, "阅卷笔记已保存");

  const createEvidence = () => runAction("create-evidence", async () => {
    if (!evidenceForm.name.trim()) throw new Error("请填写证据名称");
    await workspaceCommands.upsertEvidence({ case_id: caseId, name: evidenceForm.name, evidence_type: evidenceForm.evidence_type, proof_purpose: evidenceForm.proof_purpose, source_description: evidenceForm.source_description, authenticity_assessment_json: assessmentJson(evidenceAssessmentStatuses.authenticity, evidenceForm.authenticity), legality_assessment_json: assessmentJson(evidenceAssessmentStatuses.legality, evidenceForm.legality), relevance_assessment_json: assessmentJson(evidenceAssessmentStatuses.relevance, evidenceForm.relevance), admissibility_assessment_json: assessmentJson(evidenceAssessmentStatuses.admissibility, evidenceForm.admissibility), probative_force_assessment_json: assessmentJson(evidenceAssessmentStatuses.probative_force, evidenceForm.probative_force), corroboration_assessment_json: assessmentJson(evidenceAssessmentStatuses.corroboration, evidenceForm.corroboration), exclusion_clue_assessment_json: assessmentJson(evidenceAssessmentStatuses.exclusion_clue, evidenceForm.exclusion_clue), reasonable_doubt_impact_json: assessmentJson(evidenceAssessmentStatuses.reasonable_doubt, evidenceForm.reasonable_doubt), origin: "user" });
    setEvidenceForm(EMPTY_EVIDENCE_FORM);
    setEvidenceAssessmentStatuses(EMPTY_ASSESSMENT_STATUSES);
  }, "证据卡片已保存，待律师复核");

  const createIssue = () => runAction("create-issue", async () => {
    if (!issueForm.neutral_title.trim()) throw new Error("请填写中性争点标题");
    await workspaceCommands.upsertIssue({ case_id: caseId, ...issueForm, status: "open", origin: "user" });
    setIssueForm({ neutral_title: "", issue_type: "element", description: "", position: "neutral" });
  }, "争点已保存，待律师复核");

  const createFinding = () => runAction("create-finding", async () => {
    if (!findingForm.title.trim() || !findingForm.content.trim()) throw new Error("请填写分析标题和内容");
    await workspaceCommands.upsertFinding({ case_id: caseId, ...findingForm, origin: "user" });
    setFindingForm({ title: "", content: "", finding_type: "material_fact" });
  }, "分析记录已保存，待律师复核");

  const startAnalysis = () => runAction("start-analysis", async () => {
    const run = await workspaceCommands.startAnalysis({
      case_id: caseId,
      request_id: crypto.randomUUID(),
      template_code: "criminal_defense_five_layer",
      requested_provider: provider,
      input_snapshot_json: JSON.stringify({
        document_ids: [...selectedDocumentIds],
        evidence_ids: [...selectedEvidenceIds],
        issue_ids: [...selectedIssueIds],
      }),
      allow_fallback: true,
    });
    if (run.actual_provider !== provider && run.fallback_reason) {
      toast(`增强能力不可用，已回落至${providerLabel(run.actual_provider)}：${run.fallback_reason}`, "info");
    }
  }, "分析任务已建立，结果均需律师逐项复核");

  const createDraft = () => runAction("create-draft", async () => {
    if (!draftForm.title.trim() || !draftForm.content.trim()) throw new Error("请填写文书标题和正文");
    const evidenceIds = confirmedSelectionIds(bundle?.evidence_items ?? [], selectedEvidenceIds);
    const findingIds = confirmedSelectionIds(bundle?.findings ?? [], selectedFindingIds);
    const draft = draftForm.draft_id
      ? (bundle?.drafts ?? []).find((row) => row.id === draftForm.draft_id)
      : await workspaceCommands.createDraft({ case_id: caseId, document_type: draftForm.document_type, title: draftForm.title, created_by: "本机律师" });
    if (!draft) throw new Error("待修订文书不存在，请刷新后重试");
    await workspaceCommands.createDraftVersion({
      draft_id: draft.id,
      rendered_markdown: draftForm.content,
      content_json: JSON.stringify({ markdown: draftForm.content }),
      origin: "user",
      source_snapshot_json: JSON.stringify({ evidence_ids: evidenceIds, finding_ids: findingIds }),
    });
    setDraftForm({ draft_id: "", title: "", document_type: "defense_statement", content: "" });
  }, "文书工作稿已保存");

  const createCitation = () => runAction("create-citation", async () => {
    if (!citationTarget) throw new Error("请先选择需要引用的成果");
    await workspaceCommands.upsertCitation({
      case_id: caseId,
      owner_type: citationTarget.owner_type,
      owner_id: citationTarget.owner_id,
      citation_kind: citationForm.citation_kind,
      document_id: citationForm.citation_kind === "material" ? citationForm.document_id || null : null,
      page_start: citationForm.page_start ? Number(citationForm.page_start) : null,
      page_end: citationForm.page_end ? Number(citationForm.page_end) : null,
      locator_json: citationForm.locator ? JSON.stringify({ description: citationForm.locator }) : "{}",
      location_precision: citationForm.page_start ? "exact" : "approximate",
      excerpt: citationForm.excerpt,
      legal_title: citationForm.legal_title || undefined,
      legal_article: citationForm.legal_article || undefined,
      legal_url: citationForm.legal_url || undefined,
      verification_status: citationForm.verification_status,
    });
    setCitationTarget(null);
    setCitationForm({ citation_kind: "material", document_id: "", page_start: "", page_end: "", locator: "", excerpt: "", legal_title: "", legal_article: "", legal_url: "", verification_status: "unchecked" });
  }, "来源引用已保存");

  const openCitation = (citation: WorkspaceCitation) => {
    const document = documents.find((doc) => doc.id === citation.document_id);
    if (document && citation.integrity_status === "valid" && onOpenDocument) {
      onOpenDocument(document, citation.page_start);
      return;
    }
    toast(`来源当前无法定位；保留摘录快照：${citation.excerpt || "（无摘录）"}`, "error");
  };

  const capabilities = bundle?.capabilities ?? EMPTY_CAPABILITIES;
  const providers = availableProviders(capabilities);

  return (
    <section className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-5 py-4">
        <div>
          <div className="flex items-center gap-2">
            <ShieldCheck className="size-5" />
            <h2 className="text-lg font-semibold tracking-tight">个人刑辩五区工作台</h2>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">材料—争点—分析—文书—任务全程留痕；模型结果默认待律师复核。</p>
        </div>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {(summary?.invalid_citations ?? 0) > 0 && <RiskBadge>{summary?.invalid_citations} 条来源失效</RiskBadge>}
          {(summary?.pending_review ?? 0) > 0 && <PendingBadge>{summary?.pending_review} 项待复核</PendingBadge>}
          <Button type="button" variant="ghost" size="sm" onClick={() => void reload()} disabled={loading}>
            <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />刷新
          </Button>
        </div>
      </div>

      <nav className="grid grid-cols-2 border-b border-border bg-muted/20 md:grid-cols-5" aria-label="刑辩工作台分区">
        {WORKSPACE_ZONES.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => setZone(item.id)}
            className={cn("relative px-3 py-3 text-sm transition-colors hover:bg-muted/50", zone === item.id ? "bg-background font-semibold text-foreground" : "text-muted-foreground")}
            aria-current={zone === item.id ? "page" : undefined}
          >
            {item.label}
            {zoneBadges[item.id] > 0 && <span className="ml-1.5 rounded-full bg-amber-500/15 px-1.5 py-0.5 text-[10px] text-amber-800 dark:text-amber-200">{zoneBadges[item.id]}</span>}
            {zone === item.id && <span className="absolute inset-x-3 bottom-0 h-0.5 bg-foreground" />}
          </button>
        ))}
      </nav>

      <div className="p-5">
        {citationTarget && <CitationEditor target={citationTarget.label} documents={documents} form={citationForm} setForm={setCitationForm} onSave={createCitation} onCancel={() => setCitationTarget(null)} busy={busy === "create-citation"} />}
        {loading && !bundle ? <Loading /> : null}
        {!loading || bundle ? (
          <>
            {zone === "materials" && (
              <MaterialsZone
                documents={documents}
                notes={bundle?.review_notes ?? []}
                form={noteForm}
                setForm={setNoteForm}
                onCreate={createNote}
                onReview={(row, decision) => void review("note", row, decision)}
                busy={busy}
                onOpenDocument={(doc) => onOpenDocument?.(doc)}
                onOpenCitation={openCitation}
                onRecognizeMaterials={onRecognizeMaterials}
                recognizingMaterials={recognizingMaterials}
                onCreateEvidence={(note) => {
                  setEvidenceForm((current) => ({ ...current, name: note.title, source_description: note.content }));
                  setZone("evidence");
                }}
                onAddCitation={(note) => setCitationTarget({ owner_type: "review_note", owner_id: note.id, label: note.title })}
                selectedDocumentIds={selectedDocumentIds}
                onToggleDocument={(id) => setSelectedDocumentIds(toggleSet(selectedDocumentIds, id))}
              />
            )}
            {zone === "evidence" && (
              <EvidenceZone
                evidence={bundle?.evidence_items ?? []}
                issues={bundle?.issues ?? []}
                issueLinks={bundle?.issue_evidence_links ?? []}
                evidenceForm={evidenceForm}
                setEvidenceForm={setEvidenceForm}
                assessmentStatuses={evidenceAssessmentStatuses}
                setAssessmentStatuses={setEvidenceAssessmentStatuses}
                issueForm={issueForm}
                setIssueForm={setIssueForm}
                onCreateEvidence={createEvidence}
                onCreateIssue={createIssue}
                onReviewEvidence={(row, decision) => void review("evidence", row, decision)}
                onReviewIssue={(row, decision) => void review("issue", row, decision)}
                busy={busy}
                onOpenCitation={openCitation}
                onAnalyze={(issue) => {
                  setFindingForm({ title: issue.neutral_title, content: issue.description, finding_type: "analysis" });
                  setZone("analysis");
                }}
                selectedEvidenceIds={selectedEvidenceIds}
                selectedIssueIds={selectedIssueIds}
                onToggleEvidence={(id) => setSelectedEvidenceIds(toggleSet(selectedEvidenceIds, id))}
                onToggleIssue={(id) => setSelectedIssueIds(toggleSet(selectedIssueIds, id))}
                onAddCitation={(row) => setCitationTarget({ owner_type: "evidence", owner_id: row.id, label: row.name })}
                caseId={caseId}
                runAction={runAction}
              />
            )}
            {zone === "analysis" && (
              <AnalysisZone
                findings={bundle?.findings ?? []}
                runs={bundle?.analysis_runs ?? []}
                form={findingForm}
                setForm={setFindingForm}
                providers={providers}
                provider={provider}
                setProvider={setProvider}
                codexReason={capabilities.codex.reason}
                nativeReason={capabilities.native_llm.reason}
                onStart={startAnalysis}
                onCreate={createFinding}
                onReview={(row, decision) => void review("finding", row, decision)}
                busy={busy}
                onOpenCitation={openCitation}
                onDraft={(finding) => {
                setDraftForm((current) => ({ ...current, content: `${current.content}${current.content ? "\n\n" : ""}## ${finding.title}\n\n${finding.content}` }));
                  setZone("drafting");
                }}
                selectedFindingIds={selectedFindingIds}
                onToggleFinding={(id) => setSelectedFindingIds(toggleSet(selectedFindingIds, id))}
                onAddCitation={(row) => setCitationTarget({ owner_type: "finding", owner_id: row.id, label: row.title })}
              />
            )}
            {zone === "drafting" && (
              <DraftingZone drafts={bundle?.drafts ?? []} evidence={bundle?.evidence_items ?? []} selectedEvidenceIds={selectedEvidenceIds} onToggleEvidence={(id) => setSelectedEvidenceIds(toggleSet(selectedEvidenceIds, id))} selectedFindingCount={confirmedSelectionIds(bundle?.findings ?? [], selectedFindingIds).length} form={draftForm} setForm={setDraftForm} onCreate={createDraft} busy={busy} runAction={runAction} onAddCitation={(version, title) => setCitationTarget({ owner_type: "draft_version", owner_id: version.id, label: title })} />
            )}
            {zone === "tasks" && (
              <div className="space-y-4">
                <SectionIntro icon={<CheckCircle2 className="size-4" />} title="流程任务区" description="复用现有刑事 SOP、期限与工作记录；关联产物不会自动完成任务。" />
                {taskPanel}
                <TaskArtifactLinker caseId={caseId} bundle={bundle} runAction={runAction} />
              </div>
            )}
          </>
        ) : null}
      </div>
    </section>
  );
}

function MaterialsZone(props: {
  documents: Document[]; notes: CriminalReviewNote[]; form: { title: string; content: string; note_type: "general" | "fact" | "question" | "contradiction" | "todo"; document_id: string };
  setForm: React.Dispatch<React.SetStateAction<{ title: string; content: string; note_type: "general" | "fact" | "question" | "contradiction" | "todo"; document_id: string }>>;
  onCreate: () => void; onReview: (row: CriminalReviewNote, decision: ReviewDecision) => void; busy: string | null;
  onOpenDocument: (doc: Document) => void; onOpenCitation: (citation: WorkspaceCitation) => void; onRecognizeMaterials: () => Promise<void>;
  recognizingMaterials: boolean; onCreateEvidence: (note: CriminalReviewNote) => void;
  selectedDocumentIds: Set<string>; onToggleDocument: (id: string) => void;
  onAddCitation: (note: CriminalReviewNote) => void;
}) {
  return <div className="space-y-5">
    <SectionIntro icon={<FileSearch className="size-4" />} title="材料阅卷区" description="识别状态、文档定位、页码引用和人工阅卷笔记集中处理；重新识别不会覆盖人工笔记。" action={<Button size="sm" variant="outline" onClick={() => void props.onRecognizeMaterials()} disabled={props.recognizingMaterials}>{props.recognizingMaterials ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}重新识别材料</Button>} />
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.75fr)]">
      <div className="space-y-2">
        <Subheading>材料清单</Subheading>
        {props.documents.length ? props.documents.map((doc) => <div key={doc.id} className="flex w-full items-center gap-2 rounded-lg border border-border p-3 hover:bg-muted/40"><input type="checkbox" checked={props.selectedDocumentIds.has(doc.id)} onChange={() => props.onToggleDocument(doc.id)} aria-label={`选择材料 ${doc.filename}`} /><button type="button" onClick={() => props.onOpenDocument(doc)} className="flex min-w-0 flex-1 items-center justify-between gap-3 text-left"><span className="min-w-0"><span className="block truncate text-sm font-medium">{doc.filename}</span><span className="text-xs text-muted-foreground">{doc.category || "未分类"} · {doc.extraction_status || "未识别"}</span></span>{doc.missing ? <RiskBadge>来源失效</RiskBadge> : doc.extraction_status === "done" ? <StatusBadge status="confirmed" /> : <PendingBadge>需人工复核</PendingBadge>}</button></div>) : <Empty text="尚无案件材料；手工阅卷与模板仍可使用。" />}
      </div>
      <EditorCard title="新增阅卷笔记">
        <Input value={props.form.title} onChange={(value) => props.setForm((f) => ({ ...f, title: value }))} placeholder="笔记标题" />
        <div className="grid grid-cols-2 gap-2"><Select value={props.form.note_type} onChange={(value) => props.setForm((f) => ({ ...f, note_type: value as typeof f.note_type }))} options={[["general", "一般笔记"], ["fact", "事实摘录"], ["question", "待核问题"], ["contradiction", "矛盾线索"], ["todo", "待办"]]} /><Select value={props.form.document_id} onChange={(value) => props.setForm((f) => ({ ...f, document_id: value }))} options={[["", "不绑定材料"], ...props.documents.map((doc) => [doc.id, doc.filename])]} /></div>
        <Textarea value={props.form.content} onChange={(value) => props.setForm((f) => ({ ...f, content: value }))} placeholder="记录材料事实、疑问或矛盾；确定事实请在保存后补充页码引用。" />
        <Button size="sm" onClick={props.onCreate} disabled={props.busy === "create-note"}>{props.busy === "create-note" && <Loader2 className="size-3.5 animate-spin" />}保存笔记</Button>
      </EditorCard>
    </div>
    <RecordList empty="暂无阅卷笔记。">{props.notes.map((row) => <RecordCard key={row.id} title={row.title} status={row.review_status} origin={row.author_type} description={row.content} citations={row.citations} onOpenCitation={props.onOpenCitation} actions={<><Button size="sm" variant="ghost" onClick={() => props.onAddCitation(row)}><Link2 className="size-3" />添加来源</Button><Button size="sm" variant="ghost" onClick={() => props.onCreateEvidence(row)}>转为证据卡<ChevronRight className="size-3" /></Button><ReviewActions row={row} busy={props.busy} onReview={(decision) => props.onReview(row, decision)} /></>} />)}</RecordList>
  </div>;
}

function EvidenceZone(props: {
  caseId: string; evidence: CriminalEvidenceItem[]; issues: CriminalIssue[]; issueLinks: Awaited<ReturnType<typeof loadCriminalWorkspace>>["issue_evidence_links"]; evidenceForm: EvidenceForm;
  setEvidenceForm: React.Dispatch<React.SetStateAction<EvidenceForm>>; assessmentStatuses: Record<AssessmentKey, AssessmentStatus>; setAssessmentStatuses: React.Dispatch<React.SetStateAction<Record<AssessmentKey, AssessmentStatus>>>;
  issueForm: { neutral_title: string; issue_type: string; description: string; position: "neutral" | "prosecution" | "defense" };
  setIssueForm: React.Dispatch<React.SetStateAction<{ neutral_title: string; issue_type: string; description: string; position: "neutral" | "prosecution" | "defense" }>>;
  onCreateEvidence: () => void; onCreateIssue: () => void; onReviewEvidence: (row: CriminalEvidenceItem, decision: ReviewDecision) => void;
  onReviewIssue: (row: CriminalIssue, decision: ReviewDecision) => void; busy: string | null; onOpenCitation: (citation: WorkspaceCitation) => void; onAnalyze: (issue: CriminalIssue) => void;
  selectedEvidenceIds: Set<string>; selectedIssueIds: Set<string>; onToggleEvidence: (id: string) => void; onToggleIssue: (id: string) => void;
  onAddCitation: (row: CriminalEvidenceItem) => void;
  runAction: (key: string, action: () => Promise<unknown>, success: string) => Promise<void>;
}) {
  const [linkForm, setLinkForm] = useState({ issue_id: "", evidence_id: "", relation: "supports" as "supports" | "contradicts" | "weakens" | "contextual" | "gap", explanation: "" });
  const saveLink = () => {
    if (!linkForm.issue_id) { toast("请选择争点", "error"); return; }
    void props.runAction("save-issue-link", () => workspaceCommands.upsertIssueEvidenceLink({ case_id: props.caseId, issue_id: linkForm.issue_id, evidence_id: linkForm.relation === "gap" ? null : linkForm.evidence_id || null, relation: linkForm.relation, explanation: linkForm.explanation, origin: "user" }), "证据与争点关系已保存，待律师复核");
  };
  return <div className="space-y-5">
    <SectionIntro icon={<Scale className="size-4" />} title="证据争点区" description="按刑事证据能力、证明力、排非线索和合理怀疑审查，不使用民事“高度盖然性”替代刑事证明标准。" />
    <div className="grid gap-4 lg:grid-cols-2">
      <EditorCard title="新增证据卡"><Input value={props.evidenceForm.name} onChange={(v) => props.setEvidenceForm((f) => ({ ...f, name: v }))} placeholder="证据名称" /><div className="grid grid-cols-2 gap-2"><Input value={props.evidenceForm.evidence_type} onChange={(v) => props.setEvidenceForm((f) => ({ ...f, evidence_type: v }))} placeholder="证据类型" /><Input value={props.evidenceForm.proof_purpose} onChange={(v) => props.setEvidenceForm((f) => ({ ...f, proof_purpose: v }))} placeholder="证明目的" /></div><Textarea value={props.evidenceForm.source_description} onChange={(v) => props.setEvidenceForm((f) => ({ ...f, source_description: v }))} placeholder="来源说明" /><div className="grid gap-2 md:grid-cols-2">{([['authenticity','真实性'],['legality','合法性'],['relevance','关联性'],['admissibility','证据能力'],['probative_force','证明力'],['corroboration','印证关系'],['exclusion_clue','排非线索'],['reasonable_doubt','合理怀疑影响']] as const).map(([key,label]) => <label key={key} className="space-y-1"><span className="text-xs text-muted-foreground">{label}</span><Select value={props.assessmentStatuses[key]} onChange={(v) => props.setAssessmentStatuses((current) => ({ ...current, [key]: v as AssessmentStatus }))} options={[["not_reviewed", "未审查"], ["supported", "支持"], ["doubtful", "存疑"], ["adverse", "不利"]]} /><Textarea rows={2} value={props.evidenceForm[key]} onChange={(v) => props.setEvidenceForm((f) => ({ ...f, [key]: v }))} placeholder={`记录${label}理由；存疑项保留待律师判断`} /></label>)}</div><Button size="sm" onClick={props.onCreateEvidence}>保存并提交复核</Button></EditorCard>
      <EditorCard title="新增控辩争点"><Input value={props.issueForm.neutral_title} onChange={(v) => props.setIssueForm((f) => ({ ...f, neutral_title: v }))} placeholder="中性争点标题" /><div className="grid grid-cols-2 gap-2"><Select value={props.issueForm.issue_type} onChange={(v) => props.setIssueForm((f) => ({ ...f, issue_type: v }))} options={[["fact", "事实争点"], ["element", "构成要件"], ["procedure", "程序争点"], ["evidence_conflict", "证据矛盾"], ["evidence_gap", "证据缺口"], ["sentencing", "量刑争点"], ["other", "其他"]]} /><Select value={props.issueForm.position} onChange={(v) => props.setIssueForm((f) => ({ ...f, position: v as typeof f.position }))} options={[["neutral", "中性"], ["prosecution", "控方"], ["defense", "辩方"]]} /></div><Textarea value={props.issueForm.description} onChange={(v) => props.setIssueForm((f) => ({ ...f, description: v }))} placeholder="控辩观点、矛盾、构成要件对应和证据缺口" /><Button size="sm" onClick={props.onCreateIssue}>保存并提交复核</Button></EditorCard>
    </div>
    <div className="grid gap-4 lg:grid-cols-2"><div><Subheading>证据卡片</Subheading><RecordList empty="暂无证据卡片。">{props.evidence.map((row) => <RecordCard key={row.id} selected={props.selectedEvidenceIds.has(row.id)} onToggleSelected={() => props.onToggleEvidence(row.id)} title={row.name} description={`${row.evidence_type || "未分类"} · ${row.proof_purpose || "未填写证明目的"}\n${row.source_description || ""}`} status={row.review_status} origin={row.origin} citations={row.citations} onOpenCitation={props.onOpenCitation} actions={<><Button size="sm" variant="ghost" onClick={() => props.onAddCitation(row)}><Link2 className="size-3" />添加来源</Button><ReviewActions row={row} busy={props.busy} onReview={(decision) => props.onReviewEvidence(row, decision)} /></>} />)}</RecordList></div><div><Subheading>争点与证据缺口</Subheading><RecordList empty="暂无争点。">{props.issues.map((row) => <RecordCard key={row.id} selected={props.selectedIssueIds.has(row.id)} onToggleSelected={() => props.onToggleIssue(row.id)} title={row.neutral_title} description={`${row.issue_type} · ${row.position}\n${row.description}`} status={row.review_status} origin={row.origin} citations={row.citations} onOpenCitation={props.onOpenCitation} actions={<><Button size="sm" variant="ghost" onClick={() => props.onAnalyze(row)}>进入分析<ChevronRight className="size-3" /></Button><ReviewActions row={row} busy={props.busy} onReview={(decision) => props.onReviewIssue(row, decision)} /></>} />)}</RecordList></div></div>
    <EditorCard title="建立证据—争点关系 / 证据缺口"><div className="grid gap-2 md:grid-cols-3"><Select value={linkForm.issue_id} onChange={(v) => setLinkForm((f) => ({ ...f, issue_id: v }))} options={[["", "选择争点"], ...props.issues.map((row) => [row.id, row.neutral_title])]} /><Select value={linkForm.relation} onChange={(v) => setLinkForm((f) => ({ ...f, relation: v as typeof f.relation, evidence_id: v === "gap" ? "" : f.evidence_id }))} options={[["supports", "支持"], ["contradicts", "矛盾"], ["weakens", "削弱"], ["contextual", "背景"], ["gap", "证据缺口"]]} /><Select value={linkForm.evidence_id} onChange={(v) => setLinkForm((f) => ({ ...f, evidence_id: v }))} options={[["", linkForm.relation === "gap" ? "缺口无需证据" : "选择证据"], ...props.evidence.map((row) => [row.id, row.name])]} /></div><Textarea rows={2} value={linkForm.explanation} onChange={(v) => setLinkForm((f) => ({ ...f, explanation: v }))} placeholder="说明该证据如何支持、矛盾、削弱争点，或具体缺少什么证据" /><Button size="sm" onClick={saveLink}>保存关系</Button><RecordList empty="暂无证据—争点关系。">{props.issueLinks.map((link) => { const issue = props.issues.find((row) => row.id === link.issue_id); const evidence = props.evidence.find((row) => row.id === link.evidence_id); return <RecordCard key={link.id} title={`${issue?.neutral_title ?? "争点"} · ${relationLabel(link.relation)}`} description={link.relation === "gap" ? `证据缺口：${link.explanation}` : `${evidence?.name ?? "证据"}：${link.explanation}`} status={link.review_status} origin={link.origin} actions={<ReviewActions row={link} busy={props.busy} onReview={(decision) => void props.runAction(`review-${link.id}`, () => workspaceCommands.reviewIssueEvidenceLink(link.id, decision, link.revision), decision === "confirm" ? "关系已由律师确认" : "关系审核状态已更新")} />} />; })}</RecordList></EditorCard>
  </div>;
}

function AnalysisZone(props: {
  findings: CriminalAnalysisFinding[]; runs: Awaited<ReturnType<typeof loadCriminalWorkspace>>["analysis_runs"]; form: { title: string; content: string; finding_type: FindingType }; setForm: React.Dispatch<React.SetStateAction<{ title: string; content: string; finding_type: FindingType }>>;
  providers: Array<{ id: string; label: string; experimental?: boolean }>; provider: "manual" | "native_llm" | "codex"; setProvider: (provider: "manual" | "native_llm" | "codex") => void;
  codexReason: string | null; nativeReason: string | null; onStart: () => void; onCreate: () => void; onReview: (row: CriminalAnalysisFinding, decision: ReviewDecision) => void;
  busy: string | null; onOpenCitation: (citation: WorkspaceCitation) => void; onDraft: (finding: CriminalAnalysisFinding) => void;
  selectedFindingIds: Set<string>; onToggleFinding: (id: string) => void;
  onAddCitation: (row: CriminalAnalysisFinding) => void;
}) {
  return <div className="space-y-5">
    <SectionIntro icon={<BookOpenCheck className="size-4" />} title="案件分析区" description="严格区分材料事实、待核实事实、法律依据、分析判断和辩护策略；技术运行成功不等于律师确认。" />
    <div className="rounded-lg border border-border bg-muted/20 p-4"><div className="flex flex-wrap items-end gap-3"><label className="min-w-52 flex-1 space-y-1"><span className="text-xs font-medium">分析方式</span><Select value={props.provider} onChange={(value) => props.setProvider(value as typeof props.provider)} options={props.providers.map((item) => [item.id, `${item.label}${item.experimental ? "（实验性）" : ""}`])} /></label><Button onClick={props.onStart} disabled={props.busy === "start-analysis"}>{props.busy === "start-analysis" ? <Loader2 className="size-3.5 animate-spin" /> : <Bot className="size-3.5" />}生成结构化分析</Button></div>{!props.providers.some((item) => item.id === "codex") && <p className="mt-2 text-xs text-muted-foreground">Codex 增强入口已隐藏：{props.codexReason || "能力探测未通过"}。手工模板完整可用。</p>}{!props.providers.some((item) => item.id === "native_llm") && props.nativeReason && <p className="mt-1 text-xs text-muted-foreground">原生模型不可用：{props.nativeReason}</p>}</div>
    {props.runs.length > 0 && <div><Subheading>分析运行记录</Subheading><div className="space-y-2">{props.runs.map((run) => <div key={run.id} className="rounded-lg border border-border bg-background p-3 text-xs"><div className="flex flex-wrap items-center gap-2"><strong>{run.template_code}</strong><StatusBadge status={run.status} /><span className="text-muted-foreground">请求 {providerLabel(run.requested_provider)} → 实际 {providerLabel(run.actual_provider)}</span></div>{run.fallback_reason && <p className="mt-1 text-amber-700 dark:text-amber-300">降级原因：{run.fallback_reason}</p>}{run.error_message && <p className="mt-1 text-rose-700 dark:text-rose-300">{run.error_code}：{run.error_message}</p>}<p className="mt-1 text-muted-foreground">request_id: {run.request_id}</p></div>)}</div></div>}
    <EditorCard title="人工新增分析记录"><div className="grid gap-2 md:grid-cols-[200px_1fr]"><Select value={props.form.finding_type} onChange={(v) => props.setForm((f) => ({ ...f, finding_type: v as FindingType }))} options={Object.entries(FINDING_LABELS)} /><Input value={props.form.title} onChange={(v) => props.setForm((f) => ({ ...f, title: v }))} placeholder="分析标题" /></div><Textarea value={props.form.content} onChange={(v) => props.setForm((f) => ({ ...f, content: v }))} placeholder="以中性事实单元为基础，记录三阶层、共犯/形态/错误/竞合、证据缺口、排非线索、量刑和下一步动作。" /><Button size="sm" onClick={props.onCreate}>保存并提交复核</Button></EditorCard>
    <div className="grid gap-4 lg:grid-cols-2">{Object.entries(FINDING_LABELS).map(([type, label]) => <div key={type}><Subheading>{label}</Subheading><RecordList empty={`暂无${label}。`}>{props.findings.filter((row) => row.finding_type === type).map((row) => <RecordCard key={row.id} selected={props.selectedFindingIds.has(row.id)} onToggleSelected={() => props.onToggleFinding(row.id)} title={row.title} description={row.content} status={row.review_status} origin={row.origin} citations={row.citations} onOpenCitation={props.onOpenCitation} warning={row.finding_type === "unverified_fact" ? "持续标记为待核实" : row.finding_type === "material_fact" && !canConfirmFinding(row.finding_type, row.citations) ? "缺少有效材料引用，不能确认" : undefined} actions={<><Button size="sm" variant="ghost" onClick={() => props.onAddCitation(row)}><Link2 className="size-3" />添加来源</Button><Button size="sm" variant="ghost" disabled={row.review_status !== "confirmed"} onClick={() => props.onDraft(row)}>加入文书<ChevronRight className="size-3" /></Button><ReviewActions row={row} busy={props.busy} canConfirm={canConfirmFinding(row.finding_type, row.citations)} onReview={(decision) => props.onReview(row, decision)} /></>} />)}</RecordList></div>)}</div>
  </div>;
}

function DraftingZone(props: {
  drafts: CriminalDraftDocument[]; evidence: CriminalEvidenceItem[]; selectedEvidenceIds: Set<string>; onToggleEvidence: (id: string) => void; selectedFindingCount: number; form: { draft_id: string; title: string; document_type: string; content: string }; setForm: React.Dispatch<React.SetStateAction<{ draft_id: string; title: string; document_type: string; content: string }>>;
  onCreate: () => void; busy: string | null; runAction: (key: string, action: () => Promise<unknown>, success: string) => Promise<void>;
  onAddCitation: (version: NonNullable<ReturnType<typeof latestVersion>>, title: string) => void;
}) {
  const exportVersion = async (version: NonNullable<ReturnType<typeof latestVersion>>) => {
    const mode = version.status === "approved" ? "formal" as const : "working" as const;
    const outputPath = await save({ title: mode === "formal" ? "导出正式文书" : "导出待审核工作稿", filters: [{ name: "Markdown 文书", extensions: ["md"] }] });
    if (!outputPath) return;
    await props.runAction(`export-${version.id}`, () => workspaceCommands.exportDraft(version.id, mode, outputPath), mode === "formal" ? "正式文书已导出" : "带待审核标识的工作稿已导出");
  };
  return <div className="space-y-5">
    <SectionIntro icon={<FileText className="size-4" />} title="文书草拟区" description="只将已确认事实、证据和已核验法律依据写入确定事实段落；正式导出必须通过服务端质量门禁。" />
    <EditorCard title="选择本版本来源"><p className="text-xs text-muted-foreground">仅已由律师确认的证据可以勾选；案件分析区已勾选的已确认结论将一并进入版本来源快照（当前 {props.selectedFindingCount} 项）。</p><div className="grid gap-2 md:grid-cols-2">{props.evidence.length ? props.evidence.map((row) => <label key={row.id} className={cn("flex items-start gap-2 rounded-md border border-border bg-background px-3 py-2 text-sm", row.review_status !== "confirmed" && "opacity-50")}><input type="checkbox" checked={props.selectedEvidenceIds.has(row.id) && row.review_status === "confirmed"} disabled={row.review_status !== "confirmed"} onChange={() => props.onToggleEvidence(row.id)} className="mt-1" /><span><span className="block font-medium">{row.name}</span><span className="text-xs text-muted-foreground">{row.review_status === "confirmed" ? "已确认，可进入文书" : "待律师确认，不得进入文书"}</span></span></label>) : <Empty text="暂无证据卡片。可先到证据争点区建立并确认。" />}</div></EditorCard>
    <EditorCard title={props.form.draft_id ? "新建修订版本" : "新建文书工作稿"}><div className="grid gap-2 md:grid-cols-[240px_1fr]"><Select value={props.form.document_type} onChange={(v) => props.setForm((f) => ({ ...f, document_type: v }))} options={DOCUMENT_TYPES.map((row) => [...row])} /><Input value={props.form.title} onChange={(v) => props.setForm((f) => ({ ...f, title: v }))} placeholder="文书标题" /></div><Textarea rows={10} value={props.form.content} onChange={(v) => props.setForm((f) => ({ ...f, content: v }))} placeholder="可使用原生结构化模板起草。未确认事实必须置于“待核实清单”，不得写成确定事实。" /><div className="flex gap-2"><Button onClick={props.onCreate} disabled={props.busy === "create-draft"}>{props.busy === "create-draft" && <Loader2 className="size-3.5 animate-spin" />}{props.form.draft_id ? "保存新版本" : "保存工作稿"}</Button>{props.form.draft_id && <Button variant="ghost" onClick={() => props.setForm({ draft_id: "", title: "", document_type: "defense_statement", content: "" })}>取消修订</Button>}</div></EditorCard>
    <RecordList empty="暂无文书草稿。">{props.drafts.map((draft) => { const version = latestVersion(draft); return <RecordCard key={draft.id} title={draft.title} description={`${documentTypeLabel(draft.document_type)} · ${version ? `第 ${version.version_no} 版` : "尚无版本"}`} status={version?.status ?? "draft"} origin={version?.origin ?? "user"} citations={version?.citations} actions={version ? <><Button size="sm" variant="ghost" onClick={() => props.onAddCitation(version, draft.title)}><Link2 className="size-3" />添加来源</Button><Button size="sm" variant="ghost" onClick={() => props.setForm({ draft_id: draft.id, title: draft.title, document_type: draft.document_type, content: version.rendered_markdown })}>新建修订版</Button><Button size="sm" variant="ghost" disabled={version.status !== "draft"} onClick={() => void props.runAction(`submit-${version.id}`, () => workspaceCommands.submitDraft(version.id, version.revision), "已提交律师审核")}>提交审核</Button><Button size="sm" variant="ghost" disabled={version.status !== "pending_review"} onClick={() => { const note = window.prompt("填写退回修改意见") ?? ""; if (note.trim()) void props.runAction(`return-${version.id}`, () => workspaceCommands.returnDraft(version.id, version.revision, note), "文书已退回修改"); }}>退回修改</Button><Button size="sm" variant="outline" disabled={version.status !== "pending_review"} onClick={() => void props.runAction(`approve-${version.id}`, () => workspaceCommands.approveDraft(version.id, version.revision), "文书版本已批准")}>律师批准</Button><Button size="sm" variant="ghost" onClick={() => void exportVersion(version)}>导出{version.status === "approved" ? "正式版" : "工作稿"}</Button><Button size="sm" variant="ghost" onClick={() => void props.runAction(`archive-${draft.id}`, () => workspaceCommands.archiveDraft(draft.id, draft.revision), "文书已归档")}>归档</Button></> : undefined} />; })}</RecordList>
  </div>;
}

function CitationEditor({ target, documents, form, setForm, onSave, onCancel, busy }: {
  target: string; documents: Document[];
  form: { citation_kind: "material" | "legal" | "user_statement"; document_id: string; page_start: string; page_end: string; locator: string; excerpt: string; legal_title: string; legal_article: string; legal_url: string; verification_status: "unchecked" | "verified" | "cannot_verify" };
  setForm: React.Dispatch<React.SetStateAction<{ citation_kind: "material" | "legal" | "user_statement"; document_id: string; page_start: string; page_end: string; locator: string; excerpt: string; legal_title: string; legal_article: string; legal_url: string; verification_status: "unchecked" | "verified" | "cannot_verify" }>>;
  onSave: () => void; onCancel: () => void; busy: boolean;
}) {
  return <div className="mb-5 space-y-3 rounded-lg border border-sky-500/30 bg-sky-500/5 p-4"><div className="flex items-center justify-between"><div><h3 className="text-sm font-semibold">为“{target}”添加来源引用</h3><p className="text-xs text-muted-foreground">材料引用必须提供页码或近似定位；法源只有核验正式原文后才可标为已核验。</p></div><Button size="sm" variant="ghost" onClick={onCancel}>取消</Button></div><div className="grid gap-2 md:grid-cols-3"><Select value={form.citation_kind} onChange={(v) => setForm((f) => ({ ...f, citation_kind: v as typeof f.citation_kind }))} options={[["material", "案件材料"], ["legal", "法律依据"], ["user_statement", "当事人陈述"]]} />{form.citation_kind === "material" && <><Select value={form.document_id} onChange={(v) => setForm((f) => ({ ...f, document_id: v }))} options={[["", "选择材料"], ...documents.map((doc) => [doc.id, doc.filename])]} /><div className="grid grid-cols-2 gap-2"><Input value={form.page_start} onChange={(v) => setForm((f) => ({ ...f, page_start: v.replace(/\D/g, "") }))} placeholder="起始页" /><Input value={form.page_end} onChange={(v) => setForm((f) => ({ ...f, page_end: v.replace(/\D/g, "") }))} placeholder="结束页" /></div></>}{form.citation_kind === "legal" && <><Input value={form.legal_title} onChange={(v) => setForm((f) => ({ ...f, legal_title: v }))} placeholder="法源标题" /><Input value={form.legal_article} onChange={(v) => setForm((f) => ({ ...f, legal_article: v }))} placeholder="条号" /><Input value={form.legal_url} onChange={(v) => setForm((f) => ({ ...f, legal_url: v }))} placeholder="正式原文 URL" /><Select value={form.verification_status} onChange={(v) => setForm((f) => ({ ...f, verification_status: v as typeof f.verification_status }))} options={[["unchecked", "待核验"], ["verified", "已核验正式原文"], ["cannot_verify", "无法核验"]]} /></>}{form.citation_kind === "user_statement" && <Input value={form.locator} onChange={(v) => setForm((f) => ({ ...f, locator: v }))} placeholder="陈述人及记录时间" />}</div>{form.citation_kind === "material" && <Input value={form.locator} onChange={(v) => setForm((f) => ({ ...f, locator: v }))} placeholder="无页码时填写章节、段落或其他近似位置" />}<Textarea value={form.excerpt} onChange={(v) => setForm((f) => ({ ...f, excerpt: v }))} placeholder="摘录摘要（源材料失效后仍保留，不替代原件核验）" /><Button size="sm" onClick={onSave} disabled={busy}>{busy && <Loader2 className="size-3.5 animate-spin" />}保存引用</Button></div>;
}

function TaskArtifactLinker({ caseId, bundle, runAction }: { caseId: string; bundle: Awaited<ReturnType<typeof loadCriminalWorkspace>> | null; runAction: (key: string, action: () => Promise<unknown>, success: string) => Promise<void> }) {
  const artifacts = [
    ...(bundle?.review_notes ?? []).map((row) => ({ id: row.id, type: "review_note" as const, label: `阅卷笔记：${row.title}` })),
    ...(bundle?.evidence_items ?? []).map((row) => ({ id: row.id, type: "evidence" as const, label: `证据：${row.name}` })),
    ...(bundle?.issues ?? []).map((row) => ({ id: row.id, type: "issue" as const, label: `争点：${row.neutral_title}` })),
    ...(bundle?.findings ?? []).map((row) => ({ id: row.id, type: "finding" as const, label: `分析：${row.title}` })),
    ...(bundle?.drafts ?? []).flatMap((draft) => (draft.versions ?? []).map((version) => ({ id: version.id, type: "draft_version" as const, label: `文书：${draft.title} v${version.version_no}` }))),
  ];
  const [taskId, setTaskId] = useState("");
  const [artifactKey, setArtifactKey] = useState("");
  const [relation, setRelation] = useState<"input" | "output" | "supports" | "follow_up">("output");
  const [links, setLinks] = useState<Awaited<ReturnType<typeof workspaceCommands.listTaskArtifacts>>>([]);
  const [loadingLinks, setLoadingLinks] = useState(false);
  const loadLinks = async () => {
    if (!taskId.trim()) { toast("请先填写 SOP 任务 ID", "error"); return; }
    setLoadingLinks(true);
    try { setLinks(await workspaceCommands.listTaskArtifacts(taskId.trim())); }
    catch (cause) { toast(`读取任务产物失败：${asWorkspaceError(cause).message}`, "error"); }
    finally { setLoadingLinks(false); }
  };
  const link = () => {
    const artifact = artifacts.find((item) => `${item.type}:${item.id}` === artifactKey);
    if (!taskId.trim() || !artifact) { toast("请填写 SOP 任务 ID 并选择工作台产物", "error"); return; }
    void runAction("link-task", async () => { await workspaceCommands.linkTaskArtifact({ case_id: caseId, task_id: taskId.trim(), artifact_type: artifact.type, artifact_id: artifact.id, relation, created_by: "本机律师" }); setLinks(await workspaceCommands.listTaskArtifacts(taskId.trim())); }, "产物已关联到 SOP 任务（任务状态未自动改变）");
  };
  const unlink = (item: (typeof links)[number]) => void runAction(`unlink-${item.id}`, async () => { await workspaceCommands.unlinkTaskArtifact({ case_id: item.case_id, task_id: item.task_id, artifact_type: item.artifact_type, artifact_id: item.artifact_id, relation: item.relation, created_by: item.created_by }); setLinks(await workspaceCommands.listTaskArtifacts(item.task_id)); }, "任务产物关联已解除");
  return <EditorCard title="关联工作台产物到 SOP 任务"><p className="text-xs text-muted-foreground">从上方 SOP 任务详情复制任务 ID；关联只建立输入/输出关系，完成任务仍须使用既有“完成”动作并填写办理结果。</p><div className="grid gap-2 md:grid-cols-[1fr_auto_1.5fr_180px_auto]"><Input value={taskId} onChange={setTaskId} placeholder="SOP 任务 ID" /><Button size="sm" variant="ghost" onClick={() => void loadLinks()} disabled={loadingLinks}>{loadingLinks && <Loader2 className="size-3.5 animate-spin" />}读取关联</Button><Select value={artifactKey} onChange={setArtifactKey} options={[["", "选择产物"], ...artifacts.map((item) => [`${item.type}:${item.id}`, item.label])]} /><Select value={relation} onChange={(v) => setRelation(v as typeof relation)} options={[["input", "作为输入"], ["output", "作为产出"], ["supports", "提供支持"], ["follow_up", "后续动作"]]} /><Button size="sm" onClick={link}>建立关联</Button></div>{links.length > 0 && <div className="space-y-2 border-t border-border pt-3">{links.map((item) => { const artifact = artifacts.find((candidate) => candidate.id === item.artifact_id && candidate.type === item.artifact_type); return <div key={item.id} className="flex items-center justify-between gap-2 rounded-md bg-background px-3 py-2 text-xs"><span>{artifact?.label ?? `${item.artifact_type}:${item.artifact_id}`} · {relationLabel(item.relation)}</span><Button size="sm" variant="ghost" onClick={() => unlink(item)}>解除关联</Button></div>; })}</div>}</EditorCard>;
}

function ReviewActions({ row, onReview, busy, canConfirm = true }: { row: WorkspaceRecordBase; onReview: (decision: ReviewDecision) => void; busy: string | null; canConfirm?: boolean }) {
  const waiting = busy === `review-${row.id}`;
  if (row.review_status === "confirmed") return <span className="px-2 py-1 text-xs text-emerald-700 dark:text-emerald-300">律师已确认；修改请新建版本</span>;
  if (row.review_status === "rejected") return <Button size="sm" variant="ghost" onClick={() => onReview("reopen")} disabled={waiting}>重新打开</Button>;
  return <><Button size="sm" variant="outline" onClick={() => onReview("reject")} disabled={waiting}>拒绝</Button><Button size="sm" onClick={() => onReview("confirm")} disabled={waiting || !canConfirm}>{waiting && <Loader2 className="size-3.5 animate-spin" />}律师确认</Button></>;
}

function RecordCard({ title, description, status, origin, citations = [], onOpenCitation, warning, actions, selected, onToggleSelected }: { title: string; description: string; status: ReviewStatus | string; origin: string; citations?: WorkspaceCitation[]; onOpenCitation?: (citation: WorkspaceCitation) => void; warning?: string; actions?: ReactNode; selected?: boolean; onToggleSelected?: () => void }) {
  const selectionDisabled = status !== "confirmed";
  return <article className="rounded-lg border border-border bg-background p-3"><div className="flex flex-wrap items-start justify-between gap-2"><div className="flex min-w-0 gap-2">{onToggleSelected && <input type="checkbox" checked={Boolean(selected) && !selectionDisabled} disabled={selectionDisabled} onChange={onToggleSelected} aria-label={`选择 ${title}`} title={selectionDisabled ? "仅已确认内容可作为后续输入" : undefined} className="mt-1" />}<div className="min-w-0"><div className="flex flex-wrap items-center gap-1.5"><h4 className="text-sm font-semibold">{title}</h4><StatusBadge status={status} />{origin !== "user" && <PendingBadge>{origin === "codex" ? "Codex" : "模型"}产物</PendingBadge>}</div><p className="mt-1 whitespace-pre-wrap text-xs leading-5 text-muted-foreground">{description}</p></div></div></div>{warning && <p className="mt-2 flex items-center gap-1 text-xs text-amber-700 dark:text-amber-300"><AlertTriangle className="size-3.5" />{warning}</p>}{citations.length > 0 && <div className="mt-2 flex flex-wrap gap-1.5">{citations.map((citation) => <button key={citation.id} type="button" onClick={() => onOpenCitation?.(citation)} className={cn("inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px]", citation.integrity_status === "valid" ? "border-border hover:bg-muted" : "border-amber-500/40 bg-amber-500/10 text-amber-800 dark:text-amber-200")}><Link2 className="size-3" />{citation.source_filename_snapshot || citation.legal_title || "来源"} · {citationLocation(citation)}{citation.integrity_status !== "valid" && ` · ${citation.integrity_status === "missing" ? "缺失" : "已变化"}`}</button>)}</div>}{actions && <div className="mt-3 flex flex-wrap justify-end gap-1.5 border-t border-border pt-2">{actions}</div>}</article>;
}

function SectionIntro({ icon, title, description, action }: { icon: ReactNode; title: string; description: string; action?: ReactNode }) { return <div className="flex flex-wrap items-start justify-between gap-3"><div><h3 className="flex items-center gap-2 text-sm font-semibold">{icon}{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p></div>{action}</div>; }
function EditorCard({ title, children }: { title: string; children: ReactNode }) { return <div className="space-y-2 rounded-lg border border-border bg-muted/20 p-4"><Subheading>{title}</Subheading>{children}</div>; }
function Subheading({ children }: { children: ReactNode }) { return <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">{children}</h4>; }
function RecordList({ children, empty }: { children: ReactNode; empty: string }) { return <div className="space-y-2">{Array.isArray(children) && children.length === 0 ? <Empty text={empty} /> : children}</div>; }
function Empty({ text }: { text: string }) { return <div className="rounded-lg border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">{text}</div>; }
function Loading() { return <div className="flex items-center justify-center gap-2 py-12 text-sm text-muted-foreground"><Loader2 className="size-4 animate-spin" />正在加载五区工作台</div>; }
function Input({ value, onChange, placeholder }: { value: string; onChange: (value: string) => void; placeholder?: string }) { return <input value={value} onChange={(event) => onChange(event.currentTarget.value)} placeholder={placeholder} className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus:border-foreground" />; }
function Textarea({ value, onChange, placeholder, rows = 4 }: { value: string; onChange: (value: string) => void; placeholder?: string; rows?: number }) { return <textarea value={value} onChange={(event) => onChange(event.currentTarget.value)} placeholder={placeholder} rows={rows} className="w-full resize-y rounded-md border border-border bg-background px-3 py-2 text-sm leading-5 outline-none focus:border-foreground" />; }
function Select({ value, onChange, options }: { value: string; onChange: (value: string) => void; options: readonly (readonly [string, string])[] | string[][] }) { return <select value={value} onChange={(event) => onChange(event.currentTarget.value)} className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus:border-foreground">{options.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select>; }
function StatusBadge({ status }: { status: ReviewStatus | string }) { return <span className={cn("rounded-full px-2 py-0.5 text-[10px]", status === "confirmed" || status === "approved" ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300" : status === "rejected" ? "bg-rose-500/15 text-rose-700 dark:text-rose-300" : "bg-muted text-muted-foreground")}>{reviewLabel(status)}</span>; }
function PendingBadge({ children }: { children: ReactNode }) { return <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-[10px] text-amber-800 dark:text-amber-200">{children}</span>; }
function RiskBadge({ children }: { children: ReactNode }) { return <span className="rounded-full bg-rose-500/15 px-2 py-0.5 text-[10px] text-rose-700 dark:text-rose-300">{children}</span>; }
function countPending(rows: Array<{ review_status: ReviewStatus }> | undefined) { return (rows ?? []).filter((row) => row.review_status === "pending_review").length; }
function latestVersion(draft: CriminalDraftDocument) { return [...(draft.versions ?? [])].sort((a, b) => b.version_no - a.version_no)[0]; }
function documentTypeLabel(type: string) { return DOCUMENT_TYPES.find(([id]) => id === type)?.[1] ?? type; }
function providerLabel(provider: string) { return provider === "manual_template" || provider === "manual" ? "原生手工模板" : provider === "native_llm" ? "应用原生模型" : "Codex"; }
function toggleSet(current: Set<string>, id: string) { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next; }
function assessmentJson(status: AssessmentStatus, note: string) { return JSON.stringify({ status, note }); }
function relationLabel(relation: string) { return ({ supports: "支持", contradicts: "矛盾", weakens: "削弱", contextual: "背景", gap: "证据缺口" } as Record<string, string>)[relation] ?? relation; }
