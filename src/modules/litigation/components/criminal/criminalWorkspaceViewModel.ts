import type { CriminalAnalysisCapabilities, FindingType, ReviewStatus, WorkspaceCitation } from "./criminalWorkspaceTypes";

export const WORKSPACE_ZONES = [
  { id: "materials", label: "材料阅卷" },
  { id: "evidence", label: "证据争点" },
  { id: "analysis", label: "案件分析" },
  { id: "drafting", label: "文书草拟" },
  { id: "tasks", label: "流程任务" },
] as const;

export type WorkspaceZone = (typeof WORKSPACE_ZONES)[number]["id"];

export const FINDING_LABELS: Record<FindingType, string> = {
  material_fact: "材料事实",
  unverified_fact: "待核实事实",
  legal_rule: "法律依据",
  analysis: "分析判断",
  defense_strategy: "辩护策略",
};

export function reviewLabel(status: ReviewStatus | string) {
  return ({ draft: "草稿", pending_review: "待律师复核", confirmed: "已确认", rejected: "已拒绝", superseded: "已被替代" } as Record<string, string>)[status] ?? status;
}

export function citationLocation(citation: WorkspaceCitation) {
  if (citation.page_start) {
    return citation.page_end && citation.page_end !== citation.page_start
      ? `第 ${citation.page_start}-${citation.page_end} 页`
      : `第 ${citation.page_start} 页`;
  }
  return citation.location_precision === "approximate" ? "近似位置" : "文档位置";
}

export function availableProviders(capabilities: CriminalAnalysisCapabilities) {
  return [
    { id: "manual", label: "原生手工模板", available: true, experimental: false, reason: null },
    { id: "native_llm", label: "应用原生模型", ...capabilities.native_llm, experimental: false },
    { id: "codex", label: "Codex 增强", ...capabilities.codex, experimental: true },
  ].filter((provider) => provider.available);
}

export function canConfirmFinding(type: FindingType, citations: WorkspaceCitation[] = []) {
  if (type !== "material_fact") return true;
  return citations.some((citation) => citation.citation_kind === "material" && citation.integrity_status === "valid");
}

export function confirmedSelectionIds<T extends { id: string; review_status: string }>(rows: T[], selected: Set<string>) {
  return rows.filter((row) => row.review_status === "confirmed" && selected.has(row.id)).map((row) => row.id);
}
