/**
 * 全局 console.error/warn + window.onerror 拦截器 (2026-05-26 V0.1.11)。
 *
 * 把前端运行时报错累积到一个内存 ring buffer(上限 100 条),反馈弹窗打开时
 * 通过 collectFeedbackDiagnostic 一次性回传给 Rust 端写进 MD。
 *
 * 设计:
 *   - 不持久化(localStorage 易撑爆 + 给老用户清缓存留麻烦)。刷新页面就丢,够用
 *   - monkey-patch console.error / console.warn,**保留原函数行为**(继续 log 到 DevTools)
 *   - 监听 window.onerror + unhandledrejection 兜底
 *   - 单例,App.tsx 启动时调一次 install() 即可
 *
 * 隐私:Rust 端在 render_md 末尾会再过一次 sanitize_paths,前端不需要主动脱敏。
 */

import type { ConsoleError } from "@/lib/api";

const CAPACITY = 100;
const ring: ConsoleError[] = [];
let installed = false;

function push(level: ConsoleError["level"], args: unknown[]) {
  const message = args
    .map((a) => {
      if (a instanceof Error) {
        return `${a.name}: ${a.message}${a.stack ? `\n${a.stack}` : ""}`;
      }
      if (typeof a === "string") return a;
      try {
        return JSON.stringify(a);
      } catch {
        return String(a);
      }
    })
    .join(" ");
  if (ring.length >= CAPACITY) ring.shift();
  ring.push({
    level,
    message,
    at: new Date().toISOString(),
  });
}

export function installConsoleTap() {
  if (installed) return;
  installed = true;

  const origError = console.error.bind(console);
  const origWarn = console.warn.bind(console);

  console.error = (...args: unknown[]) => {
    push("error", args);
    origError(...args);
  };
  console.warn = (...args: unknown[]) => {
    push("warn", args);
    origWarn(...args);
  };

  window.addEventListener("error", (e) => {
    push("unhandled", [
      e.message,
      e.filename ? `${e.filename}:${e.lineno}:${e.colno}` : "",
      e.error,
    ]);
  });

  window.addEventListener("unhandledrejection", (e) => {
    push("unhandled", ["unhandledrejection:", e.reason]);
  });
}

/** 拿当前 ring 快照(给反馈弹窗用)。返回浅拷贝,buffer 自身不动。 */
export function snapshotConsoleErrors(): ConsoleError[] {
  return ring.slice();
}
