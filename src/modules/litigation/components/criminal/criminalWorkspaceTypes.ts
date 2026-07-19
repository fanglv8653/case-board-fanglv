export type ReviewStatus = "draft" | "pending_review" | "confirmed" | "rejected" | "superseded";
export type WorkspaceOrigin = "user" | "native_ai" | "codex";
export type IntegrityStatus = "valid" | "missing" | "changed";

export interface WorkspaceCitation {
  id: string;
  owner_type: "review_note" | "evidence" | "issue_link" | "finding" | "draft_version";
  owner_id: string;
  citation_kind: "material" | "legal" | "user_statement";
  document_id: string | null;
  source_filename_snapshot: string | null;
  source_path_snapshot: string | null;
  page_start: number | null;
  page_end: number | null;
  locator_json: string;
  location_precision: "exact" | "approximate";
  excerpt: string;
  legal_title: string | null;
  legal_article: string | null;
  legal_url: string | null;
  verification_status: "unchecked" | "verified" | "cannot_verify";
  integrity_status: IntegrityStatus;
  checked_at: string | null;
}

export interface WorkspaceRecordBase {
  id: string;
  case_id: string;
  revision: number;
  review_status: ReviewStatus;
  created_at: string;
  updated_at: string;
  reviewed_by?: string | null;
  reviewed_at?: string | null;
  review_note?: string | null;
  citations?: WorkspaceCitation[];
}

export interface CriminalReviewNote extends WorkspaceRecordBase {
  document_id: string | null;
  title: string;
  content: string;
  note_type: "general" | "fact" | "question" | "contradiction" | "todo";
  author_type: WorkspaceOrigin;
}

export interface CriminalEvidenceItem extends WorkspaceRecordBase {
  name: string;
  evidence_type: string;
  proof_purpose: string;
  source_description: string;
  origin: WorkspaceOrigin;
  authenticity_assessment_json?: string;
  legality_assessment_json?: string;
  relevance_assessment_json?: string;
  admissibility_assessment_json?: string;
  probative_force_assessment_json?: string;
  corroboration_assessment_json?: string;
  exclusion_clue_assessment_json?: string;
  reasonable_doubt_impact_json?: string;
}

export interface CriminalIssue extends WorkspaceRecordBase {
  issue_type: string;
  neutral_title: string;
  description: string;
  status: "open" | "confirmed" | "resolved" | "archived";
  position: "prosecution" | "defense" | "neutral";
  origin: WorkspaceOrigin;
  evidence_links?: CriminalIssueEvidenceLink[];
}

export interface CriminalIssueEvidenceLink extends WorkspaceRecordBase {
  issue_id: string;
  evidence_id: string | null;
  relation: "supports" | "contradicts" | "weakens" | "contextual" | "gap";
  explanation: string;
  origin: WorkspaceOrigin;
}

export type FindingType = "material_fact" | "unverified_fact" | "legal_rule" | "analysis" | "defense_strategy";

export interface CriminalAnalysisFinding extends WorkspaceRecordBase {
  run_id: string | null;
  finding_type: FindingType;
  title: string;
  content: string;
  confidence: number | null;
  origin: WorkspaceOrigin;
}

export interface CriminalAnalysisRun {
  id: string;
  case_id: string;
  template_code: string;
  template_version: number;
  requested_provider: "manual" | "native_llm" | "codex";
  actual_provider: "manual_template" | "native_llm" | "codex";
  status: "queued" | "running" | "succeeded" | "partial" | "failed" | "cancelled";
  request_id: string;
  fallback_from: string | null;
  fallback_to: string | null;
  fallback_reason: string | null;
  error_code: string | null;
  error_message: string | null;
  created_at: string;
}

export interface CriminalDraftVersion {
  id: string;
  draft_id: string;
  version_no: number;
  rendered_markdown: string;
  status: "draft" | "pending_review" | "approved" | "superseded";
  origin: WorkspaceOrigin;
  revision: number;
  quality_report_json: string;
  reviewed_by: string | null;
  reviewed_at: string | null;
  review_note: string | null;
  approved_at: string | null;
  citations?: WorkspaceCitation[];
}

export interface CriminalDraftDocument {
  id: string;
  case_id: string;
  document_type: string;
  title: string;
  status: "active" | "archived";
  current_version_id: string | null;
  revision: number;
  versions?: CriminalDraftVersion[];
}

