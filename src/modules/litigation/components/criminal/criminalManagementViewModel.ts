export const MANAGEMENT_TABS = [
  { id: "overview", label: "案件概览" },
  { id: "progress", label: "进展记录" },
  { id: "todo", label: "待办提醒" },
  { id: "contacts", label: "案件通讯录" },
] as const;

export type ManagementTab = (typeof MANAGEMENT_TABS)[number]["id"];

export function isManagementTab(value: string): value is ManagementTab {
  return MANAGEMENT_TABS.some((item) => item.id === value);
}
