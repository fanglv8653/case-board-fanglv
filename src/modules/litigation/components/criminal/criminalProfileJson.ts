export type StructuredListKey = "charge" | "name";

export function structuredListToText(
  rawJson: string | null | undefined,
  key: StructuredListKey,
) {
  if (!rawJson) return "";
  try {
    const parsed: unknown = JSON.parse(rawJson);
    if (!Array.isArray(parsed)) return "";
    return parsed
      .map((item) => {
        if (typeof item === "string") return item;
        if (!item || typeof item !== "object") return "";
        const value = (item as Record<string, unknown>)[key];
        return typeof value === "string" ? value : "";
      })
      .filter(Boolean)
      .join("\n");
  } catch {
    return "";
  }
}

export function textToStructuredListJson(value: string, key: StructuredListKey) {
  const items = value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => ({ [key]: item }));
  return items.length > 0 ? JSON.stringify(items) : "";
}

export function resolveStructuredListJson({
  rawJson,
  initialText,
  currentText,
  key,
}: {
  rawJson: string | null | undefined;
  initialText: string;
  currentText: string;
  key: StructuredListKey;
}) {
  if (currentText === initialText) return rawJson ?? "";
  return textToStructuredListJson(currentText, key);
}
