import { useEffect, useState } from "react";
import { open as dialogOpen, save as dialogSave } from "@tauri-apps/plugin-dialog";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  AlertTriangle,
  CheckCircle2,
  FileSearch,
  FileText,
  Gavel,
  Loader2,
  ScrollText,
  Search,
  ShieldAlert,
  Upload,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  convertDocToDocx,
  exportContractOpinionDocx,
  exportContractRedlineDocx,
  reviewContractDocx,
  type ContractReviewResponse,
  type RedlineSummary,
  type ReviewRisk,
} from "@/lib/api";
import { ContractResearchPanel, type ContractResearchSeed } from "./ContractResearchPanel";

const DIRECT_EXT = "docx";
const CONVERT_EXTS = ["doc", "rtf", "odt"];
const SUPPORTED_EXTS = [DIRECT_EXT, ...CONVERT_EXTS];

type Stance = "party_a" | "party_b" | "neutral";
type Strictness = "lenient" | "normal" | "aggressive";

const STANCE_OPTIONS: { id: Stance; label: string; hint: string }[] = [
  { id: "party_a", label: "我方代表甲方", hint: "优先保护甲方利益" },
  { id: "party_b", label: "我方代表乙方", hint: "优先保护乙方利益" },
  { id: "neutral", label: "中立审查", hint: "不预设偏向任何一方" },
];

const STRICTNESS_OPTIONS: { id: Strictness; label: string; hint: string }[] = [
  { id: "lenient", label: "克制", hint: "只提示硬伤与高风险" },
  { id: "normal", label: "常规", hint: "标准审查力度" },
  { id: "aggressive", label: "强势", hint: "尽量挑出可谈判空间" },
];

