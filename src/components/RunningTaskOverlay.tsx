/**
 * 全局任务遮罩 Modal(2026-05-25 V0.1.7)。
 *
 * 任务进行中时显示在 App 顶层,任意页面切换都看得见。
 * 不可关闭,只能等任务跑完。
 */
import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { useRunningTask } from "@/contexts/RunningTaskContext";

export function RunningTaskOverlay() {
  const { task } = useRunningTask();
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (!task) {
      setElapsed(0);
      return;
    }
    const tick = () => setElapsed(Math.floor((Date.now() - task.startedAt) / 1000));
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [task]);

  if (!task) return null;

  const minutes = Math.floor(elapsed / 60);
  const seconds = elapsed % 60;
  const elapsedText =
    minutes > 0 ? `${minutes} 分 ${seconds} 秒` : `${seconds} 秒`;

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-labelledby="running-task-title"
    >
      <div className="w-[440px] max-w-[92vw] rounded-2xl bg-card p-8 shadow-2xl ring-1 ring-border">
        <div className="flex items-start gap-4">
          <Loader2 className="mt-0.5 h-7 w-7 shrink-0 animate-spin text-primary" />
          <div className="flex-1 min-w-0">
            <h2 id="running-task-title" className="text-base font-semibold tracking-tight">
              任务执行中
            </h2>
            <p className="mt-2 text-sm text-foreground/85">{task.label}</p>
            <p className="mt-3 text-xs text-muted-foreground">
              案件:{task.caseName}
            </p>
            <p className="mt-0.5 text-xs text-muted-foreground">
              已用时:{elapsedText}
            </p>
          </div>
        </div>
        <div className="mt-6 rounded-md bg-muted/40 px-3 py-2 text-xs text-foreground/70">
          ⚠️ 请勿关闭 App。任务在后台继续运行,即使切到其他页面也不会中断。
          完成后此窗口会自动关闭。
        </div>
      </div>
    </div>
  );
}
