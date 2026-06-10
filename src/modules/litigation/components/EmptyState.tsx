import { FolderOpen, Settings as SettingsIcon } from "lucide-react";

import { Button } from "@/components/ui/button";

/* ------------------------------------------------------------------ */
/* 空状态:从来没导入过案件                                            */
/* ------------------------------------------------------------------ */

export function EmptyState({
  onImport,
  error,
  onOpenSettings,
}: {
  onImport: () => void;
  error: string | null;
  onOpenSettings: () => void;
}) {
  return (
    <main className="relative flex h-full w-full flex-col items-center justify-center bg-background px-6">
      {/* 右上角设置按钮 */}
      <button
        type="button"
        onClick={onOpenSettings}
        className="absolute right-4 top-4 rounded p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="设置"
        aria-label="设置"
      >
        <SettingsIcon className="size-4" />
      </button>

      <div className="w-full max-w-md text-center">
        <h1 className="text-2xl font-semibold tracking-tight text-foreground">
          案件看板
        </h1>
        <p className="mt-3 text-sm text-muted-foreground">
          还没有案件 · 选一个案件文件夹开始
        </p>

        <div className="mt-8 flex justify-center">
          <Button onClick={onImport}>
            <FolderOpen className="size-4" />
            导入案件文件夹
          </Button>
        </div>

        {error && (
          <p className="mt-6 text-xs text-destructive">{error}</p>
        )}

        <p className="mt-8 text-xs text-muted-foreground/70">
          V0.1 开发中 · Tauri 2 + Rust + React + Tailwind v4 + shadcn/ui
        </p>
      </div>
    </main>
  );
}
