import { useEffect, useMemo, useState } from "react";
import {
  BookOpenText,
  FileSearch,
  Loader2,
  Scale,
  Search,
  XCircle,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  transactionLegalResearch,
  type TransactionLegalResearchResponse,
} from "@/lib/api";

export interface ContractResearchSeed {
  question: string;
  riskTitle?: string;
  clauseRef?: string;
  anchorText?: string;
}

interface Props {
  open: boolean;
  contractName?: string | null;
  contractType?: string | null;
  stance?: string | null;
  seed?: ContractResearchSeed | null;
  onClose: () => void;
}

function formatError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

function stanceLabel(stance?: string | null): string {
  if (stance === "party_a") return "甲方";
  if (stance === "party_b") return "乙方";
  return "中立";
}

function traceBadge(success: boolean): string {
  return success
    ? "rounded bg-emerald-50 px-1.5 py-0.5 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300"
    : "rounded bg-red-50 px-1.5 py-0.5 text-red-700 dark:bg-red-950/30 dark:text-red-300";
}

export function ContractResearchPanel({
  open,
  contractName,
  contractType,
  stance,
  seed,
  onClose,
}: Props) {
  const [question, setQuestion] = useState(seed?.question ?? "");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<TransactionLegalResearchResponse | null>(null);

  useEffect(() => {
    if (!open) return;
    setQuestion(seed?.question ?? "");
    setError(null);
    setResult(null);
  }, [open, seed?.anchorText, seed?.clauseRef, seed?.question, seed?.riskTitle]);

  const contextChips = useMemo(() => {
    const chips: string[] = [];
    if (contractType?.trim()) chips.push(`合同类型：${contractType.trim()}`);
    if (stance?.trim()) chips.push(`我方立场：${stanceLabel(stance)}`);
    if (seed?.riskTitle?.trim()) chips.push(`风险点：${seed.riskTitle.trim()}`);
    if (seed?.clauseRef?.trim()) chips.push(`条款定位：${seed.clauseRef.trim()}`);
    return chips;
  }, [contractType, seed?.clauseRef, seed?.riskTitle, stance]);

  if (!open) return null;

  async function handleSearch() {
    if (!question.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const response = await transactionLegalResearch({
        question,
        contract_name: contractName ?? null,
        contract_type: contractType ?? null,
        stance: stance ?? null,
        risk_title: seed?.riskTitle ?? null,
        clause_ref: seed?.clauseRef ?? null,
        anchor_text: seed?.anchorText ?? null,
      });
      setResult(response);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="space-y-4 rounded-lg border border-sky-200/70 bg-sky-50/40 p-4 dark:border-sky-900/40 dark:bg-sky-950/15">
      <div className="flex items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <FileSearch className="size-4 text-sky-700 dark:text-sky-300" />
            <h3 className="text-sm font-medium text-foreground">合同法律检索</h3>
          </div>
          <p className="text-xs leading-relaxed text-muted-foreground">
            这里复用现有法条、法规、类案和本地知识库工具做最小白名单检索，只返回结构化研究摘要。
            本轮不接入 <code>case_chat</code> 外壳，也不会写入聊天历史或文书库。
          </p>
        </div>
        <Button size="sm" variant="ghost" onClick={onClose} disabled={loading}>
          收起
        </Button>
      </div>

      {contractName && (
        <p className="text-xs text-muted-foreground">
          当前合同：<span className="font-medium text-foreground">{contractName}</span>
        </p>
      )}

      {contextChips.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {contextChips.map((chip) => (
            <span
              key={chip}
              className="rounded-full bg-background/70 px-2 py-1 text-caption text-foreground/80"
            >
              {chip}
            </span>
          ))}
        </div>
      )}

      <div className="space-y-2">
        <label className="text-xs font-medium text-foreground">检索问题</label>
        <textarea
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          rows={4}
          disabled={loading}
          placeholder="例如：请检索中国法下与单方解除权触发条件过宽相关的法条、监管规则和代表性类案，并说明对我方的风险影响。"
          className="w-full resize-y rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-sky-400"
        />
        {seed?.anchorText && (
          <p className="rounded border border-border/70 bg-background/60 px-3 py-2 font-mono text-caption text-muted-foreground">
            原文片段：{seed.anchorText}
          </p>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={handleSearch} disabled={loading || !question.trim()}>
          {loading ? <Loader2 className="size-4 animate-spin" /> : <Search className="size-4" />}
          开始检索
        </Button>
        <span className="text-xs text-muted-foreground">
          结果只保留结构化依据、风险影响和下一步建议。
        </span>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          <p className="font-medium">法律检索失败</p>
          <p className="mt-0.5 break-all font-mono">{error}</p>
        </div>
      )}

      {result && (
        <div className="space-y-4">
          <section className="rounded-lg border border-border bg-background p-4">
            <div className="flex items-center gap-2">
              <Scale className="size-4 text-foreground/70" />
              <h4 className="text-sm font-medium text-foreground">
                {result.normalized_issue || result.question}
              </h4>
            </div>
            {result.scope_note && (
              <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
                {result.scope_note}
              </p>
            )}
            <div className="mt-3 rounded-md border border-sky-200/70 bg-sky-50/70 px-3 py-2 text-xs leading-relaxed text-sky-900 dark:border-sky-900/40 dark:bg-sky-950/20 dark:text-sky-200">
              <p>工具事实：法律依据中的类型、标题、定位、摘录，以及下方“可追溯引用”。</p>
              <p>模型分析：研究摘要、依据关联说明、风险影响、下一步建议、建议补问。</p>
            </div>
            {result.summary && (
              <div className="mt-3 space-y-1">
                <h5 className="text-xs font-medium text-foreground/80">研究摘要（模型分析）</h5>
                <p className="text-sm leading-relaxed text-foreground">{result.summary}</p>
              </div>
            )}
          </section>

          <div className="grid gap-4 lg:grid-cols-[1.3fr_0.9fr]">
            <div className="space-y-4">
              <section className="rounded-lg border border-border bg-background p-4">
                <div className="flex items-center gap-2">
                  <BookOpenText className="size-4 text-foreground/70" />
                  <h4 className="text-sm font-medium text-foreground">法律依据（工具事实）</h4>
                </div>
                <div className="mt-3 space-y-3">
                  {result.authorities.length === 0 ? (
                    <p className="text-xs text-muted-foreground">本轮没有形成可展示的依据条目。</p>
                  ) : (
                    result.authorities.map((authority, index) => (
                      <div
                        key={`${authority.title}-${index}`}
                        className="rounded-md border border-border/70 p-3"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="rounded bg-sky-50 px-1.5 py-0.5 text-caption text-sky-700 dark:bg-sky-950/30 dark:text-sky-300">
                            {authority.authority_type}
                          </span>
                          <span className="text-sm font-medium text-foreground">
                            {authority.title}
                          </span>
                        </div>
                        {authority.locator && (
                          <p className="mt-1 text-caption text-muted-foreground">
                            {authority.locator}
                          </p>
                        )}
                        {authority.snippet && (
                          <p className="mt-2 rounded bg-muted/40 px-2 py-1 text-xs leading-relaxed text-foreground/80">
                            {authority.snippet}
                          </p>
                        )}
                        {authority.relevance && (
                          <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
                            关联说明（模型归纳）：{authority.relevance}
                          </p>
                        )}
                      </div>
                    ))
                  )}
                </div>
              </section>

              <section className="rounded-lg border border-border bg-background p-4">
                <h4 className="text-sm font-medium text-foreground">可追溯引用（工具事实）</h4>
                <div className="mt-3 space-y-2">
                  {result.citations.length === 0 ? (
                    <p className="text-xs text-muted-foreground">本轮没有形成可回填的引用条目。</p>
                  ) : (
                    result.citations.map((citation, index) => (
                      <div
                        key={`${citation.source_type}-${citation.source_name}-${index}`}
                        className="rounded-md border border-border/70 px-3 py-2 text-xs leading-relaxed"
                      >
                        <p className="font-medium text-foreground">
                          {citation.source_type}：{citation.source_name}
                        </p>
                        {citation.locator && (
                          <p className="text-muted-foreground">{citation.locator}</p>
                        )}
                      </div>
                    ))
                  )}
                </div>
              </section>

              <section className="rounded-lg border border-border bg-background p-4">
                <h4 className="text-sm font-medium text-foreground">工具轨迹（执行记录）</h4>
                <div className="mt-3 space-y-2">
                  {result.tool_trace.length === 0 ? (
                    <p className="text-xs text-muted-foreground">本轮没有执行工具调用。</p>
                  ) : (
                    result.tool_trace.map((trace) => (
                      <div
                        key={trace.tool}
                        className="flex flex-wrap items-center gap-2 rounded-md border border-border/70 px-3 py-2 text-xs"
                      >
                        <span className="font-medium text-foreground">{trace.tool}</span>
                        <span className={traceBadge(trace.success)}>
                          {trace.success ? "success" : "failed"}
                        </span>
                        {trace.kb_hit && (
                          <span className="rounded bg-slate-100 px-1.5 py-0.5 text-slate-700 dark:bg-slate-800/80 dark:text-slate-300">
                            KB 命中
                          </span>
                        )}
                        {trace.credits_used > 0 && (
                          <span className="text-muted-foreground">积分 {trace.credits_used}</span>
                        )}
                        {trace.error_short && (
                          <span className="flex items-center gap-1 text-red-600 dark:text-red-300">
                            <XCircle className="size-3" />
                            {trace.error_short}
                          </span>
                        )}
                      </div>
                    ))
                  )}
                </div>
              </section>
            </div>

            <div className="space-y-4">
              <section className="rounded-lg border border-border bg-background p-4">
                <h4 className="text-sm font-medium text-foreground">风险影响（模型分析）</h4>
                <div className="mt-3 space-y-2 text-xs leading-relaxed text-foreground/80">
                  {result.risk_analysis.length === 0 ? (
                    <p className="text-muted-foreground">本轮没有补充风险影响说明。</p>
                  ) : (
                    result.risk_analysis.map((item, index) => (
                      <p key={index}>
                        {index + 1}. {item}
                      </p>
                    ))
                  )}
                </div>
              </section>

              <section className="rounded-lg border border-border bg-background p-4">
                <h4 className="text-sm font-medium text-foreground">下一步建议（模型分析）</h4>
                <div className="mt-3 space-y-2 text-xs leading-relaxed text-foreground/80">
                  {result.recommended_actions.length === 0 ? (
                    <p className="text-muted-foreground">本轮没有形成明确的下一步建议。</p>
                  ) : (
                    result.recommended_actions.map((item, index) => (
                      <p key={index}>
                        {index + 1}. {item}
                      </p>
                    ))
                  )}
                </div>
              </section>

              <section className="rounded-lg border border-border bg-background p-4">
                <h4 className="text-sm font-medium text-foreground">建议补问（模型分析）</h4>
                <div className="mt-3 space-y-2 text-xs leading-relaxed text-foreground/80">
                  {result.follow_up_questions.length === 0 ? (
                    <p className="text-muted-foreground">当前问题已经足够具体，暂时无需补问。</p>
                  ) : (
                    result.follow_up_questions.map((item, index) => (
                      <p key={index}>
                        {index + 1}. {item}
                      </p>
                    ))
                  )}
                </div>
              </section>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
