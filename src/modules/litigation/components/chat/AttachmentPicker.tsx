/**
 * V0.2 D6 · 「📎 引用文件」选择器(modal)。
 *
 * 用户视角:点输入框上的 📎,弹出本案件文档列表,勾选要让 AI 引用的文档,确认。
 * 系统视角:从 CaseChatPanel 拿到 case 的 documents,自己负责筛选/搜索/多选,
 *           确认时把选中 id 数组回传,**不**自己调后端。
 *
 * 设计要点(详 § 7.2):
 *   - 三态显示:
 *     1. 顶部搜索框(模糊匹配 filename;关键词高亮简化为不高亮,只过滤)
 *     2. ⭐ 置顶区(`pinned_at NOT NULL`,按 pinned_at 倒序)
 *     3. 📅 全部区(按 modified_at 倒序;若 modified_at 为 null 则放末尾)
 *   - 5 份硬上限:超过时勾选按钮置灰 + 底部红字提示
 *   - 🤖 AI artifact 用图标区分(`is_ai_artifact=true`)
 *   - 修改 vs 取消:点击勾选只改本地 state,**点「确认引用」才回调** onConfirm
 *   - Esc / 遮罩 / 取消按钮:都走 onClose,不应用变更
 *   - 不写 DB:置顶切换是后续 D7 的能力,这里只读 pinned_at
 *
 * 不抄 MarkdownModal 的全屏布局 — 这里走"中等尺寸卡片"(max-w-md,max-h-[80vh] 滚动),
 * 因为只是个选择器,不要喧宾夺主。
 */

import { useEffect, useMemo, useState } from "react";
import { X, Search, Pin, Calendar, FileText, Sparkles } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { Document } from "@/lib/types";
import { cn } from "@/lib/utils";

/** 单次能引用的文档上限,跟后端 Settings.chat_max_attached 默认值保持一致(§ 3.2)。 */
const MAX_ATTACHED = 5;

interface Props {
  /** 弹窗是否打开 */
  open: boolean;
  /** 当前案件全部 documents(由 CaseChatPanel 传入,这里不重复 fetch) */
  docs: Document[];
  /** 已经引用的 doc id 列表(initialSelected) */
  initialSelected: string[];
  /** 取消 / Esc / 遮罩点击 */
  onClose: () => void;
  /** 点「确认引用」后回调,把选中 id 数组传出去 */
  onConfirm: (selectedIds: string[]) => void;
}

