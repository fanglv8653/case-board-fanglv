/**
 * 刑事案件信息纯逻辑契约。
 *
 * 这里刻意不读写数据库、不调用飞书，也不从通用案件字段猜测刑事专属信息：
 * - 案件显示名称与纯罪名分别解析；
 * - 程序阶段决定称谓与专属阶段日期；
 * - 公诉机关、犯罪嫌疑人/被告人、委托人始终为三个独立字段。
 */

export type CriminalProcedureStage =
  | "investigation"
  | "prosecution"
  | "first_instance"
  | "second_instance"
  | "trial_other"
  | "unknown";

export type CriminalIdentityPartyTerm = "犯罪嫌疑人" | "被告人" | "犯罪嫌疑人/被告人";

export type CriminalDisplayNameSource =
  | "manual_override"
  | "party_and_charge"
  | "charge"
  | "stored_name"
  | "unknown";

export type CriminalDisplayNameUpdateSource = "manual" | "recognition" | "feishu" | "legacy";

export type CriminalStageDateField =
  | "detention_date"
  | "prosecution_received_date"
  | "first_instance_accepted_date"
  | "second_instance_accepted_date";

export interface CriminalStageDates {
  detention_date?: string | null;
  prosecution_received_date?: string | null;
  first_instance_accepted_date?: string | null;
  second_instance_accepted_date?: string | null;
  /**
   * 仅用于明确契约边界：通用 filed_at 没有刑事阶段来源语义，本模块永远忽略它。
   */
  filed_at?: string | null;
}

export interface CriminalStageDateSelection {
  field: CriminalStageDateField;
  label: "拘留日期" | "审查起诉收案日期" | "一审受理日期" | "二审受理日期";
  value: string | null;
  displayValue: string;
  status: "provided" | "missing";
}

export interface CriminalDisplayNameInput {
  displayNameOverride?: string | null;
  suspectOrDefendantName?: string | null;
  suspectedCharge?: string | null;
  storedName?: string | null;
}

export interface CriminalDisplayNameResolution {
  value: string;
  source: CriminalDisplayNameSource;
}

export interface CriminalCaseIdentityInput extends CriminalDisplayNameInput, CriminalStageDates {
  currentStage?: string | null;
  prosecutionAuthority?: string | null;
  clientName?: string | null;
}

export interface CriminalCaseIdentity {
  displayName: string;
  displayNameSource: CriminalDisplayNameSource;
  /** 只来自 suspectedCharge；绝不从案件名或文件夹名反向切割。 */
  pureCharge: string | null;
  stage: CriminalProcedureStage;
  partyTerm: CriminalIdentityPartyTerm;
  partyNameLabel: string;
  stageDate: CriminalStageDateSelection | null;
  prosecutionAuthority: string | null;
  suspectOrDefendantName: string | null;
  clientName: string | null;
}

function clean(value: string | null | undefined): string | null {
  const normalized = value?.trim();
  return normalized ? normalized : null;
}

function compactStage(value: string): string {
  return value
    .trim()
    .toLocaleLowerCase("zh-CN")
    .replace(/[\s_-]+/g, "");
}

/**
 * 阶段只按明确文字或稳定代码归类。一般“法院/审判/庭审”只能确定已进入审判，
 * 不能据此猜是一审还是二审，因此归入 trial_other。
 */
export function normalizeCriminalProcedureStage(
  stage: string | null | undefined,
): CriminalProcedureStage {
  const normalized = compactStage(stage ?? "");
  if (!normalized) return "unknown";

  if (normalized.includes("二审") || normalized.includes("secondinstance")) {
    return "second_instance";
  }
  if (normalized.includes("一审") || normalized.includes("firstinstance")) {
    return "first_instance";
  }
  if (
    normalized.includes("审查起诉") ||
    normalized.includes("prosecutionreview") ||
    normalized.includes("reviewprosecution") ||
    normalized === "prosecution" ||
    normalized.includes("检察院")
  ) {
    return "prosecution";
  }
  if (
    normalized.includes("侦查") ||
    normalized.includes("审查逮捕") ||
    normalized.includes("批准逮捕") ||
    normalized.includes("investigation")
  ) {
    return "investigation";
  }
  if (
    normalized.includes("审判") ||
    normalized.includes("庭审") ||
    normalized.includes("开庭") ||
    normalized.includes("宣判") ||
    normalized.includes("再审") ||
    normalized.includes("死刑复核") ||
    normalized === "法院" ||
    normalized.includes("trial")
  ) {
    return "trial_other";
  }
  return "unknown";
}