function extOf(path: string): string {
  return (path.split(".").pop() || "").toLowerCase();
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

function stanceLabel(stance: Stance): string {
  if (stance === "party_a") return "甲方";
  if (stance === "party_b") return "乙方";
  return "我方";
}

function levelStyle(level: string): { badge: string; card: string; label: string } {
  const normalized = level.toUpperCase();
  if (normalized === "P0") {
    return {
      badge: "bg-red-100 text-red-700 dark:bg-red-950/40 dark:text-red-300",
      card: "border-red-200/70 dark:border-red-900/40",
      label: "P0 优先处理",
    };
  }
  if (normalized === "P1") {
    return {
      badge: "bg-amber-100 text-amber-700 dark:bg-amber-950/40 dark:text-amber-300",
      card: "border-amber-200/70 dark:border-amber-900/40",
      label: "P1 建议修改",
    };
  }
  return {
    badge: "bg-slate-100 text-slate-600 dark:bg-slate-800/60 dark:text-slate-300",
    card: "border-border",
    label: "P2 优化项",
  };
}

function verdictStyle(verdict: string): string {
  if (verdict.includes("不建议")) {
    return "bg-red-50 border-red-200 text-red-700 dark:bg-red-950/20 dark:border-red-900/40 dark:text-red-300";
  }
  if (verdict.includes("有条件")) {
    return "bg-amber-50 border-amber-200 text-amber-800 dark:bg-amber-950/20 dark:border-amber-900/40 dark:text-amber-300";
  }
  if (verdict.includes("可签")) {
    return "bg-emerald-50 border-emerald-200 text-emerald-700 dark:bg-emerald-950/20 dark:border-emerald-900/40 dark:text-emerald-300";
  }
  return "bg-sky-50 border-sky-200 text-sky-800 dark:bg-sky-950/20 dark:border-sky-900/40 dark:text-sky-300";
}

function buildManualResearchQuestion(contractType?: string | null): string {
  const normalized = contractType?.trim();
  if (!normalized) return "";
  return `请围绕「${normalized}」合同，检索中国法下常见高风险条款、可直接援引的法条和代表性类案。`;
}

function buildRiskResearchSeed(
  risk: ReviewRisk,
  stance: Stance,
  contractType?: string | null,
): ContractResearchSeed {
  const contractPrefix = contractType?.trim() ? `围绕「${contractType.trim()}」合同，` : "";
  const clauseHint = risk.clause_ref ? `重点关注条款位置：${risk.clause_ref}。` : "";
  const anchorHint = risk.anchor_text ? `原文片段：${risk.anchor_text}。` : "";
  return {
    question:
      `${contractPrefix}请检索与「${risk.title}」相关的中国法依据、监管规则和代表性类案，并说明对${stanceLabel(stance)}的风险影响。` +
      clauseHint +
      anchorHint,
    riskTitle: risk.title,
    clauseRef: risk.clause_ref || undefined,
    anchorText: risk.anchor_text || undefined,
  };
}

function countByLevel(risks: ReviewRisk[]) {
  return risks.reduce(
    (acc, risk) => {
      const normalized = risk.level.toUpperCase();
      if (normalized === "P0") acc.p0 += 1;
      else if (normalized === "P1") acc.p1 += 1;
      else acc.p2 += 1;
      return acc;
    },
    { p0: 0, p1: 0, p2: 0 },
  );
}

function RiskCard({
  risk,
  index,
  onResearch,
}: {
  risk: ReviewRisk;
  index: number;
  onResearch?: (risk: ReviewRisk) => void;
}) {
  const style = levelStyle(risk.level);

  return (
    <div className={`rounded-lg border bg-card p-4 ${style.card}`}>
      <div className="flex items-start gap-2">
        <span
          className={`mt-0.5 shrink-0 rounded px-1.5 py-0.5 text-caption font-semibold ${style.badge}`}
        >
          {style.label}
        </span>
        <div className="min-w-0 flex-1">
          <h4 className="text-sm font-medium text-foreground">
            {index}. {risk.title}
            {risk.clause_ref && (
              <span className="ml-2 text-xs font-normal text-muted-foreground">
                {risk.clause_ref}
              </span>
            )}
          </h4>
        </div>
        {risk.action === "revise" && (
          <span className="shrink-0 rounded bg-sky-50 px-1.5 py-0.5 text-caption text-sky-700 dark:bg-sky-950/30 dark:text-sky-300">
            建议改正文
          </span>
        )}
      </div>

      <div className="mt-2 space-y-1.5 text-xs leading-relaxed">
        {risk.consequence && (
          <p className="text-foreground/80">
            <span className="text-muted-foreground">风险后果：</span>
            {risk.consequence}
          </p>
        )}
        {risk.anchor_text && (
          <p className="rounded bg-muted/50 px-2 py-1 font-mono text-foreground/70">
            <span className="text-muted-foreground">原文：</span>
            {risk.anchor_text}
          </p>
        )}
        {risk.suggestion && (
          <p className="text-foreground/80">
            <span className="text-muted-foreground">整改建议：</span>
            {risk.suggestion}
          </p>
        )}
        {risk.recommended_text && (
          <p className="rounded border border-sky-200/60 bg-sky-50/50 px-2 py-1 text-sky-900 dark:border-sky-900/40 dark:bg-sky-950/20 dark:text-sky-200">
            <span className="text-sky-600 dark:text-sky-400">推荐措辞：</span>
            {risk.recommended_text}
          </p>
        )}
        {risk.basis && (
          <p className="text-muted-foreground">
            <span>审查依据：</span>
            {risk.basis}
          </p>
        )}
      </div>

      {onResearch && (
        <div className="mt-3 flex justify-end">
          <Button size="sm" variant="outline" onClick={() => onResearch(risk)}>
            <Search className="size-3.5" />
            查本条依据
          </Button>
        </div>
      )}
    </div>
  );
}

export function ContractReviewTool() {
  const [docxPath, setDocxPath] = useState<string | null>(null);
  const [stance, setStance] = useState<Stance>("neutral");
  const [strictness, setStrictness] = useState<Strictness>("normal");
  const [hint, setHint] = useState("");

  const [reviewing, setReviewing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [converting, setConverting] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [busyMsg, setBusyMsg] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [resp, setResp] = useState<ContractReviewResponse | null>(null);
  const [redlineInfo, setRedlineInfo] = useState<RedlineSummary | null>(null);
  const [researchOpen, setResearchOpen] = useState(false);
  const [researchSeed, setResearchSeed] = useState<ContractResearchSeed | null>(null);

  const busy = reviewing || exporting || converting;
  const fileName = docxPath ? docxPath.split(/[\\/]/).pop() : null;
  const contractName = resp?.contract_name || fileName || null;
  const contractType = resp?.result.contract_type || hint.trim() || null;
  const risks = resp?.result.risks ?? [];
  const sortedRisks = [...risks].sort((a, b) => {
    const rank = (value: string) =>
      value.toUpperCase() === "P0" ? 0 : value.toUpperCase() === "P1" ? 1 : 2;
    return rank(a.level) - rank(b.level);
  });
  const counts = countByLevel(sortedRisks);

  const segCls = (active: boolean) =>
    `flex-1 rounded-md px-3 py-1.5 text-xs transition-colors ${
      active
        ? "bg-sky-100 font-medium text-sky-800 dark:bg-sky-950/40 dark:text-sky-200"
        : "text-muted-foreground hover:bg-accent"
    }`;

  async function handleFileSelected(path: string) {
    setError(null);
    setResp(null);
    setRedlineInfo(null);
    setResearchOpen(false);
    setResearchSeed(null);

    const ext = extOf(path);
    if (ext === DIRECT_EXT) {
      setDocxPath(path);
      return;
    }

    if (CONVERT_EXTS.includes(ext)) {
      setConverting(true);
      setBusyMsg(`正在把 .${ext} 转成 .docx…`);
      try {
        const converted = await convertDocToDocx(path);
        setDocxPath(converted);
        setBusyMsg("已完成格式转换，可以开始审查。");
        window.setTimeout(() => setBusyMsg(""), 3000);
      } catch (e) {
        setError(formatError(e));
        setBusyMsg("");
      } finally {
        setConverting(false);
      }
      return;
    }

    setError(`暂不支持 .${ext}。请上传 Word 合同（.docx，旧版 .doc/.rtf/.odt 会先自动转换）。`);
  }

  async function handlePickFile() {
    setError(null);
    try {
      const picked = await dialogOpen({
        directory: false,
        multiple: false,
        filters: [{ name: "Word 合同", extensions: SUPPORTED_EXTS }],
      });
      if (typeof picked !== "string" || !picked.trim()) return;
      await handleFileSelected(picked);
    } catch (e) {
      setError(formatError(e));
    }
  }

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          setDragging(true);
        } else if (payload.type === "drop") {
          setDragging(false);
          const path = payload.paths[0];
          if (path) void handleFileSelected(path);
        } else {
          setDragging(false);
        }
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => console.warn("listen drag-drop failed", e));

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  async function handleReview() {
    if (!docxPath) return;
    setError(null);
    setResp(null);
    setRedlineInfo(null);
    setResearchOpen(false);
    setResearchSeed(null);
    setReviewing(true);
    setBusyMsg("正在通读合同并执行三层审查，通常需要 30-90 秒。");
    try {
      const result = await reviewContractDocx(docxPath, stance, strictness, hint.trim());
      setResp(result);
    } catch (e) {
      setError(formatError(e));
    } finally {
      setReviewing(false);
      setBusyMsg("");
    }
  }

  async function handleExportOpinion() {
    if (!resp) return;
    setError(null);
    try {
      const safeName = (resp.contract_name || "合同").replace(/[\\/:*?"<>|]/g, "_").slice(0, 40);
      const picked = await dialogSave({
        defaultPath: `${safeName}_审查意见书.docx`,
        filters: [{ name: "Word 文档", extensions: ["docx"] }],
      });
      if (typeof picked !== "string" || !picked.trim()) return;

      setExporting(true);
      setBusyMsg("正在导出审查意见书…");
      await exportContractOpinionDocx(resp.result, resp.contract_name, stance, strictness, picked);
      setBusyMsg("审查意见书已导出。");
    } catch (e) {
      setError(formatError(e));
    } finally {
      setExporting(false);
      window.setTimeout(() => setBusyMsg(""), 5000);
    }
  }

  async function handleExportRedline() {
    if (!resp || !docxPath) return;
    setError(null);
    try {
      const safeName = (resp.contract_name || "合同").replace(/[\\/:*?"<>|]/g, "_").slice(0, 40);
      const picked = await dialogSave({
        defaultPath: `${safeName}_修订批注版.docx`,
        filters: [{ name: "Word 文档", extensions: ["docx"] }],
      });
      if (typeof picked !== "string" || !picked.trim()) return;

      setExporting(true);
      setBusyMsg("正在生成修订痕迹与批注版…");
      const summary = await exportContractRedlineDocx(docxPath, resp.result, "", picked);
      setRedlineInfo(summary);
      setBusyMsg(
        `修订批注版已导出：行内修订 ${summary.applied_inline} 处，整段批注 ${summary.applied_comment} 处。`,
      );
    } catch (e) {
      setError(formatError(e));
    } finally {
      setExporting(false);
      window.setTimeout(() => setBusyMsg(""), 6000);
    }
  }

  function handleOpenManualResearch() {
    setResearchSeed({ question: buildManualResearchQuestion(contractType) });
    setResearchOpen(true);
  }

  function handleRiskResearch(risk: ReviewRisk) {
    setResearchSeed(buildRiskResearchSeed(risk, stance, contractType));
    setResearchOpen(true);
  }

  return (
    <div className="relative space-y-5">
      {dragging && (
        <div className="pointer-events-none absolute inset-0 z-50 flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed border-sky-400 bg-sky-50/90 backdrop-blur-sm animate-in fade-in-0 duration-150 dark:bg-sky-950/40">
          <Upload className="size-10 text-sky-500" />
          <p className="text-base font-semibold text-sky-700 dark:text-sky-300">
            松开即可载入这份合同
          </p>
          <p className="text-xs text-sky-600 dark:text-sky-400">
            支持 .docx，旧版 .doc / .rtf / .odt 会先自动转换
          </p>
        </div>
      )}

      <section className="rounded-lg border border-sky-200/70 bg-sky-50/50 p-4 dark:border-sky-900/40 dark:bg-sky-950/15">
        <div className="flex items-start gap-2.5">
          <ShieldAlert className="mt-0.5 size-4 shrink-0 text-sky-600 dark:text-sky-400" />
          <div className="text-sm leading-relaxed text-foreground">
            上传或拖入一份合同后，系统会按“交易结构 / 文本形式 / 条款语言”三层执行审查，输出分级风险清单、审查结论、
            推荐修改措辞，并保留原有的意见书导出与修订批注版导出链路。
            <span className="mt-1 block text-xs text-muted-foreground">
              本轮新增的“法律检索”入口只做合同审查场景内的最小辅助，不接入案件聊天外壳，也不写入聊天历史。
            </span>
          </div>
        </div>
      </section>

      <section className="space-y-2 rounded-lg border border-border bg-background p-4">
        <div className="flex items-center gap-2">
          <FileText className="size-4 text-foreground/70" />
          <h3 className="text-sm font-medium text-foreground">1. 选择合同文件</h3>
        </div>
        <div className="flex items-center gap-3">
          <Button size="sm" variant="outline" onClick={handlePickFile} disabled={busy}>
            {converting ? <Loader2 className="size-3.5 animate-spin" /> : <Upload className="size-3.5" />}
            选择合同
          </Button>
          {fileName ? (
            <span className="truncate text-xs text-foreground/80" title={fileName}>
              {fileName}
            </span>
          ) : (
            <span className="text-xs text-muted-foreground">
              支持拖入或选择 Word 合同（扫描件请先 OCR）
            </span>
          )}
        </div>
      </section>

      <section className="space-y-3 rounded-lg border border-border bg-background p-4">
        <div className="flex items-center gap-2">
          <Gavel className="size-4 text-foreground/70" />
          <h3 className="text-sm font-medium text-foreground">2. 审查立场与口径</h3>
        </div>

        <div className="space-y-1.5">
          <label className="text-xs text-muted-foreground">我方立场</label>
          <div className="flex gap-1.5 rounded-lg border border-border bg-muted/30 p-1">
            {STANCE_OPTIONS.map((option) => (
              <button
                key={option.id}
                type="button"
                onClick={() => setStance(option.id)}
                disabled={busy}
                className={segCls(stance === option.id)}
                title={option.hint}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>

        <div className="space-y-1.5">
          <label className="text-xs text-muted-foreground">审查口径</label>
          <div className="flex gap-1.5 rounded-lg border border-border bg-muted/30 p-1">
            {STRICTNESS_OPTIONS.map((option) => (
              <button
                key={option.id}
                type="button"
                onClick={() => setStrictness(option.id)}
                disabled={busy}
                className={segCls(strictness === option.id)}
                title={option.hint}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>

        <div className="space-y-1.5">
          <label className="text-xs text-muted-foreground">合同类型（可选）</label>
          <input
            value={hint}
            onChange={(event) => setHint(event.target.value)}
            disabled={busy}
            placeholder="例如：房屋租赁合同 / 股权转让协议"
            className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-sky-400"
          />
        </div>
      </section>

      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={handleReview} disabled={!docxPath || busy}>
          {reviewing ? <Loader2 className="size-4 animate-spin" /> : <ShieldAlert className="size-4" />}
          开始审查
        </Button>
        <Button variant="outline" onClick={handleOpenManualResearch} disabled={busy}>
          <FileSearch className="size-4" />
          法律检索
        </Button>
        {busyMsg && <span className="text-xs text-muted-foreground">{busyMsg}</span>}
      </div>

      {error && (
        <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          <p className="font-medium">执行失败</p>
          <p className="mt-0.5 break-all font-mono">{error}</p>
        </div>
      )}

      <ContractResearchPanel
        open={researchOpen}
        contractName={contractName}
        contractType={contractType}
        stance={stance}
        seed={researchSeed}
        onClose={() => setResearchOpen(false)}
      />

      {resp && (
        <div className="space-y-4">
          <section className={`space-y-2 rounded-lg border p-4 ${verdictStyle(resp.result.conclusion.verdict)}`}>
            <div className="flex flex-wrap items-center gap-2">
              <CheckCircle2 className="size-4 shrink-0" />
              <h3 className="text-sm font-semibold">
                审查结论：{resp.result.conclusion.verdict || "未输出"}
              </h3>
              {resp.result.contract_type && (
                <span className="rounded bg-background/60 px-1.5 py-0.5 text-caption">
                  {resp.result.contract_type}
                </span>
              )}
            </div>
            {resp.result.conclusion.summary && (
              <p className="text-xs leading-relaxed">{resp.result.conclusion.summary}</p>
            )}
            {resp.result.conclusion.preconditions.length > 0 && (
              <div className="text-xs leading-relaxed">
                <p className="font-medium">签署前先决事项</p>
                <ol className="ml-4 list-decimal space-y-0.5">
                  {resp.result.conclusion.preconditions.map((item, index) => (
                    <li key={index}>{item}</li>
                  ))}
                </ol>
              </div>
            )}
          </section>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2 text-xs">
              <AlertTriangle className="size-3.5 text-muted-foreground" />
              <span className="text-muted-foreground">
                共 {sortedRisks.length} 项风险，覆盖段落 {resp.paragraph_count}
              </span>
              {counts.p0 > 0 && (
                <span className="rounded bg-red-100 px-1.5 py-0.5 font-medium text-red-700 dark:bg-red-950/40 dark:text-red-300">
                  P0 × {counts.p0}
                </span>
              )}
              {counts.p1 > 0 && (
                <span className="rounded bg-amber-100 px-1.5 py-0.5 font-medium text-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
                  P1 × {counts.p1}
                </span>
              )}
              {counts.p2 > 0 && (
                <span className="rounded bg-slate-100 px-1.5 py-0.5 font-medium text-slate-600 dark:bg-slate-800/60 dark:text-slate-300">
                  P2 × {counts.p2}
                </span>
              )}
            </div>

            <div className="flex items-center gap-2">
              <Button size="sm" variant="outline" onClick={handleExportOpinion} disabled={busy}>
                {exporting ? <Loader2 className="size-3.5 animate-spin" /> : <ScrollText className="size-3.5" />}
                审查意见书
              </Button>
              <Button size="sm" onClick={handleExportRedline} disabled={busy}>
                {exporting ? <Loader2 className="size-3.5 animate-spin" /> : <FileText className="size-3.5" />}
                修订批注版
              </Button>
            </div>
          </div>

          {redlineInfo && (
            <div className="rounded-md border border-emerald-200 bg-emerald-50/60 px-3 py-2 text-xs text-emerald-800 dark:border-emerald-900/40 dark:bg-emerald-950/20 dark:text-emerald-300">
              已生成修订批注版：行内修订 <strong>{redlineInfo.applied_inline}</strong> 处，整段批注{" "}
              <strong>{redlineInfo.applied_comment}</strong> 处。
              {redlineInfo.skipped.length > 0 && (
                <span> 另有 {redlineInfo.skipped.length} 项未能精确落点，请以审查意见书为准。</span>
              )}
            </div>
          )}

          <div className="space-y-3">
            {sortedRisks.length === 0 ? (
              <p className="rounded-lg border border-border bg-card p-4 text-sm text-muted-foreground">
                本轮未识别到明显风险点。
              </p>
            ) : (
              sortedRisks.map((risk, index) => (
                <RiskCard
                  key={`${risk.title}-${index}`}
                  risk={risk}
                  index={index + 1}
                  onResearch={handleRiskResearch}
                />
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
