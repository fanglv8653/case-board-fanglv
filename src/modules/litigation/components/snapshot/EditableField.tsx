/**
 * 2026-05-26 V0.1.13 · contentEditable 字段包装。
 *
 * 关键设计点(避免 contentEditable cursor jump):
 *   - 不用受控 `children={value}`,改用 dangerouslySetInnerHTML 一次性初始化。
 *   - 失焦(blur) / Enter / Esc 时才回传 onCommit。
 *   - 父组件务必用 `key={`${caseId}:${path}`}` 让 React 在 case 切换时 remount,
 *     普通 re-render 不会重新挂载,光标不跳。
 *   - Esc 撤销当前未提交编辑(恢复 initialValue)。
 *
 * 关键设计点(防"切编辑态丢未保存改动"):
 *   - 用户改完点右上铅笔退出编辑模式 → 条件渲染让 EditableField 卸载,
 *     blur 事件**不会触发**(元素直接消失,不是失焦),onCommit 不被调,改动丢。
 *   - 修复:useEffect cleanup 里手动 diff DOM 内容跟 initialValue,有变化就调 onCommit。
 *   - 用 ref 装 onCommit / initialValue 避免 cleanup 闭包拿到 stale 引用。
 *
 * 不接管样式 — 父组件传 className,EditableField 只加 [contenteditable] 边框 +
 * focus ring,跟现有 FactRow 的字段排版兼容。
 */
import { useEffect, useRef } from "react";
import { RotateCcw } from "lucide-react";

import { cn } from "@/lib/utils";

export interface EditableFieldProps {
  /** 初始显示值(null/空都显示 placeholder) */
  initialValue: string | null;
  /** 失焦时回传清洗后的值。空白(trim 后 "")回 null,表示"用户清空" */
  onCommit: (value: string | null) => void;
  /** 没值时的占位文字,如"未填" */
  placeholder?: string;
  /** 父组件控制是否进入可编辑态 */
  editable: boolean;
  /** 透传给 wrapper(让父组件控字号/字体/颜色) */
  className?: string;
  /** 编辑态额外样式(默认加 outline) */
  editableClassName?: string;
  /** ARIA 标签 */
  ariaLabel?: string;
  /**
   * 当前字段是否被用户改过(applyOverrides 命中)。
   * true 时编辑态右侧显示↺"恢复"按钮 — 让该字段重新跟随 LLM 重抽值。
   */
  hasOverride?: boolean;
  /** 点恢复按钮触发,清掉此字段的 override(applyOverrides 找不到 path 即回到 LLM 值) */
  onReset?: () => void;
}

export function EditableField({
  initialValue,
  onCommit,
  placeholder = "未填",
  editable,
  className,
  editableClassName,
  ariaLabel,
  hasOverride = false,
  onReset,
}: EditableFieldProps) {
  const ref = useRef<HTMLSpanElement | null>(null);
  // 把 onCommit 和 initialValue 装进 ref,unmount cleanup 时不会拿到 stale 闭包
  const onCommitRef = useRef(onCommit);
  const initialRef = useRef(initialValue);
  useEffect(() => {
    onCommitRef.current = onCommit;
  }, [onCommit]);
  useEffect(() => {
    initialRef.current = initialValue;
  }, [initialValue]);

  // 关键:initialValue 变了(切案件 / 外部 refetch 改了字段)时,**只在非编辑态**同步显示值。
  // 编辑态用户正在 type 时绝不覆盖 DOM,否则光标跳走 + 改动丢失。
  useEffect(() => {
    if (!ref.current) return;
    if (document.activeElement === ref.current) return; // 用户正在编辑,不动
    ref.current.textContent = initialValue ?? "";
  }, [initialValue]);

  // unmount-time flush:点铅笔退出编辑模式 / 切案件时,blur 不会自动触发,
  // 这里 cleanup 主动 diff DOM 内容跟 initial,有差就提交。
  useEffect(() => {
    const node = ref.current;
    return () => {
      if (!node) return;
      const raw = node.textContent ?? "";
      const trimmed = raw.trim();
      const next = trimmed === "" ? null : trimmed;
      const initTrim = (initialRef.current ?? "").trim();
      if ((next ?? "") !== initTrim) {
        onCommitRef.current(next);
      }
    };
  }, []);

  const handleBlur = () => {
    if (!ref.current) return;
    const raw = ref.current.textContent ?? "";
    const trimmed = raw.trim();
    const next = trimmed === "" ? null : trimmed;
    // 与 initial 一样就不写
    const initTrim = (initialValue ?? "").trim();
    if ((next ?? "") === initTrim) return;
    onCommit(next);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLSpanElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      ref.current?.blur(); // 触发 onBlur
    } else if (e.key === "Escape") {
      e.preventDefault();
      // 撤销:恢复 initial,blur 时 next === init 不写
      if (ref.current) ref.current.textContent = initialValue ?? "";
      ref.current?.blur();
    }
  };

  if (!editable) {
    // 非编辑态:静态文本,不挂任何编辑 listener
    return (
      <span className={className}>
        {initialValue ?? (
          <span className="text-muted-foreground/40">—</span>
        )}
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1">
      <span
        ref={ref}
        contentEditable
        suppressContentEditableWarning
        onBlur={handleBlur}
        onKeyDown={handleKeyDown}
        aria-label={ariaLabel}
        data-placeholder={placeholder}
        className={cn(
          // contentEditable 容器:外加虚线边框 + focus ring,跟非编辑态视觉区隔
          "inline-block min-w-[3em] rounded border border-dashed border-border bg-background px-1.5 py-0.5",
          "transition-colors focus:border-foreground focus:bg-card focus:outline-none focus:ring-1 focus:ring-foreground/30",
          // 空值显示 placeholder(用 CSS data attr trick — contenteditable 没原生 placeholder)
          "empty:before:text-muted-foreground/40 empty:before:content-[attr(data-placeholder)]",
          // 改过的字段加左侧色条提示"这是手改值"
          hasOverride && "border-foreground/50 bg-foreground/[0.03]",
          className,
          editableClassName,
        )}
      >
        {/* 首次挂载:DOM 直接给 initialValue;后续靠 useEffect 同步(非编辑态时) */}
        {initialValue ?? ""}
      </span>
      {/* 已改过 + 编辑态 + 父组件传了 onReset → 显示恢复按钮 */}
      {hasOverride && onReset && (
        <button
          type="button"
          // mousedown 抢在 blur 前触发,防止"先 blur 再 reset"导致 reset 立刻被一个无变更的 blur 覆盖
          onMouseDown={(e) => {
            e.preventDefault();
            onReset();
          }}
          className="rounded p-0.5 text-muted-foreground/60 transition-colors hover:bg-accent hover:text-foreground"
          title="恢复 LLM 抽取的原值(以后还会跟着新材料重抽变化)"
          aria-label="恢复原值"
        >
          <RotateCcw className="size-3" />
        </button>
      )}
    </span>
  );
}
