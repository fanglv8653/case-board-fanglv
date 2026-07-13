import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Loader2, Plus, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/lib/dialog";
import {
  deleteCaseAgencyContact,
  deleteCaseWorkItem,
  deleteCaseStageItem,
  deleteCriminalDeadlineItem,
  getCriminalCaseProfile,
  listCaseAgencyContacts,
  listCaseStageItems,
  listCaseWorkItems,
  listCriminalDeadlineItems,
  upsertCaseAgencyContact,
  upsertCaseWorkItem,
  upsertCaseStageItem,
  upsertCriminalCaseProfile,
  upsertCriminalDeadlineItem,
} from "@/lib/api";
import type {
  CaseAgencyContact,
  CaseAgencyContactUpsertInput,
  CaseStageItem,
  CaseStageItemUpsertInput,
  CaseWorkItem,
  CaseWorkItemUpsertInput,
  CriminalCaseProfile,
  CriminalCaseProfileUpsertInput,
  CriminalDeadlineItem,
  CriminalDeadlineItemUpsertInput,
} from "@/lib/types";
import { cn } from "@/lib/utils";

type ProfileForm = CriminalCaseProfileUpsertInput;
type StageForm = CaseStageItemUpsertInput;
type DeadlineForm = CriminalDeadlineItemUpsertInput;
type ContactForm = CaseAgencyContactUpsertInput;
type WorkForm = CaseWorkItemUpsertInput & {
  hours: string;
  minutes: string;
};

const EMPTY_PROFILE: Omit<ProfileForm, "case_id"> = {
  current_stage: "",
  procedure_type: "",
  case_subtype: "",
  defense_role: "",
  suspected_charge: "",
  suspect_or_defendant_name: "",
  victim_name: "",
  client_name: "",
  client_relationship: "",
  detention_center: "",
  coercive_measure_type: "",
  detention_date: "",
  arrest_request_date: "",
  arrest_review_received_date: "",
  arrest_decision_date: "",
  arrest_date: "",
  bail_start_date: "",
  residential_surveillance_start_date: "",
  transfer_for_prosecution_date: "",
  prosecution_received_date: "",
  first_instance_accepted_date: "",
  second_instance_accepted_date: "",
  judgment_received_date: "",
  ruling_received_date: "",
  notes: "",
  user_overrides_json: "",
};

const STAGE_STATUS_OPTIONS = [
  ["pending", "未开始"],
  ["active", "进行中"],
  ["completed", "已完成"],
  ["paused", "暂停"],
];

const DEADLINE_STATUS_OPTIONS = [
  ["open", "待处理"],
  ["done", "已完成"],
  ["overdue", "已逾期"],
  ["waived", "不适用"],
];

const PRIORITY_OPTIONS = [
  ["normal", "普通"],
  ["high", "高"],
  ["urgent", "紧急"],
];