export function AttachmentPicker({
  open,
  docs,
  initialSelected,
  onClose,
  onConfirm,
}: Props) {
  const [selected, setSelected] = useState<Set<string>>(() => new Set(initialSelected));
  const [query, setQuery] = useState("");

  // 每次重新打开都把 selected 跟 initialSelected 对齐
  useEffect(() => {
    if (open) {
      setSelected(new Set(initialSelected));
      setQuery("");
    }
  }, [open, initialSelected]);

  // Esc 关闭
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const { pinned, others } = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = q
      ? docs.filter((d) => d.filename.toLowerCase().includes(q))
      : docs;
    const pinnedDocs = filtered
      .filter((d) => d.pinned_at != null)
      .sort((a, b) => (b.pinned_at ?? "").localeCompare(a.pinned_at ?? ""));
    const otherDocs = filtered
      .filter((d) => d.pinned_at == null)
      .sort((a, b) => {
        const ma = a.modified_at ?? "";
        const mb = b.modified_at ?? "";
        if (!ma && !mb) return 0;
        if (!ma) return 1;
        if (!mb) return -1;
        return mb.localeCompare(ma);
      });
    return { pinned: pinnedDocs, others: otherDocs };
  }, [docs, query]);

  if (!open) return null;

  const overLimit = selected.size > MAX_ATTACHED;
  const atLimit = selected.size >= MAX_ATTACHED;

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        if (next.size >= MAX_ATTACHED) {
          // 硬上限:超额不加,UI 已经把勾选按钮置灰
          return prev;
        }
        next.add(id);
      }
      return next;
    });
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="选择要引用的案件文档"
        className="flex max-h-[80vh] w-full max-w-md flex-col rounded-lg border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 标题栏 */}
        <div className="flex items-center justify-between border-b px-4 py-3">
          <h2 className="text-sm font-semibold">📎 选择引用文档</h2>
          <button
            type="button"
            aria-label="关闭"
            onClick={onClose}
            className="grid place-items-center rounded-md p-1 text-muted-foreground hover:bg-accent"
          >
            <X className="size-4" />
          </button>
        </div>

        {/* 搜索框 */}
        <div className="border-b px-4 py-2">
          <div className="flex items-center gap-2 rounded-md border bg-background px-2">
            <Search className="size-4 text-muted-foreground" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="搜索文件名…"
              className="h-8 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
              autoFocus
            />
          </div>
        </div>

        {/* 列表区 — 可滚动 */}
        <div className="flex-1 overflow-y-auto">
          {pinned.length === 0 && others.length === 0 ? (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
              {query ? `没有匹配「${query}」的文件` : "本案件还没有文档"}
            </div>
          ) : (
            <>
              {pinned.length > 0 && (
                <Section
                  icon={<Pin className="size-3.5" />}
                  title={`置顶 (${pinned.length})`}
                >
                  {pinned.map((d) => (
                    <DocRow
                      key={d.id}
                      doc={d}
                      checked={selected.has(d.id)}
                      disabled={!selected.has(d.id) && atLimit}
                      onToggle={() => toggle(d.id)}
                    />
                  ))}
                </Section>
              )}
              {others.length > 0 && (
                <Section
                  icon={<Calendar className="size-3.5" />}
                  title={`全部 (${others.length},按修改时间倒序)`}
                >
                  {others.map((d) => (
                    <DocRow
                      key={d.id}
                      doc={d}
                      checked={selected.has(d.id)}
                      disabled={!selected.has(d.id) && atLimit}
                      onToggle={() => toggle(d.id)}
                    />
                  ))}
                </Section>
              )}
            </>
          )}
        </div>

        {/* 底栏:计数 + 操作 */}
        <div className="flex items-center justify-between gap-2 border-t bg-muted/30 px-4 py-3">
          <span
            className={cn(
              "text-xs",
              overLimit
                ? "text-destructive"
                : atLimit
                  ? "text-amber-600"
                  : "text-muted-foreground",
            )}
          >
            已选 {selected.size} 份 (最多 {MAX_ATTACHED} 份)
          </span>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onClose}>
              取消
            </Button>
            <Button
              size="sm"
              disabled={overLimit}
              onClick={() => onConfirm(Array.from(selected))}
            >
              确认引用
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

interface SectionProps {
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
}

function Section({ icon, title, children }: SectionProps) {
  return (
    <div>
      <div className="sticky top-0 z-10 flex items-center gap-1.5 border-b border-border/40 bg-background px-4 py-1.5 text-xs font-medium text-muted-foreground">
        {icon}
        {title}
      </div>
      <ul className="divide-y divide-border/40">{children}</ul>
    </div>
  );
}

interface DocRowProps {
  doc: Document;
  checked: boolean;
  /** 已经勾满 5 份且自己未选中时为 true */
  disabled: boolean;
  onToggle: () => void;
}

function DocRow({ doc, checked, disabled, onToggle }: DocRowProps) {
  return (
    <li>
      <label
        className={cn(
          "flex cursor-pointer items-center gap-2 px-4 py-2 text-sm transition-colors",
          checked ? "bg-accent" : "hover:bg-accent/40",
          disabled && "cursor-not-allowed opacity-50",
        )}
      >
        <input
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={onToggle}
          className="size-4 rounded border-border"
        />
        <span aria-hidden className="shrink-0 text-base">
          {doc.is_ai_artifact ? (
            <Sparkles className="size-4 text-violet-500" />
          ) : (
            <FileText className="size-4 text-muted-foreground" />
          )}
        </span>
        <span className="min-w-0 flex-1 truncate" title={doc.filename}>
          {doc.filename}
        </span>
        {doc.is_ai_artifact && (
          <span className="shrink-0 rounded-sm bg-violet-500/10 px-1.5 py-0.5 text-caption font-medium text-violet-700 dark:text-violet-300">
            AI 生成
          </span>
        )}
      </label>
    </li>
  );
}
