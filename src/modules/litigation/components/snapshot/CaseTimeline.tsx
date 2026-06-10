import { X } from "lucide-react";

import { type Case, type Document } from "@/lib/types";
import { computeCaseSnapshot } from "@/lib/caseSnapshot";
import { rowKeyOf } from "@/lib/userOverrides";

import { DeletedRowChips } from "./atoms";
import { EditableField } from "./EditableField";

/**
 * 办案时间轴 — 在 CardSection 内渲染
 *
 * 2026-05-23 晚十五 作者要求:时间轴只显示"办案过程节点",
 * 严格限于 KEY_DATE_WHITELIST(在 caseSnapshot 里已过滤)。
 * 不再混入"案件入库 / CaseBoard 首次扫描"等系统事件,也不再 fabricate
 * 没日期的"提交起诉状/纳入失信"等推断节点 — 抽不到日期就是数据问题,UI 不补。
 *
 * 多次开庭支持:snap.key_dates 里 event_type='开庭' 可有多条(每条不同日期)。
 *
 * 2026-05-26 V0.1.13:编辑模式可改 note + 删节点。event_type 和 date 是 row key
 * 的一部分,锁住不可改(改了同行 note override 变孤儿)。
 */
export function CaseTimeline({
  caseData: _caseData,
  snap,
  rawSnap,
  documents: _documents,
  isEditMode = false,
  caseId,
  onEditCell,
  hasCellOverride,
  onResetCell,
  onDeleteRow,
  rowDeleted,
  deletedRows,
  onUndeleteRow,
}: {
  caseData: Case;
  snap: ReturnType<typeof computeCaseSnapshot>;
  /** raw 数据(未叠 overrides)— rowKey 必须用这个算,否则用户改 row-key 字段
   *  (event_type/date)后,同节点 note override 会孤儿 */
  rawSnap: ReturnType<typeof computeCaseSnapshot>;
  documents: Document[];
  isEditMode?: boolean;
  caseId?: string;
  /** cell 编辑提交(rowKey + inner field name + value);null = 用户清空 */
  onEditCell?: (rowKey: string, inner: string, value: string | null) => void;
  /** 查 cell 是否被用户改过(决定 ↺ 是否显示) */
  hasCellOverride?: (rowKey: string, inner: string) => boolean;
  /** 恢复 cell 到 LLM 原值 */
  onResetCell?: (rowKey: string, inner: string) => void;
  /** 删节点(只删显示,不删 DB) */
  onDeleteRow?: (rowKey: string) => void;
  /** 节点是否被标删(用于过滤 — 修 V0.1.13 时间轴 × 按钮失效 bug) */
  rowDeleted?: (rowKey: string) => boolean;
  /** 已删节点清单(底部 chip 还原用) */
  deletedRows?: Array<{ rowKey: string; label: string }>;
  /** 点 chip 还原一行 */
  onUndeleteRow?: (rowKey: string) => void;
}) {
  // event_type 显示顺序(用作 fallback,date 相同/为空时按这个顺序)
  const ORDER = [
    "接案",
    "申请立案",
    "正式立案",
    "保全",
    "开庭",
    "调解",
    "判决",
    "上诉",
    "二审开庭",
    "二审判决",
    "执行立案",
    "开发票",
  ];
  const orderOf = (et: string) => {
    const i = ORDER.indexOf(et);
    return i === -1 ? 99 : i;
  };

  // index 配对 raw 节点用于 rowKey 计算(applyOverrides 不改数组长度 / 顺序)
  const withRaw = snap.key_dates.map((n, i) => ({ n, raw: rawSnap.key_dates[i] }));
  // 排序基于显示用的 snap 数据(用户改了 date 后想看新位置)
  const orderedWithRaw = [...withRaw].sort((a, b) => {
    const ad = a.n.date;
    const bd = b.n.date;
    if (ad && bd) {
      if (ad !== bd) return ad.localeCompare(bd);
      return orderOf(a.n.event_type) - orderOf(b.n.event_type);
    }
    if (ad) return -1;
    if (bd) return 1;
    return orderOf(a.n.event_type) - orderOf(b.n.event_type);
  });

  // filter 掉用户标删的节点(修 V0.1.13 时间轴 × 按钮失效 bug)
  const sorted = orderedWithRaw.filter(({ raw }) => {
    if (!rowDeleted) return true;
    return !rowDeleted(rowKeyOf("agg_key_dates", raw));
  });

  const showEdit = isEditMode && !!onEditCell && !!caseId;
  const showDelete = isEditMode && !!onDeleteRow;
  const showUndeleteChips =
    isEditMode && !!onUndeleteRow && deletedRows && deletedRows.length > 0;

  if (sorted.length === 0) {
    return (
      <>
        <div className="rounded-md border border-dashed border-border bg-muted/20 px-3 py-4 text-center text-xs text-muted-foreground">
          暂未抽到办案节点(委托合同/起诉状/受理通知/传票/判决书等扫到后会自动生成)
        </div>
        {showUndeleteChips && (
          <DeletedRowChips rows={deletedRows!} onUndelete={onUndeleteRow!} />
        )}
      </>
    );
  }

  return (
    <>
      <ol className="relative space-y-3 border-l border-foreground/30 pl-5">
        {sorted.map(({ n, raw }, i) => {
          // rowKey 必须用 raw(advisor 警告):用户改 event_type 或 date 时,
          // 同节点的 note override 仍能找到原 row,不孤儿
          const rowKey = rowKeyOf("agg_key_dates", raw);
          return (
            <li
              key={`${raw.event_type}-${raw.date ?? "x"}-${i}`}
              className="group relative rounded transition-colors hover:bg-foreground/[0.025]"
            >
              <span className="absolute -left-[26px] mt-1 size-2.5 rounded-full bg-foreground ring-2 ring-foreground/20" />
              <div className="flex flex-wrap items-baseline gap-2">
                {/* V0.1.14:event_type / date 解锁可编辑(rowKey 用 raw 算,
                    改了不导致 note override 变孤儿) */}
                {showEdit ? (
                  <>
                    <EditableField
                      key={`${caseId}:agg_key_dates:${rowKey}:event_type`}
                      initialValue={n.event_type}
                      editable
                      placeholder="事件类型"
                      onCommit={(v) => onEditCell!(rowKey, "event_type", v)}
                      ariaLabel="编辑事件类型"
                      className="text-sm font-medium"
                      hasOverride={hasCellOverride?.(rowKey, "event_type") ?? false}
                      onReset={
                        onResetCell
                          ? () => onResetCell(rowKey, "event_type")
                          : undefined
                      }
                    />
                    <EditableField
                      key={`${caseId}:agg_key_dates:${rowKey}:date`}
                      initialValue={n.date}
                      editable
                      placeholder="yyyy-mm-dd"
                      onCommit={(v) => onEditCell!(rowKey, "date", v)}
                      ariaLabel="编辑日期"
                      className="font-mono text-xs"
                      hasOverride={hasCellOverride?.(rowKey, "date") ?? false}
                      onReset={
                        onResetCell ? () => onResetCell(rowKey, "date") : undefined
                      }
                    />
                  </>
                ) : (
                  <>
                    <span className="text-sm font-medium text-foreground">
                      {n.event_type}
                    </span>
                    {n.date && (
                      <span className="font-mono text-xs text-muted-foreground">
                        {n.date}
                      </span>
                    )}
                  </>
                )}
                {showDelete && (
                  <button
                    type="button"
                    onClick={() => onDeleteRow!(rowKey)}
                    className="ml-auto rounded p-0.5 text-muted-foreground/40 opacity-0 transition-all hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100"
                    title="删除这个节点(只删显示,不删数据)"
                    aria-label="删除这个节点"
                  >
                    <X className="size-3.5" />
                  </button>
                )}
              </div>
              {/* note 可编辑 */}
              {showEdit ? (
                <div className="mt-0.5 text-xs text-muted-foreground">
                  <EditableField
                    key={`${caseId}:agg_key_dates:${rowKey}:note`}
                    initialValue={n.note}
                    editable
                    placeholder="加备注"
                    onCommit={(v) => onEditCell!(rowKey, "note", v)}
                    ariaLabel="编辑备注"
                    hasOverride={hasCellOverride?.(rowKey, "note") ?? false}
                    onReset={
                      onResetCell ? () => onResetCell(rowKey, "note") : undefined
                    }
                  />
                </div>
              ) : (
                n.note && <p className="mt-0.5 text-xs text-muted-foreground">{n.note}</p>
              )}
            </li>
          );
        })}
      </ol>
      {showUndeleteChips && (
        <DeletedRowChips rows={deletedRows!} onUndelete={onUndeleteRow!} />
      )}
    </>
  );
}
