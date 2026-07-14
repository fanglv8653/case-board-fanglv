import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical, Loader2, Plus, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/lib/dialog";
import {
  deleteCaseAgencyContact,
  deleteCaseWorkItem,
  deleteCaseStageItem,
  deleteCriminalDeadlineItem,
  confirmCriminalExtractionCandidateBatch,
  getCriminalCaseProfile,
  listCaseAgencyContacts,
  listCaseStageItems,
  listCaseWorkItems,
  listCriminalDeadlineItems,
  listCriminalExtractionCandidates,
  reextractCriminalCaseMaterials,
  rejectCriminalExtractionCandidateBatch,
  refreshCriminalDeadlines,
  reorderCaseStageItems,
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
  CriminalExtractionCandidateDetail,
  CriminalDeadlineItem,
  CriminalDeadlineItemUpsertInput,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  resolveStructuredListJson,
  structuredListToText,
} from "./criminalProfileJson";
import {
  needsApplicabilityOverrideReason,
  resolveDeadlineStageId,
} from "./criminalTimelineRules";
import {
  CriminalExtractionReviewPanel,
  type CriminalCandidateDecisionInput,
} from "./CriminalExtractionReviewPanel";
import {
  valuesAreEqual,
  parseProtectedFieldKeys,
  type CriminalExtractionCandidateBatchView,
} from "./criminalExtractionReviewModels";

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
  stage_sort_mode: "auto",
  guilty_plea_status: "",
  sentencing_recommendation: "",
  sentence_term: "",
  charge_history_json: "",
  restitution_amount: null,
  restitution_status: "",
  victim_forgiveness: "",
  surrender_status: "",
  meritorious_service_status: "",
  co_defendants_json: "",
  supplementary_investigation_1_date: "",
  supplementary_investigation_2_date: "",
  judgment_effective_date: "",
  death_penalty_review_start_date: "",
  extraction_meta_json: "",
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
  const [chargeHistoryText, setChargeHistoryText] = useState("");
  const [chargeHistoryInitialText, setChargeHistoryInitialText] = useState("");
  const [coDefendantsText, setCoDefendantsText] = useState("");
  const [coDefendantsInitialText, setCoDefendantsInitialText] = useState("");
  const [stages, setStages] = useState<CaseStageItem[]>([]);
  const [deadlines, setDeadlines] = useState<CriminalDeadlineItem[]>([]);
  const [contacts, setContacts] = useState<CaseAgencyContact[]>([]);
  const [workItems, setWorkItems] = useState<CaseWorkItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [savingProfile, setSavingProfile] = useState(false);
  const [savingList, setSavingList] = useState(false);
  const [refreshingDeadlines, setRefreshingDeadlines] = useState(false);
  const [recognizingMaterials, setRecognizingMaterials] = useState(false);
  const [submittingCandidateBatchId, setSubmittingCandidateBatchId] = useState<string | null>(null);
  const [candidateBatches, setCandidateBatches] = useState<CriminalExtractionCandidateBatchView[]>([]);
  const [reorderingStages, setReorderingStages] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [stageForm, setStageForm] = useState<StageForm | null>(null);
  const [deadlineForm, setDeadlineForm] = useState<DeadlineForm | null>(null);
  const [contactForm, setContactForm] = useState<ContactForm | null>(null);
  const [workForm, setWorkForm] = useState<WorkForm | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const timelineGroups = useMemo(() => {
    const grouped = new Map<string, CriminalDeadlineItem[]>();
    const unassigned: CriminalDeadlineItem[] = [];
    for (const deadline of deadlines) {
      const stageId = resolveDeadlineStageId(deadline, stages);
      if (!stageId) {
        unassigned.push(deadline);
        continue;
      }
      const items = grouped.get(stageId) ?? [];
      items.push(deadline);
      grouped.set(stageId, items);
    }
    return { grouped, unassigned };
  }, [deadlines, stages]);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [profile, stageRows, deadlineRows, contactRows, workRows, candidateRows] =
        await Promise.all([
          getCriminalCaseProfile(caseId),
          listCaseStageItems(caseId),
          listCriminalDeadlineItems(caseId),
          listCaseAgencyContacts(caseId),
          listCaseWorkItems({ case_id: caseId }),
          listCriminalExtractionCandidates(caseId),
        ]);
      setProfileForm(toProfileForm(caseId, profile));
      const nextChargeHistoryText = structuredListToText(
        profile?.charge_history_json,
        "charge",
      );
      const nextCoDefendantsText = structuredListToText(
        profile?.co_defendants_json,
        "name",
      );
      setChargeHistoryText(nextChargeHistoryText);
      setChargeHistoryInitialText(nextChargeHistoryText);
      setCoDefendantsText(nextCoDefendantsText);
      setCoDefendantsInitialText(nextCoDefendantsText);
      setStages(stageRows);
      setDeadlines(deadlineRows);
      setContacts(contactRows);
      setWorkItems(workRows);
      setCandidateBatches(toCandidateBatchViews(profile, candidateRows));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [caseId]);

  const recognizeMaterials = async () => {
    setRecognizingMaterials(true);
    try {
      const report = await reextractCriminalCaseMaterials(caseId);
      const warning = report.failed_count > 0
        ? `，失败 ${report.failed_count} 项${report.errors.length ? `：${report.errors.join("；")}` : ""}`
        : "";
      toast(
        `材料识别已启动：复用正文 ${report.cached_count} 份，安排 OCR ${report.scheduled_ocr_count} 份${warning}`,
        report.failed_count > 0 ? "error" : "success",
      );
      await reload();
    } catch (e) {
      toast(`重新识别案件材料失败：${e}`, "error");
    } finally {
      setRecognizingMaterials(false);
    }
  };

  const confirmCandidateBatch = async (
    batchId: string,
    expectedProfileRevision: number,
    decisions: CriminalCandidateDecisionInput[],
  ) => {
    setSubmittingCandidateBatchId(batchId);
    try {
      const result = await confirmCriminalExtractionCandidateBatch({
        batch_id: batchId,
        expected_profile_revision: expectedProfileRevision,
        decisions,
      });
      const protectedNote = result.protected_fields.length
        ? `；${result.protected_fields.length} 个字段因人工保护未覆盖`
        : "";
      toast(`已应用 ${result.applied_fields.length} 个候选字段${protectedNote}`, "success");
      await reload();
    } catch (e) {
      const message = String(e);
      toast(
        message.includes("revision") || message.includes("其他操作更新")
          ? "刑事画像已变化，候选已刷新，请重新确认。"
          : `候选确认失败：${message}`,
        "error",
      );
      if (message.includes("revision") || message.includes("其他操作更新")) {
        await reload();
      }
    } finally {
      setSubmittingCandidateBatchId(null);
    }
  };

  const rejectCandidateBatch = async (batchId: string) => {
    setSubmittingCandidateBatchId(batchId);
    try {
      await rejectCriminalExtractionCandidateBatch(batchId);
      toast("整批识别结果已拒绝", "success");
      await reload();
    } catch (e) {
      toast(`整批拒绝失败：${e}`, "error");
    } finally {
      setSubmittingCandidateBatchId(null);
    }
  };

  useEffect(() => {
    void reload();
  }, [reload]);

  const saveProfile = async () => {
    setSavingProfile(true);
    try {
      await upsertCriminalCaseProfile(
        cleanProfile({
          ...profileForm,
          charge_history_json: resolveStructuredListJson({
            rawJson: profileForm.charge_history_json,
            initialText: chargeHistoryInitialText,
            currentText: chargeHistoryText,
            key: "charge",
          }),
          co_defendants_json: resolveStructuredListJson({
            rawJson: profileForm.co_defendants_json,
            initialText: coDefendantsInitialText,
            currentText: coDefendantsText,
            key: "name",
          }),
        }),
      );
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
    const originalDeadline = deadlineForm.id
      ? deadlines.find((item) => item.id === deadlineForm.id)
      : null;
    if (needsApplicabilityOverrideReason(originalDeadline, deadlineForm)) {
      toast("自动期限的适用性已变更，请填写人工修正原因", "error");
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

  const refreshDeadlines = async () => {
    setRefreshingDeadlines(true);
    try {
      const report = await refreshCriminalDeadlines(caseId);
      toast(
        `期限已刷新：新增 ${report.generated_count}，更新 ${report.updated_count}，保留 ${report.preserved_count}，待确认 ${report.needs_confirmation_count}，跳过 ${report.skipped_count}`,
        "success",
      );
      await reload();
    } catch (e) {
      toast(`期限刷新失败：${e}`, "error");
    } finally {
      setRefreshingDeadlines(false);
    }
  };

  const handleStageDragEnd = async ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id || reorderingStages) return;
    const previous = stages;
    const oldIndex = previous.findIndex((item) => item.id === active.id);
    const newIndex = previous.findIndex((item) => item.id === over.id);
    if (oldIndex < 0 || newIndex < 0) return;
    const reordered = arrayMove(previous, oldIndex, newIndex);
    setStages(reordered);
    setReorderingStages(true);
    try {
      const persisted = await reorderCaseStageItems({
        case_id: caseId,
        ordered_ids: reordered.map((item) => item.id),
      });
      setStages(persisted);
      setProfileForm((current) => ({ ...current, stage_sort_mode: "manual" }));
      toast("阶段顺序已保存", "success");
    } catch (e) {
      setStages(previous);
      toast(`阶段排序保存失败，已恢复原顺序：${e}`, "error");
      await reload();
    } finally {
      setReorderingStages(false);
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
            维护刑事画像、办案时间轴和机关联系人。期限属于办案提醒，条件规则需人工确认。
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

      <CriminalExtractionReviewPanel
        batches={candidateBatches}
        loading={loading}
        recognizing={recognizingMaterials}
        submittingBatchId={submittingCandidateBatchId}
        onRecognize={recognizeMaterials}
        onConfirm={confirmCandidateBatch}
        onRejectBatch={rejectCandidateBatch}
      />

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
          <Field label="认罪认罚">
            <TextInput
              value={profileForm.guilty_plea_status ?? ""}
              onChange={(v) => setProfileForm({ ...profileForm, guilty_plea_status: v })}
              placeholder="未确认 / 已签署具结书"
            />
          </Field>
          <Field label="量刑建议">
            <TextInput
              value={profileForm.sentencing_recommendation ?? ""}
              onChange={(v) =>
                setProfileForm({ ...profileForm, sentencing_recommendation: v })
              }
            />
          </Field>
          <Field label="判决刑期">
            <TextInput
              value={profileForm.sentence_term ?? ""}
              onChange={(v) => setProfileForm({ ...profileForm, sentence_term: v })}
            />
          </Field>
          <Field label="退赃退赔金额">
            <input
              type="number"
              min="0"
              step="0.01"
              value={profileForm.restitution_amount ?? ""}
              onChange={(event) =>
                setProfileForm({
                  ...profileForm,
                  restitution_amount:
                    event.currentTarget.value === ""
                      ? null
                      : Number(event.currentTarget.value),
                })
              }
              className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-foreground focus:outline-none"
            />
          </Field>
          <Field label="退赃退赔状态">
            <TextInput
              value={profileForm.restitution_status ?? ""}
              onChange={(v) => setProfileForm({ ...profileForm, restitution_status: v })}
            />
          </Field>
          <Field label="被害人谅解">
            <TextInput
              value={profileForm.victim_forgiveness ?? ""}
              onChange={(v) => setProfileForm({ ...profileForm, victim_forgiveness: v })}
            />
          </Field>
          <Field label="自首情况">
            <TextInput
              value={profileForm.surrender_status ?? ""}
              onChange={(v) => setProfileForm({ ...profileForm, surrender_status: v })}
            />
          </Field>
          <Field label="立功情况">
            <TextInput
              value={profileForm.meritorious_service_status ?? ""}
              onChange={(v) =>
                setProfileForm({ ...profileForm, meritorious_service_status: v })
              }
            />
          </Field>
          <Field label="第一次退补日期">
            <DateInput
              value={profileForm.supplementary_investigation_1_date ?? ""}
              onChange={(v) =>
                setProfileForm({ ...profileForm, supplementary_investigation_1_date: v })
              }
            />
          </Field>
          <Field label="第二次退补日期">
            <DateInput
              value={profileForm.supplementary_investigation_2_date ?? ""}
              onChange={(v) =>
                setProfileForm({ ...profileForm, supplementary_investigation_2_date: v })
              }
            />
          </Field>
          <Field label="判决生效日期">
            <DateInput
              value={profileForm.judgment_effective_date ?? ""}
              onChange={(v) => setProfileForm({ ...profileForm, judgment_effective_date: v })}
            />
          </Field>
          <Field label="死刑复核起始日期">
            <DateInput
              value={profileForm.death_penalty_review_start_date ?? ""}
              onChange={(v) =>
                setProfileForm({ ...profileForm, death_penalty_review_start_date: v })
              }
            />
          </Field>
          <Field label="罪名历史" className="md:col-span-2">
            <textarea
              value={chargeHistoryText}
              onChange={(event) => setChargeHistoryText(event.currentTarget.value)}
              rows={3}
              placeholder="每行填写一个曾涉及或变更后的罪名"
              className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-foreground focus:outline-none"
            />
          </Field>
          <Field label="同案犯" className="md:col-span-1">
            <textarea
              value={coDefendantsText}
              onChange={(event) => setCoDefendantsText(event.currentTarget.value)}
              rows={3}
              placeholder="每行填写一人"
              className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-foreground focus:outline-none"
            />
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
        title="办案时间轴"
        action={
          <div className="flex flex-wrap items-center gap-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => void refreshDeadlines()}
              disabled={refreshingDeadlines || reorderingStages}
            >
              <RefreshCw className={cn("size-3.5", refreshingDeadlines && "animate-spin")} />
              {refreshingDeadlines ? "刷新中" : "刷新期限"}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setDeadlineForm(newDeadlineForm(caseId))}
            >
              <Plus className="size-3.5" />
              添加期限
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setStageForm(newStageForm(caseId))}
            >
              <Plus className="size-3.5" />
              添加阶段
            </Button>
          </div>
        }
      >
        <div className="mb-3 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-900 dark:text-amber-100">
          期限仅作为办案提醒；条件规则需人工确认，具体时点和特殊情形应结合案件事实核对，不替代法律判断。
        </div>
        {stageForm && (
          <StageEditor
            form={stageForm}
            onChange={setStageForm}
            onCancel={() => setStageForm(null)}
            onSave={saveStage}
            saving={savingList}
          />
        )}
        {deadlineForm && (
          <DeadlineEditor
            form={deadlineForm}
            stages={stages}
            onChange={setDeadlineForm}
            onCancel={() => setDeadlineForm(null)}
            onSave={saveDeadline}
            saving={savingList}
          />
        )}
        {stages.length === 0 ? (
          <ListState emptyText="还没有阶段节点，可先补录当前办案阶段。">
            {null}
          </ListState>
        ) : (
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragEnd={(event) => void handleStageDragEnd(event)}
          >
            <SortableContext
              items={stages.map((item) => item.id)}
              strategy={verticalListSortingStrategy}
            >
              <div className={cn("space-y-3", reorderingStages && "pointer-events-none opacity-70")}>
                {stages.map((item, index) => (
                  <SortableStageCard
                    key={item.id}
                    item={item}
                    index={index}
                    deadlines={timelineGroups.grouped.get(item.id) ?? []}
                    onEditStage={() => setStageForm(stageToForm(caseId, item))}
                    onDeleteStage={() => void removeStage(item)}
                    onEditDeadline={(deadline) =>
                      setDeadlineForm(deadlineToForm(caseId, deadline))
                    }
                    onDeleteDeadline={(deadline) => void removeDeadline(deadline)}
                  />
                ))}
              </div>
            </SortableContext>
          </DndContext>
        )}
        {timelineGroups.unassigned.length > 0 && (
          <div className="mt-4 rounded-lg border border-dashed border-border bg-muted/20 p-3">
            <p className="mb-2 text-sm font-semibold">未归入阶段的期限</p>
            <div className="space-y-2">
              {timelineGroups.unassigned.map((deadline) => (
                <DeadlineTimelineRow
                  key={deadline.id}
                  item={deadline}
                  onEdit={() => setDeadlineForm(deadlineToForm(caseId, deadline))}
                  onDelete={() => void removeDeadline(deadline)}
                />
              ))}
            </div>
          </div>
        )}
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

function SortableStageCard({
  item,
  index,
  deadlines,
  onEditStage,
  onDeleteStage,
  onEditDeadline,
  onDeleteDeadline,
}: {
  item: CaseStageItem;
  index: number;
  deadlines: CriminalDeadlineItem[];
  onEditStage: () => void;
  onDeleteStage: () => void;
  onEditDeadline: (item: CriminalDeadlineItem) => void;
  onDeleteDeadline: (item: CriminalDeadlineItem) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: item.id,
  });
  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn(
        "rounded-lg border border-border bg-background p-3",
        isDragging && "relative z-10 shadow-lg ring-1 ring-foreground/20",
      )}
    >
      <div className="flex items-start gap-2">
        <button
          type="button"
          className="mt-0.5 cursor-grab rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground active:cursor-grabbing"
          aria-label={`拖动排序：${item.stage_label}`}
          {...attributes}
          {...listeners}
        >
          <GripVertical className="size-4" />
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="flex size-5 items-center justify-center rounded-full bg-foreground text-caption text-background">
              {index + 1}
            </span>
            <p className="font-medium text-foreground">{item.stage_label}</p>
            <span className="rounded-full bg-muted px-2 py-0.5 text-caption text-muted-foreground">
              {labelStatus(item.status)}
            </span>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {[
              item.major_stage,
              item.started_at ? `开始 ${item.started_at}` : null,
              item.due_at ? `到期 ${item.due_at}` : null,
              item.completed_at ? `完成 ${item.completed_at}` : null,
            ]
              .filter(Boolean)
              .join(" · ") || "尚未填写阶段日期"}
          </p>
          {item.notes && <p className="mt-1 text-xs text-muted-foreground">{item.notes}</p>}
        </div>
        <RowActions onEdit={onEditStage} onDelete={onDeleteStage} />
      </div>
      <div className="ml-8 mt-3 border-l border-border pl-3">
        {deadlines.length === 0 ? (
          <p className="py-1 text-xs text-muted-foreground">本阶段暂无期限节点。</p>
        ) : (
          <div className="space-y-2">
            {deadlines.map((deadline) => (
              <DeadlineTimelineRow
                key={deadline.id}
                item={deadline}
                onEdit={() => onEditDeadline(deadline)}
                onDelete={() => onDeleteDeadline(deadline)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function DeadlineTimelineRow({
  item,
  onEdit,
  onDelete,
}: {
  item: CriminalDeadlineItem;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const muted = item.applicability_status === "not_applicable";
  return (
    <div className={cn("flex items-start justify-between gap-2 rounded-md bg-muted/40 p-2", muted && "opacity-60")}>
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-1.5">
          <p className="text-sm font-medium text-foreground">{item.title}</p>
          <DeadlineBadge>{labelStatus(item.status)}</DeadlineBadge>
          <DeadlineBadge>{labelPriority(item.priority)}</DeadlineBadge>
          <DeadlineBadge tone={item.applicability_status === "needs_confirmation" ? "warning" : "default"}>
            {labelApplicability(item.applicability_status)}
          </DeadlineBadge>
          <DeadlineBadge>{item.source_type === "auto" ? "规则生成" : "人工录入"}</DeadlineBadge>
        </div>
        <p className="mt-1 text-xs text-muted-foreground">
          {[
            item.effective_due_at ? `有效到期 ${item.effective_due_at}` : "未设置到期日",
            item.reminder_at ? `提醒 ${item.reminder_at}` : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
        {item.override_reason && (
          <p className="mt-1 text-xs text-amber-700 dark:text-amber-300">
            人工覆盖：{item.override_reason}
          </p>
        )}
        {(item.source_law || item.source_article || item.calculation_note) && (
          <p className="mt-1 text-caption text-muted-foreground">
            {[item.source_law, item.source_article, item.calculation_note]
              .filter(Boolean)
              .join(" · ")}
          </p>
        )}
      </div>
      <RowActions onEdit={onEdit} onDelete={onDelete} />
    </div>
  );
}

function DeadlineBadge({
  children,
  tone = "default",
}: {
  children: ReactNode;
  tone?: "default" | "warning";
}) {
  return (
    <span
      className={cn(
        "rounded-full px-2 py-0.5 text-caption",
        tone === "warning"
          ? "bg-amber-500/15 text-amber-800 dark:text-amber-200"
          : "bg-background text-muted-foreground",
      )}
    >
      {children}
    </span>
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
  stages,
  onChange,
  onCancel,
  onSave,
  saving,
}: {
  form: DeadlineForm;
  stages: CaseStageItem[];
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
        <Field label="所属阶段">
          <select
            value={form.stage_item_id ?? ""}
            onChange={(event) =>
              onChange({ ...form, stage_item_id: event.currentTarget.value || null })
            }
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-foreground focus:outline-none"
          >
            <option value="">未归入阶段</option>
            {stages.map((stage) => (
              <option key={stage.id} value={stage.id}>
                {stage.stage_label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="适用性">
          <SelectInput
            value={form.applicability_status ?? "confirmed"}
            options={[
              ["confirmed", "已确认"],
              ["needs_confirmation", "待确认"],
              ["not_applicable", "不适用"],
            ]}
            onChange={(v) =>
              onChange({
                ...form,
                applicability_status: v as DeadlineForm["applicability_status"],
              })
            }
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

function toCandidateBatchViews(
  profile: CriminalCaseProfile | null,
  details: CriminalExtractionCandidateDetail[],
): CriminalExtractionCandidateBatchView[] {
  const profileValues = (profile ?? {}) as Record<string, unknown>;
  const protectedState = parseProtectedFieldKeys(profile?.user_overrides_json);

  return details.map(({ batch, fields }) => ({
    ...batch,
    profile_revision: profile?.profile_revision ?? 0,
    fields: fields.map((field) => {
      const currentValueJson = profileFieldValueJson(field.field_key, profileValues[field.field_key]);
      return {
        ...field,
        current_value_json: currentValueJson,
        is_user_protected:
          protectedState.corrupt || protectedState.keys.has(field.field_key),
        protection_reason: protectedState.corrupt
          ? "人工保护记录异常，已禁止确认。请先在刑事画像中修复并保存。"
          : protectedState.keys.has(field.field_key)
            ? "该字段已有人工修改保护，候选不能覆盖；如需调整，请在刑事画像中手工编辑。"
            : null,
        has_conflict:
          currentValueJson != null &&
          currentValueJson !== "null" &&
          currentValueJson !== '""' &&
          !valuesAreEqual(currentValueJson, field.value_json),
      };
    }),
  }));
}

function profileFieldValueJson(fieldKey: string, value: unknown): string | null {
  if (value === null || value === undefined || value === "") return null;
  if (fieldKey.endsWith("_json") && typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
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
    sort_order: null,
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
    sort_order: item.sort_order,
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
    applicability_status: "confirmed",
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
    applicability_status: item.applicability_status,
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

function labelApplicability(
  status: CriminalDeadlineItem["applicability_status"] | null | undefined,
) {
  if (status === "needs_confirmation") return "待确认";
  if (status === "not_applicable") return "不适用";
  return "已确认";
}
