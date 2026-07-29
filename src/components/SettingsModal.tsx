import { useCallback, useEffect, useState } from "react";
import {
  X,
  Loader2,
  ExternalLink,
  Save,
  CheckCircle2,
  XCircle,
  Database,
  FolderOpen,
  Download,
  Upload,
  AlertTriangle,
  RefreshCw,
  Sparkles,
  Trash2,
  Plug,
  Brain,
  Wrench,
  BookText,
  SlidersHorizontal,
  User,
  ShieldCheck,
  Bell,
  Palette,
} from "lucide-react";
import { open as dialogOpen, save as dialogSave } from "@tauri-apps/plugin-dialog";
import { confirmDialog } from "@/lib/dialog";

import { Button } from "@/components/ui/button";
import { HoverHint } from "@/components/HoverHint";
import { KbSemanticIndexCard } from "@/components/KbSemanticIndexCard";
import { CredentialField } from "@/components/settings/CredentialField";
import {
  LocalUsageDashboard,
  type LocalUsageMetric,
  type LocalUsageSnapshot,
} from "@/components/settings/LocalUsageDashboard";
import {
  LocalKbRelocationCard,
  type LocalKbRelocationProgress,
  type LocalKbRelocationResult,
} from "@/components/settings/LocalKbRelocationCard";
import { LocalKbGuideCard } from "@/components/settings/LocalKbGuideCard";
import { YuandianBalanceCard } from "@/components/settings/YuandianBalanceCard";
import { LegalSkillsSettingsCard } from "@/components/settings/LegalSkillsSettingsCard";
import { DeviceSyncSettingsCard } from "@/components/settings/DeviceSyncSettingsCard";
import {
  buildLocalKbSemanticIndex,
  createLocalKb,
  detectKbStatus,
  exportKbToZip,
  getSettings,
  getLocalRecognitionUsage,
  importKbFromZip,
  migrateCurrentLocalKb,
  pruneYuandianCache,
  refreshYuandianLocalUsage,
  openInDefaultApp,
  openUrl,
  parseMcpPaste,
  deleteMcpServer,
  saveSettings,
  setCredential,
  switchExistingLocalKb,
  testMcpServer,
  updateMcpServerMetadata,
  verifyDeepSeekKey,
  verifyMiniMaxKey,
  verifyOpenAICompatKey,
  verifyMinerUKey,
  verifyPaddleVlKey,
  verifyEmbeddingKey,
  verifyYuandianKey,
  type KbConflictStrategy,
  type KbImportResult,
  type KbStatus,
} from "@/lib/api";
import type { CredentialStatus, Settings, McpServerConfig } from "@/lib/types";
import { cn } from "@/lib/utils";
import { localDateKey } from "@/lib/localDate";
import { FEATURE_FLAGS, useFeatureFlag } from "@/lib/featureFlags";
import { FONT_SCALE, useFontScale } from "@/lib/uiScale";
import { THEMES, useThemePreference } from "@/lib/theme";
import {
  CRIMINAL_NOTIFICATION_SETTINGS_EVENT,
  disableCriminalNotifications,
  enableCriminalNotifications,
  getCriminalNotificationPermission,
  isCriminalNotificationEnabled,
  sendCriminalNotificationTest,
  type CriminalNotificationPermission,
} from "@/lib/criminalNotifications";

const SETTINGS_SAVED_EVENT = "caseboard:settings-saved";

const CREDENTIAL_LOCATORS = {
  mineru: "provider/mineru/api-key",
  paddle: "provider/paddle-vl/api-key",
  deepseek: "provider/deepseek/api-key",
  minimax: "provider/minimax/api-key",
  glm: "provider/glm/api-key",
  mimo: "provider/mimo/api-key",
  custom: "provider/custom/api-key",
  yuandian: "provider/yuandian/api-key",
  embedding: "provider/embedding/api-key",
} as const;

type VerifyStatus = "idle" | "verifying" | "ok" | "fail";
type CompatBackend = "glm" | "mimo" | "custom";
type CompatSettingKey =
  | "glm_llm_endpoint"
  | "glm_llm_model"
  | "glm_llm_verified_at"
  | "mimo_llm_endpoint"
  | "mimo_llm_model"
  | "mimo_llm_verified_at"
  | "custom_llm_endpoint"
  | "custom_llm_model"
  | "custom_llm_verified_at";
type CompatFieldKind = "endpoint" | "model" | "verifiedAt";

/** 云端 AI 后端可选项(下拉)。glm/mimo/custom 共用「通用 OpenAI 兼容」配置(compat_llm_*)。 */
const CLOUD_BACKEND_OPTIONS = [
  { id: "deepseek", label: "DeepSeek(默认)" },
  { id: "minimax", label: "MiniMax(M 系列)" },
  { id: "glm", label: "智谱 GLM(OpenAI 兼容)" },
  { id: "mimo", label: "小米 MiMo(OpenAI 兼容)" },
  { id: "custom", label: "自定义(OpenAI 兼容)" },
] as const;

/** OpenAI 兼容后端预设(镜像 Rust llm::providers;切换时预填到 compat_llm_*,均可改)。 */
const COMPAT_PRESETS: Record<
  CompatBackend,
  { label: string; endpoint: string; model: string; applyUrl?: string }
> = {
  glm: {
    label: "智谱 GLM",
    endpoint: "https://open.bigmodel.cn/api/paas/v4/chat/completions",
    model: "glm-4.6",
    applyUrl: "https://open.bigmodel.cn/usercenter/apikeys",
  },
  mimo: {
    label: "小米 MiMo",
    endpoint: "https://token-plan-cn.xiaomimimo.com/v1/chat/completions",
    model: "mimo-v2.5",
  },
  custom: { label: "自定义(OpenAI 兼容)", endpoint: "", model: "" },
};

const COMPAT_FIELD_KEYS: Record<
  CompatBackend,
  {
    endpoint: CompatSettingKey;
    model: CompatSettingKey;
    verifiedAt: CompatSettingKey;
  }
> = {
  glm: {
    endpoint: "glm_llm_endpoint",
    model: "glm_llm_model",
    verifiedAt: "glm_llm_verified_at",
  },
  mimo: {
    endpoint: "mimo_llm_endpoint",
    model: "mimo_llm_model",
    verifiedAt: "mimo_llm_verified_at",
  },
  custom: {
    endpoint: "custom_llm_endpoint",
    model: "custom_llm_model",
    verifiedAt: "custom_llm_verified_at",
  },
};

function isCompatBackend(value: string | null | undefined): value is CompatBackend {
  return value === "glm" || value === "mimo" || value === "custom";
}

function compatValue(
  settings: Settings,
  backend: CompatBackend,
  kind: CompatFieldKind,
): string | null {
  const key = COMPAT_FIELD_KEYS[backend][kind];
  const value = settings[key];
  return typeof value === "string" && value.trim() ? value : null;
}

function legacyCompatValue(
  settings: Settings,
  kind: CompatFieldKind,
): string | null {
  const key =
    kind === "verifiedAt"
      ? "compat_llm_verified_at"
        : kind === "model"
          ? "compat_llm_model"
          : "compat_llm_endpoint";
  const value = settings[key];
  return typeof value === "string" && value.trim() ? value : null;
}

function effectiveCompatValue(
  settings: Settings,
  backend: CompatBackend,
  kind: CompatFieldKind,
): string | null {
  return compatValue(settings, backend, kind) || legacyCompatValue(settings, kind);
}

function setStringSetting(
  target: Partial<Settings>,
  key: CompatSettingKey,
  value: string | null,
) {
  target[key] = value as never;
}

/** 设置页底部标签页(按类型归拢散乱配置;详见 docs/设置页重构-分类方案-2026-06-16.md) */
export type SettingsTab =
  | "theme" // 主题:本机界面配色
  | "brain" // 大脑:对话大模型
  | "models" // 功能模型:OCR / Embedding 等调云端 API 的工具型模型
  | "kb" // 知识库:本地法律知识库 + 语义索引
  | "datasource" // 数据源:元典 + 外部 MCP(企查查/万得/北大法宝)
  | "toggles" // 功能开关:首页清爽开关
  | "general"; // 通用:个人信息 / 快递100

const SETTINGS_TABS: { id: SettingsTab; label: string; icon: typeof Brain }[] = [
  { id: "general", label: "通用", icon: User },
  { id: "theme", label: "主题", icon: Palette },
  { id: "brain", label: "大脑", icon: Brain },
  { id: "models", label: "功能模型", icon: Wrench },
  { id: "kb", label: "知识库", icon: BookText },
  { id: "datasource", label: "数据源", icon: Database },
  { id: "toggles", label: "功能开关", icon: SlidersHorizontal },
];

interface Props {
  /** modal 模式下必填(用户点 X / 蒙层 / Escape / 保存成功 都调它);page 模式可选 */
  onClose?: () => void;
  /** 2026-05-25 V0.1.8 · 展示形态:modal=弹窗;page=主内容区独立页(去掉 modal shell) */
  mode?: "modal" | "page";
  /** 2026-05-25 V0.1.8 · page 模式上报 dirty 状态,父组件用来在切 tab 时弹未保存提醒 */
  onDirtyChange?: (dirty: boolean) => void;
  /** 2026-05-27 · 保存成功后通知父组件(page 模式不关闭,但 settings 已经落库 ——
   *  父组件需要重判依赖项,比如右上角 DeepSeek 余额 chip 的可见性)。
   *  modal 模式下保存成功直接 onClose,父组件那侧已经会重读 settings,不需要这个。 */
  onSaved?: () => void;
  /** 2026-06-16 · 进入设置时初始落在哪个 tab。默认 "general"(通用);
   *  导入缺 LLM key 跳设置时父组件传 "brain" 深链到大脑。 */
  initialTab?: SettingsTab;
}

/**
 * 用户设置(modal 弹窗 / page 独立页 双形态)。
 *
 * 设计原则(对应 CLAUDE.md 隐私铁律):
 *   - 每个用户填自己的 token,工具不内置任何人的 key
 *   - 顶部有一行明确说明"配置只保存在你本机,不发送任何地方"
 *   - 每个字段附"如何获取/安装"链接
 *   - api_key 用 password input,不在窗口里明文显示
 *
 * 2026-05-25 V0.1.8:加 mode prop。page 模式给「设置 tab」用,modal 模式仍兼容
 * 现有「导入前 token 缺失自动弹」流程,两种形态共用同一份 form 逻辑。
 */
