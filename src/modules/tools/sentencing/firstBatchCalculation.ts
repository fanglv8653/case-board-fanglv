import { factorAppliesTo, SENTENCING_DATA, LEGAL_REFERENCES, PRINCIPAL_PENALTY_LABELS } from "./data.ts";
import { assessOffense, FIRST_BATCH_STANDARDS, FIRST_BATCH_NOTES, GUIDANCE_2024_CONFIRMATION, type FirstBatchId } from "./firstBatch.ts";
import type { MonthRange, SentencingCalculationInput, SentencingCalculationResult, SentencingFactorRule } from "./types.ts";

const PLEA_GROUP = new Set(["plea_guilty", "surrender", "confess", "confess_heavy", "confess_avoid", "court_guilty", "restitution", "deposit_return", "compensation_forgiven", "compensation_unforgiven", "forgiven_no_compensation", "reconciliation", "good_detention"]);
const UNIT_CRIMES = new Set<FirstBatchId>(["concealment", "illegal_deposits", "fundraising_fraud", "forced_transaction"]);
const validSource = (v?: string) => typeof v === "string" && v.trim().length >= 4;
const isDate = (s: string) => /^\d{4}-\d{2}-\d{2}$/.test(s) && !Number.isNaN(Date.parse(s)) && new Date(s).toISOString().slice(0, 10) === s;
const roundFloat = (v: number) => Math.round(v * 1e8) / 1e8;
const scaled = (range: MonthRange, pct: number): MonthRange => [roundFloat(range[0] * (100 + pct) / 100), range[1] == null ? null : roundFloat(range[1] * (100 + pct) / 100)];
const finite = (range?: MonthRange | null): range is [number, number] => !!range && Number.isFinite(range[0]) && range[1] != null && Number.isFinite(range[1]) && range[0] > 0 && range[1] >= range[0];
const effectMitigates = (f: SentencingFactorRule) => ["lighter_or_mitigated", "lighter_or_mitigated_or_exempt", "mitigated_or_exempt"].includes(f.legalEffect);

