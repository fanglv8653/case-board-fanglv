/**
 * 法律依据弹窗 — 通用组件(2026-05-24 e)。
 *
 * 渲染结构化的"法律依据"内容:H3 段落 + 普通段落 + 引文小字 + 表格。
 * 数据用 TS 结构化对象描述(`LegalBasisSection[]`),不用 dangerouslySetInnerHTML。
 *
 * 用法:每个工具组件 useState 维护 open,触发按钮 → 渲染 <LegalBasisModal />。
 */

import { useEffect } from "react";
import { X } from "lucide-react";

import { openUrl } from "@/lib/api";

/* ============================ 数据结构 ============================ */
export type Block =
  | { type: "para"; text: string }
  | { type: "citation"; text: string } // 小字灰色,引用法条出处
  | { type: "link"; text: string; href: string }
  | { type: "strong"; text: string }
  | { type: "table"; headers: string[]; rows: (string | { strong: string })[][] }
  | { type: "note"; text: string }; // 比 citation 更小的尾注

export interface LegalBasisSection {
  title: string;
  blocks: Block[];
}

interface Props {
  open: boolean;
  onClose: () => void;
  /** 弹窗标题(如 "计费法律依据" / "利息计算法律依据") */
  title: string;
  /** 内容章节(对应 HTML 里的 H3 + 段落 + 表格) */
  sections: LegalBasisSection[];
}

export function LegalBasisModal({ open, onClose, title, sections }: Props) {
  // Esc 关闭
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4 py-8 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="legal-basis-modal-title"
    >
      <div className="flex max-h-full w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-card shadow-2xl">
        {/* Header */}
        <header className="flex shrink-0 items-center justify-between border-b border-border bg-card/80 px-5 py-3">
          <h2
            id="legal-basis-modal-title"
            className="text-sm font-semibold text-foreground"
          >
            {title}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            aria-label="关闭"
            title="关闭(Esc)"
          >
            <X className="size-4" />
          </button>
        </header>

        {/* Body */}
        <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
          <div className="space-y-5">
            {sections.map((sec, i) => (
              <section key={i} className="space-y-2">
                <h3 className="border-b border-border/50 pb-1 text-sm font-semibold text-foreground">
                  {sec.title}
                </h3>
                <div className="space-y-2">
                  {sec.blocks.map((b, j) => (
                    <BlockRenderer key={j} block={b} />
                  ))}
                </div>
              </section>
            ))}
          </div>
        </div>

        {/* Footer */}
        <footer className="shrink-0 border-t border-border bg-muted/30 px-5 py-2 text-caption text-muted-foreground">
          以上内容仅供参考,实际办案以最新法律法规及司法解释为准。
        </footer>
      </div>
    </div>
  );
}

function BlockRenderer({ block }: { block: Block }) {
  switch (block.type) {
    case "para":
      return (
        <p className="text-sm leading-relaxed text-foreground">{block.text}</p>
      );
    case "strong":
      return (
        <p className="text-sm font-medium text-foreground">{block.text}</p>
      );
    case "citation":
      return (
        <p className="text-label leading-relaxed text-muted-foreground">
          —— {block.text}
        </p>
      );
    case "link":
      return (
        <button
          type="button"
          onClick={() =>
            void openUrl(block.href).catch((error) =>
              console.warn("openUrl failed", error),
            )
          }
          className="block break-all text-left text-label text-foreground underline underline-offset-2"
        >
          {block.text}
        </button>
      );
    case "note":
      return (
        <p className="text-label italic text-muted-foreground/85">
          {block.text}
        </p>
      );
    case "table":
      return (
        <div className="overflow-x-auto rounded-md border border-border">
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-border bg-muted/40">
                {block.headers.map((h, i) => (
                  <th
                    key={i}
                    className="px-3 py-1.5 text-left font-medium text-foreground"
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, ri) => (
                <tr
                  key={ri}
                  className="border-b border-border/50 last:border-0"
                >
                  {row.map((cell, ci) => (
                    <td
                      key={ci}
                      className={
                        typeof cell === "string"
                          ? "px-3 py-1.5 text-foreground"
                          : "px-3 py-1.5 font-medium text-foreground"
                      }
                    >
                      {typeof cell === "string" ? cell : cell.strong}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
  }
}

/* ============================ 触发按钮(辅助导出) ============================ */
export function LegalBasisButton({
  onClick,
  children = "法律依据",
}: {
  onClick: () => void;
  children?: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center gap-1 text-xs text-muted-foreground underline-offset-2 transition-colors hover:text-foreground hover:underline"
    >
      📖 {children}
    </button>
  );
}