export function SettingsModal({
  onClose,
  mode = "modal",
  onDirtyChange,
  onSaved,
  initialTab,
}: Props) {
  const isPage = mode === "page";
  const handleClose = () => {
    if (onClose) onClose();
  };
  const [settings, setSettings] = useState<Settings | null>(null);
  const [credentialDrafts, setCredentialDrafts] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false); // page 模式下保存成功显示 toast(modal 模式直接关闭)
  // 2026-05-25 V0.1.8 · 是否有未保存改动(page 模式上报给父组件做切 tab 防呆)
  const [dirty, setDirty] = useState(false);
  // 2026-06-16 · 设置页内部标签页。默认「通用」(作者要求点开设置先看通用);
  // 导入缺 LLM key 跳设置时父组件传 initialTab="brain" 深链到大脑(a92ae91 校验的是 LLM key)。
  const [tab, setTab] = useState<SettingsTab>(initialTab ?? "general");
  const [privacySummaryOpen, setPrivacySummaryOpen] = useState(false);

  // 2026-05-25 V0.1.6 · token 在线验证状态
  const [mineruStatus, setMineruStatus] = useState<VerifyStatus>("idle");
  const [mineruMsg, setMineruMsg] = useState<string>("");
  // 2026-06-12 · PaddleOCR VL(AI Studio)访问令牌验证状态
  const [paddleStatus, setPaddleStatus] = useState<VerifyStatus>("idle");
  const [paddleMsg, setPaddleMsg] = useState<string>("");
  const [deepseekStatus, setDeepseekStatus] = useState<VerifyStatus>("idle");
  const [deepseekMsg, setDeepseekMsg] = useState<string>("");
  // 2026-06-15 · MiniMax API key 在线验证状态
  const [minimaxStatus, setMinimaxStatus] = useState<VerifyStatus>("idle");
  const [minimaxMsg, setMinimaxMsg] = useState<string>("");
  // 2026-06-16 · 通用 OpenAI 兼容后端(GLM/MiMo/自定义)在线验证状态
  const [compatStatus, setCompatStatus] = useState<VerifyStatus>("idle");
  const [compatMsg, setCompatMsg] = useState<string>("");
  // 2026-05-25 V0.1.8 · 元典 API key 在线验证状态
  const [yuandianStatus, setYuandianStatus] = useState<VerifyStatus>("idle");
  const [yuandianMsg, setYuandianMsg] = useState<string>("");
  const [embeddingStatus, setEmbeddingStatus] = useState<VerifyStatus>("idle");
  const [embeddingMsg, setEmbeddingMsg] = useState<string>("");

  // settings 加载完后,如果 verified_at 非空,初始化为 "ok"(从 DB 读出来的已验证状态)
  useEffect(() => {
    if (!settings) return;
    if (settings.mineru_verified_at && mineruStatus === "idle") {
      setMineruStatus("ok");
    }
    if (settings.paddle_vl_verified_at && paddleStatus === "idle") {
      setPaddleStatus("ok");
    }
    if (settings.deepseek_verified_at && deepseekStatus === "idle") {
      setDeepseekStatus("ok");
    }
    if (settings.minimax_verified_at && minimaxStatus === "idle") {
      setMinimaxStatus("ok");
    }
    const backend = isCompatBackend(settings.cloud_llm_backend)
      ? settings.cloud_llm_backend
      : null;
    const compatVerifiedAt = backend
      ? effectiveCompatValue(settings, backend, "verifiedAt")
      : null;
    if (compatVerifiedAt && compatStatus === "idle") {
      setCompatStatus("ok");
    }
    if (settings.yuandian_verified_at && yuandianStatus === "idle") {
      setYuandianStatus("ok");
    }
    if (settings.embedding_verified_at && embeddingStatus === "idle") {
      setEmbeddingStatus("ok");
    }
    // 只在初次加载时设
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    settings?.mineru_verified_at,
    settings?.paddle_vl_verified_at,
    settings?.deepseek_verified_at,
    settings?.minimax_verified_at,
    settings?.cloud_llm_backend,
    settings?.compat_llm_verified_at,
    settings?.glm_llm_verified_at,
    settings?.mimo_llm_verified_at,
    settings?.custom_llm_verified_at,
    settings?.yuandian_verified_at,
  ]);

  const credentialConfigured = (locator: string) =>
    settings?.credential_statuses?.some(
      (status) => status.locator === locator && status.configured,
    ) ?? false;

  const credentialStatus = (locator: string) =>
    settings?.credential_statuses?.find((status) => status.locator === locator);

  const updateCredentialStatus = (status: CredentialStatus) => {
    setSettings((current) =>
      current
        ? {
            ...current,
            credential_statuses: [
              ...(current.credential_statuses ?? []).filter(
                (item) => item.locator !== status.locator,
              ),
              status,
            ],
          }
        : current,
    );
  };

  async function persistCredentialDraft(locator: string) {
    const value = credentialDrafts[locator]?.trim();
    if (!value) return;
    const status = await setCredential(locator, value);
    setCredentialDrafts((current) => ({ ...current, [locator]: "" }));
    setSettings((current) =>
      current
        ? {
            ...current,
            credential_statuses: [
              ...(current.credential_statuses ?? []).filter(
                (item) => item.locator !== locator,
              ),
              status,
            ],
          }
        : current,
    );
  }

  async function handleVerifyMineru() {
    if (!(credentialDrafts[CREDENTIAL_LOCATORS.mineru]?.trim() || credentialConfigured(CREDENTIAL_LOCATORS.mineru))) {
      setMineruStatus("fail");
      setMineruMsg("请先填入 Token");
      return;
    }
    setMineruStatus("verifying");
    setMineruMsg("");
    try {
      await persistCredentialDraft(CREDENTIAL_LOCATORS.mineru);
      const r = await verifyMinerUKey();
      if (r.ok) {
        setMineruStatus("ok");
        setMineruMsg("");
        updateField("mineru_verified_at", new Date().toISOString());
      } else {
        setMineruStatus("fail");
        setMineruMsg(r.message);
        updateField("mineru_verified_at", null);
      }
    } catch (e) {
      setMineruStatus("fail");
      setMineruMsg(String(e));
      updateField("mineru_verified_at", null);
    }
  }

  async function handleVerifyPaddle() {
    if (!(credentialDrafts[CREDENTIAL_LOCATORS.paddle]?.trim() || credentialConfigured(CREDENTIAL_LOCATORS.paddle))) {
      setPaddleStatus("fail");
      setPaddleMsg("请先填入访问令牌");
      return;
    }
    setPaddleStatus("verifying");
    setPaddleMsg("");
    try {
      await persistCredentialDraft(CREDENTIAL_LOCATORS.paddle);
      const r = await verifyPaddleVlKey();
      if (r.ok) {
        setPaddleStatus("ok");
        setPaddleMsg("");
        updateField("paddle_vl_verified_at", new Date().toISOString());
      } else {
        setPaddleStatus("fail");
        setPaddleMsg(r.message);
        updateField("paddle_vl_verified_at", null);
      }
    } catch (e) {
      setPaddleStatus("fail");
      setPaddleMsg(String(e));
      updateField("paddle_vl_verified_at", null);
    }
  }

  async function handleVerifyDeepSeek() {
    if (!settings) return;
    if (!(credentialDrafts[CREDENTIAL_LOCATORS.deepseek]?.trim() || credentialConfigured(CREDENTIAL_LOCATORS.deepseek))) {
      setDeepseekStatus("fail");
      setDeepseekMsg("请先填入 API Key");
      return;
    }
    setDeepseekStatus("verifying");
    setDeepseekMsg("");
    try {
      await persistCredentialDraft(CREDENTIAL_LOCATORS.deepseek);
      const r = await verifyDeepSeekKey(
        settings.cloud_llm_endpoint ?? undefined,
      );
      if (r.ok) {
        setDeepseekStatus("ok");
        setDeepseekMsg("");
        updateField("deepseek_verified_at", new Date().toISOString());
      } else {
        setDeepseekStatus("fail");
        setDeepseekMsg(r.message);
        updateField("deepseek_verified_at", null);
      }
    } catch (e) {
      setDeepseekStatus("fail");
      setDeepseekMsg(String(e));
      updateField("deepseek_verified_at", null);
    }
  }

  async function handleVerifyMiniMax() {
    if (!settings) return;
    if (!(credentialDrafts[CREDENTIAL_LOCATORS.minimax]?.trim() || credentialConfigured(CREDENTIAL_LOCATORS.minimax))) {
      setMinimaxStatus("fail");
      setMinimaxMsg("请先填入 API Key");
      return;
    }
    setMinimaxStatus("verifying");
    setMinimaxMsg("");
    try {
      await persistCredentialDraft(CREDENTIAL_LOCATORS.minimax);
      const r = await verifyMiniMaxKey(
        settings.minimax_endpoint ?? undefined,
      );
      if (r.ok) {
        setMinimaxStatus("ok");
        setMinimaxMsg("");
        updateField("minimax_verified_at", new Date().toISOString());
      } else {
        setMinimaxStatus("fail");
        setMinimaxMsg(r.message);
        updateField("minimax_verified_at", null);
      }
    } catch (e) {
      setMinimaxStatus("fail");
      setMinimaxMsg(String(e));
      updateField("minimax_verified_at", null);
    }
  }

  async function handleVerifyCompat() {
    if (!settings || !isCompatBackend(settings.cloud_llm_backend)) return;
    const keys = COMPAT_FIELD_KEYS[settings.cloud_llm_backend];
    const locator = CREDENTIAL_LOCATORS[settings.cloud_llm_backend];
    const endpoint = effectiveCompatValue(settings, settings.cloud_llm_backend, "endpoint") || "";
    const model = effectiveCompatValue(settings, settings.cloud_llm_backend, "model") || "";
    if (!(credentialDrafts[locator]?.trim() || credentialConfigured(locator))) {
      setCompatStatus("fail");
      setCompatMsg("请先填入 API Key");
      return;
    }
    setCompatStatus("verifying");
    setCompatMsg("");
    try {
      await persistCredentialDraft(locator);
      const r = await verifyOpenAICompatKey(
        locator,
        endpoint,
        model,
      );
      if (r.ok) {
        setCompatStatus("ok");
        setCompatMsg("");
        updateField(keys.verifiedAt, new Date().toISOString());
      } else {
        setCompatStatus("fail");
        setCompatMsg(r.message);
        updateField(keys.verifiedAt, null);
      }
    } catch (e) {
      setCompatStatus("fail");
      setCompatMsg(String(e));
      updateField(keys.verifiedAt, null);
    }
  }

  // 切换云端 AI 后端。deepseek→存 null(默认);minimax→"minimax";glm/mimo/custom→预填兼容配置。
  function handleChangeBackend(value: string) {
    if (value === "minimax") {
      updateField("cloud_llm_backend", "minimax");
      return;
    }
    if (isCompatBackend(value)) {
      const preset = COMPAT_PRESETS[value];
      const keys = COMPAT_FIELD_KEYS[value];
      const patch: Partial<Settings> = { cloud_llm_backend: value };
      if (!settings || !effectiveCompatValue(settings, value, "endpoint")) {
        setStringSetting(patch, keys.endpoint, preset.endpoint || null);
      }
      if (!settings || !effectiveCompatValue(settings, value, "model")) {
        setStringSetting(patch, keys.model, preset.model || null);
      }
      updateFields(patch);
      setCompatStatus("idle");
      setCompatMsg("");
      return;
    }
    updateField("cloud_llm_backend", null); // deepseek / 默认
  }

  async function handleVerifyYuandian(): Promise<boolean> {
    if (!(credentialDrafts[CREDENTIAL_LOCATORS.yuandian]?.trim() || credentialConfigured(CREDENTIAL_LOCATORS.yuandian))) {
      setYuandianStatus("fail");
      setYuandianMsg("请先填入 API Key");
      return false;
    }
    setYuandianStatus("verifying");
    setYuandianMsg("");
    try {
      await persistCredentialDraft(CREDENTIAL_LOCATORS.yuandian);
      const r = await verifyYuandianKey();
      if (r.ok) {
        setYuandianStatus("ok");
        setYuandianMsg("");
        updateField("yuandian_verified_at", new Date().toISOString());
        return true;
      } else {
        setYuandianStatus("fail");
        setYuandianMsg(r.message);
        updateField("yuandian_verified_at", null);
        return false;
      }
    } catch (e) {
      setYuandianStatus("fail");
      setYuandianMsg(String(e));
      updateField("yuandian_verified_at", null);
      return false;
    }
  }

  async function handleVerifyEmbedding() {
    if (!settings) return;
    if (!(credentialDrafts[CREDENTIAL_LOCATORS.embedding]?.trim() || credentialConfigured(CREDENTIAL_LOCATORS.embedding))) {
      setEmbeddingStatus("fail");
      setEmbeddingMsg("请先填入 API Key");
      return;
    }
    setEmbeddingStatus("verifying");
    setEmbeddingMsg("");
    try {
      await persistCredentialDraft(CREDENTIAL_LOCATORS.embedding);
      const dim = await verifyEmbeddingKey(
        settings.embedding_endpoint ?? "",
        settings.embedding_model ?? "",
      );
      setEmbeddingStatus("ok");
      setEmbeddingMsg(`✓ 已验证 · 向量维度 ${dim}`);
      updateField("embedding_verified_at", new Date().toISOString());
    } catch (e) {
      setEmbeddingStatus("fail");
      setEmbeddingMsg(String(e));
      updateField("embedding_verified_at", null);
    }
  }

  useEffect(() => {
    let cancelled = false;
    getSettings()
      .then((s) => {
        if (!cancelled) setSettings(s);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    // page 模式 Escape 不关页(切 tab 才离开),只 modal 模式响应
    if (isPage) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") handleClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // handleClose 不放进 deps,因为它依赖 onClose,而 onClose 是新引用每次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isPage, onClose]);

  // dirty 上报给父组件
  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  async function handleSave() {
    if (!settings) return;
    setSaving(true);
    setError(null);
    try {
      await saveSettings(prepareSettingsForSave(settings));
      setDirty(false);
      try {
        window.dispatchEvent(new CustomEvent(SETTINGS_SAVED_EVENT));
      } catch {
        // ignore event dispatch failures
      }
      // 2026-05-27 · 两种模式都要通知父组件 settings 已经变了,父组件据此重判依赖项
      // (如 DeepSeek 余额 chip 是否显示)。修复同事场景:onboarding 选"稍后再配置"
      // 进 page 模式补填 key,保存后 chip 不出现 —— 因为 page 模式只显示 toast、不触发
      // onClose,父组件的 showDeepSeekChip 状态从未更新。
      onSaved?.();
      if (isPage) {
        // page 模式:不关闭页面,显示"已保存"提示
        setSaved(true);
        setSaving(false);
        // 3 秒后清掉"已保存"提示
        setTimeout(() => setSaved(false), 3000);
      } else {
        // modal 模式:保存成功 → 自动关闭(作者 2026-05-23 晚九 反馈)
        handleClose();
      }
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  }

  function updateField<K extends keyof Settings>(key: K, value: Settings[K]) {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
    setDirty(true);
  }

  // 一次更新多个字段(切换云端服务商时:backend + 预填 endpoint/model + 清 key/verified 要原子改)
  function updateFields(patch: Partial<Settings>) {
    setSettings((prev) => (prev ? { ...prev, ...patch } : prev));
    setDirty(true);
  }

  function prepareSettingsForSave(current: Settings): Settings {
    if (!isCompatBackend(current.cloud_llm_backend)) return current;
    const backend = current.cloud_llm_backend;
    const keys = COMPAT_FIELD_KEYS[backend];
    const next: Settings = { ...current };
    if (!compatValue(next, backend, "endpoint")) {
      setStringSetting(
        next,
        keys.endpoint,
        legacyCompatValue(current, "endpoint") || COMPAT_PRESETS[backend].endpoint || null,
      );
    }
    if (!compatValue(next, backend, "model")) {
      setStringSetting(
        next,
        keys.model,
        legacyCompatValue(current, "model") || COMPAT_PRESETS[backend].model || null,
      );
    }
    if (!compatValue(next, backend, "verifiedAt")) {
      setStringSetting(next, keys.verifiedAt, legacyCompatValue(current, "verifiedAt"));
    }
    return next;
  }

  // page 模式:没有蒙层,卡片直接占主区域,scroll 由父容器管;不带 X 按钮
  // modal 模式:蒙层 + max-h 限高 + X 按钮(原有形态)
  // 注意:不能用内嵌函数组件 wrap children,那会让每次 render 重建组件类型 → 子树 unmount + state 丢失
  // 改用条件渲染同一 body JSX,React 会正确 diff
  const body = (
    <>
        {/* 标题栏 */}
        <header className="flex items-center justify-between gap-4 border-b border-border bg-card/95 px-5 py-3.5 backdrop-blur">
          <div>
            <h2
              className={cn(
                "font-semibold text-foreground",
                isPage ? "text-lg" : "text-sm",
              )}
            >
              设置
            </h2>
            <p className="mt-0.5 text-xs text-muted-foreground">
              填你自己的 token。每个用户填自己的,工具不内置任何人的 key。
            </p>
          </div>
          {!isPage && (
            <Button
              variant="ghost"
              size="icon"
              onClick={handleClose}
              aria-label="关闭"
            >
              <X className="size-4" />
            </Button>
          )}
        </header>

        {/* 内容区 */}
        <div className="flex-1 overflow-auto px-5 py-5">
          {loading && (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          )}
          {!loading && settings && (
            <>
            {/* 2026-06-16 · 标签页导航:按类型归拢散乱配置 */}
            <div className="mb-5 flex flex-wrap gap-1.5 border-b border-border pb-3">
              {SETTINGS_TABS.map((t) => {
                const Icon = t.icon;
                const active = tab === t.id;
                return (
                  <button
                    key={t.id}
                    type="button"
                    onClick={() => setTab(t.id)}
                    className={cn(
                      "inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                      active
                        ? "bg-sky-50 text-sky-700 ring-1 ring-sky-200"
                        : "text-muted-foreground hover:bg-accent hover:text-foreground",
                    )}
                  >
                    <Icon className="size-4" />
                    {t.label}
                  </button>
                );
              })}
            </div>
            <div
              className={cn(
                // page 模式:每个功能区各占一半,左右成对(更简洁、少占行);
                // 窗口恒 ≥1024(minWidth),lg 断点始终生效 → 默认就是两列。
                // modal 模式:保持单列堆叠,窄弹窗里两列会挤。
                isPage
                  ? "grid grid-cols-1 lg:grid-cols-2 gap-x-5 gap-y-5 items-start"
                  : "space-y-6",
              )}
            >
              {/* ── 通用:界面字号(放最前,字小问题最常见)── */}
              {tab === "general" && <FontScaleCard />}

              {tab === "general" && (
                <Section
                  title="我的设备同步"
                  desc="通过 NAS 挂载目录进行端到端加密备份与双向同步；支持两台电脑无缝衔接。"
                  fill
                >
                  <DeviceSyncSettingsCard />
                </Section>
              )}

              {tab === "theme" && <ThemeCard />}

              {/* ── 通用:个人信息 ── */}
              {tab === "general" && (
                  <Section title="个人信息" fill>
                    <Field
                      label="称呼"
                      hint="首页问候用,例:刘律师 / 周律师 / 李三"
                    >
                      <input
                        type="text"
                        value={settings.user_display_name ?? ""}
                        onChange={(e) =>
                          updateField(
                            "user_display_name",
                            e.target.value || null,
                          )
                        }
                        placeholder="例:刘律师"
                        className={inputCls}
                      />
                    </Field>
                  </Section>
              )}

              {/* ── 通用:本地可读的隐私说明入口 ── */}
              {tab === "general" && (
                <Section
                  title="隐私与数据说明"
                  desc="先在应用内了解数据写入与外部传输边界，再决定是否启用云端能力。"
                  fill
                >
                  <button
                    type="button"
                    onClick={() => setPrivacySummaryOpen(true)}
                    className="flex w-full items-center justify-between gap-3 rounded-lg border border-sky-200 bg-sky-50/70 px-3 py-2.5 text-left transition-colors hover:border-sky-300 hover:bg-sky-100"
                    aria-haspopup="dialog"
                  >
                    <span className="flex items-center gap-2 text-sm font-medium text-sky-800">
                      <ShieldCheck className="size-4" />
                      查看隐私与数据说明
                    </span>
                    <span className="text-xs text-sky-700">应用内可读</span>
                  </button>
                </Section>
              )}

              {/* ── 功能开关:首页日程日历 ── */}
              {tab === "toggles" && (
                <>
                  <Section
                    title="方律场景路由"
                    desc="控制聊天主链是否允许切到 Fanglv 场景路由。默认关闭;开启后仅在命中特定法律场景时接管,普通聊天仍可走原主链。"
                    fill
                  >
                    <label className="flex items-center justify-between gap-3">
                      <div className="space-y-1">
                        <span className="text-xs text-muted-foreground">
                          {settings.enable_fanglv_router
                            ? "已开启 — 命中特定法律场景时才接管"
                            : "已关闭 — 聊天始终走原主链"}
                        </span>
                        <p className="text-label text-muted-foreground">
                          建议只在需要诉讼分析、合同法律检索等 Fanglv 场景时开启。切换后对新发起的聊天生效,不会回改历史消息。
                        </p>
                      </div>
                      <button
                        type="button"
                        role="switch"
                        aria-checked={settings.enable_fanglv_router}
                        onClick={() =>
                          updateField(
                            "enable_fanglv_router",
                            !settings.enable_fanglv_router,
                          )
                        }
                        className={cn(
                          "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors",
                          settings.enable_fanglv_router
                            ? "bg-emerald-600"
                            : "bg-muted",
                        )}
                      >
                        <span
                          className={cn(
                            "inline-block size-4 rounded-full bg-white shadow transition-transform",
                            settings.enable_fanglv_router
                              ? "translate-x-4"
                              : "translate-x-0.5",
                          )}
                        />
                      </button>
                    </label>
                  </Section>
                  <Section
                    title="首页日程日历(可选)"
                    desc="把开庭/续封、带日期的待办、手动提醒汇总到首页日历;默认关闭,想体验就开,随时可关。"
                    fill
                  >
                    <label className="flex items-center justify-between gap-3">
                      <span className="text-xs text-muted-foreground">
                        {settings.home_calendar_enabled
                          ? "已开启 — 首页显示"
                          : "已关闭 — 不显示"}
                      </span>
                      <button
                        type="button"
                        role="switch"
                        aria-checked={settings.home_calendar_enabled}
                        onClick={() =>
                          updateField(
                            "home_calendar_enabled",
                            !settings.home_calendar_enabled,
                          )
                        }
                        className={cn(
                          "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors",
                          settings.home_calendar_enabled
                            ? "bg-sky-600"
                            : "bg-muted",
                        )}
                      >
                        <span
                          className={cn(
                            "inline-block size-4 rounded-full bg-white shadow transition-transform",
                            settings.home_calendar_enabled
                              ? "translate-x-4"
                              : "translate-x-0.5",
                          )}
                        />
                      </button>
                    </label>
                  </Section>
                </>
              )}

              {/* V0.3:本地模型已隐藏 → 只走云端。三个 API key(MinerU / DeepSeek / 元典)常显,
                  不再用 cloud_enabled 开关包裹(该字段保留兼容,前端不再读)。 */}
              {/* ── 功能模型:PaddleOCR(云端 OCR,排在 MinerU 前)──
                  2026-06-12:PaddleOCR VL-1.6(AI Studio)。填了 key 即自动成为
                  另一家的备用(失败/超时/额度用完自动切换);也可在下方「云端 OCR 主力」卡切为主力。
                  实测:精度与 MinerU 打平,速度约快一倍,免费 2 万页/天(MinerU 1 千页/天);
                  单文件 >100 页会自动落回 MinerU。 */}
              {tab === "models" && (
                  <Section
                    title="PaddleOCR(云端 OCR)"
                    link={{
                      label: "点这里申请访问令牌",
                      href: "https://aistudio.baidu.com/account/accessToken",
                    }}
                  >
                    <Field
                      label="访问令牌"
                      hint="选填。填了即自动成为另一家 OCR 的备用线路;免费额度 2 万页/天"
                    >
                      <div className="flex items-center gap-2">
                        <div className="flex-1">
                          <CredentialField
                            label="PaddleOCR 访问令牌"
                            locator={CREDENTIAL_LOCATORS.paddle}
                            status={credentialStatus(CREDENTIAL_LOCATORS.paddle)}
                            onStatusChange={updateCredentialStatus}
                          />
                        </div>
                        <input
                          type="password"
                          value={credentialDrafts[CREDENTIAL_LOCATORS.paddle] ?? ""}
                          onChange={(e) => {
                            setCredentialDrafts((current) => ({
                              ...current,
                              [CREDENTIAL_LOCATORS.paddle]: e.target.value,
                            }));
                            // 改 token 就重置验证状态;清空 token 时主力退回 MinerU
                            if (paddleStatus !== "idle") {
                              setPaddleStatus("idle");
                              setPaddleMsg("");
                              updateField("paddle_vl_verified_at", null);
                            }
                            if (!e.target.value) {
                              updateField("ocr_cloud_primary", null);
                            }
                          }}
                          placeholder="AI Studio 访问令牌"
                          className="hidden"
                          autoComplete="off"
                        />
                        <VerifyStatusIcon status={paddleStatus} />
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          className="disabled:cursor-not-allowed"
                          onClick={handleVerifyPaddle}
                          disabled={
                            paddleStatus === "verifying" ||
                            !credentialDrafts[CREDENTIAL_LOCATORS.paddle]?.trim() &&
                            !credentialConfigured(CREDENTIAL_LOCATORS.paddle)
                          }
                        >
                          {paddleStatus === "verifying" ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            "验证"
                          )}
                        </Button>
                      </div>
                      {paddleStatus === "fail" && paddleMsg && (
                        <p className="mt-1.5 text-xs text-red-600">
                          ✗ {paddleMsg}
                        </p>
                      )}
                      {paddleStatus === "ok" && (
                        <p className="mt-1.5 text-xs text-green-700">
                          ✓ 已验证通过,可以使用
                        </p>
                      )}
                    </Field>
                  </Section>
              )}

              {/* ── 功能模型:MinerU(云端 OCR)── */}
              {tab === "models" && (
                  <Section
                    title="MinerU"
                    link={{ label: "点这里申请 token", href: "https://mineru.net/apiManage/token" }}
                  >
                    <Field label="API Token">
                      <div className="flex items-center gap-2">
                        <div className="flex-1">
                          <CredentialField
                            label="MinerU API Token"
                            locator={CREDENTIAL_LOCATORS.mineru}
                            status={credentialStatus(CREDENTIAL_LOCATORS.mineru)}
                            onStatusChange={updateCredentialStatus}
                          />
                        </div>
                        <input
                          type="password"
                          value={credentialDrafts[CREDENTIAL_LOCATORS.mineru] ?? ""}
                          onChange={(e) => {
                            setCredentialDrafts((current) => ({
                              ...current,
                              [CREDENTIAL_LOCATORS.mineru]: e.target.value,
                            }));
                            // 改 token 就重置验证状态
                            if (mineruStatus !== "idle") {
                              setMineruStatus("idle");
                              setMineruMsg("");
                              updateField("mineru_verified_at", null);
                            }
                          }}
                          placeholder="eyJ0eXBl..."
                          className="hidden"
                          autoComplete="off"
                        />
                        <VerifyStatusIcon status={mineruStatus} />
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          className="disabled:cursor-not-allowed"
                          onClick={handleVerifyMineru}
                          disabled={
                            mineruStatus === "verifying" ||
                            !credentialDrafts[CREDENTIAL_LOCATORS.mineru]?.trim() &&
                            !credentialConfigured(CREDENTIAL_LOCATORS.mineru)
                          }
                        >
                          {mineruStatus === "verifying" ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            "验证"
                          )}
                        </Button>
                      </div>
                      {mineruStatus === "fail" && mineruMsg && (
                        <p className="mt-1.5 text-xs text-red-600">
                          ✗ {mineruMsg}
                        </p>
                      )}
                      {mineruStatus === "ok" && (
                        <p className="mt-1.5 text-xs text-green-700">
                          ✓ 已验证通过,可以使用
                        </p>
                      )}
                    </Field>
                  </Section>
              )}

              {/* ── 功能模型:云端 OCR 主力(主副选择,单独卡,默认就显示)── */}
              {tab === "models" && (
                  <Section
                    title="云端 OCR 主力"
                    desc="MinerU 与 PaddleOCR 谁当主力:主力失败、排队超时或额度用完时,自动切到另一家,无需手动干预。"
                  >
                    <Field label="选择主力">
                      <select
                        value={
                          settings.ocr_cloud_primary === "paddle-vl"
                            ? "paddle-vl"
                            : "mineru"
                        }
                        onChange={(e) =>
                          updateField(
                            "ocr_cloud_primary",
                            e.target.value === "paddle-vl" ? "paddle-vl" : null,
                          )
                        }
                        className={inputCls}
                      >
                        <option value="paddle-vl">
                          PaddleOCR 主力,MinerU 备用(推荐 · 更快、额度更高)
                        </option>
                        <option value="mineru">
                          MinerU 主力,PaddleOCR 备用
                        </option>
                      </select>
                      <p className="mt-1.5 rounded-md bg-sky-50 px-2.5 py-1.5 text-caption text-sky-800">
                        建议用 <strong>PaddleOCR 为主、MinerU 备用</strong> ——
                        PaddleOCR 速度更快、免费额度更高(2 万页/天 vs MinerU 1 千页/天),
                        批量导入更不容易卡。
                        {!credentialConfigured(CREDENTIAL_LOCATORS.paddle) &&
                          "(需先在上方「PaddleOCR」卡填访问令牌)"}
                      </p>
                    </Field>
                  </Section>
              )}

              {/* ── 数据源:元典法律开放平台(法规/案例/企业检索 + 执行查被执行人)── */}
              {tab === "datasource" && (
              <Section
                title="元典法律开放平台"
                desc="查询法律法规、裁判案例、企业信息的数据源"
                link={{
                  label: "注册后在「个人中心」申请 API key",
                  href: "https://open.chineselaw.com/profile",
                }}
              >
                <Field label="API Key">
                  <div className="flex items-center gap-2">
                    <div className="flex-1">
                      <CredentialField
                        label="元典 API Key"
                        locator={CREDENTIAL_LOCATORS.yuandian}
                        status={credentialStatus(CREDENTIAL_LOCATORS.yuandian)}
                        onStatusChange={updateCredentialStatus}
                      />
                    </div>
                    <input
                      type="password"
                      value={credentialDrafts[CREDENTIAL_LOCATORS.yuandian] ?? ""}
                      onChange={(e) => {
                        setCredentialDrafts((current) => ({
                          ...current,
                          [CREDENTIAL_LOCATORS.yuandian]: e.target.value,
                        }));
                        // 改 key 就重置验证状态
                        if (yuandianStatus !== "idle") {
                          setYuandianStatus("idle");
                          setYuandianMsg("");
                          updateField("yuandian_verified_at", null);
                        }
                      }}
                      placeholder="sk_..."
                      className="hidden"
                      autoComplete="off"
                    />
                    <VerifyStatusIcon status={yuandianStatus} />
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="disabled:cursor-not-allowed"
                      onClick={handleVerifyYuandian}
                      disabled={
                        yuandianStatus === "verifying" ||
                        !credentialDrafts[CREDENTIAL_LOCATORS.yuandian]?.trim() &&
                        !credentialConfigured(CREDENTIAL_LOCATORS.yuandian)
                      }
                    >
                      {yuandianStatus === "verifying" ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        "验证"
                      )}
                    </Button>
                  </div>
                  {yuandianStatus === "fail" && yuandianMsg && (
                    <p className="mt-1.5 text-xs text-red-600">
                      ✗ {yuandianMsg}
                    </p>
                  )}
                  {yuandianStatus === "ok" && (
                    <p className="mt-1.5 text-xs text-green-700">
                      ✓ 已验证通过,可以使用「查被执行人」等元典功能
                    </p>
                  )}
                </Field>
              </Section>
              )}

              {/* ── 大脑:云端 AI 后端 + DeepSeek / MiniMax(切换后只显示所选后端)── */}
              {tab === "brain" && (
                <>
                  <Section title="云端 AI 后端">
                    <Field label="提供商">
                      <select
                        value={settings.cloud_llm_backend ?? "deepseek"}
                        onChange={(e) => handleChangeBackend(e.target.value)}
                        className={inputCls}
                      >
                        {CLOUD_BACKEND_OPTIONS.map((o) => (
                          <option key={o.id} value={o.id}>
                            {o.label}
                          </option>
                        ))}
                      </select>
                      <p className="mt-1 text-label text-muted-foreground">
                        切换后下面只显示所选后端的配置。DeepSeek、MiniMax、GLM、MiMo、自定义模型
                        都各自独立保存,切换服务商不会互相覆盖。
                      </p>
                    </Field>
                  </Section>

                  {(settings.cloud_llm_backend ?? "deepseek") === "deepseek" && (
                  <Section
                    title="DeepSeek"
                    link={{
                      label: "点这里申请 API Key",
                      href: "https://platform.deepseek.com/api_keys",
                    }}
                  >
                    <Field label="API Key">
                      <div className="flex items-center gap-2">
                        <div className="flex-1">
                          <CredentialField
                            label="DeepSeek API Key"
                            locator={CREDENTIAL_LOCATORS.deepseek}
                            status={credentialStatus(CREDENTIAL_LOCATORS.deepseek)}
                            onStatusChange={updateCredentialStatus}
                          />
                        </div>
                        <input
                          type="password"
                          value={credentialDrafts[CREDENTIAL_LOCATORS.deepseek] ?? ""}
                          onChange={(e) => {
                            setCredentialDrafts((current) => ({
                              ...current,
                              [CREDENTIAL_LOCATORS.deepseek]: e.target.value,
                            }));
                            if (deepseekStatus !== "idle") {
                              setDeepseekStatus("idle");
                              setDeepseekMsg("");
                              updateField("deepseek_verified_at", null);
                            }
                          }}
                          placeholder="sk-..."
                          className="hidden"
                          autoComplete="off"
                        />
                        <VerifyStatusIcon status={deepseekStatus} />
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          className="disabled:cursor-not-allowed"
                          onClick={handleVerifyDeepSeek}
                          disabled={
                            deepseekStatus === "verifying" ||
                            !credentialDrafts[CREDENTIAL_LOCATORS.deepseek]?.trim() &&
                            !credentialConfigured(CREDENTIAL_LOCATORS.deepseek)
                          }
                        >
                          {deepseekStatus === "verifying" ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            "验证"
                          )}
                        </Button>
                      </div>
                      {deepseekStatus === "fail" && deepseekMsg && (
                        <p className="mt-1.5 text-xs text-red-600">
                          ✗ {deepseekMsg}
                        </p>
                      )}
                      {deepseekStatus === "ok" && (
                        <p className="mt-1.5 text-xs text-green-700">
                          ✓ 已验证通过,可以使用
                        </p>
                      )}
                    </Field>
                    <Field label="模型档位">
                      <select
                        value={settings.cloud_llm_model ?? "deepseek-v4-flash"}
                        onChange={(e) =>
                          updateField("cloud_llm_model", e.target.value || null)
                        }
                        className={inputCls}
                      >
                        <option value="deepseek-v4-flash">
                          Flash · 便宜快(默认 · 约 Pro 的 1/3 价 · 推荐日常)
                        </option>
                        <option value="deepseek-v4-pro">
                          Pro · 更准更贵(复杂分析/起草可换它)
                        </option>
                        <option value="auto">
                          自动挡 · 简单走 Flash、复杂走 Pro(均衡)
                        </option>
                      </select>
                      <p className="mt-1 text-label text-muted-foreground">
                        全程按这个档位走。Flash 省钱;觉得效果不够就换 Pro 或自动挡。
                      </p>
                    </Field>
                    {/* Endpoint 默认 https://api.deepseek.com,改了反而可能用不了 → 不暴露输入框,
                        cloud_llm_endpoint 留 null,后端按默认走。 */}
                  </Section>
                  )}

                  {(settings.cloud_llm_backend ?? "deepseek") === "minimax" && (
                  <Section
                    title="MiniMax"
                    link={{
                      label: "点这里申请 API Key",
                      href: "https://platform.minimaxi.com/user-center/payment/token-plan",
                    }}
                  >
                    <Field label="API Key">
                      <div className="flex items-center gap-2">
                        <div className="flex-1">
                          <CredentialField
                            label="MiniMax API Key"
                            locator={CREDENTIAL_LOCATORS.minimax}
                            status={credentialStatus(CREDENTIAL_LOCATORS.minimax)}
                            onStatusChange={updateCredentialStatus}
                          />
                        </div>
                        <input
                          type="password"
                          value={credentialDrafts[CREDENTIAL_LOCATORS.minimax] ?? ""}
                          onChange={(e) => {
                            setCredentialDrafts((current) => ({
                              ...current,
                              [CREDENTIAL_LOCATORS.minimax]: e.target.value,
                            }));
                            if (minimaxStatus !== "idle") {
                              setMinimaxStatus("idle");
                              setMinimaxMsg("");
                              updateField("minimax_verified_at", null);
                            }
                          }}
                          placeholder="填入 MiniMax 平台的 API Key"
                          className="hidden"
                          autoComplete="off"
                        />
                        <VerifyStatusIcon status={minimaxStatus} />
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          className="disabled:cursor-not-allowed"
                          onClick={handleVerifyMiniMax}
                          disabled={
                            minimaxStatus === "verifying" ||
                            !credentialDrafts[CREDENTIAL_LOCATORS.minimax]?.trim() &&
                            !credentialConfigured(CREDENTIAL_LOCATORS.minimax)
                          }
                        >
                          {minimaxStatus === "verifying" ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            "验证"
                          )}
                        </Button>
                      </div>
                      {minimaxStatus === "fail" && minimaxMsg && (
                        <p className="mt-1.5 text-xs text-red-600">
                          ✗ {minimaxMsg}
                        </p>
                      )}
                      {minimaxStatus === "ok" && (
                        <p className="mt-1.5 text-xs text-green-700">
                          ✓ 已验证通过,可以使用
                        </p>
                      )}
                    </Field>
                    <Field
                      label="模型档位"
                      hint="按需选择:M2.7 轻量便宜,M2.7-highspeed 速度加倍,M3 强推理(1M 上下文)"
                    >
                      <select
                        value={normalizeMinimaxModel(settings.minimax_model)}
                        onChange={(e) =>
                          updateField("minimax_model", e.target.value)
                        }
                        className={inputCls}
                      >
                        <option value="MiniMax-M2.7">
                          MiniMax-M2.7(轻量档,60 TPS,推荐日常)
                        </option>
                        <option value="MiniMax-M2.7-highspeed">
                          MiniMax-M2.7-highspeed(高速版,100 TPS)
                        </option>
                        <option value="MiniMax-M3">
                          MiniMax-M3(强推理档,1M 上下文,复杂法律分析)
                        </option>
                      </select>
                    </Field>
                    {/* Endpoint 默认 https://api.minimaxi.com;聊天真实路径
                        /v1/text/chatcompletion_v2 由后端自动补 → 不暴露输入框。 */}
                  </Section>
                  )}

                  {/* ── 通用 OpenAI 兼容后端(GLM / MiMo / 自定义)── */}
                  {["glm", "mimo", "custom"].includes(
                    settings.cloud_llm_backend ?? "",
                  ) &&
                    (() => {
                      const cur = isCompatBackend(settings.cloud_llm_backend)
                        ? settings.cloud_llm_backend
                        : "custom";
                      const preset = COMPAT_PRESETS[cur];
                      const keys = COMPAT_FIELD_KEYS[cur];
                      const locator = CREDENTIAL_LOCATORS[cur];
                      const apiKey = credentialDrafts[locator] ?? "";
                      const model = effectiveCompatValue(settings, cur, "model") ?? "";
                      const endpoint =
                        effectiveCompatValue(settings, cur, "endpoint") ??
                        preset.endpoint;
                      // 改 key/模型/地址 → 清验证态(坑#11:改了要重验)
                      const onConfigChange = () => {
                        if (compatStatus !== "idle") {
                          setCompatStatus("idle");
                          setCompatMsg("");
                          updateField(
                            keys.verifiedAt,
                            null,
                          );
                        }
                      };
                      return (
                        <Section
                          title={preset.label}
                          desc="OpenAI 兼容云端 LLM(模型名 / 接口地址都可改;改了请重新验证)"
                          link={
                            preset.applyUrl
                              ? {
                                  label: "申请 / 查看 API Key",
                                  href: preset.applyUrl,
                                }
                              : undefined
                          }
                        >
                          <Field label="API Key">
                            <div className="flex items-center gap-2">
                              <div className="flex-1">
                                <CredentialField
                                  label={`${preset.label} API Key`}
                                  locator={locator}
                                  status={credentialStatus(locator)}
                                  onStatusChange={updateCredentialStatus}
                                />
                              </div>
                              <input
                                type="password"
                                value={apiKey}
                                onChange={(e) => {
                                  setCredentialDrafts((current) => ({
                                    ...current,
                                    [locator]: e.target.value,
                                  }));
                                  onConfigChange();
                                }}
                                placeholder="填入服务商平台的 API Key"
                                className="hidden"
                                autoComplete="off"
                              />
                              <VerifyStatusIcon status={compatStatus} />
                              <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                className="disabled:cursor-not-allowed"
                                onClick={handleVerifyCompat}
                                disabled={
                                  compatStatus === "verifying" ||
                                  !apiKey.trim() &&
                                  !credentialConfigured(locator)
                                }
                              >
                                {compatStatus === "verifying" ? (
                                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                ) : (
                                  "验证"
                                )}
                              </Button>
                            </div>
                            {compatStatus === "fail" && compatMsg && (
                              <p className="mt-1.5 text-xs text-red-600">
                                ✗ {compatMsg}
                              </p>
                            )}
                            {compatStatus === "ok" && (
                              <p className="mt-1.5 text-xs text-green-700">
                                ✓ 已验证通过,可以使用
                              </p>
                            )}
                          </Field>
                          <Field
                            label="模型名"
                            hint="具体型号,以服务商控制台为准(如 glm-4.6 / mimo-v2.5)"
                          >
                            <input
                              type="text"
                              value={model}
                              onChange={(e) => {
                                updateField(
                                  keys.model,
                                  e.target.value || null,
                                );
                                onConfigChange();
                              }}
                              placeholder="如 glm-4.6"
                              className={inputCls}
                              autoComplete="off"
                            />
                          </Field>
                          <Field
                            label="接口地址"
                            hint="OpenAI 兼容的 chat completions 完整地址;只填到 base 会自动补 /v1/chat/completions"
                          >
                            <input
                              type="text"
                              value={endpoint}
                              onChange={(e) => {
                                updateField(
                                  keys.endpoint,
                                  e.target.value || null,
                                );
                                onConfigChange();
                              }}
                              placeholder="https://.../v1/chat/completions"
                              className={inputCls}
                              autoComplete="off"
                            />
                          </Field>
                        </Section>
                      );
                    })()}
                </>
              )}

              {/* ── 功能模型:硅基流动(Embedding 语义检索;留空后端默认 bge-m3 免费)──
                  填了才启用,否则回退关键词选材料。接口地址 / 模型不暴露。 */}
              {tab === "models" && (
              <Section
                title="硅基流动 API"
                desc="Embedding 语义检索 · 云端 API 服务"
                link={{
                  label: "申请 API key",
                  href: "https://cloud.siliconflow.cn/me/account/ak",
                }}
              >
                <Field label="API Key">
                  <div className="flex items-center gap-2">
                    <div className="flex-1">
                      <CredentialField
                        label="Embedding API Key"
                        locator={CREDENTIAL_LOCATORS.embedding}
                        status={credentialStatus(CREDENTIAL_LOCATORS.embedding)}
                        onStatusChange={updateCredentialStatus}
                      />
                    </div>
                    <input
                      type="password"
                      value={credentialDrafts[CREDENTIAL_LOCATORS.embedding] ?? ""}
                      onChange={(e) => {
                        setCredentialDrafts((current) => ({
                          ...current,
                          [CREDENTIAL_LOCATORS.embedding]: e.target.value,
                        }));
                        if (embeddingStatus !== "idle") {
                          setEmbeddingStatus("idle");
                          setEmbeddingMsg("");
                          updateField("embedding_verified_at", null);
                        }
                      }}
                      placeholder="sk-..."
                      className="hidden"
                      autoComplete="off"
                    />
                    <VerifyStatusIcon status={embeddingStatus} />
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="disabled:cursor-not-allowed"
                      onClick={handleVerifyEmbedding}
                      disabled={
                        embeddingStatus === "verifying" ||
                        !credentialDrafts[CREDENTIAL_LOCATORS.embedding]?.trim() &&
                        !credentialConfigured(CREDENTIAL_LOCATORS.embedding)
                      }
                    >
                      {embeddingStatus === "verifying" ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        "验证"
                      )}
                    </Button>
                  </div>
                  {embeddingStatus === "fail" && embeddingMsg && (
                    <p className="mt-1.5 text-xs text-red-600">✗ {embeddingMsg}</p>
                  )}
                  {embeddingStatus === "ok" && (
                    <p className="mt-1.5 text-xs text-green-700">
                      {embeddingMsg || "✓ 已验证通过"}
                    </p>
                  )}
                </Field>
              </Section>
              )}

              {/* ── 知识库:法律向量检索维护(法条+案例+企业语义索引)── */}
              {tab === "kb" && (
                <LocalKbGuideCard configuredRoot={settings.local_kb_root ?? null} />
              )}

              {tab === "brain" && <LegalSkillsSettingsCard />}

              {tab === "kb" && (
              <KbSemanticIndexCard
                embeddingConfigured={credentialConfigured(CREDENTIAL_LOCATORS.embedding)}
                autoIndex={settings.kb_semantic_auto_index !== false}
                onAutoChange={(v) => updateField("kb_semantic_auto_index", v)}
              />
              )}

              {/* 快递100 配置已迁到「法律工具 → 快递查询」页内,就近配置(2026-06-16)。 */}

              {/* ── 数据源:元典本地估算 + 识别服务本地用量 ── */}
              {tab === "datasource" && <YuandianBalanceCard />}

              {tab === "datasource" && (
              <IntegratedUsageDashboard onValidateConnection={handleVerifyYuandian} />
              )}

              {/* ── 知识库:本地知识库三态卡 ── */}
              {tab === "kb" && (
              <LocalKbCard
                kbRoot={settings.local_kb_root ?? null}
                kbEnabled={settings.local_kb_enabled !== false}
                onKbRootChange={(p) => updateField("local_kb_root", p)}
                onKbEnabledChange={(b) => updateField("local_kb_enabled", b)}
              />
              )}

              {tab === "kb" && (
              <IntegratedLocalKbRelocationCard
                configuredRoot={settings.local_kb_root ?? null}
                onRootChanged={(path) => {
                  updateField("local_kb_root", path);
                  updateField("local_kb_enabled", true);
                }}
              />
              )}

              {/* V0.3:本地模型已隐藏 → 删「各模块走本机/云端」切换器 + 本机模型(ollama)配置段。
                  字段(ocr_provider/llm_provider/ollama_*)保留在后端/types,以后接新本地模型再恢复 UI。 */}

              {/* ── 功能开关:首页清爽开关(featureFlags)── */}
              {tab === "toggles" && <CriminalNotificationSettingsCard />}

              {tab === "toggles" && <FeatureFlagsCard />}

              {/* ── 数据源:外部工具(MCP)白名单(企查查/万得/北大法宝 等远程 HTTP)──
                  整宽,AI 助手消费外部 MCP server 工具 */}
              {tab === "datasource" && (
              <SecureMcpServersCard
                servers={settings.mcp_servers ?? []}
                onChange={(next) => updateField("mcp_servers", next)}
              />
              )}


              {/* 错误展示 */}
              {error && (
                <div className="rounded-md border border-destructive/30 bg-destructive/5 p-3 lg:col-span-2">
                  <p className="text-xs font-medium text-destructive">出错了</p>
                  <p className="mt-1 font-mono text-caption text-muted-foreground">
                    {error}
                  </p>
                </div>
              )}
            </div>
            </>
          )}

        </div>

        {/* 底部按钮栏 */}
        <footer className="flex items-center justify-between gap-4 border-t border-border bg-card/95 px-5 py-3 backdrop-blur">
          <span
            className={cn(
              "text-caption",
              saved
                ? "text-green-700 animate-in fade-in-0 duration-200"
                : "text-muted-foreground",
            )}
          >
            {saved
              ? "✓ 已保存 · 下次导入案件时生效(已在跑的任务不切换后端)"
              : settings === null
                ? ""
                : dirty
                  ? "● 有未保存改动 · 别忘了点保存"
                  : "改完点保存"}
          </span>
          <div className="flex gap-2">
            {!isPage && (
              <Button variant="outline" size="sm" onClick={handleClose}>
                取消
              </Button>
            )}
            <Button
              size="sm"
              onClick={handleSave}
              disabled={saving || !settings || (isPage && !dirty)}
            >
              {saving ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Save className="size-3.5" />
              )}
              保存
            </Button>
          </div>
        </footer>
    </>
  );

  if (isPage) {
    return (
      <>
        <div className="mx-auto flex h-full w-full max-w-5xl flex-col overflow-hidden">
          {body}
        </div>
        {privacySummaryOpen && (
          <PrivacySummaryDialog onClose={() => setPrivacySummaryOpen(false)} />
        )}
      </>
    );
  }
  return (
    <>
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-foreground/20 px-4 py-8 backdrop-blur-sm animate-in fade-in-0 duration-200"
        onClick={handleClose}
      >
        <div
          className="flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border bg-card shadow-2xl animate-in zoom-in-95 fade-in-0 duration-300 ease-out"
          onClick={(e) => e.stopPropagation()}
        >
          {body}
        </div>
      </div>
      {privacySummaryOpen && (
        <PrivacySummaryDialog onClose={() => setPrivacySummaryOpen(false)} />
      )}
    </>
  );
}

