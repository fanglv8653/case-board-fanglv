/**
 * 发现新版本提示对话框。
 *
 * 2026-05-25 V0.1.8 作者拍板:**不强制更新**,只提示。
 *   - 「去下载」→ 浏览器开 lawtools.top
 *   - 「取消」→ 关闭对话框,本次会话不再提示(下次启动还会再检测)
 */

import { Download, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { openUrl } from "@/lib/api";
import type { UpdateInfo } from "@/lib/types";

interface Props {
  info: UpdateInfo;
  onClose: () => void;
}

export function UpdateAvailableDialog({ info, onClose }: Props) {
  const handleDownload = async () => {
    const url = info.download_url ?? "https://lawtools.top/";
    try {
      await openUrl(url);
    } catch (e) {
      alert(`打开浏览器失败:${e}\n\n请手动访问 ${url}`);
    }
    onClose();
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 p-4 animate-in fade-in-0 duration-200">
      <div className="w-full max-w-md overflow-hidden rounded-lg border border-border bg-card shadow-2xl animate-in zoom-in-95 fade-in-0 duration-300">
        {/* 顶部 */}
        <header className="flex items-start justify-between gap-3 border-b border-border bg-card/95 px-5 py-4">
          <div>
            <h2 className="text-base font-semibold">🎉 发现新版本</h2>
            <p className="mt-1 text-xs text-muted-foreground">
              当前 <span className="font-mono">v{info.current}</span> →
              最新 <span className="font-mono font-semibold text-foreground">v{info.latest}</span>
              {info.released_at && (
                <span className="ml-2 text-caption">({info.released_at})</span>
              )}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
            aria-label="关闭"
          >
            <X className="size-4" />
          </button>
        </header>

        {/* 更新内容 */}
        {info.notes && (
          <div className="border-b border-border bg-muted/30 px-5 py-3">
            <p className="text-label font-medium uppercase tracking-wide text-muted-foreground">
              更新内容
            </p>
            <p className="mt-1.5 whitespace-pre-wrap text-xs leading-relaxed text-foreground">
              {info.notes}
            </p>
          </div>
        )}

        {/* 说明 */}
        <div className="px-5 py-4">
          <p className="text-xs text-muted-foreground">
            升级不是强制的 — 当前版本仍可继续使用。点「去下载」会用浏览器打开
            <span className="mx-1 font-mono">lawtools.top</span>
            ,按提示下载新的 dmg。
          </p>
        </div>

        {/* 按钮 */}
        <footer className="flex items-center justify-end gap-2 border-t border-border bg-card/95 px-5 py-3">
          <Button variant="outline" size="sm" onClick={onClose}>
            取消
          </Button>
          <Button size="sm" onClick={handleDownload}>
            <Download className="mr-1.5 size-3.5" />
            去下载
          </Button>
        </footer>
      </div>
    </div>
  );
}
