import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronRight,
  FileSearch,
  Loader2,
  RefreshCw,
  ShieldAlert,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { confirmDialog } from "@/lib/dialog";
import { cn } from "@/lib/utils";
import {
  candidateBatchStatusLabel,
  confidenceLabel,
  criminalFieldLabel,
  formatCandidateFieldValue,
  parseTechnicalWarnings,
  shouldDefaultAccept,
  valuesAreEqual,
  type CandidateDecision,
  type CriminalExtractionCandidateBatchView,
} from "./criminalExtractionReviewModels";

export interface CriminalCandidateDecisionInput {
  field_key: string;
  decision: "accept" | "reject";
  note: string | null;
}

interface Props {
  batches: CriminalExtractionCandidateBatchView[];
  loading: boolean;
  recognizing: boolean;
  submittingBatchId: string | null;
  onRecognize: () => Promise<void>;
  onConfirm: (
    batchId: string,
    expectedProfileRevision: number,
    decisions: CriminalCandidateDecisionInput[],
  ) => Promise<void>;
  onRejectBatch: (batchId: string) => Promise<void>;
}

const REVIEWABLE = new Set(["pending", "partially_confirmed"]);

export function CriminalExtractionReviewPanel({
  batches,
  loading,
  recognizing,
  submittingBatchId,
  onRecognize,
  onConfirm,
  onRejectBatch,
}: Props) {
  const sortedBatches = useMemo(
    () => [...batches].sort((a, b) => b.created_at.localeCompare(a.created_at)),
    [batches],
  );
  const preferredBatchId =
    sortedBatches.find((batch) => REVIEWABLE.has(batch.review_status))?.id ??
    sortedBatches[0]?.id ??
    null;
  const [expandedBatchId, setExpandedBatchId] = useState<string | null>(null);
  const [decisions, setDecisions] = useState<Record<string, CandidateDecision>>({});

  useEffect(() => {
    setExpandedBatchId((current) =>
      current && sortedBatches.some((batch) => batch.id === current)
        ? current
        : preferredBatchId,
    );
  }, [preferredBatchId, sortedBatches]);

  useEffect(() => {
    setDecisions((current) => {
      const next = { ...current };
      for (const batch of sortedBatches) {
        for (const field of batch.fields) {
          if (next[field.id]) continue;
          next[field.id] = shouldDefaultAccept(field) ? "accept" : "pending";
        }
      }
      return next;
    });
  }, [sortedBatches]);

  const pendingCount = batches.reduce(
    (sum, batch) =>
      sum + batch.fields.filter((field) => field.review_status === "pending").length,
    0,
  );

  return (
    <section className="rounded-xl border border-border bg-background/60 p-4" aria-labelledby="criminal-extraction-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <FileSearch className="size-4 text-primary" />
            <h3 id="criminal-extraction-title" className="text-sm font-semibold">
              案件材料识别
            </h3>
            {pendingCount > 0 && (
              <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-300">
                待确认 {pendingCount}
              </span>
            )}
          </div>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">
            优先复用已提取正文，缺少缓存时才可能重新 OCR。识别结果只生成候选，不会自动覆盖刑事画像；请逐字段核对后确认。
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          onClick={() => void onRecognize()}
          disabled={recognizing || loading}
          aria-label={recognizing ? "正在重新识别案件材料" : "重新识别案件材料"}
        >
          {recognizing ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
          {recognizing ? "识别中" : "重新识别案件材料"}
        </Button>
      </div>

      {loading ? (
        <div className="mt-4 flex items-center gap-2 rounded-lg border border-dashed px-3 py-4 text-xs text-muted-foreground">
          <Loader2 className="size-3.5 animate-spin" />
          正在读取识别候选
        </div>
      ) : sortedBatches.length === 0 ? (
        <div className="mt-4 rounded-lg border border-dashed px-3 py-4 text-center text-xs text-muted-foreground">
          暂无识别候选。可点击“重新识别案件材料”生成待核对结果。
        </div>
      ) : (
        <div className="mt-4 space-y-2">
          {sortedBatches.map((batch) => {
            const expanded = expandedBatchId === batch.id;
            const warnings = parseTechnicalWarnings(batch.warning_json);
            const reviewable = REVIEWABLE.has(batch.review_status);
            const submitting = submittingBatchId === batch.id;
            const decidedFields = batch.fields
              .filter((field) => field.review_status === "pending")
              .flatMap((field): CriminalCandidateDecisionInput[] => {
                const decision = decisions[field.id] ?? "pending";
                return decision === "pending"
                  ? []
                  : [{ field_key: field.field_key, decision, note: null }];
              });

            return (
              <article key={batch.id} className="overflow-hidden rounded-lg border border-border bg-card">
                <button
                  type="button"
                  className="flex w-full items-start justify-between gap-3 px-3 py-3 text-left hover:bg-muted/40"
                  onClick={() => setExpandedBatchId(expanded ? null : batch.id)}
                  aria-expanded={expanded}
                  aria-controls={`criminal-candidate-${batch.id}`}
                >
                  <div className="flex min-w-0 gap-2">
                    {expanded ? <ChevronDown className="mt-0.5 size-4 shrink-0" /> : <ChevronRight className="mt-0.5 size-4 shrink-0" />}
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2 text-xs">
                        <span className="truncate font-medium">{batch.source_filename}</span>
                        <span className="rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">
                          {batch.document_type || "其他刑事材料"}
                        </span>
                        <BatchStatus batch={batch} />
                      </div>
                      <p className="mt-1 text-[11px] text-muted-foreground">
                        生成于 {formatDateTime(batch.created_at)} · {batch.fields.length} 个字段
                      </p>
                    </div>
                  </div>
                  {(batch.technical_status !== "success" || warnings.length > 0) && (
                    <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600" aria-label="包含技术警告" />
                  )}
                </button>

                {expanded && (
                  <div id={`criminal-candidate-${batch.id}`} className="border-t border-border px-3 py-3">
                    {(batch.error_message || warnings.length > 0) && (
                      <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-800 dark:text-amber-200">
                        {batch.error_message && <p>{batch.error_message}</p>}
                        {warnings.map((warning, index) => (
                          <p key={`${batch.id}-warning-${index}`}>{warning}</p>
                        ))}
                      </div>
                    )}

                    {batch.fields.length === 0 ? (
                      <p className="py-3 text-center text-xs text-muted-foreground">该批次没有可确认字段。</p>
                    ) : (
                      <div className="space-y-2">
                        {batch.fields.map((field) => {
                          const protectedField = field.is_user_protected || field.review_status === "protected";
                          const noChange = valuesAreEqual(field.current_value_json, field.value_json);
                          const decision = decisions[field.id] ?? "pending";
                          const mutable = reviewable && field.review_status === "pending" && !protectedField;
                          return (
                            <div key={field.id} className="rounded-md border border-border p-3">
                              <div className="flex flex-wrap items-start justify-between gap-2">
                                <div>
                                  <p className="text-xs font-medium">{criminalFieldLabel(field.field_key)}</p>
                                  <p className="mt-0.5 text-[11px] text-muted-foreground">
                                    {confidenceLabel(field.confidence)} · 来源：{field.source_filename}
                                  </p>
                                </div>
                                <div className="flex flex-wrap items-center gap-1.5">
                                  {noChange && <Tag className="bg-muted text-muted-foreground">无变化</Tag>}
                                  {protectedField && (
                                    <Tag className="bg-blue-500/10 text-blue-700 dark:text-blue-300">
                                      <ShieldAlert className="size-3" /> 人工保护
                                    </Tag>
                                  )}
                                  {field.has_conflict && <Tag className="bg-amber-500/10 text-amber-700 dark:text-amber-300">存在冲突</Tag>}
                                </div>
                              </div>

                              <div className="mt-3 grid gap-2 md:grid-cols-2">
                                <ValueCard label="当前画像" value={formatCandidateFieldValue(field.field_key, field.current_value_json)} />
                                <ValueCard label="识别候选" value={formatCandidateFieldValue(field.field_key, field.value_json)} candidate />
                              </div>
                              {field.evidence_excerpt && (
                                <div className="mt-2 rounded bg-muted/50 px-2.5 py-2 text-[11px] leading-5 text-muted-foreground">
                                  <span className="font-medium text-foreground">证据摘录：</span>
                                  {field.evidence_excerpt}
                                </div>
                              )}
                              {protectedField && (
                                <p className="mt-2 text-[11px] text-blue-700 dark:text-blue-300">
                                  {field.protection_reason || "该字段已有人工修改保护，候选不能覆盖；如需调整，请在刑事画像中手工编辑。"}
                                </p>
                              )}

                              {reviewable && field.review_status === "pending" && (
                                <div className="mt-3 flex flex-wrap gap-2" role="group" aria-label={`${criminalFieldLabel(field.field_key)}候选决定`}>
                                  <DecisionButton
                                    selected={decision === "accept"}
                                    disabled={!mutable}
                                    onClick={() => setDecisions((current) => ({ ...current, [field.id]: decision === "accept" ? "pending" : "accept" }))}
                                  >
                                    <Check className="size-3.5" /> 接受
                                  </DecisionButton>
                                  <DecisionButton
                                    selected={decision === "reject"}
                                    disabled={!mutable}
                                    onClick={() => setDecisions((current) => ({ ...current, [field.id]: decision === "reject" ? "pending" : "reject" }))}
                                  >
                                    <X className="size-3.5" /> 拒绝
                                  </DecisionButton>
                                  {decision === "pending" && <span className="self-center text-[11px] text-muted-foreground">暂不决定</span>}
                                </div>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    )}

                    {reviewable && (
                      <div className="mt-3 flex flex-wrap justify-end gap-2 border-t border-border pt-3">
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          disabled={submitting}
                          onClick={async () => {
                            const ok = await confirmDialog(`拒绝来自「${batch.source_filename}」的整批识别结果？`, { danger: true, okLabel: "整批拒绝" });
                            if (ok) await onRejectBatch(batch.id);
                          }}
                        >
                          整批拒绝
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          disabled={submitting || decidedFields.length === 0}
                          onClick={async () => {
                            const ok = await confirmDialog(`提交 ${decidedFields.length} 个字段决定？未选择字段将继续保持待确认。`, { okLabel: "确认提交" });
                            if (ok) await onConfirm(batch.id, batch.profile_revision, decidedFields);
                          }}
                        >
                          {submitting && <Loader2 className="size-3.5 animate-spin" />}
                          提交已选决定
                        </Button>
                      </div>
                    )}
                  </div>
                )}
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}

function BatchStatus({ batch }: { batch: CriminalExtractionCandidateBatchView }) {
  const warning = batch.technical_status !== "success";
  return (
    <span className={cn("rounded px-1.5 py-0.5 text-[11px] font-medium", warning ? "bg-amber-500/15 text-amber-700 dark:text-amber-300" : "bg-primary/10 text-primary")}>
      {candidateBatchStatusLabel(batch.review_status, batch.technical_status)}
    </span>
  );
}

function Tag({ className, children }: { className?: string; children: ReactNode }) {
  return <span className={cn("inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px]", className)}>{children}</span>;
}

function ValueCard({ label, value, candidate = false }: { label: string; value: string; candidate?: boolean }) {
  return (
    <div className={cn("rounded-md border px-2.5 py-2", candidate ? "border-primary/25 bg-primary/5" : "border-border bg-muted/20")}>
      <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className="mt-1 break-words text-xs leading-5">{value}</p>
    </div>
  );
}

function DecisionButton({ selected, disabled, onClick, children }: { selected: boolean; disabled: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      disabled={disabled}
      aria-pressed={selected}
      onClick={onClick}
      className={cn(
        "inline-flex h-7 items-center gap-1 rounded-md border px-2.5 text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-40",
        selected ? "border-primary bg-primary text-primary-foreground" : "border-border hover:bg-muted",
      )}
    >
      {children}
    </button>
  );
}

function formatDateTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString("zh-CN", { hour12: false });
}
