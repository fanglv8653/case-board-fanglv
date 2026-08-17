/**
 * v0.8.4 安全更新入口。
 *
 * Rust 负责检查、签名验证、下载、停机屏障和启动独立安装 helper。前端不直接调用
 * updater 的 install API，也不以浏览器缓存猜测安装是否成功。
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface PendingUpdate {
  version: string;
  notes: string | null;
}

export type DownloadProgress = {
  downloaded: number;
  total: number;
  phase: "downloading" | "verified";
};

export class AppUpdateError extends Error {
  constructor(public readonly code: string) {
    super(code);
    this.name = "AppUpdateError";
  }
}

function updateErrorCode(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === "string" && /^UPD_[A-Z_]+$/.test(code)) return code;
  }
  if (typeof error === "string") {
    const match = error.match(/UPD_[A-Z_]+/);
    if (match) return match[0];
  }
  return "UPD_UNKNOWN";
}

/**
 * 请求后端准备更新。成功路径会由独立 helper 在安全停机后接管，本进程随即退出。
 */
export async function startAppUpdate(
  expectedVersion: string,
  notes: string | null,
  onProgress: (progress: DownloadProgress) => void,
): Promise<void> {
  const unlisten = await listen<DownloadProgress>("app-update-progress", (event) => {
    onProgress(event.payload);
  });
  try {
    await invoke("start_app_update", { expectedVersion, notes });
  } catch (error) {
    throw new AppUpdateError(updateErrorCode(error));
  } finally {
    unlisten();
  }
}

/**
 * 仅消费 helper 写入、后端验证并原子 claim 的一次性成功回执。
 */
export async function consumeJustUpdated(): Promise<PendingUpdate | null> {
  try {
    return await invoke<PendingUpdate | null>("claim_app_update_success");
  } catch {
    // 回执无效、过期或已消费都不得显示成功；启动本身不受影响。
    return null;
  }
}