const PRIVACY_POLICY_URL =
  "https://github.com/fanglv8653/case-board-fanglv/blob/main/PRIVACY.md";

function PrivacySummaryDialog({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-foreground/35 px-4 py-8 backdrop-blur-sm"
      onClick={onClose}
    >
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="privacy-summary-title"
        className="max-h-[85vh] w-full max-w-xl overflow-auto rounded-xl border border-border bg-card shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-start justify-between gap-4 border-b border-border px-5 py-4">
          <div>
            <h3
              id="privacy-summary-title"
              className="flex items-center gap-2 text-base font-semibold text-foreground"
            >
              <ShieldCheck className="size-5 text-sky-700" />
              隐私与数据说明
            </h3>
            <p className="mt-1 text-xs text-muted-foreground">
              以下摘要保存在应用内，无需联网即可阅读。
            </p>
          </div>
          <Button variant="ghost" size="icon" onClick={onClose} aria-label="关闭隐私说明">
            <X className="size-4" />
          </Button>
        </header>

        <div className="space-y-4 px-5 py-4 text-sm text-foreground">
          <PrivacySummaryItem title="本地写入">
            案件、文档索引、设置和运行日志默认写入本机应用数据目录。请自行管理设备权限、备份与删除。
          </PrivacySummaryItem>
          <PrivacySummaryItem title="云端 LLM / OCR">
            只有在你配置并使用云端大模型或 OCR 时，所选材料及完成任务所需的上下文才可能发送给对应服务商处理。提交前请确认材料可外发并尽量脱敏。
          </PrivacySummaryItem>
          <PrivacySummaryItem title="第三方集成">
            元典、MCP 等第三方能力仅在你配置并主动使用时连接；相关服务按各自规则处理查询内容、访问令牌和返回结果。
          </PrivacySummaryItem>
          <PrivacySummaryItem title="条件遥测">
            应用不以广告追踪为目的。只有构建版本在编译时启用遥测且具备必要配置时，才会发送粗粒度运行诊断；更新检查等外部请求按你实际启用和触发的功能发生。
          </PrivacySummaryItem>
          <PrivacySummaryItem title="反馈不会自动发送">
            只有你主动生成时，应用才会在桌面创建本地 Markdown 反馈文件。请先预览并移除不应外发的案件信息和个人信息，再自行决定是否通过应用外渠道发送；应用本身不自动上传反馈。
          </PrivacySummaryItem>
        </div>

        <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-border px-5 py-3">
          <button
            type="button"
            onClick={() =>
              openUrl(PRIVACY_POLICY_URL).catch((e) => console.warn("openUrl failed", e))
            }
            className="inline-flex items-center gap-1.5 text-xs font-medium text-sky-700 hover:text-sky-800"
          >
            <ExternalLink className="size-3.5" />
            查看完整 PRIVACY.md（需联网）
          </button>
          <Button type="button" size="sm" onClick={onClose}>
            我已了解
          </Button>
        </footer>
      </section>
    </div>
  );
}

