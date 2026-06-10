/**
 * V0.2 D6 · 输入框上方的"📎 已引用文件"chip 区。
 *
 * 用户视角:确认引用了哪几份文档,可以 × 删除。
 * 系统视角:从 CaseChatPanel 的 `attached_doc_ids` 状态反推出 doc meta 后传进来。
 *
 * 设计要点:
 *   - 空数组 → 返回 null,整个区域不占位(避免空 div 占行高,详 § 7.1 案件详情布局)
 *   - AI artifact 用 🤖,普通文档用 📄 — 跟 AttachmentPicker 的标识一致
 *   - 文件名超长 truncate,但 tooltip 给完整名(title 属性,系统原生 tooltip)
 *   - chip 一行能放下就放下,溢出 wrap;不做横向 scroll(用户引用顶多 5 份)
 */

import { X } from "lucide-react";

export interface AttachmentChipDoc {
  id: string;
  filename: string;
  /** 是否是 AI 跑出来的中间产物(总览/调查/精要等),决定图标 */
  is_ai_artifact: boolean;
}

interface Props {
  docs: AttachmentChipDoc[];
  /** 用户点 × 时调用 */
  onRemove: (docId: string) => void;
  /** 流式中禁用 × 按钮(避免删了正在被读的引用) */
  disabled?: boolean;
}

export function AttachmentChips({ docs, onRemove, disabled = false }: Props) {
  if (docs.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5 border-b border-border/40 px-3 py-2">
      <span className="text-xs text-muted-foreground shrink-0">已引用:</span>
      {docs.map((d) => (
        <span
          key={d.id}
          title={d.filename}
          className="inline-flex max-w-[200px] items-center gap-1 rounded-md border border-border/60 bg-accent/50 px-2 py-0.5 text-xs"
        >
          <span aria-hidden className="shrink-0">
            {d.is_ai_artifact ? "🤖" : "📄"}
          </span>
          <span className="truncate">{d.filename}</span>
          <button
            type="button"
            aria-label={`移除 ${d.filename}`}
            disabled={disabled}
            onClick={() => onRemove(d.id)}
            className="ml-0.5 grid place-items-center rounded-sm text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:cursor-not-allowed disabled:opacity-40"
          >
            <X className="size-3" />
          </button>
        </span>
      ))}
    </div>
  );
}
