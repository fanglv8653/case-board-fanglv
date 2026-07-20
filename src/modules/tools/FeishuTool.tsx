import { useState } from "react";
import { CalendarClock, TableProperties } from "lucide-react";

import { cn } from "@/lib/utils";
import { FeishuCalendarTool } from "./FeishuCalendarTool";
import { FeishuSyncPreview } from "./FeishuSyncPreview";

type Tab = "sync" | "calendar";

export function FeishuTool() {
  const [tab, setTab] = useState<Tab>("sync");
  return <div className="space-y-5">
    <div role="tablist" aria-label="飞书连接功能" className="inline-flex rounded-lg border border-border bg-muted/30 p-1">
      <TabButton active={tab === "sync"} onClick={() => setTab("sync")} controls="feishu-sync-panel"><TableProperties />案件同步预览</TabButton>
      <TabButton active={tab === "calendar"} onClick={() => setTab("calendar")} controls="feishu-calendar-panel"><CalendarClock />日历设置</TabButton>
    </div>
    <div id={tab === "sync" ? "feishu-sync-panel" : "feishu-calendar-panel"} role="tabpanel">
      {tab === "sync" ? <FeishuSyncPreview /> : <div className="mx-auto max-w-3xl"><FeishuCalendarTool /></div>}
    </div>
  </div>;
}

function TabButton({ active, onClick, controls, children }: { active: boolean; onClick: () => void; controls: string; children: React.ReactNode }) {
  return <button type="button" role="tab" aria-selected={active} aria-controls={controls} onClick={onClick} className={cn("inline-flex items-center gap-2 rounded-md px-3 py-2 text-sm text-muted-foreground [&_svg]:size-4", active && "bg-card font-medium text-foreground shadow-sm")}>{children}</button>;
}
