import { useEffect, useMemo, useState, type ReactElement } from "react";
import { Loader2, Pencil, Plus, RotateCcw, Trash2, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { listCaseFees, setCaseFeeDeleted, upsertCaseFee } from "@/lib/api";
import { confirmDialog } from "@/lib/dialog";
import type { CaseFee, CaseFeeInput, FeeRecord } from "@/lib/types";
import { isApplicableCriminalFee } from "./criminalFeeModels";

const EMPTY_FORM = (caseId: string): CaseFeeInput => ({
  case_id: caseId,
  item_name: "",
  amount: 0,
  charged_at: "",
  receipt_no: "",
  notes: "",
});

export function CriminalFeesCard({ caseId, recognizedFees }: { caseId: string; recognizedFees: FeeRecord[] }) {
  const [records, setRecords] = useState<CaseFee[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState<CaseFeeInput | null>(null);

  const load = async () => {
    try {
      setRecords(await listCaseFees(caseId));
    } catch (error) {
      toast(`读取收费记录失败：${String(error)}`, "error");
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => { void load(); }, [caseId]);

  const applicableRecognized = useMemo(
    () => recognizedFees.filter((fee) => isApplicableCriminalFee(fee.item)),
    [recognizedFees],
  );
  const activeManual = records.filter((record) => !record.deleted_at);
  const deletedManual = records.filter((record) => record.deleted_at);

  const edit = (record: CaseFee) => setForm({
    id: record.id,
    case_id: record.case_id,
    item_name: record.item_name,
    amount: record.amount,
    charged_at: record.charged_at ?? "",
    receipt_no: record.receipt_no ?? "",
    notes: record.notes ?? "",
  });

  const save = async () => {
    if (!form) return;
    setSaving(true);
    try {
      await upsertCaseFee(form);
      setForm(null);
      await load();
      toast("收费记录已保存", "success");
    } catch (error) {
      toast(`保存收费记录失败：${String(error)}`, "error");
    } finally {
      setSaving(false);
    }
  };

  const toggleDeleted = async (record: CaseFee, deleted: boolean) => {
    if (deleted) {
      const ok = await confirmDialog(`删除收费记录「${record.item_name}」？删除后可以恢复。`, { danger: true, okLabel: "删除" });
      if (!ok) return;
    }
    try {
      await setCaseFeeDeleted(record.id, deleted);
      await load();
      toast(deleted ? "收费记录已删除，可在下方恢复" : "收费记录已恢复", "success");
    } catch (error) {
      toast(`操作失败：${String(error)}`, "error");
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex justify-end">
        <Button type="button" size="sm" onClick={() => setForm(EMPTY_FORM(caseId))}>
          <Plus className="size-3.5" /> 添加收费记录
        </Button>
      </div>
      {form && (
        <div className="grid gap-2 rounded-lg border border-primary/20 bg-primary/5 p-3 md:grid-cols-5">
          <Field label="收费项目"><input value={form.item_name} onChange={(e) => setForm({ ...form, item_name: e.target.value })} /></Field>
          <Field label="金额（元）"><input type="number" min="0" step="0.01" value={form.amount} onChange={(e) => setForm({ ...form, amount: Number(e.target.value) })} /></Field>
          <Field label="收费/到账日期"><input type="date" value={form.charged_at ?? ""} onChange={(e) => setForm({ ...form, charged_at: e.target.value })} /></Field>
          <Field label="收据或发票号"><input value={form.receipt_no ?? ""} onChange={(e) => setForm({ ...form, receipt_no: e.target.value })} /></Field>
          <Field label="备注"><input value={form.notes ?? ""} onChange={(e) => setForm({ ...form, notes: e.target.value })} /></Field>
          <div className="flex justify-end gap-2 md:col-span-5">
            <Button type="button" size="sm" variant="ghost" onClick={() => setForm(null)}><X className="size-3.5" />取消</Button>
            <Button type="button" size="sm" disabled={saving || !form.item_name.trim() || form.amount < 0} onClick={() => void save()}>{saving && <Loader2 className="size-3.5 animate-spin" />}保存</Button>
          </div>
        </div>
      )}
      {loading ? (
        <p className="text-xs text-muted-foreground">正在读取收费记录…</p>
      ) : applicableRecognized.length === 0 && activeManual.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-5 text-center text-xs text-muted-foreground">暂无收费记录，可手工添加。</p>
      ) : (
        <div className="overflow-x-auto rounded-lg border">
          <table className="w-full text-left text-xs">
            <thead className="bg-muted/60 text-muted-foreground"><tr><th className="px-3 py-2">收费项目</th><th className="px-3 py-2">金额（元）</th><th className="px-3 py-2">日期</th><th className="px-3 py-2">票据号</th><th className="px-3 py-2">备注/来源</th><th className="px-3 py-2">操作</th></tr></thead>
            <tbody>
              {activeManual.map((record) => <tr key={record.id} className="border-t"><td className="px-3 py-2">{record.item_name}</td><td className="px-3 py-2">{record.amount.toLocaleString("zh-CN")}</td><td className="px-3 py-2">{record.charged_at || "—"}</td><td className="px-3 py-2">{record.receipt_no || "—"}</td><td className="px-3 py-2">{record.notes || "—"} <span className="text-muted-foreground">· 人工</span></td><td className="px-3 py-2"><div className="flex gap-1"><Button type="button" size="sm" variant="ghost" onClick={() => edit(record)}><Pencil className="size-3.5" /></Button><Button type="button" size="sm" variant="ghost" onClick={() => void toggleDeleted(record, true)}><Trash2 className="size-3.5" /></Button></div></td></tr>)}
              {applicableRecognized.map((record, index) => <tr key={`recognized-${index}`} className="border-t"><td className="px-3 py-2">{record.item}</td><td className="px-3 py-2">{record.amount?.toLocaleString("zh-CN") ?? "—"}</td><td className="px-3 py-2">{record.charged_at || "—"}</td><td className="px-3 py-2">{record.receipt_no || "—"}</td><td className="px-3 py-2">{record.note || "—"} <span className="text-muted-foreground">· 自动识别候选</span></td><td className="px-3 py-2 text-muted-foreground">—</td></tr>)}
            </tbody>
          </table>
        </div>
      )}
      {deletedManual.length > 0 && <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"><span>已删除：</span>{deletedManual.map((record) => <Button key={record.id} type="button" size="sm" variant="outline" onClick={() => void toggleDeleted(record, false)}><RotateCcw className="size-3.5" />{record.item_name}</Button>)}</div>}
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactElement<{ className?: string }> }) {
  return <label className="space-y-1 text-xs"><span className="text-muted-foreground">{label}</span><span className="block [&>input]:h-8 [&>input]:w-full [&>input]:rounded-md [&>input]:border [&>input]:bg-background [&>input]:px-2">{children}</span></label>;
}
