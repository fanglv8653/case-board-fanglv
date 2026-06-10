/**
 * 非诉应抽取字段类别 — 数据来自 docs/抽取与聚合方法论-v0.1.md §3.2
 *
 * 按字段分组(项目基本 / 主体 / 行政 / 时间线 / 金额 / 法律关系 / 风险 / 文件版本)展示,
 * 跟诉讼字段对齐处用 ✅ 标记,差异点 ⚠️ 单独说明。
 */

interface FieldDef {
  field: string;
  desc: string;
  /** 是否与诉讼字段对齐(✅ = 直接复用,⚠️ = 非诉特有 / 差异) */
  alignment: "aligned" | "specific";
}

interface FieldGroup {
  group: string;
  fields: FieldDef[];
}

const GROUPS: FieldGroup[] = [
  {
    group: "项目基本",
    fields: [
      { field: "project_type", desc: "非诉「案由」(并购 / 增资 / 破产清算 / 合同尽调 ...)", alignment: "specific" },
      { field: "project_status", desc: "进行中 / 完结 / 中止", alignment: "specific" },
      { field: "project_stage", desc: "业务里程碑(尽调中 / 待签约 / 待交割 / 工商办理中 / 已交割)", alignment: "specific" },
      { field: "engaged_at", desc: "接案日 — 对应诉讼 filed_at", alignment: "aligned" },
      { field: "expected_close_at", desc: "预计完结日 / 交割日 — 复用诉讼字段", alignment: "aligned" },
    ],
  },
  {
    group: "主体",
    fields: [
      { field: "our_clients[]", desc: "我方(委托方)— 对应诉讼 plaintiffs", alignment: "aligned" },
      { field: "counterparties[]", desc: "对方(交易对手 / 被并购方 / 异议方)— 对应 defendants", alignment: "aligned" },
      { field: "related_entities[]", desc: "关联方 / 标的公司 / 第三方 — 对应 third_parties", alignment: "aligned" },
      { field: "party_contacts[].is_our_side", desc: "完全复用诉讼字段,只显示我方联系人", alignment: "aligned" },
    ],
  },
  {
    group: "行政",
    fields: [
      { field: "agencies[]", desc: "工商 / 税务 / 外汇 / 知识产权局 / 海关 / 行业监管 — 非诉特有", alignment: "specific" },
      { field: "regulatory_filings[]", desc: "{ agency, filing_no, filed_at, decided_at, outcome } — 非诉特有", alignment: "specific" },
    ],
  },
  {
    group: "时间线",
    fields: [
      { field: "key_dates[]", desc: "复用诉讼字段,但白名单要扩(签约 / 交割 / 工商核准 / 公告 / 审批批复 / 异议期届满 ...)", alignment: "aligned" },
    ],
  },
  {
    group: "金额",
    fields: [
      { field: "transaction_amount", desc: "交易对价 / 增资金额 / 收购对价 — 对应 claim_amount", alignment: "aligned" },
      { field: "fees[]", desc: "律师费 / 政府收费 / 评估费 / 公证费 / 翻译费 — 复用结构", alignment: "aligned" },
    ],
  },
  {
    group: "法律关系",
    fields: [
      {
        field: "relationships[]",
        desc: "{ type:股东/控股/质押/担保/关联, from, to, since, evidence_doc_id } — 非诉特有,执行 / 破产追责高度依赖",
        alignment: "specific",
      },
    ],
  },
  {
    group: "风险",
    fields: [
      {
        field: "risk_findings[]",
        desc: "{ level:高/中/低, category, description, source_doc_id } — 非诉特有,尽调产出物",
        alignment: "specific",
      },
    ],
  },
  {
    group: "文件版本",
    fields: [
      {
        field: "doc_version",
        desc: "{ stage:草本/修订/终稿/签字版, version_label, signed_at } — 非诉特有,合同迭代追踪",
        alignment: "specific",
      },
    ],
  },
];

export function FieldFrameworkTable() {
  return (
    <div className="space-y-4">
      {GROUPS.map((g) => (
        <div
          key={g.group}
          className="overflow-hidden rounded-lg border border-border bg-card"
        >
          <header className="border-b border-border bg-muted/30 px-4 py-2 text-xs font-semibold text-foreground">
            {g.group}
          </header>
          <ul className="divide-y divide-border/50">
            {g.fields.map((f) => (
              <li
                key={f.field}
                className="flex items-start gap-3 px-4 py-2.5 text-sm"
              >
                <span
                  className={
                    f.alignment === "aligned"
                      ? "mt-0.5 shrink-0 rounded bg-emerald-100 px-1.5 py-0.5 text-caption font-medium text-emerald-800"
                      : "mt-0.5 shrink-0 rounded bg-amber-100 px-1.5 py-0.5 text-caption font-medium text-amber-800"
                  }
                  title={f.alignment === "aligned" ? "与诉讼字段对齐 / 直接复用" : "非诉特有字段"}
                >
                  {f.alignment === "aligned" ? "对齐" : "特有"}
                </span>
                <code className="shrink-0 font-mono text-xs text-foreground">
                  {f.field}
                </code>
                <span className="text-muted-foreground">{f.desc}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}

      <p className="rounded-md border border-dashed border-border bg-muted/20 px-4 py-3 text-label text-muted-foreground">
        ⚠️ 同一系统两套权威表:`CATEGORY_PRIORITY` 表非诉不复用(非诉的「权威类别」是
        工商核准 &gt; 终稿合同 &gt; 内审报告 &gt; 尽调底稿,需要另一张表)。
      </p>
    </div>
  );
}
