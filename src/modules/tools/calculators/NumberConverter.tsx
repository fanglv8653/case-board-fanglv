/**
 * 数字大写转换器 — React 原生实现(2026-05-24 e)。
 *
 * 计算逻辑见 ../lib/numberToChinese.ts(100% 移植自 lawtools.top/number.html)。
 * UI 用项目 shadcn + Tailwind 灰白克制风,跟 App 整体一致。
 */

import { useMemo, useState } from "react";
import { Check, Copy, RotateCcw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import {
  numberToChineseReadout,
  numberToChineseUppercase,
  sanitizeAmountInput,
} from "../lib/numberToChinese";

const MAX_VALUE = 999_999_999_999;

export function NumberConverter() {
  const [raw, setRaw] = useState("");
  const [copied, setCopied] = useState(false);

  // 解析 + 计算
  const { upper, readout, error, num } = useMemo(() => {
    const v = raw.trim();
    if (!v || v === ".") {
      return { upper: "", readout: "", error: "", num: null as number | null };
    }
    const n = parseFloat(v);
    if (isNaN(n)) {
      return { upper: "", readout: "", error: "请输入有效金额", num: null };
    }
    if (n < 0) {
      return { upper: "", readout: "", error: "金额不能为负数", num: null };
    }
    if (n > MAX_VALUE) {
      return {
        upper: "",
        readout: "",
        error: `金额超出上限(${MAX_VALUE.toLocaleString("zh-CN")} 元)`,
        num: null,
      };
    }
    return {
      upper: numberToChineseUppercase(n),
      readout: numberToChineseReadout(Math.floor(n)),
      error: "",
      num: n,
    };
  }, [raw]);

  const handleCopy = async () => {
    if (!upper) return;
    try {
      await navigator.clipboard.writeText(upper);
    } catch {
      // fallback
      const ta = document.createElement("textarea");
      ta.value = upper;
      document.body.appendChild(ta);
      ta.select();
      try {
        document.execCommand("copy");
      } catch {
        /* no-op */
      }
      document.body.removeChild(ta);
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="space-y-5">
      {/* 输入区 */}
      <div className="space-y-2">
        <label
          htmlFor="number-input"
          className="block text-xs font-medium text-muted-foreground"
        >
          阿拉伯数字金额(元 · 最多保留 2 位小数)
        </label>
        <div className="relative">
          <input
            id="number-input"
            type="text"
            inputMode="decimal"
            placeholder="例如:12345.67"
            value={raw}
            onChange={(e) => setRaw(sanitizeAmountInput(e.target.value))}
            className="w-full rounded-md border border-border bg-card px-4 py-3 font-mono text-lg text-foreground outline-none transition-colors focus:border-foreground/50 focus:ring-1 focus:ring-foreground/20"
            autoFocus
          />
          {raw && (
            <button
              type="button"
              onClick={() => setRaw("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              aria-label="清空"
              title="清空"
            >
              <RotateCcw className="size-3.5" />
            </button>
          )}
        </div>
        {error && <p className="text-xs text-destructive">{error}</p>}
      </div>

      {/* 大写金额结果 */}
      <div className="space-y-2">
        <div className="flex items-baseline justify-between">
          <label className="text-xs font-medium text-muted-foreground">
            大写金额(财务规范 · 圆角分)
          </label>
          {upper && (
            <Button
              variant="outline"
              size="sm"
              onClick={handleCopy}
              className="h-7 gap-1 text-xs"
            >
              {copied ? (
                <>
                  <Check className="size-3" />
                  已复制
                </>
              ) : (
                <>
                  <Copy className="size-3" />
                  复制
                </>
              )}
            </Button>
          )}
        </div>
        <div
          className={cn(
            "min-h-[3.5rem] rounded-md border bg-card px-4 py-3 text-base leading-relaxed",
            upper
              ? "border-border text-foreground"
              : "border-dashed border-border/70 text-muted-foreground/60",
          )}
        >
          {upper || "请输入金额"}
        </div>
      </div>

      {/* 普通读法 */}
      {readout && (
        <div className="space-y-2">
          <label className="text-xs font-medium text-muted-foreground">
            普通读法(整数部分)
          </label>
          <div className="rounded-md border border-dashed border-border bg-muted/20 px-4 py-2 text-sm text-foreground">
            {readout}
            {num !== null && num > Math.floor(num) && (
              <span className="text-muted-foreground"> · 小数另算</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
