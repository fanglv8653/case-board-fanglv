export type CalendarHostMode = "workspace" | "civil" | "criminal";

/**
 * The full calendar is a workspace-level overview. Domain case lists keep their
 * compact key-date summary without mounting another full calendar instance.
 */
export function shouldMountFullCalendar(mode: CalendarHostMode): boolean {
  return mode === "workspace";
}