/** 审阅稿首批路径：条件候选、人工基准、明确幅度、刑种和从轻/减轻分开。 */
export function calculateFirstBatch(id: FirstBatchId, input: SentencingCalculationInput): SentencingCalculationResult {
  const r: SentencingCalculationResult = {
    ...input, factors: { ...input.factors }, firstBatchReview: input.firstBatchReview ? structuredClone(input.firstBatchReview) : undefined,
    judgeAdjustment: input.judgeAdjustment ?? 0, factTier: null, manualBasePenaltyRange: input.manualBasePenaltyRange ? [...input.manualBasePenaltyRange] : null,
    temporalRuleBasis: input.temporalRuleBasis ?? null, calculationStatus: "blocked", process: [], warnings: [...FIRST_BATCH_NOTES[id]], blockingIssues: [], ruleset: SENTENCING_DATA.ruleset,
    legalReferences: LEGAL_REFERENCES[input.crimeName as keyof typeof LEGAL_REFERENCES],
  };
  const stop = (message: string, status: SentencingCalculationResult["calculationStatus"] = "requires_manual_review") => {
    r.calculationStatus = status; r.blockingIssues.push(message); r.disposition = "manual_review";
    r.process.push({ step: "人工核验", detail: message }); return r;
  };
  const review = input.firstBatchReview ?? {};
  const facts = review.facts ?? {};
  if (!Number.isFinite(input.amount) || input.amount < 0) return stop("金额须为有效非负数；未知金额不可填0", "blocked");
  if (!isDate(input.crimeDate)) return stop("须提供有效 YYYY-MM-DD 行为日期，跨期另核", "blocked");
  if (input.areaType !== "全国") return stop("首批规则的具体地域在专属事实中选择，不能沿用其他罪名地区值", "blocked");
  r.offenseAssessment = assessOffense(id, input.amount, facts);
  r.process.push({ step: "条件核验", detail: r.offenseAssessment.reasons.join("；") });
  if (r.offenseAssessment.status !== "candidate") return stop(r.offenseAssessment.reasons.join("；"));
  const standard = FIRST_BATCH_STANDARDS[id].find(s => s.tier === r.offenseAssessment?.tier);
  if (!standard) return stop("条件档位未配置，停止测算", "blocked");
  r.tier = standard.tier; r.tierLabel = standard.tier; r.factTier = standard.tier;
  r.standardDetail = { ...standard, sourceIds: [...new Set([...standard.sourceIds, ...r.offenseAssessment.sourceIds, "batch-L18", "batch-L27"])] }; r.statutoryPenalty = standard.statutory;
  r.process.push({ step: "法定刑", detail: standard.statutory.label });
  if (input.factTier && input.factTier !== standard.tier) return stop("人工档位与已核条件冲突；请修正事实及依据，不允许覆盖降档", "blocked");
  const cutoff = id === "concealment" ? "2025-08-26" : id === "casino" ? "2024-07-01" : ["illegal_deposits", "fundraising_fraud"].includes(id) ? "2022-03-01" : "2021-07-01";
  if (input.crimeDate < cutoff && (!review.currentRulesConfirmed || !validSource(review.temporalSource))) return stop("历史/跨期行为须完成新旧规则有利性比较，并填写采用当前规则的依据");
  if (standard.startMin != null) {
    r.startingPointRange = [standard.startMin, standard.startMax ?? null];
    r.startingPointKinds = standard.startKinds;
  }
  if (id === "casino") {
    r.process.push({ step: "2024指导（二）人工适用确认", detail: `${GUIDANCE_2024_CONFIRMATION.confirmedOn}经办律师确认继续引用；${GUIDANCE_2024_CONFIRMATION.boundary}${validSource(review.guidelineAdoptionSource) ? ` 本案补充依据：${review.guidelineAdoptionSource}` : ""}` });
  }
  r.additionalPenaltyReview = additionalPenalty(id, standard.tier, input.amount, facts, review.age);
  if (review.elementsConfirmed !== true || !validSource(review.evidenceSource)) return stop("须确认全部构成、责任能力、证据、排除事由与个人归责，并填写材料定位；追诉条件不等于定罪");
  if (!Number.isInteger(review.age) || review.age! < 16) return stop("首批普通构成须行为时年满16周岁；未满16转特定犯罪责任年龄审查", "blocked");
  if (!["individual", "unit", "unit_responsible"].includes(review.subject ?? "")) return stop("须确认自然人、单位或单位直接责任人员");
  if (review.subject !== "individual" && !UNIT_CRIMES.has(id)) return stop("本罪没有本稿已确认的单位犯罪路径，不能将单位责任套入自然人刑期", "blocked");
  if (review.subject === "unit") return stop("单位仅列罚金及责任审查；直接负责人员须另建测算，不生成单位有期徒刑");
  const allRules = [...SENTENCING_DATA.priorityFactors, ...SENTENCING_DATA.generalFactors];
  const selected: SentencingFactorRule[] = [];
  for (const [name, enabled] of Object.entries(input.factors)) {
    if (!enabled) continue;
    const f = allRules.find(f => f.name === name);
    if (!f || !factorAppliesTo(f.id, id, standard.tier)) return stop(`情节“${name}”未知或不适用于本罪/档位`, "blocked");
    selected.push(f);
  }
  const ids = new Set(selected.map(f => f.id));
  if (review.age! < 18 && !ids.has("minor_16_18")) return stop("16至不满18周岁应当从轻或减轻，须记录相应情节与选择");
  if (review.age! >= 18 && selected.some(f => f.id.startsWith("minor_"))) return stop("行为年龄与未成年人情节冲突", "blocked");
  if (review.age! < 75 && ids.has("elderly_intent")) return stop("未满75周岁不能套高龄情节", "blocked");
  if (ids.has("recidivist") && (review.age! < 18 || ids.has("prior_crime"))) return stop("累犯资格/同一前科重复评价冲突", "blocked");
  if (ids.has("vulnerable_target") && (facts.special37 === true || facts.vulnerableBad === true || facts.minorUse === true || facts.recruitMinor === true)) return stop("弱势对象已用于门槛、升档或构成，须去除重复固定加重", "blocked");
  if (ids.has("reconciliation")) {
    if (!["extortion", "illegal_detention"].includes(id) || facts.reconciliationEligible !== true) return stop("刑诉288和解仅本批非拘/敲诈可能适用，须核民间纠纷、可能三年以下及五年内无故意前罪等全部条件", "blocked");
  }
  if (ids.has("deposit_return") && facts.preProsecutionReturn !== true) return stop("非吸专属退赔须公诉前积极退赃退赔、减少损害全部资格", "blocked");
  if (ids.has("deposit_return") && ids.has("restitution") && !ids.has("plea_guilty")) return stop("非吸专属与通用退赔同一事实不能重复相减", "blocked");
  const hasPlea = ids.has("plea_guilty");
  for (const a of selected) for (const b of selected) {
    if (a.id >= b.id) continue;
    const absorbed = hasPlea && PLEA_GROUP.has(a.id) && PLEA_GROUP.has(b.id);
    if (!absorbed && ((a.conflictGroup && a.conflictGroup === b.conflictGroup) || a.conflictsWith?.includes(b.id) || b.conflictsWith?.includes(a.id))) return stop(`互斥或重复情节：${a.name}、${b.name}`, "blocked");
  }
  if (selected.length && !validSource(review.adjustmentSource)) return stop("量刑情节资格、独立性、从轻/减轻选择及幅度须填写核验理由");
  const decision = review.mitigationDecision;
  if (!decision) return stop("须明确从轻/本档、依法减轻或免罚分支；勾选自首/从犯不自动下档");
  if (decision === "exempt" || ids.has("suspension_no_harm")) {
    if (ids.has("suspension_no_harm") || selected.some(f => f.legalEffect.includes("exempt")) || facts.lenientConditions === true || facts.normalOperationCleared === true) {
      return stop("依法免罚候选须独立审查并记录，不表示为0个月；本轮不将非刑期结果写入旧月数存储");
    }
    return stop("未确认免罚法定资格，不能凭选择直接免罚", "blocked");
  }
  const mustMitigate = ids.has("coerced") || ids.has("excessive_defense") || ids.has("excessive_escape") || ids.has("suspension_harm");
  if (mustMitigate && decision !== "mitigated") return stop("应当减轻/免除的情节不能只按本档从轻", "blocked");
  if (decision === "mitigated" && !selected.some(effectMitigates)) return stop("没有已核法定减轻资格，不能越法定最低；特殊减轻须最高法核准", "blocked");
  const kind = review.penaltyKind;
  const options = [...standard.statutory.options, ...(decision === "mitigated" ? standard.statutory.mitigatedOptions ?? [] : [])];
  if (!kind || !options.some(o => o.kind === kind)) return stop("须选择本分支法律允许的刑种，非所有罪均可单罚或无期");
  const fineError = validateFine(id, standard.tier, input.amount, facts, review, r.additionalPenaltyReview!);
  if (fineError) return stop(fineError, "blocked");
  if (["fine_only", "political_rights", "life", "exemption", "death"].includes(kind)) return stop(`已选择${PRINCIPAL_PENALTY_LABELS[kind]}，停止月份公式；依刑种独立审查，不生成0个月或15年替代值`);
  const base = input.manualBasePenaltyRange;
  if (!finite(base) || !validSource(input.manualBaseSource)) return stop("请提供人工核验的有限基准刑上下限及来源；缺口不由旧广东或外省增月公式补齐", "requires_manual_base");
  if (base[0] < standard.statutory.monthBounds[0] || base[1] > standard.statutory.monthBounds[1]!) return stop("人工基准刑超出原法定刑档；减轻在调节后处理，不能先降低基准刑", "blocked");
  r.basePenaltyRange = [...base];
  r.process.push({ step: "人工核验基准刑", detail: input.manualBaseSource!, valueRange: [...base] });
  const active = selected.filter(f => !(hasPlea && PLEA_GROUP.has(f.id)));
  const qualitative = active.filter(f => !f.quantitative || f.id === "mental_disorder");
  if (qualitative.length) return stop(`情节“${qualitative.map(f => f.name).join("、")}”没有已确认统一百分比，保留人工宣告刑判断，不虚构默认减幅`);
  const priority = active.filter(f => SENTENCING_DATA.priorityFactors.some(p => p.id === f.id));
  if (priority.length > 1 && (!validSource(review.specialOrderSource) || !["sequential", "combined"].includes(review.specialAdjustmentMethod ?? ""))) return stop("多个特殊情节的当地顺序及方法待核；未选择已核方法及来源不自动逐项连乘");
  let orderedPriority = priority;
  if (priority.length > 1 && review.specialAdjustmentMethod === "sequential") {
    const order = review.specialOrder ?? [];
    if (order.length !== priority.length || new Set(order).size !== priority.length || !order.every(id => priority.some(f => f.id === id))) return stop("特殊情节逐项调节须明确不重复的完整顺序，不能按程序列表默认决定");
    orderedPriority = order.map(id => priority.find(f => f.id === id)!);
  }
  const percentages = review.factorPercents ?? {};
  for (const f of active) {
    const pct = percentages[f.id];
    if (!Number.isFinite(pct) || pct < (f.minPct ?? 0) || pct > (f.maxPct ?? 0)) return stop(`“${f.name}”需人工选择规范幅度内百分比（${f.minPct}至${f.maxPct}%）`);
  }
  let current: MonthRange = [...base];
  r.priorityAdjustments = [];
  if (priority.length > 1 && review.specialAdjustmentMethod === "combined") {
    const sum = priority.reduce((sum, f) => sum + percentages[f.id], 0);
    if (sum <= -100) return stop("特殊情节合并达到100%，转免罚/法律适用独立审查，不生成0月", "blocked");
    current = scaled(current, sum);
    r.priorityAdjustments = priority.map(f => ({ factor: f.name, percentRange: [percentages[f.id], percentages[f.id]], newRange: [...current] }));
    r.process.push({ step: "特殊情节人工核定方法", detail: `按已核当地同基数合并：${priority.map(f => f.name).join("、")}；${review.specialOrderSource}` });
  } else for (const f of orderedPriority) {
    current = scaled(current, percentages[f.id]);
    r.priorityAdjustments.push({ factor: f.name, percentRange: [percentages[f.id], percentages[f.id]], newRange: [...current] });
    r.process.push({ step: "特殊情节人工核定顺序", detail: `${f.name}，${percentages[f.id]}%；${review.specialOrderSource ?? "单一特殊情节"}`, valueRange: [...current] });
  }
  let pct = 0;
  if (hasPlea) {
    const total = review.integratedReductionPct;
    const ordinaryLimit = selected.some(f => f.id !== "plea_guilty" && PLEA_GROUP.has(f.id)) ? 60 : 30;
    if (total == null || !Number.isFinite(total) || total < 0 || total >= 100) return stop("须输入认罪认罚综合减幅；免罚走独立分支，不将100%变为0月");
    if (total > ordinaryLimit && (review.lighterCaseException !== true || !validSource(review.adjustmentSource))) return stop(`超过常规${ordinaryLimit}%须较轻例外及具体法定依据，不设无条件60%硬上限`);
    pct -= total;
    r.process.push({ step: "认罪认罚综合评价", detail: `按2026新版去重，一次计入${total}%；已吸收：${selected.filter(f => PLEA_GROUP.has(f.id)).map(f => f.name).join("、")}` });
  }
  const general = active.filter(f => !priority.includes(f));
  pct += general.reduce((sum, f) => sum + percentages[f.id], 0);
  if (pct <= -100) return stop("普通减幅合计达到100%或超过，须检查重复、免罚资格及合法方案，不截为0月", "blocked");
  let raw = scaled(current, pct);
  for (const f of general) if (f.minimumIncreaseMonths != null) {
    const lowExtra = Math.max(0, f.minimumIncreaseMonths - current[0] * percentages[f.id] / 100);
    const highExtra = Math.max(0, f.minimumIncreaseMonths - current[1]! * percentages[f.id] / 100);
    raw = [roundFloat(raw[0] + lowExtra), roundFloat(raw[1]! + highExtra)];
  }
  const judge = input.judgeAdjustment ?? 0;
  if (!Number.isFinite(judge) || Math.abs(judge) > 20 || (judge !== 0 && !validSource(input.judgeAdjustmentReason))) return stop("裁判情景调整限正负20%，非零须具体依据；不得当作额外优惠", "blocked");
  if (judge !== 0) raw = scaled(raw, judge);
  r.rawAdjustedPenaltyRange = [...raw];
  r.generalAdjustments = general.map(f => ({ factor: f.name, percentRange: [percentages[f.id], percentages[f.id]], newRange: [...raw] }));
  r.process.push({ step: "情节调节中间值", detail: "特殊顺序按人工确认来源；普通同一基数同向相加、逆向相减，保留小数", valueRange: [...raw] });
  const bounds = decision === "mitigated" ? standard.statutory.mitigatedMonthBounds : standard.statutory.monthBounds;
  if (decision === "mitigated" && !bounds) return stop("本档以下减轻涉及其他刑种或免罚，须人工处理，不夹回原档最低");
  const low = bounds![0], high = bounds![1]!;
  if (decision === "mitigated" && raw[1]! >= standard.statutory.monthBounds[0]) return stop("已选择减轻，但结果仍有原档刑期；须拆成明确减轻情景并核下一幅度");
  let adjusted: MonthRange = [Math.max(low, Math.min(raw[0], high)), Math.max(low, Math.min(raw[1]!, high))];
  if (adjusted[0] !== raw[0] || adjusted[1] !== raw[1]) r.process.push({ step: "法定幅度审查", detail: decision === "mitigated" ? "依法减轻按下一幅度审查" : "本档从轻/从重控制在原法定幅度", valueRange: [...adjusted] });
  if (!Number.isInteger(adjusted[0]) || !Number.isInteger(adjusted[1])) {
    if (!finite(review.roundedRange) || !validSource(review.roundingSource)) return stop("中间值含不足月数；当地取整依据未核，不一律向下/向上取整");
    if (review.roundedRange.some((v, i) => !Number.isInteger(v) || Math.abs(v - adjusted[i]!) >= 1)) return stop("人工整月结果须与各端点不足一月对应，不得随意改写测算值", "blocked");
    adjusted = [...review.roundedRange];
    if (adjusted[0] < low || adjusted[1]! > high || (decision === "mitigated" && adjusted[1]! >= standard.statutory.monthBounds[0])) return stop("整月结果越过已选法定幅度", "blocked");
  }
  const kindCompatible = options.some(o => o.kind === kind && o.minimumMonths != null && adjusted[0] >= o.minimumMonths && adjusted[1]! <= o.maximumMonths!);
  if (!kindCompatible) return stop("结果与所选刑种的期限不相容；有期一般六个月起，拘役1至6月、管制3至24月，不拼接不相容端点");
  r.finalPenaltyRange = adjusted; r.finalPenaltyKinds = [kind];
  r.finalSentence = `人工情景测算：${PRINCIPAL_PENALTY_LABELS[kind]}${adjusted[0]}至${adjusted[1]}个月；非法院判决`;
  r.probationReview = probation(review.age!, adjusted, kind, ids.has("recidivist"), review);
  r.calculationStatus = "complete"; r.disposition = "term_range";
  r.process.push({ step: "测算结果", detail: r.finalSentence, valueRange: [...adjusted] });
  return r;
}