function PrivacySummaryItem({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-border bg-background/60 px-3 py-2.5">
      <h4 className="text-sm font-semibold text-foreground">{title}</h4>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">{children}</p>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* 小组件                                                              */
/* ------------------------------------------------------------------ */

const inputCls = cn(
  "h-9 w-full rounded-md border border-border bg-background px-3 text-sm",
  "placeholder:text-muted-foreground/60",
  "transition-[border-color,box-shadow]",
  "focus:outline-none focus:border-foreground focus:ring-1 focus:ring-foreground/20",
);

/**
 * MiniMax 模型档位归一:历史 settings 里可能是 "minimax-M3"(小写 m)或 null/空,
 * select 的 value 必须精确匹配某个 option.value 才能高亮。旧的 "MiniMax-M2" 已被
 * M2.7 取代,统一归并到 M2.7(后端 null 默认也已对齐 M2.7)。无法识别的值(用户
 * 手填过的)原样返回 —— select 不高亮,用户重选一次即可。
 */
function normalizeMinimaxModel(raw: string | null | undefined): string {
  if (!raw) return "MiniMax-M2.7";
  const lower = raw.trim().toLowerCase();
  if (lower === "minimax-m2.7" || lower === "minimax-m2") return "MiniMax-M2.7";
  if (lower === "minimax-m2.7-highspeed") return "MiniMax-M2.7-highspeed";
  if (lower === "minimax-m3") return "MiniMax-M3";
  return raw;
}

/** 验证状态图标:ok=绿勾 / fail=红叉 / 其他=不显示 */
function VerifyStatusIcon({ status }: { status: VerifyStatus }) {
  if (status === "ok") {
    return <CheckCircle2 className="h-5 w-5 shrink-0 text-green-600" aria-label="已验证" />;
  }
  if (status === "fail") {
    return <XCircle className="h-5 w-5 shrink-0 text-red-500" aria-label="验证失败" />;
  }
  return null;
}

function Section({
  title,
  desc,
  link,
  children,
  fill,
}: {
  title: string;
  desc?: string;
  link?: { label: string; href: string };
  children: React.ReactNode;
  /** true 时撑满网格行高(同一排卡片等高)。默认 false = 自然紧凑高度。 */
  fill?: boolean;
}) {
  return (
    <section className={fill ? "flex h-full flex-col" : undefined}>
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-foreground">{title}</h3>
          {desc && <p className="mt-0.5 text-xs text-muted-foreground">{desc}</p>}
        </div>
        {link && (
          <button
            type="button"
            onClick={() => openUrl(link.href).catch((e) => console.warn("openUrl failed", e))}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-sky-200 bg-sky-50 px-2.5 py-1 text-xs font-medium text-sky-700 transition-colors hover:border-sky-300 hover:bg-sky-100"
            title={link.href}
          >
            <ExternalLink className="size-3.5" />
            {link.label}
          </button>
        )}
      </div>
      {/* 默认自然高度(配对相近高度卡 + items-start 不留空);fill=true 时撑满行高(同排等高) */}
      <div
        className={cn(
          "space-y-3 rounded-lg border border-border bg-background/50 p-4",
          fill && "flex-1",
        )}
      >
        {children}
      </div>
    </section>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs font-medium text-foreground">
        {label}
      </span>
      {children}
      {hint && (
        <span className="mt-1 block text-caption text-muted-foreground">
          {hint}
        </span>
      )}
    </label>
  );
}

/** 刑事任务系统通知。偏好只保存在本机，权限申请只由用户点击触发。 */
function CriminalNotificationSettingsCard() {
  const [enabled, setEnabled] = useState(isCriminalNotificationEnabled);
  const [permission, setPermission] =
    useState<CriminalNotificationPermission>("denied");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    const sync = () => setEnabled(isCriminalNotificationEnabled());
    window.addEventListener(CRIMINAL_NOTIFICATION_SETTINGS_EVENT, sync);
    void getCriminalNotificationPermission().then(setPermission);
    return () => {
      window.removeEventListener(CRIMINAL_NOTIFICATION_SETTINGS_EVENT, sync);
    };
  }, []);

  async function handleToggle() {
    setMessage(null);
    if (enabled) {
      disableCriminalNotifications();
      setPermission(await getCriminalNotificationPermission());
      setMessage("自动提醒已关闭，应用内任务列表仍可正常使用。");
      return;
    }

    setBusy(true);
    const result = await enableCriminalNotifications();
    setPermission(result);
    setMessage(
      result === "granted"
        ? "已开启。应用运行期间会扫描并发送到期提醒。"
        : result === "denied"
          ? "系统未授予通知权限，自动提醒保持关闭；应用内提醒不受影响。"
          : "无法访问系统通知，请检查 Windows 通知设置后重试。",
    );
    setBusy(false);
  }

  async function handleTest() {
    setBusy(true);
    const result = await sendCriminalNotificationTest();
    setPermission(result);
    setMessage(
      result === "granted"
        ? "测试提醒已交给 Windows；系统是否展示仍受通知设置和专注模式影响。"
        : result === "denied"
          ? "系统未授予通知权限，未发送测试提醒。"
          : "测试提醒发送失败，请检查 Windows 通知设置。",
    );
    setBusy(false);
  }

  return (
    <Section
      title="刑事案件 Windows 提醒"
      desc="提醒仅在案件看板运行期间扫描和发送；关闭应用后不会驻留后台。"
      fill
    >
      <div className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5">
        <div className="flex min-w-0 items-start gap-2.5">
          <Bell className="mt-0.5 size-4 shrink-0 text-sky-600" />
          <div className="space-y-1">
            <p className="text-sm font-medium text-foreground">
              {enabled ? "自动提醒已开启" : "自动提醒已关闭"}
            </p>
            <p className="text-label text-muted-foreground">
              只有点击开启或发送测试提醒时才会申请系统权限；权限拒绝后不会后台循环弹窗。
            </p>
          </div>
        </div>
        <button
          type="button"
          role="switch"
          aria-label="刑事案件 Windows 自动提醒"
          aria-checked={enabled}
          disabled={busy}
          onClick={() => void handleToggle()}
          className={cn(
            "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors disabled:opacity-50",
            enabled ? "bg-emerald-600" : "bg-muted",
          )}
        >
          <span
            className={cn(
              "inline-block size-4 rounded-full bg-white shadow transition-transform",
              enabled ? "translate-x-4.5" : "translate-x-0.5",
            )}
          />
        </button>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={busy}
          onClick={() => void handleTest()}
        >
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Bell className="size-3.5" />}
          发送测试提醒
        </Button>
        <span className="text-caption text-muted-foreground">
          权限状态：
          {permission === "granted"
            ? "已授权"
            : permission === "denied"
              ? "未授权"
              : "不可用"}
        </span>
      </div>
      {message && (
        <p className="rounded-md bg-muted px-2.5 py-2 text-caption text-muted-foreground">
          {message}
        </p>
      )}
      <p className="text-label leading-relaxed text-muted-foreground">
        数据库中的“已发送”只表示通知已交付给系统，不代表用户已经阅读；案件内提醒和办理记录始终保留。
      </p>
    </Section>
  );
}

