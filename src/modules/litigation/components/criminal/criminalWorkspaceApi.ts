import { invoke } from "@tauri-apps/api/core";

import type {
  CriminalAnalysisCapabilities,
  CriminalAnalysisFinding,
  CriminalAnalysisRun,
  CriminalDraftDocument,
  CriminalDraftVersion,
  CriminalEvidenceItem,
  CriminalIssue,
  CriminalReviewNote,
  CriminalWorkspaceSummary,
  ReviewDecision,
  StartAnalysisInput,
  UpsertCitationInput,
  UpsertEvidenceInput,
  UpsertFindingInput,
  UpsertIssueInput,
  UpsertReviewNoteInput,
  CreateDraftInput,
  CreateDraftVersionInput,
  WorkspaceCitation,
  LinkTaskArtifactInput,
  CriminalIssueEvidenceLink,
  UpsertIssueEvidenceLinkInput,
  TaskArtifactLink,
  WorkspaceBundle,
  WorkspaceError,
} from "./criminalWorkspaceTypes";

const EMPTY_SUMMARY: CriminalWorkspaceSummary = {
  case_id: "", review_notes: 0, evidence_items: 0, issues: 0, findings: 0, drafts: 0,
  pending_review: 0, invalid_citations: 0, open_tasks: 0,
};

const MANUAL_CAPABILITIES: CriminalAnalysisCapabilities = {
  manual: true,
  native_llm: { available: false, reason: "原生模型未配置" },
  codex: { available: false, experimental: true, reason: "Codex 未连接" },
};

export function asWorkspaceError(cause: unknown): WorkspaceError {
  if (typeof cause === "object" && cause !== null && "code" in cause && "message" in cause) {
    const value = cause as Partial<WorkspaceError>;
    return {
      code: String(value.code),
      message: String(value.message),
      retryable: Boolean(value.retryable),
      details: value.details,
    };
  }
  const raw = String(cause);
  const coded = raw.match(/^([A-Z][A-Z0-9_]+):\s*(.*)$/s);
  if (coded) return { code: coded[1], message: coded[2], retryable: coded[1] === "DATABASE_WRITE_FAILED" || coded[1] === "PROVIDER_FAILED" };
  try {
    const parsed = JSON.parse(raw) as Partial<WorkspaceError>;
    if (parsed.code && parsed.message) {
      return { code: parsed.code, message: parsed.message, retryable: Boolean(parsed.retryable), details: parsed.details };
    }
  } catch {
    // Tauri may return an ordinary string for older backends.
  }
  return { code: "UNKNOWN", message: raw, retryable: false };
}

export async function loadCriminalWorkspace(caseId: string): Promise<WorkspaceBundle> {
  const [summary, review_notes, evidence_items, issues, findings, draftRows, capabilities, analysis_runs, issue_evidence_links] = await Promise.all([
    invoke<CriminalWorkspaceSummary>("get_criminal_defense_workspace", { caseId }),
    invoke<CriminalReviewNote[]>("list_criminal_review_notes", { caseId, documentId: null, reviewStatus: null }),
    invoke<CriminalEvidenceItem[]>("list_criminal_evidence_items", { caseId }),
    invoke<CriminalIssue[]>("list_criminal_issues", { caseId }),
    invoke<CriminalAnalysisFinding[]>("list_criminal_analysis_findings", { caseId, runId: null }),
    invoke<CriminalDraftDocument[]>("list_criminal_drafts", { caseId }),
    invoke<CriminalAnalysisCapabilities>("get_criminal_analysis_capabilities", { caseId }).catch(() => MANUAL_CAPABILITIES),
    invoke<CriminalAnalysisRun[]>("list_criminal_analysis_runs", { caseId }),
    invoke<CriminalIssueEvidenceLink[]>("list_criminal_issue_evidence_links", { caseId, issueId: null }),
  ]);
  const drafts = await Promise.all(draftRows.map(async (draft) => {
    const [, versions] = await invoke<[CriminalDraftDocument, CriminalDraftVersion[]]>("get_criminal_draft", { draftId: draft.id });
    return { ...draft, versions };
  }));
  const citations = await invoke<WorkspaceCitation[]>("list_criminal_source_citations", { caseId, ownerType: null, ownerId: null });
  const attach = <T extends { id: string }>(rows: T[]) => rows.map((row) => ({ ...row, citations: citations.filter((citation) => citation.owner_id === row.id) }));
  const attachedDrafts = drafts.map((draft) => ({ ...draft, versions: (draft.versions ?? []).map((version) => ({ ...version, citations: citations.filter((citation) => citation.owner_id === version.id) })) }));
  return { summary: { ...EMPTY_SUMMARY, ...summary }, review_notes: attach(review_notes), evidence_items: attach(evidence_items), issues: attach(issues), findings: attach(findings), drafts: attachedDrafts, capabilities, analysis_runs, issue_evidence_links };
}