export interface CriminalWorkspaceSummary {
  case_id: string;
  review_notes: number;
  evidence_items: number;
  issues: number;
  findings: number;
  drafts: number;
  pending_review: number;
  invalid_citations: number;
  open_tasks: number;
}

export interface ProviderCapability {
  available: boolean;
  reason: string | null;
  experimental?: boolean;
}

export interface CriminalAnalysisCapabilities {
  manual: boolean;
  native_llm: ProviderCapability;
  codex: ProviderCapability;
}

export interface WorkspaceError {
  code: string;
  message: string;
  retryable: boolean;
  details?: unknown;
}

export interface WorkspaceBundle {
  summary: CriminalWorkspaceSummary;
  review_notes: CriminalReviewNote[];
  evidence_items: CriminalEvidenceItem[];
  issues: CriminalIssue[];
  findings: CriminalAnalysisFinding[];
  drafts: CriminalDraftDocument[];
  capabilities: CriminalAnalysisCapabilities;
  analysis_runs: CriminalAnalysisRun[];
  issue_evidence_links: CriminalIssueEvidenceLink[];
}

export type ReviewDecision = "confirm" | "reject" | "reopen";

export interface UpsertReviewNoteInput {
  id?: string; case_id: string; document_id?: string | null; title: string; content: string;
  note_type?: "general" | "fact" | "question" | "contradiction" | "todo";
  review_status?: "draft" | "pending_review"; author_type?: WorkspaceOrigin; expected_revision?: number;
}
export interface UpsertEvidenceInput {
  id?: string; case_id: string; name: string; evidence_type?: string; proof_purpose?: string; source_description?: string;
  originality_status?: string; authenticity_assessment_json?: string; legality_assessment_json?: string;
  relevance_assessment_json?: string; admissibility_assessment_json?: string; probative_force_assessment_json?: string;
  corroboration_assessment_json?: string; exclusion_clue_assessment_json?: string; reasonable_doubt_impact_json?: string;
  origin?: WorkspaceOrigin; expected_revision?: number;
}
export interface UpsertIssueInput {
  id?: string; case_id: string; issue_type: string; neutral_title: string; description?: string;
  status?: "open" | "confirmed" | "resolved" | "archived"; position?: "prosecution" | "defense" | "neutral";
  origin?: WorkspaceOrigin; expected_revision?: number;
}
export interface UpsertFindingInput {
  id?: string; case_id: string; run_id?: string | null; finding_type: FindingType; title: string; content: string;
  confidence?: number | null; origin?: WorkspaceOrigin; expected_revision?: number;
}
export interface StartAnalysisInput {
  case_id: string; request_id: string; template_code: string; requested_provider: "manual" | "native_llm" | "codex";
  input_snapshot_json: string; allow_fallback?: boolean;
}
export interface CreateDraftInput { case_id: string; document_type: string; title: string; created_by: string; }
export interface CreateDraftVersionInput { draft_id: string; content_json: string; rendered_markdown: string; origin?: WorkspaceOrigin; source_snapshot_json?: string; }
export interface UpsertCitationInput {
  id?: string; case_id: string; owner_type: WorkspaceCitation["owner_type"]; owner_id: string; citation_kind: WorkspaceCitation["citation_kind"];
  document_id?: string | null; page_start?: number | null; page_end?: number | null; locator_json?: string;
  location_precision?: "exact" | "approximate"; excerpt?: string; legal_title?: string; legal_article?: string;
  legal_url?: string; verification_status?: WorkspaceCitation["verification_status"]; expected_revision?: number;
}
export interface LinkTaskArtifactInput { case_id: string; task_id: string; artifact_type: "review_note" | "evidence" | "issue" | "finding" | "draft_version"; artifact_id: string; relation: "input" | "output" | "supports" | "follow_up"; created_by: string; }
export interface TaskArtifactLink extends LinkTaskArtifactInput { id: string; created_at: string; }
export interface UpsertIssueEvidenceLinkInput { id?: string; case_id: string; issue_id: string; evidence_id?: string | null; relation: CriminalIssueEvidenceLink["relation"]; explanation?: string; origin?: WorkspaceOrigin; expected_revision?: number; }
