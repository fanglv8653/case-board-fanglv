/**
 * 法律计算工具卡片 — 可点击,跳到工具视图。
 *
 * 可选 badge(如"重写中")— 在标题旁加 amber 小标签。
 */

import type { LucideIcon } from "lucide-react";

interface Props {
  icon: LucideIcon;
  title: string;
  desc: string;
  onClick: () => void;
  /** 可选状态标签(如 "重写中" / "Beta" / "新") */
  badge?: string;
}

export function LegalToolCard({ icon: Icon, title, desc, onClick, badge }: Props) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group flex items-start gap-3 rounded-lg border border-border bg-card px-4 py-3 text-left transition-all hover:border-foreground/30 hover:bg-card/80 hover:shadow-sm"
    >
      <Icon className="mt-0.5 size-5 shrink-0 text-foreground/70 transition-colors group-hover:text-foreground" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <h3 className="text-sm font-medium text-foreground">{title}</h3>
          {badge && (
            <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-amber-800">
              {badge}
            </span>
          )}
        </div>
        <p className="mt-0.5 text-label leading-relaxed text-muted-foreground">
          {desc}
        </p>
      </div>
    </button>
  );
}