export const workspaceCommands = {
  upsertReviewNote: (input: UpsertReviewNoteInput) => invoke<CriminalReviewNote>("upsert_criminal_review_note", { input }),
  reviewReviewNote: (id: string, decision: ReviewDecision, expectedRevision: number, note = "") =>
    invoke<CriminalReviewNote>("review_criminal_review_note", { input: { id, decision, actor: "本机律师", note, expected_revision: expectedRevision } }),
  upsertEvidence: (input: UpsertEvidenceInput) => invoke<CriminalEvidenceItem>("upsert_criminal_evidence_item", { input }),
  reviewEvidence: (id: string, decision: ReviewDecision, expectedRevision: number, note = "") =>
    invoke<CriminalEvidenceItem>("review_criminal_evidence_item", { input: { id, decision, actor: "本机律师", note, expected_revision: expectedRevision } }),
  upsertIssue: (input: UpsertIssueInput) => invoke<CriminalIssue>("upsert_criminal_issue", { input }),
  reviewIssue: (id: string, decision: ReviewDecision, expectedRevision: number, note = "") =>
    invoke<CriminalIssue>("review_criminal_issue", { input: { id, decision, actor: "本机律师", note, expected_revision: expectedRevision } }),
  startAnalysis: (input: StartAnalysisInput) => invoke<CriminalAnalysisRun>("start_criminal_analysis", { input }),
  upsertFinding: (input: UpsertFindingInput) => invoke<CriminalAnalysisFinding>("upsert_criminal_analysis_finding", { input }),
  reviewFinding: (id: string, decision: ReviewDecision, expectedRevision: number, note = "") =>
    invoke<CriminalAnalysisFinding>("review_criminal_analysis_finding", { input: { id, decision, actor: "本机律师", note, expected_revision: expectedRevision } }),
  createDraft: (input: CreateDraftInput) => invoke<CriminalDraftDocument>("create_criminal_draft", { input }),
  createDraftVersion: (input: CreateDraftVersionInput) => invoke<CriminalDraftVersion>("create_criminal_draft_version", { input }),
  submitDraft: (versionId: string, expectedRevision: number) =>
    invoke<CriminalDraftVersion>("submit_criminal_draft_version_for_review", { versionId, expectedRevision }),
  approveDraft: (versionId: string, expectedRevision: number, reviewNote = "") =>
    invoke<CriminalDraftVersion>("approve_criminal_draft_version", { input: { version_id: versionId, actor: "本机律师", review_note: reviewNote, expected_revision: expectedRevision } }),
  exportDraft: (versionId: string, mode: "working" | "formal", outputPath: string) =>
    invoke<string>("export_criminal_draft_version", { input: { version_id: versionId, mode, output_path: outputPath } }),
  upsertCitation: (input: UpsertCitationInput) => invoke<WorkspaceCitation>("upsert_criminal_source_citation", { input }),
  refreshIntegrity: (caseId: string) => invoke("refresh_criminal_source_integrity", { caseId, documentId: null }),
  linkTaskArtifact: (input: LinkTaskArtifactInput) => invoke("link_criminal_workspace_artifact_to_task", { input }),
  upsertIssueEvidenceLink: (input: UpsertIssueEvidenceLinkInput) => invoke<CriminalIssueEvidenceLink>("upsert_criminal_issue_evidence_link", { input }),
  reviewIssueEvidenceLink: (id: string, decision: ReviewDecision, expectedRevision: number, note = "") => invoke("review_criminal_issue_evidence_link", { input: { id, decision, actor: "本机律师", note, expected_revision: expectedRevision } }),
  returnDraft: (versionId: string, expectedRevision: number, reviewNote: string) => invoke<CriminalDraftVersion>("return_criminal_draft_version", { input: { version_id: versionId, actor: "本机律师", review_note: reviewNote, expected_revision: expectedRevision } }),
  archiveDraft: (draftId: string, expectedRevision: number) => invoke<CriminalDraftDocument>("archive_criminal_draft", { draftId, expectedRevision }),
  unlinkTaskArtifact: (input: LinkTaskArtifactInput) => invoke("unlink_criminal_workspace_artifact_from_task", { input }),
  listTaskArtifacts: (taskId: string) => invoke<TaskArtifactLink[]>("list_criminal_task_artifacts", { taskId }),
};
