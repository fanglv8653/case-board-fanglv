/**
 * 非诉模块入口(2026-05-24 b·首版骨架)。
 *
 * ⚠️ 当前阶段:收集需求 · 计划开发中(占位 + 字段框架预览)
 * - 数据来源:`docs/抽取与聚合方法论-v0.1.md` §3(非诉字段清单 — 框架级)
 * - 真正的非诉案件导入 / LLM 抽取 / 详情看板,等需求收集完再做
 * - 目的:把整个非诉的设计意图视觉化,作者可以拿这个页面给同事看做引导
 *
 * 这个模块完全独立 — 不依赖诉讼模块的任何 state、组件、IPC。
 * 后续真做时,在本目录下加 `Module/components/*` 即可。
 */

import { FileQuestion, Sparkles } from "lucide-react";

import { BusinessTypeTable } from "./components/BusinessTypeTable";
import { FieldFrameworkTable } from "./components/FieldFrameworkTable";

export function TransactionModule() {
  return (
    <main className="flex h-full w-full flex-col bg-background">
      {/* 顶部说明条 */}
      <div className="border-b border-amber-200/70 bg-amber-50/60 px-8 py-3">
        <div className="mx-auto flex max-w-6xl items-start gap-3">
          <Sparkles className="mt-0.5 size-4 shrink-0 text-amber-700" />
          <div className="min-w-0 flex-1 text-xs text-amber-900">
            <p className="font-medium">非诉模块 · 收集需求 · 计划开发中</p>
            <p className="mt-0.5 text-amber-800/85">
              CaseBoard 当前主力是诉讼案件管理。非诉模块正在收集需求、计划开发中,
              字段定义 / 抽取 prompt / 里程碑识别仍在对齐。下面是当前的字段框架,供查看与反馈。
            </p>
          </div>
        </div>
      </div>

      {/* 正文 */}
      <div className="flex-1 overflow-auto">
        <div className="mx-auto max-w-6xl space-y-6 px-8 py-6">
          {/* Hero */}
          <header className="rounded-xl border border-border bg-card px-6 py-5">
            <div className="flex items-start gap-3">
              <FileQuestion className="mt-0.5 size-5 shrink-0 text-muted-foreground" />
              <div>
                <h1 className="text-lg font-semibold tracking-tight text-foreground">
                  非诉项目看板
                </h1>
                <p className="mt-1 text-xs text-muted-foreground">
                  并购 · 增资 · 破产清算 · 合规审查 · 商标专利 · 行政许可 · 合同尽调
                  —— 跟诉讼看板共用同一套 App,但字段、阶段、文档归类完全独立。
                </p>
              </div>
            </div>
          </header>

          {/* 业务类型表 */}
          <section className="space-y-2">
            <SectionTitle title="非诉典型业务类型" subtitle="决定下批要支持的 project_type 枚举" />
            <BusinessTypeTable />
          </section>

          {/* 字段框架表 */}
          <section className="space-y-2">
            <SectionTitle
              title="应抽取字段类别"
              subtitle="对齐诉讼字段命名,差异点单独标注。详见 docs/抽取与聚合方法论-v0.1.md §3.2"
            />
            <FieldFrameworkTable />
          </section>
        </div>
      </div>
    </main>
  );
}

function SectionTitle({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 px-1">
      <h2 className="text-sm font-semibold text-foreground">{title}</h2>
      {subtitle && (
        <span className="text-label text-muted-foreground">{subtitle}</span>
      )}
    </div>
  );
}
