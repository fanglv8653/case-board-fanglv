/**
 * 顶部模块导航条:左边「工作台」按钮 + 七个业务模块 tab。
 *
 * 设计(2026-05-24 b 作者拍板):顶部横排,当前 tab 下划线高亮,苹果风克制。
 * 2026-05-24 e:加首页按钮,任何模块/任何子页都可以一键回诉讼首页(HomeView)。
 * 2026-06-25 UX-P1:首页语义改为全局工作台,与诉讼业务 tab 分离。
 *
 * App.tsx 用 useState 跟 activeModule,这里只是个 dumb component。
 */

import { useLayoutEffect, useRef, useState, type ComponentType } from "react";
import {
  Briefcase,
  CircleDollarSign,
  FileQuestion,
  Gavel,
  Home,
  Scale,
  Settings as SettingsIcon,
  Users,
  Wrench,
} from "lucide-react";

import { cn } from "@/lib/utils";
// 私人专属功能接缝(双轨发布模型):开源仓 getPrivateTopTabs() 返回 [] → 无「独立」标签。
import { getPrivateTopTabs } from "@/private";

type TabIcon = ComponentType<{ className?: string }>;
type ModuleGroup = "case" | "work" | "system";

export type ModuleId =
  | "litigation"
  | "criminal"
  | "execution"
  | "income"
  | "transaction"
  | "tools"
  | "team"
  | "settings";

// 2026-05-24 j · 加「执行」tab(诉讼之后),自动筛 workflow_status='执行中' 的案件
// 2026-05-25 V0.1.8 · 加「设置」tab(工具之后),作者反馈:别人找不到右上角齿轮
const MODULES: {
  id: string;
  label: string;
  compactLabel?: string;
  icon: TabIcon;
  group: ModuleGroup;
  title?: string;
}[] = [
  {
    id: "criminal",
    label: "刑事",
    icon: Scale,
    group: "case",
  },
  { id: "litigation", label: "民事", icon: Briefcase, group: "case" },
  { id: "execution", label: "执行", icon: Gavel, group: "case" },
  { id: "income", label: "收入", icon: CircleDollarSign, group: "work" },
  { id: "transaction", label: "非诉", icon: FileQuestion, group: "work" },
  { id: "tools", label: "工具", icon: Wrench, group: "work" },
  // 2026-06-10 团队版 Phase 1:LAN 接力同步团队看板(未入团显示引导页)
  { id: "team", label: "团队", icon: Users, group: "system" },
  { id: "settings", label: "设置", icon: SettingsIcon, group: "system" },
];

export function ModuleTabs({
  active,
  onSwitch,
  onGoHome,
  rightSlot,
}: {
  active: string;
  onSwitch: (id: string) => void | Promise<void | boolean>;
  /** 「工作台」按钮点击:切到默认工作台 + 重置到 HomeView(由 App.tsx 处理) */
  onGoHome: () => void | Promise<void | boolean>;
  /** 2026-05-24 e:右侧自定义插槽(给 DeepSeekBalanceChip 等用) */
  rightSlot?: React.ReactNode;
}) {
  // 单条「滑动下划线」:跟踪当前激活 tab 的位置/宽度,切换时 transition-all 平滑滑过去
  // (取代原来每个 tab 各自条件渲染下划线 → 切换时硬切)。
  const rowRef = useRef<HTMLDivElement>(null);
  const [underline, setUnderline] = useState<{ left: number; width: number }>({
    left: 0,
    width: 0,
  });
  useLayoutEffect(() => {
    const el = rowRef.current?.querySelector<HTMLElement>(
      `[data-tab="${active}"]`,
    );
    if (el) {
      // 内缩 8px(对齐原 inset-x-2 视觉),让下划线比 tab 略窄更精致
      setUnderline({ left: el.offsetLeft + 8, width: el.offsetWidth - 16 });
    } else {
      setUnderline({ left: 0, width: 0 });
    }
  }, [active]);

  // 七个业务模块 + 私人专属顶层 tab(开源仓为空)。「独立」排最后(设置之后)。
  const allTabs: typeof MODULES = [
    ...MODULES,
    ...getPrivateTopTabs().map((t) => ({
      id: t.id,
      label: t.label,
      icon: t.icon,
      group: "system" as const,
    })),
  ];

  return (
    <nav className="shrink-0 border-b border-border bg-card/50 px-3 sm:px-5 lg:px-8">
      <div
        ref={rowRef}
        className="relative mx-auto flex w-full max-w-6xl flex-wrap items-center gap-2 py-1.5 lg:flex-nowrap"
      >
        {/* 左侧工作台按钮 — 单独样式,跟业务 tab 视觉区分(图标 + 边框 + 不带下划线) */}
        <button
          type="button"
          onClick={onGoHome}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-border bg-card px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="回到今日工作台"
          aria-label="回到今日工作台"
        >
          <Home className="size-3.5" />
          <span className="hidden font-medium min-[420px]:inline">工作台</span>
        </button>

        <div className="min-w-0 flex-1 overflow-x-auto">
          <div className="relative flex min-w-max items-center gap-1 pr-2">
            {/* 核心 tab + 私人「独立」tab */}
            {allTabs.map((m, index) => {
              const isActive = m.id === active;
              const Icon = m.icon;
              const prev = allTabs[index - 1];
              const groupBreak = prev && prev.group !== m.group;
              return (
                <button
                  key={m.id}
                  type="button"
                  data-tab={m.id}
                  onClick={() => onSwitch(m.id)}
                  title={m.title ?? m.label}
                  className={cn(
                    "relative flex items-center gap-1.5 px-2.5 py-3 text-sm transition-colors sm:px-3 lg:px-4",
                    groupBreak && "ml-2 border-l border-border pl-3 sm:ml-3 sm:pl-4",
                    isActive
                      ? "text-foreground"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                  aria-current={isActive ? "page" : undefined}
                  aria-label={m.title ?? m.label}
                >
                  <Icon className="size-4 shrink-0" />
                  <span className="font-medium">{m.compactLabel ?? m.label}</span>
                </button>
              );
            })}

            {/* 单条滑动下划线(平滑移动到激活 tab) */}
            <span
              className="pointer-events-none absolute bottom-0 h-0.5 rounded-full bg-foreground transition-all duration-300 ease-out"
              style={{ left: underline.left, width: underline.width }}
            />
          </div>
        </div>

        {/* 右侧插槽(DeepSeek 余额 chip 等) */}
        {rightSlot && (
          <div className="ml-auto flex shrink-0 items-center gap-2">{rightSlot}</div>
        )}
      </div>
    </nav>
  );
}
