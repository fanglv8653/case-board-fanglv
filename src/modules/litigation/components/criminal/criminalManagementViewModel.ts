export const MANAGEMENT_TABS = [
  { id: "overview", label: "案件概览" },
  { id: "progress", label: "进展记录" },
  { id: "todo", label: "待办提醒" },
  { id: "contacts", label: "案件通讯录" },
] as const;

export type ManagementTab = (typeof MANAGEMENT_TABS)[number]["id"];

export function isManagementTab(value: string): value is ManagementTab {
  return MANAGEMENT_TABS.some((item) => item.id === value);
}

export interface ManagementContactLike {
  agency_type?: string | null;
  agency_name?: string | null;
  contact_role?: string | null;
  contact_name?: string | null;
  phone?: string | null;
}

export interface LegacyContactCandidate {
  key: string;
  agencyType: string;
  agencyName: string;
  contactRole: string;
  contactName: string;
  phone: string;
  sourceLabel: string;
}

function clean(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function parseObjectArray(value: string | null | undefined): Array<Record<string, unknown>> {
  if (!value) return [];
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed)
      ? parsed.filter((item): item is Record<string, unknown> => Boolean(item && typeof item === "object"))
      : [];
  } catch {
    return [];
  }
}

export function parseManagementPartyNames(value: string | null | undefined): string[] {
  if (!value) return [];
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed)
      ? parsed.map(clean).filter(Boolean)
      : [];
  } catch {
    return [];
  }
}

export function resolveProsecutionAgency(
  contacts: ManagementContactLike[],
  legacyPartyNames: string[] = [],
): string {
  const names = contacts
    .filter((contact) =>
      [contact.agency_type, contact.agency_name, contact.contact_role]
        .map(clean)
        .some((value) => /检察|公诉/.test(value)),
    )
    .map((contact) => clean(contact.agency_name))
    .filter(Boolean);
  const confirmed = [...new Set(names)].join("、");
  if (confirmed) return confirmed;
  const legacy = legacyPartyNames.filter((name) => /检察院|检察机关|公诉机关/.test(name));
  return legacy.length > 0 ? `${[...new Set(legacy)].join("、")}（待核实）` : "";
}

function candidateMatchesFormal(
  candidate: LegacyContactCandidate,
  formalContacts: ManagementContactLike[],
): boolean {
  return formalContacts.some((contact) => {
    const sameName = clean(contact.contact_name) === candidate.contactName;
    const formalPhone = clean(contact.phone);
    const samePhone = Boolean(formalPhone && candidate.phone && formalPhone === candidate.phone);
    const sameAgency = Boolean(
      clean(contact.agency_name) && clean(contact.agency_name) === candidate.agencyName,
    );
    return sameName && (samePhone || sameAgency);
  });
}

export function buildLegacyContactCandidates(
  input: {
    courtContactsJson?: string | null;
    partyContactsJson?: string | null;
    fallbackAgencyName?: string | null;
  },
  formalContacts: ManagementContactLike[],
): LegacyContactCandidate[] {
  const courtCandidates = parseObjectArray(input.courtContactsJson).map((item) => {
    const contactName = clean(item.name);
    const contactRole = clean(item.role);
    const agencyName = clean(input.fallbackAgencyName) || "待确认办案机关";
    const agencyType = /检察|公诉/.test(`${agencyName}${contactRole}`)
      ? "检察机关"
      : /公安|侦查/.test(`${agencyName}${contactRole}`)
        ? "公安机关"
        : /法院|法官/.test(`${agencyName}${contactRole}`)
          ? "法院"
          : "办案机关";
    return {
      key: `court:${agencyName}:${contactName}:${clean(item.phone)}`,
      agencyType,
      agencyName,
      contactRole,
      contactName,
      phone: clean(item.phone),
      sourceLabel: "材料聚合的办案机关联系人",
    };
  });
  const partyCandidates = parseObjectArray(input.partyContactsJson).map((item) => {
    const role = clean(item.role) || "当事人/委托人";
    const contactName = clean(item.name);
    return {
      key: `party:${role}:${contactName}:${clean(item.phone)}`,
      agencyType: "当事人及委托关系",
      agencyName: role,
      contactRole: role,
      contactName,
      phone: clean(item.phone),
      sourceLabel: "材料聚合的当事人联系人",
    };
  });

  const unique = new Map<string, LegacyContactCandidate>();
  for (const candidate of [...courtCandidates, ...partyCandidates]) {
    if (!candidate.contactName || candidateMatchesFormal(candidate, formalContacts)) continue;
    const signature = `${candidate.agencyName}|${candidate.contactName}|${candidate.phone}`;
    if (!unique.has(signature)) unique.set(signature, candidate);
  }
  return [...unique.values()];
}
