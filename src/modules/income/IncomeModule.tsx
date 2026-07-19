import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  CircleDollarSign,
  Edit3,
  Loader2,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";

import {
  deleteIncomeRecord,
  listCases,
  listIncomeRecords,
  summarizeIncomeRecords,
  upsertIncomeRecord,
} from "@/lib/api";
import { confirmDialog } from "@/lib/dialog";
import type {
  Case,
  IncomeArchiveHoldbackStatus,
  IncomeInvoiceStatus,
  IncomeRecord,
  IncomeRecordFilter,
  IncomeRecordUpsertInput,
  IncomeSourceType,
  IncomeSummary,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import { getCaseDisplayName } from "@/lib/caseIdentity";
import { toast } from "@/components/ui/toast";

type FilterState = {
  range: "month" | "quarter" | "year" | "custom";
  monthFrom: string;
  monthTo: string;
  sourceType: "all" | IncomeSourceType;
  holdbackStatus: "all" | IncomeArchiveHoldbackStatus;
  invoiceStatus: IncomeInvoiceStatus;
  query: string;
};

type FormState = {
  id: string | null;
  recordStatus?: "draft" | "confirmed";
  caseId: string;
  manualCaseName: string;
  lawyerFeeTotal: string;
  sourceType: IncomeSourceType;
  collaboratorName: string;
  shareRatioPercent: string;
  firmDeductionPercent: string;
  archiveHoldbackPercent: string;
  archiveHoldbackStatus: IncomeArchiveHoldbackStatus;
  archiveReturnedAt: string;
  archiveReturnedAmount: string;
  invoiceDate: string;
  invoiceNo: string;
  recognizedMonth: string;
  actualIncomeOverridden: boolean;
  actualIncomeAmount: string;
  actualIncomeOverrideNote: string;
  note: string;
};

const EMPTY_SUMMARY: IncomeSummary = {
  record_count: 0,
  lawyer_fee_total_sum: 0,
  personal_share_sum: 0,
  firm_deduction_sum: 0,
  archive_holdback_sum: 0,
  actual_income_sum: 0,
  holding_amount_sum: 0,
  returned_holdback_sum: 0,
  invoiced_fee_sum: 0,
  overridden_count: 0,
};

const SOURCE_LABEL: Record<IncomeSourceType, string> = {
  personal: "个人",
  collaboration: "合作",
};

const HOLDBACK_LABEL: Record<IncomeArchiveHoldbackStatus, string> = {
  holding: "暂押中",
  returned: "已返还",
  not_returned: "不返还",
};

const inputCls =
  "w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:border-sky-400 focus:outline-none";
const selectCls =
  "w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground focus:border-sky-400 focus:outline-none";

export function IncomeModule() {
  const [records, setRecords] = useState<IncomeRecord[]>([]);
  const [summary, setSummary] = useState<IncomeSummary>(EMPTY_SUMMARY);
  const [monthSummary, setMonthSummary] = useState<IncomeSummary>(EMPTY_SUMMARY);
  const [quarterSummary, setQuarterSummary] = useState<IncomeSummary>(EMPTY_SUMMARY);
  const [yearSummary, setYearSummary] = useState<IncomeSummary>(EMPTY_SUMMARY);
  const [cases, setCases] = useState<Case[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<FormState | null>(null);
  const [filters, setFilters] = useState<FilterState>(() => {
    const now = new Date();
    const currentMonth = formatMonth(now);
    return {
      range: "year",
      monthFrom: `${now.getFullYear()}-01`,
      monthTo: currentMonth,
      sourceType: "all",
      holdbackStatus: "all",
      invoiceStatus: "all",
      query: "",
    };
  });

  const apiFilter = useMemo(() => buildFilter(filters), [filters]);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [caseRows, incomeRows, filteredSummary, thisMonth, thisQuarter, thisYear] =
        await Promise.all([
          listCases(),
          listIncomeRecords(apiFilter),
          summarizeIncomeRecords(apiFilter),
          summarizeIncomeRecords(rangeFilter(getCurrentMonthRange())),
          summarizeIncomeRecords(rangeFilter(getCurrentQuarterRange())),
          summarizeIncomeRecords(rangeFilter(getCurrentYearRange())),
        ]);
      setCases(caseRows);
      setRecords(incomeRows);
      setSummary(filteredSummary);
      setMonthSummary(thisMonth);
      setQuarterSummary(thisQuarter);
      setYearSummary(thisYear);
    } catch (e) {
      setError(String(e));
      toast(`读取收入台账失败:${e}`, "error");
    } finally {
      setLoading(false);
    }
  }, [apiFilter]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const openCreate = () => setEditing(makeBlankForm(cases[0] ?? null));
  const openEdit = (record: IncomeRecord) => setEditing(formFromRecord(record));

  async function saveForm(form: FormState) {
    const input = toUpsertInput(form);
    if (!input) return;
    setSaving(true);
    try {
      await upsertIncomeRecord(input);
      toast(form.id ? "收入记录已更新" : "收入记录已新增", "success");
      setEditing(null);
      await reload();
    } catch (e) {
      toast(`保存失败:${e}`, "error");
    } finally {
      setSaving(false);
    }
  }

  async function removeRecord(record: IncomeRecord) {
    const ok = await confirmDialog(`删除「${displayCaseName(record)}」的收入记录?`, {
      danger: true,
      okLabel: "删除",
    });
    if (!ok) return;
    try {
      await deleteIncomeRecord(record.id);
      toast("收入记录已删除", "success");
      await reload();
    } catch (e) {
      toast(`删除失败:${e}`, "error");
    }
  }

  return (
    <main className="flex h-full w-full flex-col bg-background">
      <header className="shrink-0 border-b border-border bg-card/50 px-6 py-3">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center gap-3">
          <div className="flex items-center gap-2">
            <CircleDollarSign className="size-4 text-sky-600" />
            <h1 className="text-sm font-semibold text-foreground">收入台账</h1>
            <span className="rounded bg-muted px-1.5 py-0.5 text-caption text-muted-foreground">
              {summary.record_count} 条
            </span>
          </div>
          <p className="text-caption text-muted-foreground">
            私人财务台账,不进入工作台、团队同步、AI 抽取或 MCP。
          </p>
          <div className="flex-1" />
          <button
            type="button"
            onClick={() => void reload()}
            disabled={loading}
            className="inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground disabled:opacity-50"
          >
            <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
            刷新
          </button>
          <button
            type="button"
            onClick={openCreate}
            className="inline-flex items-center gap-1.5 rounded-md bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-700"
          >
            <Plus className="size-3.5" />
            新增收入记录
          </button>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-auto px-6 py-6">
        <div className="mx-auto max-w-7xl space-y-5">
          <SummaryCards
            filtered={summary}
            month={monthSummary}
            quarter={quarterSummary}
            year={yearSummary}
          />

          <FilterBar filters={filters} onChange={setFilters} />

          {error && (
            <div className="flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-800">
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}

          {loading ? (
            <div className="flex h-44 items-center justify-center rounded-lg border border-border bg-card">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : records.length === 0 ? (
            <EmptyIncome onCreate={openCreate} />
          ) : (
            <IncomeTable
              records={records}
              onEdit={openEdit}
              onDelete={(record) => void removeRecord(record)}
            />
          )}
        </div>
      </div>

      {editing && (
        <IncomeFormDrawer
          form={editing}
          cases={cases}
          saving={saving}
          onChange={setEditing}
          onClose={() => setEditing(null)}
          onSubmit={() => void saveForm(editing)}
        />
      )}
    </main>
  );
}

function SummaryCards({
  filtered,
  month,
  quarter,
  year,
}: {
  filtered: IncomeSummary;
  month: IncomeSummary;
  quarter: IncomeSummary;
  year: IncomeSummary;
}) {
  const cards = [
    { label: "本月实际收入", value: month.actual_income_sum, tone: "sky" },
    { label: "本季实际收入", value: quarter.actual_income_sum, tone: "emerald" },
    { label: "本年实际收入", value: year.actual_income_sum, tone: "violet" },
    { label: "筛选内律师费总额", value: filtered.lawyer_fee_total_sum },
    { label: "律所扣除", value: filtered.firm_deduction_sum },
    { label: "归档暂押", value: filtered.archive_holdback_sum },
    { label: "暂押待返还", value: filtered.holding_amount_sum, tone: "amber" },
    { label: "已返还暂押", value: filtered.returned_holdback_sum },
    { label: "已开票金额", value: filtered.invoiced_fee_sum },
  ];
  return (
    <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
      {cards.map((card) => (
        <div
          key={card.label}
          className={cn(
            "rounded-lg border border-border bg-card px-4 py-3",
            card.tone === "sky" && "border-sky-200 bg-sky-50/50",
            card.tone === "emerald" && "border-emerald-200 bg-emerald-50/50",
            card.tone === "violet" && "border-violet-200 bg-violet-50/50",
            card.tone === "amber" && "border-amber-200 bg-amber-50/50",
          )}
        >
          <p className="text-caption text-muted-foreground">{card.label}</p>
          <p className="mt-1 font-mono text-lg font-semibold text-foreground">
            {formatMoney(card.value)}
          </p>
        </div>
      ))}
      <div className="rounded-lg border border-border bg-card px-4 py-3">
        <p className="text-caption text-muted-foreground">筛选内覆盖记录</p>
        <p className="mt-1 text-lg font-semibold text-foreground">
          {filtered.overridden_count} 条
        </p>
      </div>
    </section>
  );
}

function FilterBar({
  filters,
  onChange,
}: {
  filters: FilterState;
  onChange: (next: FilterState) => void;
}) {
  const patch = (p: Partial<FilterState>) => onChange({ ...filters, ...p });
  return (
    <section className="rounded-lg border border-border bg-card p-4">
      <div className="grid gap-3 md:grid-cols-6">
        <SmallField label="时间范围">
          <select
            className={selectCls}
            value={filters.range}
            onChange={(e) => {
              const range = e.target.value as FilterState["range"];
              const derived =
                range === "month"
                  ? getCurrentMonthRange()
                  : range === "quarter"
                    ? getCurrentQuarterRange()
                    : range === "year"
                      ? getCurrentYearRange()
                      : { from: filters.monthFrom, to: filters.monthTo };
              patch({ range, monthFrom: derived.from, monthTo: derived.to });
            }}
          >
            <option value="month">本月</option>
            <option value="quarter">本季</option>
            <option value="year">本年</option>
            <option value="custom">自定义</option>
          </select>
        </SmallField>
        <SmallField label="起始月份">
          <input
            type="month"
            className={inputCls}
            value={filters.monthFrom}
            onChange={(e) => patch({ range: "custom", monthFrom: e.target.value })}
          />
        </SmallField>
        <SmallField label="结束月份">
          <input
            type="month"
            className={inputCls}
            value={filters.monthTo}
            onChange={(e) => patch({ range: "custom", monthTo: e.target.value })}
          />
        </SmallField>
        <SmallField label="案源">
          <select
            className={selectCls}
            value={filters.sourceType}
            onChange={(e) => patch({ sourceType: e.target.value as FilterState["sourceType"] })}
          >
            <option value="all">全部</option>
            <option value="personal">个人</option>
            <option value="collaboration">合作</option>
          </select>
        </SmallField>
        <SmallField label="暂押状态">
          <select
            className={selectCls}
            value={filters.holdbackStatus}
            onChange={(e) =>
              patch({ holdbackStatus: e.target.value as FilterState["holdbackStatus"] })
            }
          >
            <option value="all">全部</option>
            <option value="holding">暂押中</option>
            <option value="returned">已返还</option>
            <option value="not_returned">不返还</option>
          </select>
        </SmallField>
        <SmallField label="开票状态">
          <select
            className={selectCls}
            value={filters.invoiceStatus}
            onChange={(e) => patch({ invoiceStatus: e.target.value as IncomeInvoiceStatus })}
          >
            <option value="all">全部</option>
            <option value="invoiced">已开票</option>
            <option value="not_invoiced">未开票</option>
          </select>
        </SmallField>
      </div>
      <div className="mt-3 flex items-center gap-2">
        <Search className="size-4 text-muted-foreground" />
        <input
          type="search"
          className={inputCls}
          placeholder="搜索案件名 / 合作方 / 发票编号"
          value={filters.query}
          onChange={(e) => patch({ query: e.target.value })}
        />
      </div>
    </section>
  );
}

function IncomeTable({
  records,
  onEdit,
  onDelete,
}: {
  records: IncomeRecord[];
  onEdit: (record: IncomeRecord) => void;
  onDelete: (record: IncomeRecord) => void;
}) {
  return (
    <section className="overflow-hidden rounded-lg border border-border bg-card">
      <div className="overflow-x-auto">
        <table className="min-w-[1180px] w-full text-left text-xs">
          <thead className="border-b border-border bg-muted/40 text-caption uppercase tracking-wide text-muted-foreground">
            <tr>
              <th className="px-3 py-2">确认月份</th>
              <th className="px-3 py-2">案件</th>
              <th className="px-3 py-2 text-right">律师费</th>
              <th className="px-3 py-2">案源</th>
              <th className="px-3 py-2">合作方</th>
              <th className="px-3 py-2 text-right">分成</th>
              <th className="px-3 py-2 text-right">律所扣除</th>
              <th className="px-3 py-2 text-right">归档暂押</th>
              <th className="px-3 py-2">暂押状态</th>
              <th className="px-3 py-2">开票</th>
              <th className="px-3 py-2">发票号</th>
              <th className="px-3 py-2 text-right">实际收入</th>
              <th className="px-3 py-2">备注</th>
              <th className="px-3 py-2 text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {records.map((r) => (
              <tr key={r.id} className="border-b border-border/60 last:border-0">
                <td className="px-3 py-2 font-mono text-muted-foreground">{r.recognized_month}</td>
                <td className="max-w-[220px] px-3 py-2">
                  <p className="line-clamp-2 font-medium text-foreground">{displayCaseName(r)}</p>
                  {r.record_status === "draft" && (
                    <p className="mt-0.5 text-caption font-medium text-amber-700">待确认（不计入统计）</p>
                  )}
                  {!r.case_id && (
                    <p className="mt-0.5 text-caption text-muted-foreground">手工记录</p>
                  )}
                </td>
                <td className="px-3 py-2 text-right font-mono">{formatMoney(r.lawyer_fee_total)}</td>
                <td className="px-3 py-2">{SOURCE_LABEL[r.source_type]}</td>
                <td className="px-3 py-2 text-muted-foreground">{r.collaborator_name || "—"}</td>
                <td className="px-3 py-2 text-right font-mono">{formatPercent(r.share_ratio)}</td>
                <td className="px-3 py-2 text-right font-mono" title="按律师费总额 × 律所扣除比例">
                  {formatMoney(r.firm_deduction_amount)}
                </td>
                <td className="px-3 py-2 text-right font-mono" title="按律师费总额 × 归档暂押比例">
                  {formatMoney(r.archive_holdback_amount)}
                </td>
                <td className="px-3 py-2">
                  <HoldbackBadge status={r.archive_holdback_status} />
                  {r.archive_returned_amount > 0 && (
                    <p className="mt-0.5 font-mono text-caption text-muted-foreground">
                      已返 {formatMoney(r.archive_returned_amount)}
                    </p>
                  )}
                </td>
                <td className="px-3 py-2 font-mono text-muted-foreground">{r.invoice_date || "未开票"}</td>
                <td className="px-3 py-2 text-muted-foreground">{r.invoice_no || "—"}</td>
                <td className="px-3 py-2 text-right">
                  <span className="font-mono font-semibold text-foreground">
                    {formatMoney(r.actual_income_amount)}
                  </span>
                  {!!r.actual_income_overridden && (
                    <p className="text-caption text-amber-700">手工覆盖</p>
                  )}
                </td>
                <td className="max-w-[180px] px-3 py-2 text-muted-foreground">
                  <span className="line-clamp-2">{r.note || r.actual_income_override_note || "—"}</span>
                </td>
                <td className="px-3 py-2 text-right">
                  <div className="flex justify-end gap-1">
                    <button
                      type="button"
                      onClick={() => onEdit(r)}
                      className="rounded p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                      title="编辑"
                    >
                      <Edit3 className="size-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={() => onDelete(r)}
                      className="rounded p-1.5 text-muted-foreground hover:bg-red-50 hover:text-red-600"
                      title="删除"
                    >
                      <Trash2 className="size-3.5" />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function IncomeFormDrawer({
  form,
  cases,
  saving,
  onChange,
  onClose,
  onSubmit,
}: {
  form: FormState;
  cases: Case[];
  saving: boolean;
  onChange: (next: FormState) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  const preview = calcPreview(form);
  const patch = (p: Partial<FormState>) => {
    const next = { ...form, ...p };
    if (p.sourceType === "personal") {
      next.shareRatioPercent = "100";
      next.collaboratorName = "";
    }
    if (p.invoiceDate && !form.recognizedMonth) {
      next.recognizedMonth = p.invoiceDate.slice(0, 7);
    }
    onChange(next);
  };
  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/20">
      <div className="h-full w-full max-w-3xl overflow-auto border-l border-border bg-background shadow-xl">
        <header className="sticky top-0 z-10 flex items-center gap-3 border-b border-border bg-card px-5 py-3">
          <h2 className="text-sm font-semibold text-foreground">
            {form.id ? "编辑收入记录" : "新增收入记录"}
          </h2>
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
            title="关闭"
          >
            <X className="size-4" />
          </button>
        </header>

        <div className="space-y-5 px-5 py-5">
          <div className="grid gap-3 sm:grid-cols-2">
            <SmallField label="关联案件">
              <select
                className={selectCls}
                value={form.caseId}
                onChange={(e) => {
                  const selected = cases.find((c) => c.id === e.target.value);
                  patch({
                    caseId: e.target.value,
                    manualCaseName: selected?.name ?? form.manualCaseName,
                  });
                }}
              >
                <option value="">不关联案件,手工记录</option>
                {cases.map((c) => (
                  <option key={c.id} value={c.id}>
                    {getCaseDisplayName(c)}
                  </option>
                ))}
              </select>
            </SmallField>
            <SmallField label="案件名称">
              <input
                className={inputCls}
                value={form.manualCaseName}
                onChange={(e) => patch({ manualCaseName: e.target.value })}
                placeholder="未关联案件时必填"
              />
            </SmallField>
            <SmallField label="律师费总额(元)">
              <input
                type="number"
                min={0}
                step={0.01}
                className={inputCls}
                value={form.lawyerFeeTotal}
                onChange={(e) => patch({ lawyerFeeTotal: e.target.value })}
              />
            </SmallField>
            <SmallField label="案源">
              <select
                className={selectCls}
                value={form.sourceType}
                onChange={(e) => patch({ sourceType: e.target.value as IncomeSourceType })}
              >
                <option value="personal">个人</option>
                <option value="collaboration">合作</option>
              </select>
            </SmallField>
            {form.sourceType === "collaboration" && (
              <>
                <SmallField label="合作方">
                  <input
                    className={inputCls}
                    value={form.collaboratorName}
                    onChange={(e) => patch({ collaboratorName: e.target.value })}
                    placeholder="合作律师 / 渠道 / 客户来源"
                  />
                </SmallField>
                <SmallField label="分成比例(%)">
                  <input
                    type="number"
                    min={0}
                    max={100}
                    step={0.01}
                    className={inputCls}
                    value={form.shareRatioPercent}
                    onChange={(e) => patch({ shareRatioPercent: e.target.value })}
                  />
                </SmallField>
              </>
            )}
            <SmallField label="律所固定扣除(%)">
              <input
                type="number"
                min={0}
                max={100}
                step={0.01}
                className={inputCls}
                value={form.firmDeductionPercent}
                onChange={(e) => patch({ firmDeductionPercent: e.target.value })}
              />
            </SmallField>
            <SmallField label="归档暂押比例(%)">
              <input
                type="number"
                min={0}
                max={100}
                step={0.01}
                className={inputCls}
                value={form.archiveHoldbackPercent}
                onChange={(e) => patch({ archiveHoldbackPercent: e.target.value })}
              />
            </SmallField>
            <SmallField label="暂押状态">
              <select
                className={selectCls}
                value={form.archiveHoldbackStatus}
                onChange={(e) =>
                  patch({ archiveHoldbackStatus: e.target.value as IncomeArchiveHoldbackStatus })
                }
              >
                <option value="holding">暂押中</option>
                <option value="returned">已返还</option>
                <option value="not_returned">不返还</option>
              </select>
            </SmallField>
            {form.archiveHoldbackStatus === "returned" && (
              <>
                <SmallField label="返还日期">
                  <input
                    type="date"
                    className={inputCls}
                    value={form.archiveReturnedAt}
                    onChange={(e) => patch({ archiveReturnedAt: e.target.value })}
                  />
                </SmallField>
                <SmallField label="返还金额(元)">
                  <input
                    type="number"
                    min={0}
                    step={0.01}
                    className={inputCls}
                    value={form.archiveReturnedAmount}
                    onChange={(e) => patch({ archiveReturnedAmount: e.target.value })}
                  />
                </SmallField>
              </>
            )}
            <SmallField label="开票日期">
              <input
                type="date"
                className={inputCls}
                value={form.invoiceDate}
                onChange={(e) =>
                  patch({
                    invoiceDate: e.target.value,
                    recognizedMonth: form.recognizedMonth || e.target.value.slice(0, 7),
                  })
                }
              />
            </SmallField>
            <SmallField label="发票编号">
              <input
                className={inputCls}
                value={form.invoiceNo}
                onChange={(e) => patch({ invoiceNo: e.target.value })}
              />
            </SmallField>
            <SmallField label="收入确认月份">
              <input
                type="month"
                className={inputCls}
                value={form.recognizedMonth}
                onChange={(e) => patch({ recognizedMonth: e.target.value })}
              />
            </SmallField>
            <SmallField label="备注">
              <input
                className={inputCls}
                value={form.note}
                onChange={(e) => patch({ note: e.target.value })}
                placeholder="自由备注"
              />
            </SmallField>
          </div>

          <div className="rounded-lg border border-border bg-card p-4">
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <p className="text-xs font-semibold text-foreground">保存前计算预览</p>
              <span className="text-caption text-muted-foreground">
                实际落库金额以后端 API 计算结果为准
              </span>
            </div>
            <div className="grid gap-2 text-xs sm:grid-cols-4">
              <PreviewItem label="分成前个人份额" value={preview.personalShare} />
              <PreviewItem label="律所扣除" value={preview.firmDeduction} />
              <PreviewItem label="归档暂押" value={preview.archiveHoldback} />
              <PreviewItem label="默认实际收入" value={preview.defaultActualIncome} strong />
            </div>
            <p className="mt-3 text-caption text-muted-foreground">
              公式:律师费总额 × 分成比例 - 律师费总额 × 律所扣除比例 - 律师费总额 × 归档暂押比例。
            </p>
          </div>

          <div className="rounded-lg border border-amber-200 bg-amber-50/60 p-4">
            <label className="flex items-start gap-2 text-xs text-amber-900">
              <input
                type="checkbox"
                checked={form.actualIncomeOverridden}
                onChange={(e) => patch({ actualIncomeOverridden: e.target.checked })}
                className="mt-0.5 accent-amber-700"
              />
              <span>
                手工覆盖实际收入
                <span className="mt-0.5 block text-caption text-amber-800/80">
                  开启后统计将按手工金额计算,表格会显示“手工覆盖”标记。
                </span>
              </span>
            </label>
            {form.actualIncomeOverridden && (
              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                <SmallField label="覆盖后的实际收入(元)">
                  <input
                    type="number"
                    step={0.01}
                    className={inputCls}
                    value={form.actualIncomeAmount}
                    onChange={(e) => patch({ actualIncomeAmount: e.target.value })}
                  />
                </SmallField>
                <SmallField label="覆盖原因">
                  <input
                    className={inputCls}
                    value={form.actualIncomeOverrideNote}
                    onChange={(e) => patch({ actualIncomeOverrideNote: e.target.value })}
                    placeholder="例如:扣除口径另有约定"
                  />
                </SmallField>
              </div>
            )}
          </div>

          <div className="flex justify-end gap-2 border-t border-border pt-4">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground"
            >
              取消
            </button>
            <button
              type="button"
              onClick={onSubmit}
              disabled={saving}
              className="rounded-md bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-700 disabled:opacity-50"
            >
              {saving ? "保存中..." : form.recordStatus === "draft" ? "确认并纳入统计" : "保存"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function SmallField({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="space-y-1">
      <span className="block text-caption font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      {children}
    </label>
  );
}

function PreviewItem({
  label,
  value,
  strong,
}: {
  label: string;
  value: number;
  strong?: boolean;
}) {
  return (
    <div className="rounded-md border border-border bg-background px-3 py-2">
      <p className="text-caption text-muted-foreground">{label}</p>
      <p className={cn("mt-1 font-mono text-sm", strong ? "font-semibold text-foreground" : "text-foreground")}>
        {formatMoney(value)}
      </p>
    </div>
  );
}

function HoldbackBadge({ status }: { status: IncomeArchiveHoldbackStatus }) {
  return (
    <span
      className={cn(
        "rounded-full px-2 py-0.5 text-caption",
        status === "holding" && "bg-amber-50 text-amber-700",
        status === "returned" && "bg-emerald-50 text-emerald-700",
        status === "not_returned" && "bg-muted text-muted-foreground",
      )}
    >
      {HOLDBACK_LABEL[status]}
    </span>
  );
}

function EmptyIncome({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="rounded-lg border border-dashed border-border bg-card/30 p-12 text-center">
      <CircleDollarSign className="mx-auto size-10 text-muted-foreground/40" />
      <h2 className="mt-4 text-base font-semibold text-foreground">还没有收入记录</h2>
      <p className="mx-auto mt-2 max-w-md text-sm text-muted-foreground">
        收入台账独立保存,可以关联已有案件,也可以手工记录未导入案件。
      </p>
      <button
        type="button"
        onClick={onCreate}
        className="mt-5 inline-flex items-center gap-1.5 rounded-md bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-700"
      >
        <Plus className="size-3.5" />
        新增第一条
      </button>
    </div>
  );
}

function makeBlankForm(seedCase: Case | null): FormState {
  const currentMonth = formatMonth(new Date());
  return {
    id: null,
    caseId: seedCase?.id ?? "",
    manualCaseName: seedCase?.name ?? "",
    lawyerFeeTotal: "",
    sourceType: "personal",
    collaboratorName: "",
    shareRatioPercent: "100",
    firmDeductionPercent: "15",
    archiveHoldbackPercent: "5",
    archiveHoldbackStatus: "holding",
    archiveReturnedAt: "",
    archiveReturnedAmount: "",
    invoiceDate: "",
    invoiceNo: "",
    recognizedMonth: currentMonth,
    actualIncomeOverridden: false,
    actualIncomeAmount: "",
    actualIncomeOverrideNote: "",
    note: "",
  };
}

function formFromRecord(record: IncomeRecord): FormState {
  return {
    id: record.id,
    recordStatus: record.record_status,
    caseId: record.case_id ?? "",
    manualCaseName: record.manual_case_name ?? record.case_name ?? "",
    lawyerFeeTotal: String(record.lawyer_fee_total),
    sourceType: record.source_type,
    collaboratorName: record.collaborator_name ?? "",
    shareRatioPercent: ratioToPercent(record.share_ratio),
    firmDeductionPercent: ratioToPercent(record.firm_deduction_rate),
    archiveHoldbackPercent: ratioToPercent(record.archive_holdback_rate),
    archiveHoldbackStatus: record.archive_holdback_status,
    archiveReturnedAt: record.archive_returned_at ?? "",
    archiveReturnedAmount: record.archive_returned_amount ? String(record.archive_returned_amount) : "",
    invoiceDate: record.invoice_date ?? "",
    invoiceNo: record.invoice_no ?? "",
    recognizedMonth: record.recognized_month,
    actualIncomeOverridden: !!record.actual_income_overridden,
    actualIncomeAmount: record.actual_income_overridden ? String(record.actual_income_amount) : "",
    actualIncomeOverrideNote: record.actual_income_override_note ?? "",
    note: record.note ?? "",
  };
}

function toUpsertInput(form: FormState): IncomeRecordUpsertInput | null {
  const lawyerFeeTotal = parseNumber(form.lawyerFeeTotal);
  if (lawyerFeeTotal <= 0) {
    toast("请填写大于 0 的律师费总额", "error");
    return null;
  }
  if (!form.caseId && !form.manualCaseName.trim()) {
    toast("未关联案件时,案件名称必填", "error");
    return null;
  }
  const shareRatio = percentToRatio(form.shareRatioPercent);
  const firmRate = percentToRatio(form.firmDeductionPercent);
  const holdbackRate = percentToRatio(form.archiveHoldbackPercent);
  if ([shareRatio, firmRate, holdbackRate].some((v) => v < 0 || v > 1)) {
    toast("分成、扣除和暂押比例需在 0% 到 100% 之间", "error");
    return null;
  }
  if (!/^\d{4}-\d{2}$/.test(form.recognizedMonth)) {
    toast("请填写收入确认月份", "error");
    return null;
  }
  if (form.actualIncomeOverridden && form.actualIncomeAmount.trim() === "") {
    toast("开启手工覆盖后,请填写覆盖后的实际收入", "error");
    return null;
  }
  return {
    id: form.id,
    record_status: form.recordStatus === "draft" ? "confirmed" : undefined,
    case_id: form.caseId || null,
    manual_case_name: form.manualCaseName.trim() || null,
    lawyer_fee_total: lawyerFeeTotal,
    source_type: form.sourceType,
    collaborator_name:
      form.sourceType === "collaboration" ? form.collaboratorName.trim() || null : null,
    share_ratio: shareRatio,
    firm_deduction_rate: firmRate,
    archive_holdback_rate: holdbackRate,
    archive_holdback_status: form.archiveHoldbackStatus,
    archive_returned_at:
      form.archiveHoldbackStatus === "returned" ? form.archiveReturnedAt || null : null,
    archive_returned_amount:
      form.archiveHoldbackStatus === "returned" ? parseNumber(form.archiveReturnedAmount) : 0,
    invoice_date: form.invoiceDate || null,
    invoice_no: form.invoiceNo.trim() || null,
    recognized_month: form.recognizedMonth,
    actual_income_overridden: form.actualIncomeOverridden ? 1 : 0,
    actual_income_amount: form.actualIncomeOverridden
      ? parseNumber(form.actualIncomeAmount)
      : null,
    actual_income_override_note: form.actualIncomeOverridden
      ? form.actualIncomeOverrideNote.trim() || null
      : null,
    note: form.note.trim() || null,
  };
}

function calcPreview(form: FormState) {
  const fee = parseNumber(form.lawyerFeeTotal);
  const share = fee * percentToRatio(form.shareRatioPercent);
  const firm = fee * percentToRatio(form.firmDeductionPercent);
  const holdback = fee * percentToRatio(form.archiveHoldbackPercent);
  return {
    personalShare: roundMoney(share),
    firmDeduction: roundMoney(firm),
    archiveHoldback: roundMoney(holdback),
    defaultActualIncome: roundMoney(share - firm - holdback),
  };
}

function buildFilter(filters: FilterState): IncomeRecordFilter {
  return {
    month_from: filters.monthFrom || null,
    month_to: filters.monthTo || null,
    source_type: filters.sourceType === "all" ? null : filters.sourceType,
    archive_holdback_status:
      filters.holdbackStatus === "all" ? null : filters.holdbackStatus,
    invoice_status: filters.invoiceStatus,
    query: filters.query.trim() || null,
  };
}

function rangeFilter(range: { from: string; to: string }): IncomeRecordFilter {
  return { month_from: range.from, month_to: range.to, invoice_status: "all" };
}

function getCurrentMonthRange() {
  const month = formatMonth(new Date());
  return { from: month, to: month };
}

function getCurrentQuarterRange() {
  const now = new Date();
  const year = now.getFullYear();
  const quarterStartMonth = Math.floor(now.getMonth() / 3) * 3;
  return {
    from: formatMonth(new Date(year, quarterStartMonth, 1)),
    to: formatMonth(new Date(year, quarterStartMonth + 2, 1)),
  };
}

function getCurrentYearRange() {
  const year = new Date().getFullYear();
  return { from: `${year}-01`, to: `${year}-12` };
}

function formatMonth(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  return `${y}-${m}`;
}

function displayCaseName(record: IncomeRecord): string {
  return record.case_name || record.manual_case_name || "未命名案件";
}

function parseNumber(value: string): number {
  const n = Number.parseFloat(value);
  return Number.isFinite(n) ? n : 0;
}

function percentToRatio(value: string): number {
  return parseNumber(value) / 100;
}

function ratioToPercent(value: number): string {
  return (value * 100).toFixed(2).replace(/\.?0+$/, "");
}

function roundMoney(value: number): number {
  return Math.round(value * 100) / 100;
}

function formatMoney(value: number): string {
  return `¥${roundMoney(value).toLocaleString("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

function formatPercent(value: number): string {
  return `${ratioToPercent(value)}%`;
}
