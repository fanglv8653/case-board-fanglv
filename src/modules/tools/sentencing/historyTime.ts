/** SQLite CURRENT_TIMESTAMP is UTC; legacy ISO timestamps carry their own offset. */
export function formatHistoryTimestamp(value: string | null | undefined): string {
  if (!value) return "时间待核";
  const match = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?(Z|[+-]\d{2}:\d{2})?$/.exec(value);
  if (!match) return "时间待核";
  const [, year, month, day, hour, minute, second, fraction, zone] = match;
  const prefix = `${year}-${month}-${day}T${hour}:${minute}:${second}`;
  const utc = new Date(`${prefix}Z`);
  if (!Number.isFinite(utc.getTime()) || utc.toISOString().slice(0, 19) !== prefix) return "时间待核";
  const normalized = `${prefix}${fraction ? `.${fraction.slice(0, 3).padEnd(3, "0")}` : ""}${zone ?? "Z"}`;
  const date = new Date(normalized);
  if (!Number.isFinite(date.getTime())) return "时间待核";
  return new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai", year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23",
  }).format(date);
}
