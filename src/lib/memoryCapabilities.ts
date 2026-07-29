export type MemoryCapability = {
  id: string;
  label: string;
  supported: boolean;
  behavior: string;
};

/**
 * v0.8.1 的 AI 入口记忆能力登记表。
 *
 * 未列为 supported 的入口不得携带记忆注入凭证；后续接入时必须先完成
 * 预览、逐轮确认、案件隔离和审计，再修改本表。
 */
export const MEMORY_CAPABILITIES: readonly MemoryCapability[] = [
  {
    id: "case_chat",
    label: "案件 AI 聊天（含案件内诉讼分析与法律检索场景）",
    supported: true,
    behavior: "仅使用当前案件已确认的逐轮预览，发送一次后即消费。",
  },
  {
    id: "contract_review",
    label: "独立合同审查",
    supported: false,
    behavior: "v0.8.1 不注入记忆。",
  },
  {
    id: "contract_draft",
    label: "独立合同起草",
    supported: false,
    behavior: "v0.8.1 不注入记忆。",
  },
  {
    id: "transaction_research",
    label: "独立法律检索与深度研究",
    supported: false,
    behavior: "v0.8.1 不注入记忆；案件聊天内的检索场景除外。",
  },
  {
    id: "document_generation",
    label: "律师函、诉讼文书及其他独立生成入口",
    supported: false,
    behavior: "v0.8.1 不注入记忆。",
  },
  {
    id: "material_ai",
    label: "材料抽取、AI 整理与字段候选",
    supported: false,
    behavior: "始终按材料人工确认链路运行，不读取记忆。",
  },
] as const;