function validateFine(id: FirstBatchId, tier: string, amount: number, facts: Record<string, unknown>, review: NonNullable<SentencingCalculationInput["firstBatchReview"]>, notes: string[]): string | null {
  const noFine = ["affray", "illegal_detention"].includes(id) || (id === "disturbance" && tier === "一次");
  if (review.propertyPenaltyKind === "confiscation") {
    if (id !== "fundraising_fraud" || tier !== "巨大" || !validSource(review.fineSource)) return "本分支无可直接采用的没收财产路径，或欠缺具体依据";
    if (review.fineRange) return "罚金与没收财产的选择及适用依据须分别确认，不能把两个备选财产刑同时套入";
    notes.push(`人工选择没收财产：${review.fineSource}；违法所得追缴另列。`); return null;
  }
  if (review.propertyPenaltyKind === "none" && !noFine && !(id === "disturbance" || id === "obstructing_duty" && review.penaltyKind !== "fine_only")) return "本分支须罚金或法定其他财产刑，不能直接选择不判财产刑";
  if (!review.fineRange) return null; // 月数情景与尚待核的财产刑分别展示，不捏造金额。
  if (noFine || review.propertyPenaltyKind === "none" || (id === "obstructing_duty" && review.penaltyKind !== "fine_only")) return "所选刑种/档位无一般并处罚金规定，不能额外并罚";
  if (!finite(review.fineRange) || !validSource(review.fineSource)) return "人工罚金须提供有效正数上下限及具体依据，不能填0替代未核";
  let min = review.age! < 18 ? 500 : 1000; let max = Infinity;
  if (id === "extortion") {
    if (typeof facts.obtainedAmount !== "number") return "敲诈罚金先核是否实际取得财物及其数额";
    min = 2000; max = facts.obtainedAmount === 0 ? 100000 : amount * 2;
  }
  if (id === "illegal_deposits") [min, max] = tier === "普通" ? [50000, 1000000] : tier === "巨大" ? [100000, 5000000] : [500000, Infinity];
  if (id === "fundraising_fraud") [min, max] = tier === "较大" ? [100000, 5000000] : [500000, Infinity];
  if ((id === "illegal_deposits" || id === "fundraising_fraud") && review.mitigationDecision === "mitigated") {
    if (review.fineSpecialDecision !== true) return "非法集资依法减轻后的特别罚金适用须单独人工核实，不从最终月数倒推";
    notes.push("特别罚金减轻分支已经人工单独确认，具体理由见来源；未以通用下限替代特别规定。");
  } else if (review.fineRange[0] < min || review.fineRange[1] > max) return `人工罚金超出已选分支的${min}元至${max === Infinity ? "未规定统一上限" : max + "元"}范围，特别减轻/临界情形须另核`;
  notes.push(`人工罚金${review.fineRange[0]}至${review.fineRange[1]}元；依据：${review.fineSource}`); return null;
}