/**
 * 2026-06-16 · 首页功能开关卡(「功能开关」tab)。
 * 作者偏好清爽首页:新功能默认关,想用再开,逐设备生效(localStorage)。
 * 只注册并渲染已有真实控制对象的开关；缺失能力不得先放空开关。
 */
function FeatureFlagsCard() {
  const flags = FEATURE_FLAGS;
  if (flags.length === 0) return null;
  return (
    <Section
      title="界面功能开关"
      desc="只列出已经接入真实界面的功能。切换即时生效，仅保存在这台机器，不改案件数据。"
    >
      <div className="space-y-1">
        {flags.map((f) => (
          <FeatureFlagToggle key={f.name} name={f.name} />
        ))}
      </div>
    </Section>
  );
}

function FeatureFlagToggle({
  name,
}: {
  name: (typeof FEATURE_FLAGS)[number]["name"];
}) {
  const [on, setOn] = useFeatureFlag(name);
  const meta = FEATURE_FLAGS.find((f) => f.name === name)!;
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-background/50 p-3">
      <div className="min-w-0">
        <p className="text-sm font-medium text-foreground">{meta.title}</p>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {meta.description}
        </p>
        <p className="mt-1 text-label font-medium text-muted-foreground">
          {on ? "已开启并即时生效" : "已关闭"}
        </p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={on}
        aria-label={meta.title}
        onClick={() => setOn(!on)}
        className={cn(
          "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
          on ? "bg-primary" : "bg-muted",
        )}
      >
        <span
          className={cn(
            "inline-block size-4 rounded-full bg-white shadow transition-transform",
            on ? "translate-x-4" : "translate-x-0.5",
          )}
        />
      </button>
    </div>
  );
}

