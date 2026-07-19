/**
 * 案件「领域」归类:以 cases.legal_domain 为统一事实源。
 *
 * migration 0047 起，后端导入、识别门禁与前端 tab 共用同一字段；人工修正优先。
 * 下方启发式只用于尚未完成迁移或 legal_domain=unknown 的历史数据兼容。
 *
 * 旧版兼容启发式(2026-06-17):
 *   - 命中即判刑事:案号含「刑」(刑初/刑终/刑核…)/ 案由含「罪」/ 出现刑事**专属**词
 *     (起诉书·公诉·被告人·犯罪嫌疑人·逮捕·刑事拘留·取保候审·看守所·量刑·缓刑·羁押·认罪认罚)。
 *   - 关键词只收**刑事专属**词:刻意排除「检察院 /
 *     公安局 / 公安机关 / 侦查」——这些在交通事故、行政诉讼(告公安局)、国家赔偿等民事/行政
 *     案件里也常出现,留着会误吞。「被告人」「逮捕」「量刑」等是刑事专属(民事用「被告」、无「逮捕/量刑」)。
 *   - 真实刑事案件几乎都有罪名(案由含「罪」)或「刑」字案号,主信号足够;关键词只作次要兜底。
 *   - 无可靠信号时宁可保持非刑事；用户可在案件编辑模式中人工纠正。
 */

import type { Case } from "@/lib/types";
import { normalizeCaseLegalDomain } from "@/lib/caseIdentity";

/**
 * 刑事**专属**关键词(出现在案由 / 状态文字 / 摘要里基本可断定刑事)。
 * 刻意不含「检察院 / 公安局 / 公安机关 / 侦查」—— 那些在民事/行政案件里也常见,会误吞(见文件头注释)。
 */
const CRIMINAL_KEYWORDS = [
  "犯罪嫌疑人",
  "被告人", // 刑事专属;民事用「被告」(无「人」)
  "公诉",
  "起诉书",
  "逮捕", // 刑事专属;行政是「行政拘留」、民事无
  "刑事拘留",
  "取保候审",
  "看守所",
  "量刑",
  "缓刑",
  "羁押",
  "认罪认罚",
];

/** 把若干可能为空的字段拼成一个待检索字符串(含案件/文件夹名,便于导入即时判定)。 */
function haystack(c: Case): string {
  return [
    c.name,
    c.cause,
    c.agg_cause,
    c.court,
    c.agg_court,
    c.agg_court_type,
    c.agg_status_text,
    c.case_summary,
    c.workflow_status,
  ]
    .filter(Boolean)
    .join(" ");
}

/**
 * 判断一个案件是否为刑事案件：优先统一领域字段，unknown 时兼容旧启发式。
 *
 * 命中任一信号即判刑事:
 *   1. 案号含「刑」字(刑初 / 刑终 / 刑核 / 刑申 …)—— 最强信号;
 *   2. 案由(cause / agg_cause)含「罪」字 —— 罪名几乎只出现在刑事;
 *   3. 任一拼接字段命中刑事专属关键词。
 */
export function isCriminalCase(c: Case): boolean {
  // migration 0047 起由后端统一落库；人工修正和可靠推断都优先于旧启发式。
  const legalDomain = normalizeCaseLegalDomain(c.legal_domain);
  if (legalDomain !== "unknown") return legalDomain === "criminal";

  // 仅兼容尚未完成迁移/无法可靠推断的历史数据。
  // 案号或案件/文件夹名含「刑」(刑初/刑终…)—— 最强信号,导入瞬间(agg 字段未抽出)即可判。
  const caseNo = `${c.case_no ?? ""} ${c.agg_case_no ?? ""} ${c.name ?? ""}`;
  if (caseNo.includes("刑")) return true;

  const cause = `${c.cause ?? ""} ${c.agg_cause ?? ""} ${c.name ?? ""}`;
  if (cause.includes("罪")) return true;

  const hay = haystack(c);
  return CRIMINAL_KEYWORDS.some((kw) => hay.includes(kw));
}

/** 拆分案件列表为 { civil, criminal } 两组(刑事 tab / 诉讼 tab 各取一组)。 */
export function splitCasesByDomain(cases: Case[]): {
  civil: Case[];
  criminal: Case[];
} {
  const civil: Case[] = [];
  const criminal: Case[] = [];
  for (const c of cases) {
    (isCriminalCase(c) ? criminal : civil).push(c);
  }
  return { civil, criminal };
}