export function CriminalCasePanel({ caseId }: { caseId: string }) {
  const [profileForm, setProfileForm] = useState<ProfileForm>({
    case_id: caseId,
    ...EMPTY_PROFILE,
  });
  const [stages, setStages] = useState<CaseStageItem[]>([]);
  const [deadlines, setDeadlines] = useState<CriminalDeadlineItem[]>([]);
  const [contacts, setContacts] = useState<CaseAgencyContact[]>([]);
  const [workItems, setWorkItems] = useState<CaseWorkItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [savingProfile, setSavingProfile] = useState(false);
  const [savingList, setSavingList] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [stageForm, setStageForm] = useState<StageForm | null>(null);
  const [deadlineForm, setDeadlineForm] = useState<DeadlineForm | null>(null);
  const [contactForm, setContactForm] = useState<ContactForm | null>(null);
  const [workForm, setWorkForm] = useState<WorkForm | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [profile, stageRows, deadlineRows, contactRows, workRows] =
        await Promise.all([
          getCriminalCaseProfile(caseId),
          listCaseStageItems(caseId),
          listCriminalDeadlineItems(caseId),
          listCaseAgencyContacts(caseId),
          listCaseWorkItems({ case_id: caseId }),
        ]);
      setProfileForm(toProfileForm(caseId, profile));
      setStages(stageRows);
      setDeadlines(deadlineRows);
      setContacts(contactRows);
      setWorkItems(workRows);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [caseId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const saveProfile = async () => {
    setSavingProfile(true);
    try {
      await upsertCriminalCaseProfile(cleanProfile(profileForm));
      toast("刑事画像已保存", "success");
      await reload();
    } catch (e) {
      toast(`刑事画像保存失败:${e}`, "error");
    } finally {
      setSavingProfile(false);
    }
  };

  const saveStage = async () => {
    if (!stageForm?.stage_label?.trim()) {
      toast("请填写阶段名称", "error");
      return;
    }
    setSavingList(true);
    try {
      await upsertCaseStageItem(cleanStage(stageForm));
      setStageForm(null);
      toast("阶段节点已保存", "success");
      await reload();
    } catch (e) {
      toast(`阶段节点保存失败:${e}`, "error");
    } finally {
      setSavingList(false);
    }
  };

  const saveDeadline = async () => {
    if (!deadlineForm?.title?.trim()) {
      toast("请填写期限名称", "error");
      return;
    }
    setSavingList(true);
    try {
      await upsertCriminalDeadlineItem(cleanDeadline(deadlineForm));
      setDeadlineForm(null);
      toast("期限节点已保存", "success");
      await reload();
    } catch (e) {
      toast(`期限节点保存失败:${e}`, "error");
    } finally {
      setSavingList(false);
    }
  };

  const saveContact = async () => {
    if (!contactForm?.agency_name?.trim() && !contactForm?.contact_name?.trim()) {
      toast("请填写机关名称或联系人", "error");
      return;
    }
    setSavingList(true);
    try {
      await upsertCaseAgencyContact(cleanContact(contactForm));
      setContactForm(null);
      toast("机关联系人已保存", "success");
      await reload();
    } catch (e) {
      toast(`机关联系人保存失败:${e}`, "error");
    } finally {
      setSavingList(false);
    }
  };

  const saveWork = async () => {
    if (!workForm?.occurred_at.trim() || !workForm.work_type?.trim() || !workForm.content.trim()) {
      toast("请填写时间、阶段和工作内容", "error");
      return;
    }
    const hours = Number(workForm.hours || 0);
    const minutes = Number(workForm.minutes || 0);
    if (
      !Number.isInteger(hours) ||
      !Number.isInteger(minutes) ||
      hours < 0 ||
      minutes < 0 ||
      minutes > 59
    ) {
      toast("工时须为非负小时和 0-59 分钟", "error");
      return;
    }
    setSavingList(true);
    try {
      const { hours: _hours, minutes: _minutes, ...input } = workForm;
      await upsertCaseWorkItem({
        ...input,
        case_id: caseId,
        title: `${input.work_type} ${input.occurred_at.slice(0, 10)}`,
        duration_minutes: hours * 60 + minutes,
      });
      setWorkForm(null);
      toast("工作记录已保存", "success");
      await reload();
    } catch (e) {
      toast(`工作记录保存失败:${e}`, "error");
    } finally {
      setSavingList(false);
    }
  };

  const confirmWork = async (item: CaseWorkItem) => {
    setSavingList(true);
    try {
      await upsertCaseWorkItem({
        ...item,
        confirmation_status: "confirmed",
      });
      toast("工作记录已确认并计入工时", "success");
      await reload();
    } catch (e) {
      toast(`确认失败:${e}`, "error");
    } finally {
      setSavingList(false);
    }
  };

  const removeWork = async (item: CaseWorkItem) => {
    if (!(await confirmDialog(`删除工作记录「${item.title}」？`, { danger: true }))) {
      return;
    }
    try {
      await deleteCaseWorkItem(item.id);
      toast("工作记录已删除", "success");
      await reload();
    } catch (e) {
      toast(`删除失败:${e}`, "error");
    }
  };

  const openWorkForm = (item?: CaseWorkItem) => {
    const duration = item?.duration_minutes ?? 0;
    setWorkForm(
      item
        ? {
            ...item,
            hours: String(Math.floor(duration / 60)),
            minutes: String(duration % 60),
          }
        : {
            occurred_at: new Date().toISOString().slice(0, 16),
            work_type: "",
            title: "",
            content: "",
            duration_minutes: 0,
            source: "manual",
            confirmation_status: "confirmed",
            hours: "0",
            minutes: "0",
          },
    );
  };

  const removeStage = async (item: CaseStageItem) => {
    if (!(await confirmDialog(`删除阶段节点「${item.stage_label}」？`, { danger: true }))) {
      return;
    }
    try {
      await deleteCaseStageItem(item.id);
      await reload();
    } catch (e) {
      toast(`删除失败:${e}`, "error");
    }
  };

  const removeDeadline = async (item: CriminalDeadlineItem) => {
    if (!(await confirmDialog(`删除期限节点「${item.title}」？`, { danger: true }))) {
      return;
    }
    try {
      await deleteCriminalDeadlineItem(item.id);
      await reload();
    } catch (e) {
      toast(`删除失败:${e}`, "error");
    }
  };

  const removeContact = async (item: CaseAgencyContact) => {
    const label = item.agency_name || item.contact_name || "该联系人";
    if (!(await confirmDialog(`删除机关联系人「${label}」？`, { danger: true }))) {
      return;
    }
    try {
      await deleteCaseAgencyContact(item.id);
      await reload();
    } catch (e) {
      toast(`删除失败:${e}`, "error");
    }
  };

  if (loading) {
    return (
      <section className="rounded-xl border border-border bg-card p-5">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          正在加载刑事业务信息
        </div>
      </section>
    );
  }

  return (
    <section className="space-y-5 rounded-xl border border-border bg-card p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="font-mono text-caption uppercase tracking-wider text-muted-foreground">
            CRIMINAL WORKSPACE
          </p>
          <h2 className="mt-1 text-lg font-semibold tracking-tight">刑事案件工作区</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            人工维护画像、阶段、期限和机关联系人。期限仅记录和提醒，不自动生成规则。
          </p>
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={reload}>
          <RefreshCw className="size-3.5" />
          刷新
        </Button>
      </div>

      {error && (
        <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          刑事信息加载失败：{error}
        </div>
      )}

      <Panel title="刑事画像">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <Field label="当前阶段">
            <TextInput
              value={profileForm.current_stage}
              onChange={(v) => setProfileForm({ ...profileForm, current_stage: v })}
              placeholder="侦查 / 审查起诉 / 一审"
            />
          </Field>
          <Field label="程序类型">
            <TextInput
              value={profileForm.procedure_type}
              onChange={(v) => setProfileForm({ ...profileForm, procedure_type: v })}
              placeholder="普通 / 简易 / 速裁"
            />
          </Field>
          <Field label="辩护身份">
            <TextInput
              value={profileForm.defense_role}
              onChange={(v) => setProfileForm({ ...profileForm, defense_role: v })}
              placeholder="侦查阶段辩护人"
            />
          </Field>
          <Field label="涉嫌罪名">
            <TextInput
              value={profileForm.suspected_charge}
              onChange={(v) => setProfileForm({ ...profileForm, suspected_charge: v })}
              placeholder="如：诈骗罪"
            />
          </Field>
          <Field label="嫌疑人/被告人">
            <TextInput
              value={profileForm.suspect_or_defendant_name}
              onChange={(v) =>
                setProfileForm({ ...profileForm, suspect_or_defendant_name: v })
              }
            />
          </Field>
          <Field label="委托关系">
            <TextInput
              value={profileForm.client_relationship}
              onChange={(v) =>
                setProfileForm({ ...profileForm, client_relationship: v })
              }
            />
          </Field>
          <Field label="羁押场所">
            <TextInput
              value={profileForm.detention_center}
              onChange={(v) => setProfileForm({ ...profileForm, detention_center: v })}
            />
          </Field>
          <Field label="强制措施">
            <TextInput
              value={profileForm.coercive_measure_type}
              onChange={(v) =>
                setProfileForm({ ...profileForm, coercive_measure_type: v })
              }
            />
          </Field>
          <Field label="拘留日期">
            <DateInput
              value={profileForm.detention_date}
              onChange={(v) => setProfileForm({ ...profileForm, detention_date: v })}
            />
          </Field>
          <Field label="逮捕日期">
            <DateInput
              value={profileForm.arrest_date}
              onChange={(v) => setProfileForm({ ...profileForm, arrest_date: v })}
            />
          </Field>
          <Field label="移送审查起诉">
            <DateInput
              value={profileForm.transfer_for_prosecution_date}
              onChange={(v) =>
                setProfileForm({
                  ...profileForm,
                  transfer_for_prosecution_date: v,
                })
              }
            />
          </Field>
          <Field label="检察院受理">
            <DateInput
              value={profileForm.prosecution_received_date}
              onChange={(v) =>
                setProfileForm({ ...profileForm, prosecution_received_date: v })
              }
            />
          </Field>
          <Field label="一审受理">
            <DateInput
              value={profileForm.first_instance_accepted_date}
              onChange={(v) =>
                setProfileForm({
                  ...profileForm,
                  first_instance_accepted_date: v,
                })
              }
            />
          </Field>
          <Field label="二审受理">
            <DateInput
              value={profileForm.second_instance_accepted_date}
              onChange={(v) =>
                setProfileForm({
                  ...profileForm,
                  second_instance_accepted_date: v,
                })
              }
            />
          </Field>
          <Field label="判决/裁定接收">
            <div className="grid grid-cols-2 gap-2">
              <DateInput
                value={profileForm.judgment_received_date}
                onChange={(v) =>
                  setProfileForm({ ...profileForm, judgment_received_date: v })
                }
              />
              <DateInput
                value={profileForm.ruling_received_date}
                onChange={(v) =>
                  setProfileForm({ ...profileForm, ruling_received_date: v })
                }
              />
            </div>
          </Field>
          <Field label="备注" className="md:col-span-3">
            <textarea
              value={profileForm.notes ?? ""}
              onChange={(e) =>
                setProfileForm({ ...profileForm, notes: e.currentTarget.value })
              }
              rows={3}
              className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-foreground focus:outline-none"
            />
          </Field>
        </div>
        <div className="mt-3 flex justify-end">
          <Button type="button" size="sm" onClick={saveProfile} disabled={savingProfile}>
            {savingProfile && <Loader2 className="size-3.5 animate-spin" />}
            保存刑事画像
          </Button>
        </div>
      </Panel>

      <Panel
        title="阶段节点"
        action={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setStageForm(newStageForm(caseId))}
          >
            <Plus className="size-3.5" />
            添加阶段
          </Button>
        }
      >
        {stageForm && (
          <StageEditor
            form={stageForm}
            onChange={setStageForm}
            onCancel={() => setStageForm(null)}
            onSave={saveStage}
            saving={savingList}
          />
        )}
        <ListState emptyText="还没有阶段节点，可先补录当前诉讼阶段。">
          {stages.map((item) => (
            <ListRow key={item.id}>
              <div className="min-w-0">
                <p className="font-medium text-foreground">{item.stage_label}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {[item.major_stage, labelStatus(item.status), item.started_at, item.due_at]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
                {item.notes && (
                  <p className="mt-1 text-xs text-muted-foreground">{item.notes}</p>
                )}
              </div>
              <RowActions
                onEdit={() => setStageForm(stageToForm(caseId, item))}
                onDelete={() => void removeStage(item)}
              />
            </ListRow>
          ))}
        </ListState>
      </Panel>

      <Panel
        title="期限节点"
        action={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setDeadlineForm(newDeadlineForm(caseId))}
          >
            <Plus className="size-3.5" />
            添加期限
          </Button>
        }
      >
        {deadlineForm && (
          <DeadlineEditor
            form={deadlineForm}
            onChange={setDeadlineForm}
            onCancel={() => setDeadlineForm(null)}
            onSave={saveDeadline}
            saving={savingList}
          />
        )}
        <ListState emptyText="还没有期限节点。本轮只支持人工录入，不自动生成期限规则。">
          {deadlines.map((item) => (
            <ListRow key={item.id}>
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="font-medium text-foreground">{item.title}</p>
                  <span className="rounded-full bg-muted px-2 py-0.5 text-caption text-muted-foreground">
                    {labelStatus(item.status)}
                  </span>
                  <span className="rounded-full bg-muted px-2 py-0.5 text-caption text-muted-foreground">
                    {labelPriority(item.priority)}
                  </span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {[
                    item.major_stage,
                    item.minor_stage,
                    item.effective_due_at ? `到期 ${item.effective_due_at}` : null,
                    item.reminder_at ? `提醒 ${item.reminder_at}` : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
                {(item.source_law || item.source_article || item.calculation_note) && (
                  <p className="mt-1 text-xs text-muted-foreground">
                    {[item.source_law, item.source_article, item.calculation_note]
                      .filter(Boolean)
                      .join(" · ")}
                  </p>
                )}
              </div>
              <RowActions
                onEdit={() => setDeadlineForm(deadlineToForm(caseId, item))}
                onDelete={() => void removeDeadline(item)}
              />
            </ListRow>
          ))}
        </ListState>
      </Panel>

      <Panel
        title="机关联系人"
        action={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setContactForm(newContactForm(caseId))}
          >
            <Plus className="size-3.5" />
            添加联系人
          </Button>
        }
      >
        {contactForm && (
          <ContactEditor
            form={contactForm}
            onChange={setContactForm}
            onCancel={() => setContactForm(null)}
            onSave={saveContact}
            saving={savingList}
          />
        )}
        <ListState emptyText="还没有机关联系人。可记录公安、检察院、法院、看守所等联络信息。">
          {contacts.map((item) => (
            <ListRow key={item.id}>
              <div className="min-w-0">
                <p className="font-medium text-foreground">
                  {item.agency_name || "未命名机关"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {[
                    item.stage_scope,
                    item.agency_type,
                    item.contact_role,
                    item.contact_name,
                    item.phone,
                    item.case_no ? `案号 ${item.case_no}` : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
                {item.notes && (
                  <p className="mt-1 text-xs text-muted-foreground">{item.notes}</p>
                )}
              </div>
              <RowActions
                onEdit={() => setContactForm(contactToForm(caseId, item))}
                onDelete={() => void removeContact(item)}
              />
            </ListRow>
          ))}
        </ListState>
      </Panel>

      <Panel title="工作记录">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2 text-sm text-muted-foreground">
          <span>{formatConfirmedDuration(workItems)}</span>
          <Button type="button" size="sm" onClick={() => openWorkForm()}>
            <Plus className="size-3.5" />
            新增工作记录
          </Button>
        </div>
        {workForm && (
          <div className="mb-3 grid gap-3 rounded-lg border border-border bg-background p-3 md:grid-cols-2">
            <Field label="时间">
              <input
                className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
                type="datetime-local"
                value={workForm.occurred_at.slice(0, 16)}
                onChange={(event) =>
                  setWorkForm({ ...workForm, occurred_at: event.target.value })
                }
              />
            </Field>
            <Field label="阶段">
              <TextInput
                value={workForm.work_type}
                onChange={(value) => setWorkForm({ ...workForm, work_type: value })}
                placeholder="例如：侦查阶段"
              />
            </Field>
            <Field label="工作内容" className="md:col-span-2">
              <textarea
                className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
                value={workForm.content}
                onChange={(event) =>
                  setWorkForm({ ...workForm, content: event.target.value })
                }
              />
            </Field>
            <Field label="工作时间">
              <div className="flex items-center gap-2">
                <input
                  className="w-20 rounded-md border border-border bg-background px-3 py-2 text-sm"
                  inputMode="numeric"
                  value={workForm.hours}
                  onChange={(event) => setWorkForm({ ...workForm, hours: event.target.value })}
                />
                <span>小时</span>
                <input
                  className="w-20 rounded-md border border-border bg-background px-3 py-2 text-sm"
                  inputMode="numeric"
                  value={workForm.minutes}
                  onChange={(event) => setWorkForm({ ...workForm, minutes: event.target.value })}
                />
                <span>分钟</span>
              </div>
            </Field>
            <div className="flex items-end justify-end gap-2">
              <Button type="button" variant="ghost" onClick={() => setWorkForm(null)}>
                取消
              </Button>
              <Button type="button" disabled={savingList} onClick={() => void saveWork()}>
                保存
              </Button>
            </div>
          </div>
        )}
        <ListState emptyText="暂未关联工作记录。">
          {workItems.map((item) => (
            <ListRow key={item.id}>
              <div className="min-w-0">
                <p className="font-medium text-foreground">{item.title}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {[
                    item.occurred_at,
                    item.work_type,
                    item.confirmation_status === "pending" ? "待确认（不计工时）" : "已确认",
                    workSourceLabel(item),
                    item.source_filename,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
                {item.content && (
                  <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                    {item.content}
                  </p>
                )}
              </div>
              <div className="flex flex-wrap items-center gap-1">
                <Button type="button" variant="ghost" size="sm" onClick={() => openWorkForm(item)}>
                  编辑
                </Button>
                {item.confirmation_status === "pending" && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={savingList}
                    onClick={() => void confirmWork(item)}
                  >
                    确认
                  </Button>
                )}
                <Button type="button" variant="ghost" size="sm" onClick={() => void removeWork(item)}>
                  <Trash2 className="size-3.5" />
                </Button>
              </div>
            </ListRow>
          ))}
        </ListState>
      </Panel>
    </section>
  );
}

function formatConfirmedDuration(items: CaseWorkItem[]) {
  const total = items
    .filter((item) => item.confirmation_status === "confirmed")
    .reduce((sum, item) => sum + (item.duration_minutes ?? 0), 0);
  return `已确认工时：共 ${Math.floor(total / 60)} 小时 ${total % 60} 分钟`;
}

function workSourceLabel(item: CaseWorkItem) {
  if (item.source === "manual") return "手工";
  if (item.source === "feishu" || item.external_source === "feishu") return "飞书导入";
  return "材料提取";
}

function Panel({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="rounded-lg border border-border bg-background/50 p-4">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-semibold tracking-tight">{title}</h3>
        {action}
      </div>
      {children}
    </div>
  );
}

function Field({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <label className={cn("space-y-1", className)}>
      <span className="text-caption font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      {children}
    </label>
  );
}

function TextInput({
  value,
  onChange,
  placeholder,
}: {
  value?: string | null;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <input
      type="text"
      value={value ?? ""}
      onChange={(e) => onChange(e.currentTarget.value)}
      placeholder={placeholder}
      className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground placeholder:text-muted-foreground/60 focus:border-foreground focus:outline-none"
    />
  );
}

function DateInput({
  value,
  onChange,
}: {
  value?: string | null;
  onChange: (value: string) => void;
}) {
  return (
    <input
      type="date"
      value={(value ?? "").slice(0, 10)}
      onChange={(e) => onChange(e.currentTarget.value)}
      className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground focus:border-foreground focus:outline-none"
    />
  );
}

function SelectInput({
  value,
  options,
  onChange,
}: {
  value?: string | null;
  options: string[][];
  onChange: (value: string) => void;
}) {
  return (
    <select
      value={value ?? ""}
      onChange={(e) => onChange(e.currentTarget.value)}
      className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground focus:border-foreground focus:outline-none"
    >
      {options.map(([v, label]) => (
        <option key={v} value={v}>
          {label}
        </option>
      ))}
    </select>
  );
}

function ListState({
  emptyText,
  children,
}: {
  emptyText: string;
  children: ReactNode | ReactNode[];
}) {
  const items = Array.isArray(children) ? children.filter(Boolean) : children ? [children] : [];
  if (items.length === 0) {
    return (
      <div className="rounded-md border border-dashed border-border bg-muted/30 px-3 py-5 text-center text-sm text-muted-foreground">
        {emptyText}
      </div>
    );
  }
  return <div className="space-y-2">{items}</div>;
}

function ListRow({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3 rounded-md border border-border bg-card px-3 py-3 text-sm">
      {children}
    </div>
  );
}

function RowActions({
  onEdit,
  onDelete,
}: {
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="flex shrink-0 items-center gap-1">
      <Button type="button" variant="ghost" size="sm" onClick={onEdit}>
        编辑
      </Button>
      <button
        type="button"
        onClick={onDelete}
        className="rounded p-1.5 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
        aria-label="删除"
      >
        <Trash2 className="size-3.5" />
      </button>
    </div>
  );
}

function StageEditor({
  form,
  onChange,
  onCancel,
  onSave,
  saving,
}: {
  form: StageForm;
  onChange: (form: StageForm) => void;
  onCancel: () => void;
  onSave: () => void;
  saving: boolean;
}) {
  return (
    <div className="mb-3 rounded-md border border-border bg-card p-3">
      <div className="grid grid-cols-1 gap-3 md:grid-cols-4">
        <Field label="阶段名称">
          <TextInput
            value={form.stage_label}
            onChange={(v) => onChange({ ...form, stage_label: v })}
          />
        </Field>
        <Field label="大阶段">
          <TextInput
            value={form.major_stage}
            onChange={(v) => onChange({ ...form, major_stage: v })}
          />
        </Field>
        <Field label="状态">
          <SelectInput
            value={form.status}
            options={STAGE_STATUS_OPTIONS}
            onChange={(v) => onChange({ ...form, status: v })}
          />
        </Field>
        <Field label="开始日期">
          <DateInput
            value={form.started_at}
            onChange={(v) => onChange({ ...form, started_at: v })}
          />
        </Field>
        <Field label="到期日期">
          <DateInput
            value={form.due_at}
            onChange={(v) => onChange({ ...form, due_at: v })}
          />
        </Field>
        <Field label="完成日期">
          <DateInput
            value={form.completed_at}
            onChange={(v) => onChange({ ...form, completed_at: v })}
          />
        </Field>
        <Field label="提醒日期">
          <DateInput
            value={form.reminder_at}
            onChange={(v) => onChange({ ...form, reminder_at: v })}
          />
        </Field>
        <Field label="备注">
          <TextInput
            value={form.notes}
            onChange={(v) => onChange({ ...form, notes: v })}
          />
        </Field>
      </div>
      <EditorActions onCancel={onCancel} onSave={onSave} saving={saving} />
    </div>
  );
}

function DeadlineEditor({
  form,
  onChange,
  onCancel,
  onSave,
  saving,
}: {
  form: DeadlineForm;
  onChange: (form: DeadlineForm) => void;
  onCancel: () => void;
  onSave: () => void;
  saving: boolean;
}) {
  return (
    <div className="mb-3 rounded-md border border-border bg-card p-3">
      <div className="grid grid-cols-1 gap-3 md:grid-cols-4">
        <Field label="期限名称">
          <TextInput
            value={form.title}
            onChange={(v) => onChange({ ...form, title: v })}
          />
        </Field>
        <Field label="大阶段">
          <TextInput
            value={form.major_stage}
            onChange={(v) => onChange({ ...form, major_stage: v })}
          />
        </Field>
        <Field label="小阶段">
          <TextInput
            value={form.minor_stage}
            onChange={(v) => onChange({ ...form, minor_stage: v })}
          />
        </Field>
        <Field label="状态">
          <SelectInput
            value={form.status}
            options={DEADLINE_STATUS_OPTIONS}
            onChange={(v) => onChange({ ...form, status: v })}
          />
        </Field>
        <Field label="触发日期">
          <DateInput
            value={form.trigger_date}
            onChange={(v) => onChange({ ...form, trigger_date: v })}
          />
        </Field>
        <Field label="默认到期">
          <DateInput
            value={form.default_due_at}
            onChange={(v) => onChange({ ...form, default_due_at: v })}
          />
        </Field>
        <Field label="人工到期">
          <DateInput
            value={form.manual_due_at}
            onChange={(v) =>
              onChange({ ...form, manual_due_at: v, effective_due_at: v || form.default_due_at })
            }
          />
        </Field>
        <Field label="有效到期">
          <DateInput
            value={form.effective_due_at}
            onChange={(v) => onChange({ ...form, effective_due_at: v })}
          />
        </Field>
        <Field label="提醒日期">
          <DateInput
            value={form.reminder_at}
            onChange={(v) => onChange({ ...form, reminder_at: v })}
          />
        </Field>
        <Field label="优先级">
          <SelectInput
            value={form.priority}
            options={PRIORITY_OPTIONS}
            onChange={(v) => onChange({ ...form, priority: v })}
          />
        </Field>
        <Field label="依据">
          <TextInput
            value={form.source_law}
            onChange={(v) => onChange({ ...form, source_law: v })}
            placeholder="刑诉法 / 司法解释"
          />
        </Field>
        <Field label="条文">
          <TextInput
            value={form.source_article}
            onChange={(v) => onChange({ ...form, source_article: v })}
          />
        </Field>
        <Field label="计算说明" className="md:col-span-2">
          <TextInput
            value={form.calculation_note}
            onChange={(v) => onChange({ ...form, calculation_note: v })}
          />
        </Field>
        <Field label="例外说明">
          <TextInput
            value={form.exception_note}
            onChange={(v) => onChange({ ...form, exception_note: v })}
          />
        </Field>
        <Field label="人工修正原因">
          <TextInput
            value={form.override_reason}
            onChange={(v) => onChange({ ...form, override_reason: v })}
          />
        </Field>
      </div>
      <EditorActions onCancel={onCancel} onSave={onSave} saving={saving} />
    </div>
  );
}

function ContactEditor({
  form,
  onChange,
  onCancel,
  onSave,
  saving,
}: {
  form: ContactForm;
  onChange: (form: ContactForm) => void;
  onCancel: () => void;
  onSave: () => void;
  saving: boolean;
}) {
  return (
    <div className="mb-3 rounded-md border border-border bg-card p-3">
      <div className="grid grid-cols-1 gap-3 md:grid-cols-4">
        <Field label="阶段范围">
          <TextInput
            value={form.stage_scope}
            onChange={(v) => onChange({ ...form, stage_scope: v })}
          />
        </Field>
        <Field label="机关类型">
          <TextInput
            value={form.agency_type}
            onChange={(v) => onChange({ ...form, agency_type: v })}
            placeholder="公安 / 检察院 / 法院"
          />
        </Field>
        <Field label="机关名称">
          <TextInput
            value={form.agency_name}
            onChange={(v) => onChange({ ...form, agency_name: v })}
          />
        </Field>
        <Field label="联系人角色">
          <TextInput
            value={form.contact_role}
            onChange={(v) => onChange({ ...form, contact_role: v })}
          />
        </Field>
        <Field label="联系人">
          <TextInput
            value={form.contact_name}
            onChange={(v) => onChange({ ...form, contact_name: v })}
          />
        </Field>
        <Field label="电话">
          <TextInput
            value={form.phone}
            onChange={(v) => onChange({ ...form, phone: v })}
          />
        </Field>
        <Field label="案号">
          <TextInput
            value={form.case_no}
            onChange={(v) => onChange({ ...form, case_no: v })}
          />
        </Field>
        <Field label="查询码">
          <TextInput
            value={form.query_code}
            onChange={(v) => onChange({ ...form, query_code: v })}
          />
        </Field>
        <Field label="备注" className="md:col-span-4">
          <TextInput
            value={form.notes}
            onChange={(v) => onChange({ ...form, notes: v })}
          />
        </Field>
      </div>
      <EditorActions onCancel={onCancel} onSave={onSave} saving={saving} />
    </div>
  );
}

function EditorActions({
  onCancel,
  onSave,
  saving,
}: {
  onCancel: () => void;
  onSave: () => void;
  saving: boolean;
}) {
  return (
    <div className="mt-3 flex justify-end gap-2">
      <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
        取消
      </Button>
      <Button type="button" size="sm" onClick={onSave} disabled={saving}>
        {saving && <Loader2 className="size-3.5 animate-spin" />}
        保存
      </Button>
    </div>
  );
}

function toProfileForm(
  caseId: string,
  profile: CriminalCaseProfile | null,
): ProfileForm {
  if (!profile) return { case_id: caseId, ...EMPTY_PROFILE };
  return {
    ...EMPTY_PROFILE,
    ...profile,
    case_id: caseId,
  };
}

function newStageForm(caseId: string): StageForm {
  return {
    case_id: caseId,
    domain: "criminal",
    major_stage: "",
    stage_label: "",
    status: "active",
    started_at: "",
    due_at: "",
    completed_at: "",
    reminder_at: "",
    source: "manual",
    notes: "",
  };
}

function stageToForm(caseId: string, item: CaseStageItem): StageForm {
  return {
    id: item.id,
    case_id: caseId,
    domain: item.domain || "criminal",
    major_stage: item.major_stage ?? "",
    stage_label: item.stage_label,
    status: item.status,
    started_at: item.started_at ?? "",
    due_at: item.due_at ?? "",
    completed_at: item.completed_at ?? "",
    reminder_at: item.reminder_at ?? "",
    source: item.source,
    external_source: item.external_source,
    external_record_id: item.external_record_id,
    raw_payload_json: item.raw_payload_json,
    notes: item.notes ?? "",
  };
}

function newDeadlineForm(caseId: string): DeadlineForm {
  return {
    case_id: caseId,
    title: "",
    major_stage: "",
    minor_stage: "",
    trigger_date: "",
    default_due_at: "",
    manual_due_at: "",
    effective_due_at: "",
    reminder_at: "",
    priority: "normal",
    status: "open",
    source_type: "manual",
    source_law: "",
    source_article: "",
    calculation_note: "",
    exception_note: "",
    override_reason: "",
  };
}

function deadlineToForm(caseId: string, item: CriminalDeadlineItem): DeadlineForm {
  return {
    id: item.id,
    case_id: caseId,
    stage_item_id: item.stage_item_id,
    rule_code: item.rule_code,
    title: item.title,
    major_stage: item.major_stage ?? "",
    minor_stage: item.minor_stage ?? "",
    trigger_date: item.trigger_date ?? "",
    trigger_time: item.trigger_time,
    default_due_at: item.default_due_at ?? "",
    manual_due_at: item.manual_due_at ?? "",
    effective_due_at: item.effective_due_at ?? "",
    reminder_at: item.reminder_at ?? "",
    priority: item.priority,
    status: item.status,
    source_type: item.source_type,
    source_law: item.source_law ?? "",
    source_article: item.source_article ?? "",
    source_url: item.source_url,
    calculation_note: item.calculation_note ?? "",
    exception_type: item.exception_type,
    exception_note: item.exception_note ?? "",
    override_reason: item.override_reason ?? "",
    completed_at: item.completed_at,
  };
}

function newContactForm(caseId: string): ContactForm {
  return {
    case_id: caseId,
    stage_scope: "",
    agency_type: "",
    agency_name: "",
    contact_role: "",
    contact_name: "",
    phone: "",
    case_no: "",
    query_code: "",
    notes: "",
    source: "manual",
  };
}

function contactToForm(caseId: string, item: CaseAgencyContact): ContactForm {
  return {
    id: item.id,
    case_id: caseId,
    stage_scope: item.stage_scope ?? "",
    agency_type: item.agency_type ?? "",
    agency_name: item.agency_name ?? "",
    contact_role: item.contact_role ?? "",
    contact_name: item.contact_name ?? "",
    phone: item.phone ?? "",
    case_no: item.case_no ?? "",
    query_code: item.query_code ?? "",
    notes: item.notes ?? "",
    source: item.source,
    external_record_id: item.external_record_id,
  };
}

function cleanProfile(form: ProfileForm): ProfileForm {
  return cleanRecord(form) as ProfileForm;
}

function cleanStage(form: StageForm): StageForm {
  return {
    ...(cleanRecord(form) as StageForm),
    domain: "criminal",
    source: form.source || "manual",
  };
}

function cleanDeadline(form: DeadlineForm): DeadlineForm {
  const effective = form.effective_due_at || form.manual_due_at || form.default_due_at || null;
  return {
    ...(cleanRecord(form) as DeadlineForm),
    effective_due_at: effective,
    source_type: form.source_type || "manual",
  };
}

function cleanContact(form: ContactForm): ContactForm {
  return {
    ...(cleanRecord(form) as ContactForm),
    source: form.source || "manual",
  };
}

function cleanRecord<T extends object>(form: T): T {
  return Object.fromEntries(
    Object.entries(form as Record<string, unknown>).map(([key, value]) => [
      key,
      typeof value === "string" && value.trim() === "" ? null : value,
    ]),
  ) as unknown as T;
}

function labelStatus(status: string | null | undefined) {
  switch (status) {
    case "pending":
      return "未开始";
    case "active":
      return "进行中";
    case "completed":
    case "done":
      return "已完成";
    case "overdue":
      return "已逾期";
    case "paused":
      return "暂停";
    case "waived":
      return "不适用";
    default:
      return status || "未标记";
  }
}

function labelPriority(priority: string | null | undefined) {
  switch (priority) {
    case "urgent":
      return "紧急";
    case "high":
      return "高";
    case "normal":
      return "普通";
    default:
      return priority || "普通";
  }
}
