/**
 * 全局任务锁(2026-05-25 V0.1.7)。
 *
 * 用途:解决"长任务(查被执行人 / 深挖 / 合并报告)运行时,用户切其他页面
 * 后任务状态丢失,以为被中断"的问题。
 *
 * 设计:
 *   - 全局唯一的 task state,放在 App 层
 *   - 任务进行中,App 顶层显示遮罩 Modal,任意页面都看见
 *   - 其他长任务按钮 disable,防止并发
 *   - 任务函数包装成 `runWithLock(task, fn)`,自动 start / end
 *
 * 重要:**Tauri command 本身在后端 tokio runtime 跑,前端 unmount 不会
 * 真的中断任务**。报告会照常落盘 + 写 DB。这个 Context 解决的是 UI 层
 * "看见任务在跑"的问题,以及防止用户重复点。
 */
import {
  createContext,
  useCallback,
  useContext,
  useState,
  type ReactNode,
} from "react";

export type TaskKind =
  | "yuandian_basic" // 查被执行人 · ~30-90s
  | "yuandian_deep_dive" // P2 深挖 · ~60-180s
  | "yuandian_full_report" // 合并完整报告 · ~30-60s
  | "global_extract"; // 案件分析报告 · ~10-30s

export interface RunningTask {
  kind: TaskKind;
  caseId: string;
  caseName: string;
  /** 显示在遮罩 modal 上,如 "正在查被执行人 · 元典聚合查询 + LLM,预计 30-90 秒" */
  label: string;
  startedAt: number;
}

interface RunningTaskCtx {
  task: RunningTask | null;
  /** 包装异步函数:自动 start + end,处理已有任务的并发拦截。唯一对外入口。 */
  runWithLock: <T>(
    t: Omit<RunningTask, "startedAt">,
    fn: () => Promise<T>,
  ) => Promise<T | null>;
}

const Ctx = createContext<RunningTaskCtx | null>(null);

export function RunningTaskProvider({ children }: { children: ReactNode }) {
  const [task, setTask] = useState<RunningTask | null>(null);

  const runWithLock = useCallback(
    async <T,>(
      t: Omit<RunningTask, "startedAt">,
      fn: () => Promise<T>,
    ): Promise<T | null> => {
      // 用 setState 闭包内部判断,避免 stale closure
      let blocked = false;
      setTask((prev) => {
        if (prev) {
          blocked = true;
          return prev;
        }
        return { ...t, startedAt: Date.now() };
      });
      if (blocked) {
        alert(
          `有任务正在执行,请稍后再试。\n\n${task ? task.label : ""}`,
        );
        return null;
      }
      try {
        return await fn();
      } finally {
        setTask(null);
      }
    },
    [task],
  );

  return (
    <Ctx.Provider value={{ task, runWithLock }}>
      {children}
    </Ctx.Provider>
  );
}

export function useRunningTask(): RunningTaskCtx {
  const v = useContext(Ctx);
  if (!v) {
    throw new Error("useRunningTask must be used within RunningTaskProvider");
  }
  return v;
}
