/**
 * 左下角版本号 chip。
 *
 * 2026-05-25 V0.1.8 加。设计:
 *   - 灰色小字 `CaseBoard v0.1.8`,固定左下角(fixed)
 *   - 落后时右上角加小红点 + tooltip 说"有更新"
 *   - 点击触发"手动检查" → 父组件 setUpdateInfo + 弹 UpdateAvailableDialog
 *
 * 不自己持有 UpdateInfo state,完全由 App.tsx 统一管理(启动检测的结果也喂这里)。
 */

import { useState } from "react";
import { Loader2 } from "lucide-react";

import { Chip } from "@/components/ui/chip";
import { checkForUpdate } from "@/lib/api";
import type { UpdateInfo } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  version: string;
  /** 启动时检测到的 UpdateInfo;null = 还没检测过 / 检测失败 */
  updateInfo: UpdateInfo | null;
  /** 父组件回调:用户点 chip 触发手动检查,把最新 UpdateInfo 喂回去 */
  onCheck: (info: UpdateInfo) => void;
}

export function VersionChip({ version, updateInfo, onCheck }: Props) {
  const [checking, setChecking] = useState(false);
  const hasUpdate = updateInfo?.has_update === true;

  const handleClick = async () => {
    if (checking) return;
    setChecking(true);
    try {
      const info = await checkForUpdate();
      onCheck(info);
    } catch {
      // 静默失败,不打扰
    } finally {
      setChecking(false);
    }
  };

  return (
    <Chip
      asChild
      size="sm"
      className={cn(
        "fixed bottom-2 left-2 z-50 rounded-md border-border/50 bg-card/80 px-2 py-1 font-mono backdrop-blur",
        hasUpdate && "border-amber-400/60 text-amber-700",
      )}
    >
      <button
        type="button"
        onClick={handleClick}
        disabled={checking}
        title={
          hasUpdate
            ? `有新版本 v${updateInfo?.latest} 可下载,点击查看`
            : "点击检查更新"
        }
      >
        {checking && <Loader2 className="size-3 animate-spin" />}
        <span>v{version}</span>
        {hasUpdate && (
          <span className="relative ml-0.5 flex size-1.5">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
            <span className="relative inline-flex size-1.5 rounded-full bg-amber-500" />
          </span>
        )}
      </button>
    </Chip>
  );
}