function ThemeCard() {
  const [theme, setTheme] = useThemePreference();
  return (
    <section className="lg:col-span-2" aria-labelledby="settings-theme-title">
      <div className="mb-3">
        <h3 id="settings-theme-title" className="text-sm font-semibold text-foreground">
          界面主题
        </h3>
        <p className="mt-0.5 text-xs text-muted-foreground">
          即时生效并保存在本机，不依赖页面底部“保存”，也不会进入设备同步。
        </p>
      </div>
      <div className="grid gap-3 rounded-lg border border-border bg-background/50 p-4 md:grid-cols-2">
        {THEMES.map((item) => {
          const selected = theme === item.id;
          return (
            <button
              key={item.id}
              type="button"
              aria-pressed={selected}
              onClick={() => setTheme(item.id)}
              className={cn(
                "surface-interactive rounded-lg border p-4 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
                selected
                  ? "border-primary bg-accent/70 shadow-sm"
                  : "border-border bg-card/70 hover:border-primary/40",
              )}
            >
              <span className="flex items-start justify-between gap-3">
                <span>
                  <span className="flex gap-1.5" aria-hidden="true">
                    {item.swatches.map((color) => (
                      <span
                        key={color}
                        className="size-5 rounded-full border border-black/10"
                        style={{ backgroundColor: color }}
                      />
                    ))}
                  </span>
                  <span className="mt-3 block text-sm font-semibold text-foreground">
                    {item.label}
                  </span>
                </span>
                <span
                  className={cn(
                    "mt-0.5 inline-flex size-5 items-center justify-center rounded-full border text-xs",
                    selected
                      ? "border-primary bg-primary text-primary-foreground"
                      : "border-border text-transparent",
                  )}
                  aria-hidden="true"
                >
                  ✓
                </span>
              </span>
              <span className="mt-1.5 block text-xs leading-5 text-muted-foreground">
                {item.description}
              </span>
            </button>
          );
        })}
        <p className="text-label text-muted-foreground md:col-span-2">
          暗色模式继续使用方律当前暗色配色；“墨绿象牙”仅调整亮色界面。
        </p>
      </div>
    </section>
  );
}

/**
 * 2026-06-16 · 界面字号微调卡(「通用」tab)。有用户反映字小 → 全局等比缩放
 * (改根字号,Tailwind rem 单位连字带间距一起放大)。逐设备 localStorage,实时生效。
 */
function FontScaleCard() {
  const [scale, setScale] = useFontScale();
  const pct = Math.round(scale * 100);
  const presets: { label: string; v: number }[] = [
    { label: "小", v: 0.9 },
    { label: "标准", v: 1.0 },
    { label: "大", v: 1.15 },
    { label: "特大", v: 1.3 },
  ];
  return (
    <Section
      title="界面字号"
      desc="觉得字小就调大 —— 整个界面(文字 + 间距)等比缩放。只影响这台机器,随时可调。"
    >
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">当前缩放</span>
          <span className="text-sm font-semibold text-foreground">{pct}%</span>
        </div>
        <input
          type="range"
          min={FONT_SCALE.MIN}
          max={FONT_SCALE.MAX}
          step={FONT_SCALE.STEP}
          value={scale}
          onChange={(e) => setScale(parseFloat(e.target.value))}
          className="w-full accent-sky-600"
          aria-label="界面字号缩放"
        />
        <div className="flex flex-wrap items-center gap-1.5">
          {presets.map((p) => (
            <button
              key={p.label}
              type="button"
              onClick={() => setScale(p.v)}
              className={cn(
                "rounded-md border px-2.5 py-1 text-xs font-medium transition-colors",
                Math.abs(scale - p.v) < 0.001
                  ? "border-sky-300 bg-sky-50 text-sky-700"
                  : "border-border text-muted-foreground hover:bg-accent hover:text-foreground",
              )}
            >
              {p.label}
            </button>
          ))}
          <button
            type="button"
            onClick={() => setScale(FONT_SCALE.DEFAULT)}
            className="ml-auto rounded-md border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          >
            重置
          </button>
        </div>
        <p className="rounded-md bg-muted/40 px-3 py-2 text-xs text-foreground">
          示例:这行字会随缩放即时变大变小,调到看着舒服为止。
        </p>
      </div>
    </Section>
  );
}

