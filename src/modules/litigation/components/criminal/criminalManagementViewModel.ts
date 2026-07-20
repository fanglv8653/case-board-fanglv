export const MANAGEMENT_TABS = [
  { id: "progress", label: "进展与任务" },
  { id: "profile", label: "案件信息" },
  { id: "timeline", label: "阶段与期限" },
  { id: "contacts", label: "案件通讯录" },
  { id: "work", label: "工作记录" },
] as const;

export type ManagementTab = (typeof MANAGEMENT_TABS)[number]["id"];

export function isManagementTab(value: string): value is ManagementTab {
  return MANAGEMENT_TABS.some((item) => item.id === value);
}
