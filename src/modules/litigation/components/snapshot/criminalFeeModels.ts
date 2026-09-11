const EXCLUDED_CRIMINAL_ITEMS = ["案件受理费", "保全费", "财产保全费", "保全申请费"];

export function isApplicableCriminalFee(item: string) {
  const normalized = item.replace(/\s+/g, "");
  return !EXCLUDED_CRIMINAL_ITEMS.some((excluded) => normalized.includes(excluded));
}