export function criminalPartyTermForIdentityStage(
  stage: string | CriminalProcedureStage | null | undefined,
): CriminalIdentityPartyTerm {
  const normalized =
    stage === "investigation" ||
    stage === "prosecution" ||
    stage === "first_instance" ||
    stage === "second_instance" ||
    stage === "trial_other" ||
    stage === "unknown"
      ? stage
      : normalizeCriminalProcedureStage(stage);

  if (normalized === "investigation" || normalized === "prosecution") {
    return "犯罪嫌疑人";
  }
  if (
    normalized === "first_instance" ||
    normalized === "second_instance" ||
    normalized === "trial_other"
  ) {
    return "被告人";
  }
  // 组合称谓明确表达“阶段未确认”，不替用户猜测其程序身份。
  return "犯罪嫌疑人/被告人";
}

/**
 * 纯罪名只接受刑事画像中的 suspectedCharge。案件显示名、文件夹名和当事人字段
 * 即使包含“罪”字也不能反向切割成罪名。
 */
export function resolvePureCriminalCharge(
  suspectedCharge: string | null | undefined,
): string | null {
  return clean(suspectedCharge);
}

export function resolveCriminalDisplayName(
  input: CriminalDisplayNameInput,
): CriminalDisplayNameResolution {
  const manual = clean(input.displayNameOverride);
  if (manual) return { value: manual, source: "manual_override" };

  const party = clean(input.suspectOrDefendantName);
  const charge = resolvePureCriminalCharge(input.suspectedCharge);
  if (party && charge) {
    return {
      value: charge.includes(party) ? charge : `${party}${charge}`,
      source: "party_and_charge",
    };
  }
  if (charge) return { value: charge, source: "charge" };

  const storedName = clean(input.storedName);
  if (storedName) return { value: storedName, source: "stored_name" };
  return { value: "未命名刑事案件", source: "unknown" };
}

/**
 * 自动识别和飞书候选不得覆盖已有人工 display_name_override；人工编辑则可替换或清空。
 * 本函数只返回合并结果，不执行任何写入。
 */
export function mergeCriminalDisplayNameOverride(
  existingOverride: string | null | undefined,
  candidateOverride: string | null | undefined,
  source: CriminalDisplayNameUpdateSource,
): string | null {
  const existing = clean(existingOverride);
  const candidate = clean(candidateOverride);
  if (source === "manual") return candidate;
  return existing ?? candidate;
}

/**
 * 选择当前刑事阶段自己的日期锚点。阶段明确但字段缺失时仍返回 field key，
 * 供 UI 定位可编辑输入，同时 value=null/displayValue=待核实；绝不回退 filed_at，
 * 也不借用其他阶段已有日期。只有阶段本身无法定位时才返回 null。
 */
export function selectCriminalStageDate(
  stage: string | CriminalProcedureStage | null | undefined,
  dates: CriminalStageDates,
): CriminalStageDateSelection | null {
  const normalized =
    stage === "investigation" ||
    stage === "prosecution" ||
    stage === "first_instance" ||
    stage === "second_instance" ||
    stage === "trial_other" ||
    stage === "unknown"
      ? stage
      : normalizeCriminalProcedureStage(stage);

  const contract: Partial<
    Record<
      CriminalProcedureStage,
      { field: CriminalStageDateField; label: CriminalStageDateSelection["label"] }
    >
  > = {
    investigation: { field: "detention_date", label: "拘留日期" },
    prosecution: {
      field: "prosecution_received_date",
      label: "审查起诉收案日期",
    },
    first_instance: {
      field: "first_instance_accepted_date",
      label: "一审受理日期",
    },
    second_instance: {
      field: "second_instance_accepted_date",
      label: "二审受理日期",
    },
  };
  const selected = contract[normalized];
  if (!selected) return null;
  const value = clean(dates[selected.field]);
  return {
    ...selected,
    value,
    displayValue: value ?? "待核实",
    status: value ? "provided" : "missing",
  };
}

export function buildCriminalCaseIdentity(input: CriminalCaseIdentityInput): CriminalCaseIdentity {
  const stage = normalizeCriminalProcedureStage(input.currentStage);
  const partyTerm = criminalPartyTermForIdentityStage(stage);
  const displayName = resolveCriminalDisplayName(input);

  return {
    displayName: displayName.value,
    displayNameSource: displayName.source,
    pureCharge: resolvePureCriminalCharge(input.suspectedCharge),
    stage,
    partyTerm,
    partyNameLabel: `${partyTerm}姓名`,
    stageDate: selectCriminalStageDate(stage, input),
    // 三个字段分别清洗，禁止互相回退或从显示名称中推断。
    prosecutionAuthority: clean(input.prosecutionAuthority),
    suspectOrDefendantName: clean(input.suspectOrDefendantName),
    clientName: clean(input.clientName),
  };
}
