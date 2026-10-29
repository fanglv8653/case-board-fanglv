import type { FirstBatchReview, OffenseFacts, PrincipalPenaltyKind, SentencingFactorRule } from "./types.ts";
import { FIRST_BATCH_NOTES, FIRST_BATCH_STANDARDS, visibleFields, type FirstBatchId } from "./firstBatch.ts";
import { PRINCIPAL_PENALTY_LABELS, SENTENCING_DATA } from "./data.ts";

const inputClass = "h-10 w-full rounded-md border border-border bg-background px-3";
const sectionClass = "space-y-3 rounded-xl border border-border bg-card p-5";
type Props = { id: FirstBatchId; value: FirstBatchReview; onChange: (value: FirstBatchReview) => void };
const tri = (v: unknown) => typeof v === "boolean" ? String(v) : "";
const parseTri = (v: string) => v === "" ? undefined : v === "true";
const number = (v: string) => v.trim() === "" ? null : Number(v);

export function FirstBatchFactsForm({ id, value, onChange }: Props) {
  const facts = value.facts ?? {};
  const change = (key: string, next: OffenseFacts[string]) => {
    let nextFacts: OffenseFacts = { ...facts, [key]: next };
    // 切换类型后移除被隐藏分支字段，后续又切回时仍须重新核实。
    const visible = new Set(visibleFields(id, nextFacts).map(f => f.key));
    nextFacts = Object.fromEntries(Object.entries(nextFacts).filter(([k]) => visible.has(k) || k === "reconciliationEligible"));
    onChange({ ...value, facts: nextFacts, elementsConfirmed: false });
  };
  return <section className={sectionClass} data-testid="first-batch-facts">
    <h3 className="text-sm font-semibold">定罪条件与个人责任核验</h3>
    <p className="text-xs text-muted-foreground">空白为待核，不会按“否”或0处理。下列条件只生成候选；先核证据、排除与转罪，再确认本人的构成责任。</p>
    <ul className="list-disc space-y-1 pl-5 text-xs text-muted-foreground">{FIRST_BATCH_NOTES[id].map(note => <li key={note}>{note}</li>)}</ul>
    <div className="grid gap-4 md:grid-cols-2">
      {visibleFields(id, facts).map(field => <label className="space-y-1 text-sm" key={field.key}>{field.label}
        {field.type === "boolean" ? <select className={inputClass} data-testid={`batch-${field.key}`} value={tri(facts[field.key])} onChange={e => change(field.key, e.target.value === "" ? null : e.target.value === "true")}><option value="">待核实</option><option value="true">是（已核实）</option><option value="false">否（已核实）</option></select>
          : field.type === "select" ? <select className={inputClass} data-testid={`batch-${field.key}`} value={String(facts[field.key] ?? "")} onChange={e => change(field.key, e.target.value)}><option value="">请选择</option>{field.options?.map(([key, label]) => <option key={key} value={key}>{label}</option>)}</select>
          : <input className={inputClass} data-testid={`batch-${field.key}`} type={field.type === "number" ? "number" : "text"} min={field.type === "number" ? 0 : undefined} step={field.integer ? 1 : "any"} value={String(facts[field.key] ?? "")} onChange={e => change(field.key, field.type === "number" ? number(e.target.value) : e.target.value)} />}
        {field.note && <span className="block text-xs text-muted-foreground">{field.note}</span>}
      </label>)}
      <label className="space-y-1 text-sm">行为时年龄（周岁，跨生日分情景）
        <input data-testid="batch-age" className={inputClass} type="number" min="0" step="1" value={value.age ?? ""} onChange={e => onChange({ ...value, age: number(e.target.value), elementsConfirmed: false })} />
      </label>
      <label className="space-y-1 text-sm">责任主体
        <select data-testid="batch-subject" className={inputClass} value={value.subject ?? ""} onChange={e => onChange({ ...value, subject: e.target.value as FirstBatchReview["subject"], elementsConfirmed: false })}>
          <option value="">请选择</option><option value="individual">自然人</option><option value="unit">单位（罚金独立审查）</option><option value="unit_responsible">单位直接责任人员（分别确认责任）</option>
        </select>
      </label>
      <label className="space-y-1 text-sm md:col-span-2">构成、排除、个人责任及证据材料定位
        <input data-testid="batch-evidenceSource" className={inputClass} value={value.evidenceSource ?? ""} placeholder="材料名/页码/事实认定、反证与归责依据" onChange={e => onChange({ ...value, evidenceSource: e.target.value, elementsConfirmed: false })} />
      </label>
      <label className="flex items-start gap-2 text-sm md:col-span-2"><input data-testid="batch-elementsConfirmed" type="checkbox" checked={value.elementsConfirmed === true} onChange={e => onChange({ ...value, elementsConfirmed: e.target.checked })} />已结合主客观要件、责任能力、证据标准、例外、明知和个人责任核实所选情景，不能仅凭金额或追诉门槛确认构罪</label>
    </div>
    <details className="text-sm"><summary className="cursor-pointer">历史与跨期规则核验</summary><div className="mt-3 space-y-2">
      <label className="flex gap-2"><input type="checkbox" checked={!!value.currentRulesConfirmed} onChange={e => onChange({ ...value, currentRulesConfirmed: e.target.checked })} />已作行为时版本与现行规则的整体有利性比较，确认本情景采用当前规则</label>
      <label className="block">版本、日期与比较依据<input className={inputClass} value={value.temporalSource ?? ""} onChange={e => onChange({ ...value, temporalSource: e.target.value })} /></label>
    </div></details>
    {id === "casino" && <label className="block space-y-1 text-sm">2024指导（二）已由经办律师2026-10-29确认继续引用；本案补充依据（可留空）
      <input data-testid="batch-guidelineAdoptionSource" className={inputClass} value={value.guidelineAdoptionSource ?? ""} placeholder="本案采用条款、个别适用异议及核验；广东增量仍须明确人工依据" onChange={e => onChange({ ...value, guidelineAdoptionSource: e.target.value })} />
    </label>}
  </section>;
}

