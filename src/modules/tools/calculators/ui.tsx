/**
 * 计算器共享 UI 元件 — TabBtn / DetailRow。
 *
 * 原 LawyerFee / LitigationFee / Interest 各抄一份(TabBtn 三处逐字节相同、
 * DetailRow 两处全等一处精简),2026-06-03 收口到此(B9 + B11)。
 * 行为零变化:精简版 DetailRow 等价于通用版 strong=false。
 */
import { type ReactNode } from "react";

import { cn } from "@/lib/utils";

/** 计算器里的 tab 切换按钮。 */
export function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "rounded px-3 py-1 text-xs font-medium transition-colors",
        active
          ? "bg-foreground text-background"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

/** 结果明细行;strong=true 时高亮(用于「合计」行)。 */
export function DetailRow({
  label,
  value,
  strong = false,
}: {
  label: string;
  value: string;
  strong?: boolean;
}) {
  return (
    <div className="flex items-baseline justify-between border-b border-border/40 py-1.5 last:border-0">
      <dt
        className={cn(
          "text-xs",
          strong ? "font-medium text-foreground" : "text-muted-foreground",
        )}
      >
        {label}
      </dt>
      <dd
        className={cn(
          "font-mono",
          strong
            ? "text-base font-semibold text-foreground"
            : "text-sm text-foreground",
        )}
      >
        {value}
      </dd>
    </div>
  );
}

/** 计算结果统一风险提示；应放在每个结果卡片内部。 */
export function CalculatorDisclaimer() {
  return (
    <p className="border-t border-border/70 pt-2 text-[10px] leading-relaxed text-muted-foreground">
      本计算结果仅供参考，不构成正式法律意见。具体金额以法院裁判、有效规则或正式报价为准。
    </p>
  );
}
