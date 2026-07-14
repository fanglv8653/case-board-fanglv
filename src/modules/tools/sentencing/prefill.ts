import { SENTENCING_DATA } from "./data.ts";
import type { CrimeName } from "./types.ts";

export interface SentencingCaseContext {
  caseId: string;
  profileRevision: number | null;
  suspectedCharge?: string | null;
  chargeHistoryJson?: string | null;
}

export interface SentencingPrefill {
  caseId: string | null;
  expectedProfileRevision: number | null;
  crimeName: CrimeName | null;
  crimeCandidates: CrimeName[];
  amount: null;
  crimeDate: null;
  areaType: null;
  factTier: null;
  factors: Record<string, never>;
  requiresCrimeConfirmation: boolean;
}

const CRIME_NAMES = new Set<CrimeName>(SENTENCING_DATA.crimes.map((crime) => crime.name));
const SEPARATORS = /[\s、,，/；;|]+/;

function exactCrimeNames(value: unknown): CrimeName[] {
  if (typeof value !== "string") return [];
  return value
    .split(SEPARATORS)
    .map((part) => part.trim())
    .filter((part): part is CrimeName => CRIME_NAMES.has(part as CrimeName));
}

function historyValues(raw: string | null | undefined): unknown[] {
  if (!raw?.trim()) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.flatMap((item) => {
      if (typeof item === "string") return [item];
      if (!item || typeof item !== "object") return [];
      const record = item as Record<string, unknown>;
      return [record.charge, record.name].filter((value) => typeof value === "string");
    });
  } catch {
    return [];
  }
}

/**
 * 案件入口只搬运可精确验证的罪名与 revision。金额、日期、地区、事实档位和
 * 量刑情节均保持空白，避免从退赔、羁押/受理日期、法院或自由文本作危险推断。
 */
export function buildSentencingPrefill(context?: SentencingCaseContext | null): SentencingPrefill {
  if (!context) {
    return {
      caseId: null,
      expectedProfileRevision: null,
      crimeName: null,
      crimeCandidates: [],
      amount: null,
      crimeDate: null,
      areaType: null,
      factTier: null,
      factors: {},
      requiresCrimeConfirmation: false,
    };
  }

  const candidates = [
    ...exactCrimeNames(context.suspectedCharge),
    ...historyValues(context.chargeHistoryJson).flatMap(exactCrimeNames),
  ].filter((name, index, all) => all.indexOf(name) === index);

  return {
    caseId: context.caseId,
    expectedProfileRevision: context.profileRevision,
    crimeName: candidates.length === 1 ? candidates[0] : null,
    crimeCandidates: candidates,
    amount: null,
    crimeDate: null,
    areaType: null,
    factTier: null,
    factors: {},
    requiresCrimeConfirmation: candidates.length > 1,
  };
}
