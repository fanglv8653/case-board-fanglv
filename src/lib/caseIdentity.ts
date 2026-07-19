import type { Case, LegalDomain } from "@/lib/types";

export type CaseLegalDomain = LegalDomain;

type CaseIdentityFields = Pick<
  Case,
  | "name"
  | "cause"
  | "agg_cause"
  | "agg_plaintiffs"
  | "agg_defendants"
  | "user_overrides_json"
> & {
  legal_domain?: string | null;
  domain_source?: string | null;
  display_name_override?: string | null;
};

function clean(value: string | null | undefined): string | null {
  const normalized = value?.trim();
  return normalized ? normalized : null;
}

function parseStringArray(value: string | null | undefined): string[] {
  if (!value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((item): item is string => typeof item === "string")
      .map((item) => item.trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

function overriddenCause(caseData: CaseIdentityFields): string | null {
  let overridden: string | null | undefined;
  if (caseData.user_overrides_json) {
    try {
      const parsed = JSON.parse(caseData.user_overrides_json) as {
        fields?: Record<string, string | null>;
      };
      if (parsed.fields && "agg_cause" in parsed.fields) {
        overridden = parsed.fields.agg_cause;
      }
    } catch {
      // 损坏的旧 overlay 不应阻断案件列表渲染，继续使用案件字段。
    }
  }
  return clean(overridden === undefined ? caseData.agg_cause ?? caseData.cause : overridden);
}

export function normalizeCaseLegalDomain(value: string | null | undefined): CaseLegalDomain {
  return value === "criminal" || value === "civil" || value === "other"
    ? value
    : "unknown";
}

/**
 * 全应用唯一的案件显示名称规则：人工名称 > 当事人+罪名/案由 > 罪名/案由 > 文件夹名。
 */
export function getCaseDisplayName(caseData: CaseIdentityFields): string {
  const manualName = clean(caseData.display_name_override);
  if (manualName) return manualName;

  const cause = overriddenCause(caseData);
  if (cause) {
    const plaintiffs = parseStringArray(caseData.agg_plaintiffs);
    const defendants = parseStringArray(caseData.agg_defendants);
    const domain = normalizeCaseLegalDomain(caseData.legal_domain);
    const party = clean(
      domain === "criminal"
        ? defendants[0] ?? plaintiffs[0]
        : plaintiffs[0] ?? defendants[0],
    );
    if (party && !cause.includes(party)) return `${party}${cause}`;
    return cause;
  }

  return clean(caseData.name) ?? "未命名案件";
}

export function caseMatchesSearch(caseData: CaseIdentityFields, query: string): boolean {
  const needle = query.trim().toLocaleLowerCase("zh-CN");
  if (!needle) return true;
  const haystack = [
    getCaseDisplayName(caseData),
    caseData.display_name_override,
    overriddenCause(caseData),
    caseData.name,
    ...parseStringArray(caseData.agg_plaintiffs),
    ...parseStringArray(caseData.agg_defendants),
  ]
    .filter((value): value is string => Boolean(value))
    .join(" ")
    .toLocaleLowerCase("zh-CN");
  return haystack.includes(needle);
}

export function formatRecognitionFailure(error: unknown): string {
  const raw = String(error)
    .replace(/^Error:\s*/i, "")
    .trim();
  if (raw.includes("DOMAIN_MISMATCH")) {
    return "案件领域不符：请进入编辑模式，将案件领域修正为“刑事”后重试。";
  }
  if (raw.includes("MATERIAL_UNREADABLE")) {
    return "案件材料不可读取：请检查文件是否缺失、被占用或尚未完成 OCR。";
  }
  if (raw.includes("RECOGNITION_ENGINE_FAILED")) {
    return "识别引擎运行失败：请检查 OCR/模型配置或稍后重试。";
  }
  return raw || "未知错误";
}

export function formatRecognitionFailureList(errors: readonly unknown[]): string {
  return errors.map(formatRecognitionFailure).join("；");
}
