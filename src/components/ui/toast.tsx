import { useEffect, useState } from "react";
import { CheckCircle2, XCircle, Info } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * 极轻量全局 toast(无第三方库)。模块级 pub-sub:任意组件 `import { toast }` 直接调,
 * 不用 props/context 透传。挂一个 `<ToastViewport/>` 在 App 根即可。
 *
 * 取代刺眼的 window.alert / 各处静态 toast div;统一进入动画(tw-animate-css)。
 * 用法:toast("已导入 3 个案件", "success") / toast("导入失败:...", "error")。
 */
export type ToastKind = "success" | "error" | "info";
type ToastItem = { id: number; message: string; kind: ToastKind };

let seq = 0;
let items: ToastItem[] = [];
const listeners = new Set<(items: ToastItem[]) => void>();

function emit() {
  for (const l of listeners) l(items.slice());
}

/** 弹一条 toast,返回它的 id。durationMs<=0 表示不自动消失(长任务态,需手动 dismissToast)。 */
export function toast(
  message: string,
  kind: ToastKind = "info",
  durationMs = 4500,
): number {
  const id = ++seq;
  items = [...items, { id, message, kind }];
  emit();
  if (durationMs > 0) {
    setTimeout(() => dismissToast(id), durationMs);
  }
  return id;
}

/** 主动关掉某条 toast(用于长任务态完成后替换)。 */
export function dismissToast(id: number) {
  items = items.filter((t) => t.id !== id);
  emit();
}

export function ToastViewport() {
  const [list, setList] = useState<ToastItem[]>([]);
  useEffect(() => {
    listeners.add(setList);
    return () => {
      listeners.delete(setList);
    };
  }, []);

  if (list.length === 0) return null;
  return (
    <div className="pointer-events-none fixed inset-x-0 top-3 z-[200] flex flex-col items-center gap-2">
      {list.map((t) => (
        <div
          key={t.id}
          className={cn(
            "pointer-events-auto flex max-w-[90vw] items-center gap-2 rounded-lg border bg-card px-4 py-2.5 text-sm shadow-lg",
            "animate-in fade-in-0 slide-in-from-top-2 duration-300 ease-out",
            t.kind === "error" ? "border-destructive/30" : "border-border",
          )}
          role="status"
        >
          {t.kind === "success" && (
            <CheckCircle2 className="size-4 shrink-0 text-green-600" />
          )}
          {t.kind === "error" && (
            <XCircle className="size-4 shrink-0 text-destructive" />
          )}
          {t.kind === "info" && (
            <Info className="size-4 shrink-0 text-muted-foreground" />
          )}
          <span className="text-foreground">{t.message}</span>
        </div>
      ))}
    </div>
  );
}