function additionalPenalty(id: FirstBatchId, tier: string, amount: number, facts: Record<string, unknown>, age?: number | null): string[] {
  const generic = `罚金无优先特别规定时普通最低1000元，未成年人最低500元；不覆盖特别罚金或法定减轻。当前年龄${age ?? "待核"}。`;
  if (id === "affray" || id === "illegal_detention" || (id === "disturbance" && tier === "一次")) return ["本罪本档无一般罚金规定；不得自动并罚。非法拘禁普通档另有剥政刑种。"];
  if (id === "obstructing_duty") return ["罚金是可选择的单处刑种，选择有期/拘役/管制不自动并罚。", generic];
  if (id === "extortion") {
    const obtained = typeof facts.obtainedAmount === "number" ? facts.obtainedAmount : null;
    return [obtained == null ? "罚金需先核实际取得与未取得分支。" : obtained === 0 ? "未获得财物：罚金2000元至10万元；最终金额人工核定。" : `罚金2000元至依法认定敲诈数额2倍（${amount * 2}元）；实际取得、索要、未遂口径另核。`];
  }
  if (id === "illegal_deposits") return [tier === "普通" ? "罚金5万至100万元，并处或单处。" : tier === "巨大" ? "并处罚金10万至500万元。" : "并处罚金50万元以上。", "3/10年临界、依法减轻及单位/责任人员分别人工核罚金；不由最终月份倒推。"];
  if (id === "fundraising_fraud") return [tier === "较大" ? "并处罚金10万至500万元。" : "并处罚金50万元以上或者没收财产；可判无期，须另审。", "7年临界及依法减轻时罚金适用另核。"];
  return [id === "disturbance" ? "高级分支可以并处罚金，不是必须。" : id === "casino" ? "必须并处罚金，金额与缴纳能力须人工核验。" : tier === "普通" || tier === "严重" && id === "forced_transaction" ? "普通档并处或者单处罚金；刑种选择须合法。" : "并处罚金。", generic, "罚金、追缴、退赔、没收财产分别记录；数罪罚金相加，不套有期并罚折减。"];
}

function probation(age: number, range: MonthRange, kind: string, recidivist: boolean, review: NonNullable<SentencingCalculationInput["firstBatchReview"]>): string {
  if (recidivist || review.probationExcluded === true) return "累犯/犯罪集团首要分子等已核排除，不适用缓刑。";
  if (!["fixed_term", "detention"].includes(kind) || range[1]! > 36) return "本情景不符合拘役或三年以下有期徒刑的缓刑刑种/期限前提；争议区间须拆分。";
  if (review.probationConditionsConfirmed !== true || review.probationExcluded !== false) return "缓刑待人工核全部72条条件及74条排除；谅解或三年整不等于必缓。";
  return age < 18 || age >= 75 || review.pregnancyConfirmed === true ? "全部条件及法定身份已确认，应当宣告缓刑；认定时点与具体考验期仍核。" : "可进入缓刑裁量；有期考验期原判以上五年以下且不少于一年，拘役原判以上一年以下且不少于二个月。";
}
