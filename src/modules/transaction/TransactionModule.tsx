/**
 * 非诉模块入口(2026-06-17 重写)。
 *
 * 原占位(字段框架预览)已移除;非诉 tab 当前承载「合同审查」功能 ——
 * 上传合同 .docx → AI 三层审查 → 风险清单 + 审查结论 + 导出审查意见书 Word。
 *
 * 见 `docs/提案-合同审查-2026-06-17.md`(含实施进度 checklist)。
 * 本模块完全独立 —— 不依赖诉讼模块的任何 state、组件、IPC。
 */

import { ShieldCheck } from "lucide-react";

import { BetaBadge } from "@/components/BetaBadge";
import { ContractReviewTool } from "./ContractReviewTool";

export function TransactionModule() {
  return (
    <main className="flex h-full w-full flex-col bg-background">
      <div className="flex-1 overflow-auto">
        <div className="mx-auto max-w-4xl space-y-5 px-8 py-6">
          {/* Hero */}
          <header className="flex items-start gap-3">
            <ShieldCheck className="mt-0.5 size-5 shrink-0 text-sky-600 dark:text-sky-400" />
            <div>
              <div className="flex items-center gap-2">
                <h1 className="text-lg font-semibold tracking-tight text-foreground">合同审查</h1>
                <BetaBadge />
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                交易结构 · 文本形式 · 条款语言 三层扫描,分级风险清单 + 审查意见书 ——
                给非诉合同把关。
              </p>
              <p className="mt-1 text-[11px] text-muted-foreground/70">
                审查方法论参考杨卫薪律师 contract-copilot(CC BY-NC),prompt / 引擎 /
                意见书均由本系统自建。
              </p>
            </div>
          </header>

          <ContractReviewTool />
        </div>
      </div>
    </main>
  );
}