export function FirstBatchAdjustmentForm({ id, value, onChange, factors }: Props & { factors: SentencingFactorRule[] }) {
  const hasPlea = factors.some(f => f.id === "plea_guilty");
  const groupIds = new Set(["plea_guilty", "surrender", "confess", "confess_heavy", "confess_avoid", "court_guilty", "restitution", "deposit_return", "compensation_forgiven", "compensation_unforgiven", "forgiven_no_compensation", "reconciliation", "good_detention"]);
  const active = factors.filter(f => !(hasPlea && groupIds.has(f.id)));
  const specialFactors = active.filter(f => SENTENCING_DATA.priorityFactors.some(p => p.id === f.id));
  const kinds = [...new Set(FIRST_BATCH_STANDARDS[id].flatMap(s => [...s.statutory.options, ...(value.mitigationDecision === "mitigated" ? s.statutory.mitigatedOptions ?? [] : [])]).map(o => o.kind))];
  return <section className={sectionClass} data-testid="first-batch-adjustment">
    <h3 className="text-sm font-semibold">情景分支与人工幅度</h3>
    <p className="text-xs text-muted-foreground">情节勾选记录事实。每项须核资格和去重，再选择本情景幅度；自首或从犯不自动取得下档结果。无可靠增月公式仍录入人工基准刑与来源。</p>
    <div className="grid gap-4 md:grid-cols-2">
      <label className="space-y-1 text-sm">从轻、减轻或免罚
        <select className={inputClass} data-testid="batch-mitigationDecision" value={value.mitigationDecision ?? ""} onChange={e => onChange({ ...value, mitigationDecision: e.target.value as FirstBatchReview["mitigationDecision"] })}><option value="">请选择本情景</option><option value="lighter">本档处理（从轻/从重/无情节）</option><option value="mitigated">依法减轻（另核法定资格及下一档）</option><option value="exempt">免罚（独立人工审查）</option></select>
      </label>
      <label className="space-y-1 text-sm">拟采用的宣告刑种
        <select className={inputClass} data-testid="batch-penaltyKind" value={value.penaltyKind ?? ""} onChange={e => onChange({ ...value, penaltyKind: e.target.value as PrincipalPenaltyKind })}><option value="">请选择</option>{kinds.map(k => <option key={k} value={k}>{PRINCIPAL_PENALTY_LABELS[k]}</option>)}</select>
      </label>
      {hasPlea && <label className="space-y-1 text-sm">认罪认罚综合减幅（%，重合供述/退赔/赔谅只计一次）
        <input data-testid="batch-integratedReductionPct" className={inputClass} type="number" min="0" max="99.99" step="any" value={value.integratedReductionPct ?? ""} onChange={e => onChange({ ...value, integratedReductionPct: number(e.target.value) })} />
      </label>}
      {active.map(f => <label key={f.id} className="space-y-1 text-sm">{f.name}{f.quantitative && f.id !== "mental_disorder" ? `（${f.minPct}～${f.maxPct}%，减少填负值）` : "（无统一比例，保留人工判断）"}
        {f.quantitative && f.id !== "mental_disorder" && <input data-testid={`batch-pct-${f.id}`} className={inputClass} type="number" step="any" min={f.minPct} max={f.maxPct} value={value.factorPercents?.[f.id] ?? ""} onChange={e => { const next = { ...value.factorPercents }; if (e.target.value.trim() === "") delete next[f.id]; else next[f.id] = Number(e.target.value); onChange({ ...value, factorPercents: next }); }} />}
      </label>)}
      <label className="space-y-1 text-sm md:col-span-2">资格、独立性、比例和从轻/减轻选择的具体依据
        <input className={inputClass} data-testid="batch-adjustmentSource" value={value.adjustmentSource ?? ""} onChange={e => onChange({ ...value, adjustmentSource: e.target.value })} placeholder="认罚须按2026新版综合去重；例外超过常规幅度须说明较轻及法定依据" />
      </label>
      {hasPlea && <label className="flex gap-2 text-sm"><input type="checkbox" checked={!!value.lighterCaseException} onChange={e => onChange({ ...value, lighterCaseException: e.target.checked })} />已核较轻例外及超常规从宽依据</label>}
      {factors.some(f => f.id === "reconciliation") && <label className="flex gap-2 text-sm"><input data-testid="batch-reconciliationEligible" type="checkbox" checked={value.facts?.reconciliationEligible === true} onChange={e => onChange({ ...value, facts: { ...value.facts, reconciliationEligible: e.target.checked } })} />已核刑诉288条民间纠纷、可能三年以下、五年内无故意前罪及自愿审查等全部资格</label>}
      <label className="space-y-1 text-sm md:col-span-2">多个特殊情节的顺序及当地方法依据（如涉及）
        <input className={inputClass} data-testid="batch-specialOrderSource" value={value.specialOrderSource ?? ""} onChange={e => onChange({ ...value, specialOrderSource: e.target.value })} />
      </label>
      {specialFactors.length > 1 && <>
        <label className="space-y-1 text-sm">已核当地特殊情节处理方法<select data-testid="batch-specialAdjustmentMethod" className={inputClass} value={value.specialAdjustmentMethod ?? ""} onChange={e => onChange({ ...value, specialAdjustmentMethod: e.target.value as FirstBatchReview["specialAdjustmentMethod"], specialOrder: [], roundedRange: null })}><option value="">待核，不预设连乘</option><option value="sequential">按已核顺序逐项调节</option><option value="combined">按已核来源同一基数合并</option></select></label>
        {value.specialAdjustmentMethod === "sequential" && specialFactors.map((_, index) => <label key={index} className="space-y-1 text-sm">特殊情节第{index + 1}项<select data-testid={"batch-specialOrder-" + index} className={inputClass} value={value.specialOrder?.[index] ?? ""} onChange={e => { const order = [...value.specialOrder ?? []]; order[index] = e.target.value; onChange({ ...value, specialOrder: order, roundedRange: null }); }}><option value="">请选择，不得重复</option>{specialFactors.map(f => <option key={f.id} value={f.id}>{f.name}</option>)}</select></label>)}
      </>}
    </div>
    <details className="text-sm"><summary className="cursor-pointer">不足月数、缓刑与附加刑审查</summary>
      <div className="mt-3 grid gap-3 md:grid-cols-2">
        <label>依法核定的整月下限<input data-testid="batch-roundedLow" className={inputClass} type="number" step="1" value={value.roundedRange?.[0] ?? ""} onChange={e => onChange({ ...value, roundedRange: e.target.value === "" ? null : [Number(e.target.value), value.roundedRange?.[1] ?? null] })} /></label>
        <label>依法核定的整月上限<input data-testid="batch-roundedHigh" className={inputClass} type="number" step="1" value={value.roundedRange?.[1] ?? ""} onChange={e => onChange({ ...value, roundedRange: [value.roundedRange?.[0] ?? 0, number(e.target.value)] })} /></label>
        <label className="md:col-span-2">当地取整或不足月处理依据<input data-testid="batch-roundingSource" className={inputClass} value={value.roundingSource ?? ""} onChange={e => onChange({ ...value, roundingSource: e.target.value })} /></label>
        <label>缓刑72条全部条件<select className={inputClass} value={tri(value.probationConditionsConfirmed)} onChange={e => onChange({ ...value, probationConditionsConfirmed: parseTri(e.target.value) })}><option value="">待核</option><option value="true">已确认全部符合</option><option value="false">不符合</option></select></label>
        <label>存在累犯/集团首要分子等74条排除<select className={inputClass} value={tri(value.probationExcluded)} onChange={e => onChange({ ...value, probationExcluded: parseTri(e.target.value) })}><option value="">待核</option><option value="true">是</option><option value="false">否（已核）</option></select></label>
        <label className="flex gap-2"><input type="checkbox" checked={!!value.pregnancyConfirmed} onChange={e => onChange({ ...value, pregnancyConfirmed: e.target.checked })} />孕妇身份及法定认定时点已核</label>
        <label>人工财产刑选择<select data-testid="batch-propertyPenaltyKind" className={inputClass} value={value.propertyPenaltyKind ?? ""} onChange={e => onChange({ ...value, propertyPenaltyKind: e.target.value as FirstBatchReview["propertyPenaltyKind"], fineRange: null })}><option value="">待核实</option><option value="fine">罚金（先核并处或单处）</option><option value="confiscation">没收财产（仅法律允许分支）</option><option value="none">本情景不判财产刑（须法律允许）</option></select></label>
        <label>人工罚金下限（元）<input data-testid="batch-fineLow" className={inputClass} type="number" min="0" step="any" value={value.fineRange?.[0] ?? ""} onChange={e => onChange({ ...value, fineRange: e.target.value === "" ? null : [Number(e.target.value), value.fineRange?.[1] ?? Number(e.target.value)] })} /></label>
        <label>人工罚金上限（元）<input data-testid="batch-fineHigh" className={inputClass} type="number" min="0" step="any" value={value.fineRange?.[1] ?? ""} onChange={e => onChange({ ...value, fineRange: e.target.value === "" ? null : [value.fineRange?.[0] ?? Number(e.target.value), Number(e.target.value)] })} /></label>
        <label className="md:col-span-2">罚金、临界或减轻分支的独立依据<input data-testid="batch-fineSource" className={inputClass} value={value.fineSource ?? ""} onChange={e => onChange({ ...value, fineSource: e.target.value })} /></label>
        <label className="flex gap-2 md:col-span-2"><input type="checkbox" checked={!!value.fineSpecialDecision} onChange={e => onChange({ ...value, fineSpecialDecision: e.target.checked })} />已单独核实非法集资减轻后的特别罚金依据（不由最终月数反推）</label>
      </div>
      <p className="mt-3 text-xs text-muted-foreground">罚金、没收、追缴及退赔在结果中按各罪分别提示，具体金额仍须人工确定；非刑期分支保留审查结果，不用0月表示。羁押折抵与数罪并罚另作审查。</p>
    </details>
  </section>;
}
