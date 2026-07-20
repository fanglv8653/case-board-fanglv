export type CriminalPartyTerm = "犯罪嫌疑人" | "被告人" | "犯罪嫌疑人/被告人";

const SUSPECT_STAGES = ["侦查", "审查逮捕", "审查起诉", "检察院", "补充侦查"];
const DEFENDANT_STAGES = [
  "一审",
  "二审",
  "审判",
  "庭前",
  "开庭",
  "庭审",
  "宣判",
  "再审",
  "死刑复核",
  "法院",
];

export function criminalPartyTermForStage(stage: string | null | undefined): CriminalPartyTerm {
  const normalized = stage?.trim() ?? "";
  if (SUSPECT_STAGES.some((keyword) => normalized.includes(keyword))) return "犯罪嫌疑人";
  if (DEFENDANT_STAGES.some((keyword) => normalized.includes(keyword))) return "被告人";
  return "犯罪嫌疑人/被告人";
}

export function criminalPartyNameLabel(stage: string | null | undefined): string {
  return `${criminalPartyTermForStage(stage)}姓名`;
}

export function normalizeCriminalPartyRoleForStage(
  role: string | null | undefined,
  stage: string | null | undefined,
): string {
  if (role !== "犯罪嫌疑人" && role !== "被告人") return role?.trim() ?? "";
  const term = criminalPartyTermForStage(stage);
  return term === "犯罪嫌疑人/被告人" ? role : term;
}
