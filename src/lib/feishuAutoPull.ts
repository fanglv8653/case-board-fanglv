import { getFeishuConnectionStatus, pullFeishuSyncPreview } from "@/lib/api";
import { createFeishuAutoPullCoordinator } from "@/lib/feishuAutoPullCore";

const AUTO_PULL_INTERVAL_MS = 30 * 60 * 1000;

const runFeishuAutoPull = createFeishuAutoPullCoordinator(
  {
    isOnline: () => navigator.onLine,
    now: () => Date.now(),
    getConnectionStatus: getFeishuConnectionStatus,
    // 该命令只更新 feishu_sync_* 预演、快照和审计表；不修改案件业务表，也不写飞书。
    pullPreview: pullFeishuSyncPreview,
  },
  AUTO_PULL_INTERVAL_MS,
);

let runtimeStarted = false;

export function startFeishuReadonlyAutoPullRuntime() {
  if (runtimeStarted) return;
  runtimeStarted = true;

  const pull = () => {
    void runFeishuAutoPull().then((result) => {
      if (result.reason === "failed") {
        console.warn("[feishu-readonly-auto-pull] preview refresh failed");
      }
    });
  };
  const pullWhenVisible = () => {
    if (document.visibilityState === "visible") pull();
  };

  pull();
  window.addEventListener("focus", pull);
  window.addEventListener("online", pull);
  document.addEventListener("visibilitychange", pullWhenVisible);
  window.setInterval(pull, AUTO_PULL_INTERVAL_MS);
}