// =============================================================================
// V0.3.6 · 外部工具(MCP)配置卡(整宽)
// CaseBoard 当 MCP **客户端**,把外部 MCP server 暴露的工具并入 AI 助手 —— 加能力 = 配一个
// server、热加载,不必改 Rust 重出 dmg。当前只支持 stdio 子进程(npx 等);默认空 = 桥接关闭。
// 详 docs/adr/0008。注意:产出的 transport 形状必须跟后端 serde 完全一致,否则整次保存会失败。
// =============================================================================

const mcpTextareaCls = cn(
  "w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-xs leading-relaxed",
  "placeholder:text-muted-foreground/60",
  "transition-[border-color,box-shadow]",
  "focus:outline-none focus:border-foreground focus:ring-1 focus:ring-foreground/20",
);

function SecureMcpServersCard({
  servers,
  onChange,
}: {
  servers: McpServerConfig[];
  onChange: (next: McpServerConfig[]) => void;
}) {
  const [rows, setRows] = useState(servers);
  const [pasteText, setPasteText] = useState("");
  const [pasteBusy, setPasteBusy] = useState(false);
  const [message, setMessage] = useState<{ error: boolean; lines: string[] } | null>(null);

  useEffect(() => setRows(servers), [servers]);

  function commit(next: McpServerConfig[]) {
    setRows(next);
    onChange(next);
  }

  async function importPaste() {
    if (!pasteText.trim() || pasteBusy) return;
    setPasteBusy(true);
    setMessage(null);
    try {
      const result = await parseMcpPaste(pasteText);
      commit(result.servers);
      setPasteText("");
      setMessage({
        error: false,
        lines: ["配置已安全导入；敏感值未返回界面。", ...result.warnings],
      });
    } catch (error) {
      setMessage({ error: true, lines: [String(error)] });
    } finally {
      setPasteBusy(false);
    }
  }

  return (
    <div className="lg:col-span-2">
      <section>
        <div className="mb-3 flex items-start justify-between gap-3">
          <div>
            <h3 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
              <Plug className="size-4 text-muted-foreground" />
              外部工具（MCP）
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              配置中的令牌、请求头、敏感 URL 与环境变量会立即迁入 Windows
              凭据管理器。界面和 settings.json 只保存稳定 UUID、定位符及“已配置”状态。
            </p>
          </div>
          <button
            type="button"
            onClick={() =>
              openUrl("https://github.com/modelcontextprotocol/servers").catch((error) =>
                console.warn("openUrl failed", error),
              )
            }
            className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-sky-200 bg-sky-50 px-2.5 py-1 text-xs font-medium text-sky-700 hover:bg-sky-100"
          >
            <ExternalLink className="size-3.5" />
            看可用 server
          </button>
        </div>

        <div className="space-y-3 rounded-lg border border-border bg-background/50 p-4">
          <div className="rounded-md border border-sky-200 bg-sky-50/50 p-3">
            <p className="mb-1.5 text-xs font-medium text-sky-900">
              粘贴平台接入配置。导入成功后输入框会立即清空，敏感值不会回填。
            </p>
            <textarea
              rows={3}
              value={pasteText}
              onChange={(event) => setPasteText(event.target.value)}
              placeholder="粘贴 mcpServers JSON 或 claude mcp add 命令"
              className={mcpTextareaCls}
              spellCheck={false}
              autoComplete="off"
            />
            <button
              type="button"
              onClick={importPaste}
              disabled={pasteBusy || !pasteText.trim()}
              className="mt-2 rounded-md bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-700 disabled:opacity-50"
            >
              {pasteBusy ? "安全导入中…" : "安全导入"}
            </button>
            {message && (
              <div className={cn("mt-2 text-xs", message.error ? "text-red-600" : "text-emerald-700")}>
                {message.lines.map((line, index) => (
                  <p key={index}>{line}</p>
                ))}
              </div>
            )}
          </div>

          {rows.length === 0 ? (
            <p className="text-xs text-muted-foreground">尚未配置外部工具。</p>
          ) : (
            rows.map((server) => (
              <SecureMcpServerRow key={server.server_id} server={server} onChange={commit} />
            ))
          )}
        </div>
      </section>
    </div>
  );
}

function SecureMcpServerRow({
  server,
  onChange,
}: {
  server: McpServerConfig;
  onChange: (next: McpServerConfig[]) => void;
}) {
  const [name, setName] = useState(server.name);
  const [enabled, setEnabled] = useState(server.enabled);
  const [busy, setBusy] = useState<"save" | "test" | "delete" | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    setName(server.name);
    setEnabled(server.enabled);
  }, [server]);

  async function saveMetadata() {
    setBusy("save");
    setStatus(null);
    try {
      onChange(await updateMcpServerMetadata(server.server_id, name, enabled));
      setStatus("名称和启用状态已保存；凭据定位符保持不变。");
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(null);
    }
  }

  async function runTest() {
    setBusy("test");
    setStatus(null);
    try {
      const report = await testMcpServer(server.server_id);
      setStatus(`连接成功，发现 ${report.tool_count} 个工具。`);
    } catch (error) {
      setStatus(`连接失败：${String(error)}`);
    } finally {
      setBusy(null);
    }
  }

  async function remove() {
    const confirmed = await confirmDialog("删除该 MCP server 及其本机凭据？此操作不可撤销。", {
      title: "删除 MCP server",
      okLabel: "删除",
      danger: true,
    });
    if (!confirmed) return;
    setBusy("delete");
    try {
      onChange(await deleteMcpServer(server.server_id));
    } catch (error) {
      setStatus(String(error));
      setBusy(null);
    }
  }

  const secretEntries =
    server.transport.type === "http"
      ? Object.entries(server.transport.headers)
      : Object.entries(server.transport.env);
  return (
    <div className={cn("rounded-md border border-border bg-card/60 p-3", !enabled && "opacity-70")}>
      <div className="flex flex-wrap items-center gap-2">
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          className={cn(inputCls, "min-w-52 flex-1")}
          aria-label="MCP server 名称"
        />
        <label className="flex items-center gap-1.5 text-xs">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
            className="accent-sky-600"
          />
          启用
        </label>
        <button type="button" onClick={saveMetadata} disabled={busy !== null} className="rounded-md border px-2.5 py-1 text-xs">
          {busy === "save" ? "保存中…" : "保存"}
        </button>
        <button type="button" onClick={runTest} disabled={busy !== null} className="rounded-md border border-sky-200 bg-sky-50 px-2.5 py-1 text-xs text-sky-700">
          {busy === "test" ? "测试中…" : "测试连接"}
        </button>
        <button type="button" onClick={remove} disabled={busy !== null} className="rounded-md p-1.5 text-muted-foreground hover:text-destructive" aria-label="删除 MCP server">
          <Trash2 className="size-4" />
        </button>
      </div>

      <div className="mt-2 space-y-1 text-caption text-muted-foreground">
        <p>稳定 ID：{server.server_id}</p>
        <p>
          {server.transport.type === "http"
            ? server.transport.url.kind === "secret"
              ? "服务地址：已安全保存"
              : `服务地址：${server.transport.url.value}`
            : `本地命令：${server.transport.command}`}
        </p>
        {secretEntries.length > 0 && (
          <p>
            {server.transport.type === "http" ? "请求头" : "环境变量"}：
            {secretEntries
              .map(([key, reference]) => `${key}（${reference.configured ? "已配置" : "未配置"}）`)
              .join("、")}
          </p>
        )}
        <p>凭据集合：{server.complete.configured ? "完整" : "不完整（运行时将拒绝连接）"}</p>
      </div>
      {status && <p className="mt-2 text-xs text-muted-foreground">{status}</p>}
    </div>
  );
}

// =============================================================================
// V0.2 D7 · 本地知识库三态卡 + 元典积分卡
// =============================================================================

/** macOS Documents/Desktop 权限被拒时,这个 URL 直接打开系统设置 → 文件与文件夹 */
const MACOS_PRIVACY_FILES_URL =
  "x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders";
const DEFAULT_KB_PATH = "~/Documents/知识库";

