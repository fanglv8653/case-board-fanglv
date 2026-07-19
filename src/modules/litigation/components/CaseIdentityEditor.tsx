import { useEffect, useMemo, useState } from "react";
import { Loader2, Save } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { updateCaseLegalIdentity } from "@/lib/api";
import {
  getCaseDisplayName,
  normalizeCaseLegalDomain,
  type CaseLegalDomain,
} from "@/lib/caseIdentity";
import type { Case } from "@/lib/types";

const DOMAIN_OPTIONS: Array<{ value: CaseLegalDomain; label: string }> = [
  { value: "criminal", label: "刑事" },
  { value: "civil", label: "民事" },
  { value: "other", label: "其他" },
  { value: "unknown", label: "待确认" },
];

export function CaseIdentityEditor({
  caseData,
  onSaved,
}: {
  caseData: Case;
  onSaved: () => void;
}) {
  const initialDomain = normalizeCaseLegalDomain(caseData.legal_domain);
  const initialName = caseData.display_name_override ?? "";
  const [legalDomain, setLegalDomain] = useState<CaseLegalDomain>(initialDomain);
  const [displayName, setDisplayName] = useState(initialName);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setLegalDomain(normalizeCaseLegalDomain(caseData.legal_domain));
    setDisplayName(caseData.display_name_override ?? "");
  }, [caseData.id, caseData.legal_domain, caseData.display_name_override]);

  const automaticName = useMemo(
    () => getCaseDisplayName({ ...caseData, display_name_override: null }),
    [caseData],
  );
  const normalizedName = displayName.trim();
  const changed =
    legalDomain !== initialDomain || normalizedName !== (caseData.display_name_override ?? "").trim();

  const save = async () => {
    if (!changed || saving) return;
    setSaving(true);
    try {
      await updateCaseLegalIdentity(caseData.id, legalDomain, normalizedName || null);
      toast("案件名称与领域已保存", "success");
      onSaved();
    } catch (error) {
      toast(`保存案件设置失败：${error}`, "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mt-3 rounded-lg border border-border bg-background/70 p-3">
      <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_10rem_auto] md:items-end">
        <label className="grid gap-1.5 text-xs font-medium text-foreground">
          案件显示名称
          <input
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
            placeholder={automaticName}
            className="h-9 rounded-md border border-input bg-background px-3 text-sm font-normal outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
          />
          <span className="font-normal text-muted-foreground">
            留空时自动显示“当事人姓名＋罪名/案由”：{automaticName}
          </span>
        </label>
        <label className="grid gap-1.5 text-xs font-medium text-foreground">
          案件领域
          <select
            value={legalDomain}
            onChange={(event) => setLegalDomain(event.target.value as CaseLegalDomain)}
            className="h-9 rounded-md border border-input bg-background px-2 text-sm font-normal outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
          >
            {DOMAIN_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <Button type="button" size="sm" disabled={!changed || saving} onClick={() => void save()}>
          {saving ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
          保存
        </Button>
      </div>
      <p className="mt-2 text-[11px] text-muted-foreground">
        人工选择将优先于自动识别；改为民事后，刑事材料识别会按预期停止。
      </p>
    </div>
  );
}
