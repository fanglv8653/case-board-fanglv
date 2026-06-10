import { type Document, STAGE_ORDER } from "@/lib/types";

export type GroupKey = (typeof STAGE_ORDER)[number] | "其他";

export function groupByStage(docs: Document[]): Record<GroupKey, Document[]> {
  const groups: Record<GroupKey, Document[]> = {
    立案: [],
    一审: [],
    二审: [],
    再审: [],
    执行: [],
    证据: [],
    身份信息: [],
    其他: [],
  };
  for (const doc of docs) {
    if (doc.is_ai_artifact) continue; // AI 产物单独成组,这里跳过
    const key = (doc.stage ?? "其他") as GroupKey;
    if (key in groups) groups[key].push(doc);
    else groups["其他"].push(doc);
  }
  return groups;
}