function LocalKbCard({
  kbRoot,
  kbEnabled,
  onKbRootChange,
  onKbEnabledChange,
}: {
  kbRoot: string | null;
  kbEnabled: boolean;
  onKbRootChange: (p: string | null) => void;
  onKbEnabledChange: (b: boolean) => void;
}) {
  const [status, setStatus] = useState<KbStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [busyMsg, setBusyMsg] = useState("");
  const [importResult, setImportResult] = useState<KbImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await detectKbStatus();
      setStatus(s);
      setError(null);
    } catch (e) {
      setError(formatErr(e));
    }
  }, []);

  // 打开 Settings + kbRoot/kbEnabled 变化时重新检测
  useEffect(() => {
    refresh();
  }, [refresh, kbRoot, kbEnabled]);

  async function handleCreateDefault() {
    await handleCreate(DEFAULT_KB_PATH);
  }

  async function handleChoosePath() {
    try {
      const picked = await dialogOpen({ directory: true, multiple: false });
      if (typeof picked === "string" && picked.trim()) {
        await handleCreate(picked);
      }
    } catch (e) {
      setError(formatErr(e));
    }
  }

  async function handleCreate(path: string) {
    setBusy(true);
    setBusyMsg("创建中…");
    setError(null);
    try {
      const r = await createLocalKb(path);
      onKbRootChange(path);
      onKbEnabledChange(true);
      setBusyMsg(
        r.reused_existing
          ? `已绑定到已有目录(补 ${r.dirs_created} 个子目录)`
          : `新建成功(${r.dirs_created} 目录 / ${r.files_created} 文件)`,
      );
      await refresh();
    } catch (e) {
      setError(formatErr(e));
    } finally {
      setBusy(false);
      window.setTimeout(() => setBusyMsg(""), 3000);
    }
  }

  async function handleImport() {
    setError(null);
    setImportResult(null);
    try {
      const picked = await dialogOpen({
        directory: false,
        multiple: false,
        filters: [{ name: "CaseBoard KB 资料包", extensions: ["zip"] }],
      });
      if (typeof picked !== "string" || !picked.trim()) return;
      setBusy(true);
      setBusyMsg("导入中…");
      // 默认 OverwriteOlder(智能合并 — 旧的覆盖,新的保留)
      const strategy: KbConflictStrategy = "overwrite_older";
      const r = await importKbFromZip(picked, strategy);
      setImportResult(r);
      setBusyMsg(
        `导入完成:新增 ${r.added} / 跳过 ${r.skipped} / 覆盖 ${r.overwritten}${r.failed ? ` / 失败 ${r.failed}` : ""}`,
      );
      await refresh();
    } catch (e) {
      setError(formatErr(e));
    } finally {
      setBusy(false);
      window.setTimeout(() => setBusyMsg(""), 5000);
    }
  }

  async function handleExport() {
    setError(null);
    try {
      const today = new Date().toISOString().slice(0, 10);
      const picked = await dialogSave({
        defaultPath: `caseboard-kb-share-${today}.zip`,
        filters: [{ name: "Zip", extensions: ["zip"] }],
      });
      if (typeof picked !== "string" || !picked.trim()) return;
      setBusy(true);
      setBusyMsg("导出中…");
      const r = await exportKbToZip(picked);
      setBusyMsg(
        `导出完成 · ${r.total_items} 条 · ${formatBytes(r.total_size_bytes)}`,
      );
    } catch (e) {
      setError(formatErr(e));
    } finally {
      setBusy(false);
      window.setTimeout(() => setBusyMsg(""), 5000);
    }
  }

  // P2 · 清理过期检索缓存(只清搜索/向量列表,法规/案例/企业全文详情不动)。需二次确认。
  async function handlePruneCache() {
    setError(null);
    const ok = await confirmDialog(
      "清理 30 天前的检索列表缓存(法规/案例关键词检索、语义检索结果)?\n\n法规/法条/案例的全文详情、入库的企业档案都不会动,放心清。",
      { title: "清理过期检索缓存", okLabel: "清理", danger: true },
    );
    if (!ok) return;
    try {
      setBusy(true);
      setBusyMsg("清理中…");
      const r = await pruneYuandianCache(30);
      setBusyMsg(
        r.removed_entries === 0
          ? "没有 30 天前的检索缓存可清"
          : `已清理 ${r.removed_entries} 条过期检索缓存(删 ${r.removed_files} 个文件)`,
      );
      await refresh();
    } catch (e) {
      setError(formatErr(e));
    } finally {
      setBusy(false);
      window.setTimeout(() => setBusyMsg(""), 5000);
    }
  }

  return (
    <Section
      title="本地法律知识库"
      desc="启用后,法律检索优先查本地缓存,只在缺时调元典 — 大幅省积分。"
    >
      {/* 状态条 */}
      <div className="rounded-md border border-border bg-background p-3">
        {status === null && (
          <p className="text-xs text-muted-foreground">
            <Loader2 className="mr-1 inline size-3 animate-spin" />
            检测中…
          </p>
        )}

        {status?.state === "bound" && (
          <div className="space-y-2">
            <div className="flex items-center justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <Database className="size-4 shrink-0 text-emerald-600" />
                <span className="truncate text-xs font-medium">
                  ✓ 已绑定 <span className="font-mono">{status.root}</span>
                </span>
              </div>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={refresh}
                title="重新检测"
                disabled={busy}
              >
                <RefreshCw className={cn("size-3.5", busy && "animate-spin")} />
              </Button>
            </div>
            <KbStatsRow status={status} />
            <div className="flex flex-wrap gap-1.5 pt-1">
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => openInDefaultApp(status.root)}
                disabled={busy}
              >
                <FolderOpen className="size-3.5" />
                打开目录
              </Button>
              <HoverHint hint="导入同事的元典缓存资料包,自动查重合并;只合并元典缓存,不碰你的笔记/案件/客户">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={handleImport}
                  disabled={busy}
                >
                  <Upload className="size-3.5" />
                  导入资料包
                </Button>
              </HoverHint>
              <HoverHint
                hint={
                  status.cache_count === 0
                    ? "知识库还没缓存,无内容可导(本功能仅导出元典缓存)"
                    : "仅导出元典缓存(法规/案例/企业查询结果),不含你的笔记/案件/客户信息"
                }
              >
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={handleExport}
                  disabled={busy || status.cache_count === 0}
                >
                  <Download className="size-3.5" />
                  导出资料包
                </Button>
              </HoverHint>
              <HoverHint hint="清理 30 天前的法规/案例检索列表 + 语义检索结果(全文详情、企业档案不动);去冗余、腾空间。需二次确认">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={handlePruneCache}
                  disabled={busy || status.cache_count === 0}
                >
                  <Trash2 className="size-3.5" />
                  清理过期缓存
                </Button>
              </HoverHint>
            </div>
          </div>
        )}

        {status?.state === "unbound" && (
          <div className="space-y-2.5">
            <div className="flex items-center gap-2 text-xs">
              <AlertTriangle className="size-4 shrink-0 text-amber-500" />
              <span className="font-medium">未检测到本地知识库</span>
              {status.configured_root && (
                <span className="text-muted-foreground">
                  · 默认路径 <span className="font-mono">{status.configured_root}</span> 不存在
                </span>
              )}
            </div>
            <p className="text-label text-muted-foreground">
              本地知识库让法律检索先查本地、只在缺时调元典,大幅节省积分。
            </p>
            <div className="flex flex-wrap gap-1.5">
              <Button
                type="button"
                size="sm"
                onClick={handleCreateDefault}
                disabled={busy}
              >
                <Sparkles className="size-3.5" />
                在 {DEFAULT_KB_PATH} 新建
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={handleChoosePath}
                disabled={busy}
              >
                <FolderOpen className="size-3.5" />
                选择其他路径…
              </Button>
              <HoverHint hint="需先新建或选定一个知识库目录再导入。导入的是元典缓存资料包,不含笔记/案件/客户">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={handleImport}
                  disabled={busy}
                >
                  <Upload className="size-3.5" />
                  导入资料包
                </Button>
              </HoverHint>
            </div>
          </div>
        )}

        {status?.state === "permission_denied" && (
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-xs">
              <AlertTriangle className="size-4 shrink-0 text-red-600" />
              <span className="font-medium">
                🔒 <span className="font-mono">{status.root}</span> 存在,但 CaseBoard 无访问权限
              </span>
            </div>
            <p className="text-label text-muted-foreground">
              请到 系统设置 → 隐私与安全 → 文件与文件夹 → CaseBoard → 勾选"文稿"。
            </p>
            <div className="flex flex-wrap gap-1.5">
              <Button
                type="button"
                size="sm"
                onClick={() => openUrl(MACOS_PRIVACY_FILES_URL).catch(() => {})}
              >
                <ExternalLink className="size-3.5" />
                打开系统设置
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={refresh}
                disabled={busy}
              >
                <RefreshCw className="size-3.5" />
                重新检查
              </Button>
            </div>
          </div>
        )}
      </div>

      {/* busy / 错误 / 导入摘要 */}
      {busyMsg && (
        <p className="text-xs text-muted-foreground">
          {busy && <Loader2 className="mr-1 inline size-3 animate-spin" />}
          {busyMsg}
        </p>
      )}
      {error && (
        <p className="text-xs text-red-600">
          <XCircle className="mr-1 inline size-3" />
          {error}
        </p>
      )}
      {importResult && importResult.conflicts.length > 0 && (
        <details className="text-xs text-muted-foreground">
          <summary className="cursor-pointer">查看 {importResult.conflicts.length} 条冲突明细</summary>
          <ul className="mt-1 max-h-32 space-y-0.5 overflow-y-auto pl-3">
            {importResult.conflicts.slice(0, 50).map((c, i) => (
              <li key={i} className="font-mono text-caption">
                <span
                  className={cn(
                    c.action === "failed" && "text-red-600",
                    c.action === "overwrite" && "text-amber-600",
                  )}
                >
                  [{c.action}]
                </span>{" "}
                {c.path} — {c.reason}
              </li>
            ))}
          </ul>
        </details>
      )}

      {/* 高级:路径手填 + 总开关 */}
      <details className="text-xs">
        <summary className="cursor-pointer text-muted-foreground">高级设置</summary>
        <div className="mt-2 space-y-2 rounded border border-border bg-background/50 p-2.5">
          <Field label="知识库路径(手填,支持 ~/)">
            <input
              type="text"
              value={kbRoot ?? ""}
              onChange={(e) => onKbRootChange(e.target.value || null)}
              placeholder="~/Documents/知识库"
              className={cn(inputCls, "font-mono")}
            />
          </Field>
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={kbEnabled}
              onChange={(e) => onKbEnabledChange(e.target.checked)}
              className="size-3.5"
            />
            <span>启用本地优先(关闭后所有检索直接调元典)</span>
          </label>
        </div>
      </details>
    </Section>
  );
}

function KbStatsRow({
  status,
}: {
  status: Extract<KbStatus, { state: "bound" }>;
}) {
  const breakdownText = Object.entries(status.cache_breakdown)
    .filter(([, n]) => n > 0)
    .map(([k, n]) => `${k} ${n}`)
    .join(" / ");
  return (
    <ul className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-label text-muted-foreground">
      <li>
        已检索内容:
        <span className="ml-1 font-medium text-foreground">
          {status.content_count} 篇
        </span>
      </li>
      <li>
        元典缓存:
        <span className="ml-1 font-medium text-foreground">
          {status.cache_count}
        </span>
        {breakdownText && (
          <span className="text-muted-foreground/70"> ({breakdownText})</span>
        )}
      </li>
      <li>
        占用:
        <span className="ml-1 font-medium text-foreground">
          {status.total_size_bytes != null
            ? formatBytes(status.total_size_bytes)
            : "—"}
        </span>
      </li>
      <li className="col-span-2">
        最近写入:
        <span className="ml-1 font-medium text-foreground">
          {status.last_write_at ? formatDateTime(status.last_write_at) : "—"}
        </span>
      </li>
    </ul>
  );
}

function IntegratedUsageDashboard({
  onValidateConnection,
}: {
  onValidateConnection: () => Promise<boolean>;
}) {
  const [snapshot, setSnapshot] = useState<LocalUsageSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const refreshLocal = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const today = localDateKey();
      const month = today.slice(0, 7);
      const [daily, monthly, yuandian] = await Promise.all([
        getLocalRecognitionUsage({ granularity: "day", from: today, to: today }),
        getLocalRecognitionUsage({ granularity: "month", from: month, to: month }),
        refreshYuandianLocalUsage(),
      ]);
      const mapMetrics = (
        period: "day" | "month",
        overview: typeof daily,
      ): LocalUsageMetric[] =>
        overview.buckets.map((bucket) => ({
          period,
          provider: bucket.providerModel,
          stage: bucket.stage,
          success_count: bucket.successCount,
          failure_count: bucket.failureCount,
          average_elapsed_ms: bucket.averageElapsedMs,
          rate_limited_429_count: bucket.rateLimit429Count,
          page_count: bucket.pageCount,
          page_count_unavailable_reason: overview.capabilities.pageCountAvailable
            ? null
            : overview.capabilities.pageCountReason,
          fallback_count: bucket.fallbackCount,
          fallback_unavailable_reason: overview.capabilities.fallbackCountAvailable
            ? null
            : overview.capabilities.fallbackCountReason,
        }));
      setSnapshot({
        metrics: [...mapMetrics("day", daily), ...mapMetrics("month", monthly)],
        last_refreshed_at: yuandian.refreshedAt,
        official_balance: yuandian.officialBalance,
        official_balance_unavailable_reason: yuandian.isOfficialBalance
          ? null
          : "未提供官方余额接口",
        yuandian_estimate: yuandian.hasAnyRecord
          ? {
              year_month: yuandian.current.year_month,
              estimated_credits: yuandian.current.credits_used,
              recorded_api_calls: yuandian.current.api_calls,
              local_kb_hits: yuandian.current.kb_hits,
              total_estimated_credits: yuandian.totalEstimatedCredits,
              estimate_basis: yuandian.estimateBasis,
              has_any_record: yuandian.hasAnyRecord,
            }
          : null,
      });
    } catch (error) {
      setLoadError(formatErr(error));
      throw error;
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refreshLocal().catch(() => undefined);
  }, [refreshLocal]);

  return (
    <div className="lg:col-span-2">
      <LocalUsageDashboard
        snapshot={snapshot}
        loading={loading}
        loadError={loadError}
        onValidateConnection={onValidateConnection}
        onRefreshLocal={refreshLocal}
      />
    </div>
  );
}

function IntegratedLocalKbRelocationCard({
  configuredRoot,
  onRootChanged,
}: {
  configuredRoot: string | null;
  onRootChanged: (path: string) => void;
}) {
  const [currentPath, setCurrentPath] = useState(configuredRoot || "尚未绑定");

  useEffect(() => {
    detectKbStatus()
      .then((status) => {
        if (status.state === "bound") setCurrentPath(status.root);
      })
      .catch(() => {
        if (configuredRoot) setCurrentPath(configuredRoot);
      });
  }, [configuredRoot]);

  function progress(
    callback: (value: LocalKbRelocationProgress) => void,
    phase: string,
    message: string,
  ) {
    callback({ phase, completed: 0, total: null, message });
  }

  function mapResult(result: Awaited<ReturnType<typeof switchExistingLocalKb>>): LocalKbRelocationResult {
    return {
      target_path: result.new_root,
      backup_path: result.backup_path || null,
      recovery_path: null,
      index_rebuild_required: result.index_rebuild_required,
    };
  }

  async function runRelocation(
    mode: "switch" | "migrate",
    targetPath: string,
    onProgress: (value: LocalKbRelocationProgress) => void,
  ) {
    progress(
      onProgress,
      mode === "switch" ? "validating_existing" : "copying_and_verifying",
      mode === "switch" ? "正在校验已有知识库…" : "正在复制并校验知识库，旧目录不会删除…",
    );
    const backend =
      mode === "switch"
        ? await switchExistingLocalKb(targetPath)
        : await migrateCurrentLocalKb(targetPath);
    const result = mapResult(backend);
    setCurrentPath(result.target_path);
    onRootChanged(result.target_path);
    return result;
  }

  return (
    <div className="lg:col-span-2">
      <LocalKbRelocationCard
        currentPath={currentPath}
        onPickDirectory={async () => {
          const picked = await dialogOpen({ directory: true, multiple: false });
          return typeof picked === "string" && picked.trim() ? picked : null;
        }}
        onConfirm={(mode, targetPath) =>
          confirmDialog(
            mode === "switch_existing"
              ? `切换到已有知识库：\n${targetPath}\n\n旧目录不会删除。`
              : `迁移当前知识库到：\n${targetPath}\n\n迁移成功后旧目录仍保留为回退备份。`,
            {
              title: mode === "switch_existing" ? "确认切换知识库" : "确认迁移知识库",
              okLabel: mode === "switch_existing" ? "切换" : "迁移",
            },
          )
        }
        onSwitchExisting={(target, onProgress) =>
          runRelocation("switch", target, onProgress)
        }
        onMigrateCurrent={(target, onProgress) =>
          runRelocation("migrate", target, onProgress)
        }
        onRebuildSemanticIndex={async (_target, onProgress) => {
          progress(onProgress, "rebuilding_semantic_index", "正在按新目录重建语义索引…");
          await buildLocalKbSemanticIndex();
          onProgress({
            phase: "semantic_index_complete",
            completed: 1,
            total: 1,
            message: "语义索引重建完成。",
          });
        }}
      />
    </div>
  );
}

function formatErr(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatDateTime(iso: string): string {
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return iso;
    return d.toLocaleString("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}
