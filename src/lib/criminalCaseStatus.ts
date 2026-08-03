import type { CriminalCaseProfile } from "@/lib/types";

export type CriminalStageId =
  | "收案委托"
  | "侦查"
  | "审查逮捕"
  | "审查起诉"
  | "一审"
  | "上诉及二审"
  | "再审/审判监督"
  | "执行"
  | "待确认";

export interface CriminalStageDef {
  id: CriminalStageId;
  label: string;
  color: string;
  order: number;
}
export const CRIMINAL_STAGE_LIST: CriminalStageDef[] = [
  { id: "收案委托", label: "收案委托", color: "bg-slate-100 text-slate-700", order: 1 },
  { id: "侦查", label: "侦查", color: "bg-blue-100 text-blue-800", order: 2 },
  { id: "审查逮捕", label: "审查逮捕", color: "bg-cyan-100 text-cyan-800", order: 3 },
  { id: "审查起诉", label: "审查起诉", color: "bg-amber-100 text-amber-800", order: 4 },
  { id: "一审", label: "一审", color: "bg-violet-100 text-violet-800", order: 5 },
  { id: "上诉及二审", label: "上诉及二审", color: "bg-fuchsia-100 text-fuchsia-800", order: 6 },
  { id: "再审/审判监督", label: "再审/审判监督", color: "bg-rose-100 text-rose-800", order: 7 },
  { id: "执行", label: "执行", color: "bg-emerald-100 text-emerald-800", order: 8 },
  { id: "待确认", label: "待确认", color: "bg-muted text-muted-foreground", order: 90 },
];

const BY_ID = Object.fromEntries(CRIMINAL_STAGE_LIST.map((item) => [item.id, item])) as Record<
  CriminalStageId,
  CriminalStageDef
>;

export function normalizeCriminalStageId(value: string | null | undefined): CriminalStageId {
  const raw = value?.trim();
  if (!raw) return "待确认";
  if (raw in BY_ID) return raw as CriminalStageId;
  const normalized = raw.toLowerCase().replace(/[\s_\-\/（）()]/g, "");
  if (/审查逮捕|批捕|arrestreview/.test(normalized)) return "审查逮捕";
  if (/审查起诉|检察院审查|prosecution/.test(normalized)) return "审查起诉";
  if (/再审|审判监督|retrial/.test(normalized)) return "再审/审判监督";
  if (/二审|上诉|secondinstance|appeal/.test(normalized)) return "上诉及二审";
  if (/一审|firstinstance/.test(normalized)) return "一审";
  if (/侦查|investigation/.test(normalized)) return "侦查";
  if (/执行|execution/.test(normalized)) return "执行";
  if (/收案|委托|engagement|intake/.test(normalized)) return "收案委托";
  return "待确认";
}

export function resolveCriminalCaseStatus(
  profile: Pick<CriminalCaseProfile, "current_stage"> | null | undefined,
): CriminalStageDef {
  return BY_ID[normalizeCriminalStageId(profile?.current_stage)];
}
