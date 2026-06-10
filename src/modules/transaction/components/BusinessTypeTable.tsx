/**
 * 非诉典型业务类型表 — 数据来自 docs/抽取与聚合方法论-v0.1.md §3.1
 *
 * ⚠️ 改一处必须改两处:数组顺序 / 文案要跟方法论文档同步。
 */

interface BusinessType {
  type: string;
  materials: string;
  milestones: string;
}

const ROWS: BusinessType[] = [
  {
    type: "公司股权架构调整 / 增资 / 减资",
    materials: "股东会决议 / 章程修订 / 工商核准",
    milestones: "决议 → 工商核准 → 新执照",
  },
  {
    type: "并购 / 资产收购",
    materials: "LOI/MOU / SPA / 尽调报告 / 交割备忘录",
    milestones: "LOI → 尽调 → 签约 → 交割 → 工商",
  },
  {
    type: "破产 / 清算 / 重整",
    materials: "受理裁定 / 债权申报 / 重整计划 / 表决记录",
    milestones: "受理 → 申报 → 表决 → 批准 / 转破",
  },
  {
    type: "合规 / 内控审查",
    materials: "内审表 / 风险清单 / 整改报告",
    milestones: "启动 → 出报告 → 整改 → 复核",
  },
  {
    type: "合同尽调 / 起草 / 谈判",
    materials: "草本 / 修订稿 / 终稿 / 签字版",
    milestones: "草本 → 修订 → 终稿 → 签字",
  },
  {
    type: "商标专利 / 知识产权",
    materials: "受理通知 / 审查意见 / 证书 / 异议",
    milestones: "申请 → 受理 → 审查 → 授权",
  },
  {
    type: "行政许可 / 备案",
    materials: "申请书 / 反馈函 / 批复",
    milestones: "申请 → 反馈 → 批复",
  },
];

export function BusinessTypeTable() {
  return (
    <div className="overflow-hidden rounded-lg border border-border bg-card">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-border bg-muted/30 text-caption uppercase tracking-wider text-muted-foreground">
            <th className="px-4 py-2 text-left font-normal">业务类型</th>
            <th className="px-4 py-2 text-left font-normal">典型材料</th>
            <th className="px-4 py-2 text-left font-normal">关键里程碑</th>
          </tr>
        </thead>
        <tbody>
          {ROWS.map((r) => (
            <tr key={r.type} className="border-b border-border/50 last:border-0">
              <td className="px-4 py-2.5 font-medium text-foreground">{r.type}</td>
              <td className="px-4 py-2.5 text-muted-foreground">{r.materials}</td>
              <td className="px-4 py-2.5 text-muted-foreground">{r.milestones}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
