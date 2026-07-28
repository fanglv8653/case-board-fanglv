export const CRIMINAL_STAGE_ORDER = [
  "委托身份材料",
  "侦查",
  "审查起诉",
  "审判",
  "二审",
] as const;

export type CriminalStage = (typeof CRIMINAL_STAGE_ORDER)[number];

export interface CriminalStageDocument {
  source_path: string;
  filename: string;
  stage: string | null;
  is_ai_artifact: boolean;
}

export type CriminalStageGroups<T> = Record<CriminalStage, T[]> & {
  未分类: T[];
};

const DIRECTORY_KEYWORDS: ReadonlyArray<
  readonly [CriminalStage, readonly string[]]
> = [
  ["二审", ["二审", "第二审", "上诉", "终审"]],
  [
    "委托身份材料",
    ["委托", "身份", "主体", "授权", "律师手续", "辩护手续", "亲属关系"],
  ],
  [
    "审查起诉",
    ["审查起诉", "移送起诉", "检察院", "检察", "公诉阶段", "起诉阶段"],
  ],
  ["侦查", ["侦查", "公安", "刑侦", "经侦", "立案侦查", "侦办"]],
  ["审判", ["审判", "一审", "第一审", "法院", "庭审"]],
];

const FILENAME_KEYWORDS: ReadonlyArray<
  readonly [CriminalStage, readonly string[]]
> = [
  ["二审", ["二审", "2审", "第二审", "上诉状", "上诉案件"]],
  [
    "委托身份材料",
    [
      "身份证",
      "户口簿",
      "户籍",
      "授权委托书",
      "委托合同",
      "律师事务所函",
      "辩护手续",
      "亲属关系",
    ],
  ],
  [
    "审查起诉",
    ["审查起诉", "起诉书", "不起诉决定书", "公诉意见书", "量刑建议书"],
  ],
  [
    "侦查",
    [
      "立案决定书",
      "拘留",
      "逮捕",
      "取保候审",
      "搜查",
      "扣押",
      "讯问笔录",
      "起诉意见书",
    ],
  ],
  ["审判", ["判决书", "裁定书", "庭审", "开庭", "法庭", "一审"]],
];

function normalizeHint(value: string): string {
  return value
    .trim()
    .toLocaleLowerCase("zh-CN")
    .replace(/[\s\-_.．、（）()\[\]【】]/gu, "");
}

function stageFromKeywords(
  value: string,
  rules: ReadonlyArray<readonly [CriminalStage, readonly string[]]>,
): CriminalStage | null {
  for (const [stage, keywords] of rules) {
    if (keywords.some((keyword) => value.includes(keyword))) return stage;
  }
  return null;
}

function stageFromDirectory(value: string): CriminalStage | null {
  if (
    value === "2审" ||
    value.startsWith("2审材料") ||
    value.startsWith("2审阶段")
  ) {
    return "二审";
  }
  if (
    value === "1审" ||
    value.startsWith("1审材料") ||
    value.startsWith("1审阶段")
  ) {
    return "审判";
  }
  return stageFromKeywords(value, DIRECTORY_KEYWORDS);
}

function normalizedPath(value: string): string {
  return value.split("\\").join("/").replace(/\/+/gu, "/").replace(/\/$/u, "");
}

function relativePathParts(
  sourcePath: string,
  sourceFolder: string,
): { directories: string[]; filename: string } {
  const source = normalizedPath(sourcePath);
  const root = normalizedPath(sourceFolder);
  const sourceParts = source.split("/");
  const filename = sourceParts[sourceParts.length - 1] ?? "";

  if (
    root.length === 0 ||
    (source.toLocaleLowerCase("zh-CN") !== root.toLocaleLowerCase("zh-CN") &&
      !source
        .toLocaleLowerCase("zh-CN")
        .startsWith(`${root.toLocaleLowerCase("zh-CN")}/`))
  ) {
    // 共用目录等不在案件根目录下的文件不能使用绝对路径父目录推断阶段。
    return { directories: [], filename };
  }

  const relative = source.slice(root.length).replace(/^\/+/u, "");
  const parts = relative.split("/").filter(Boolean);
  return {
    directories: parts.slice(0, -1).map(normalizeHint),
    filename: parts[parts.length - 1] ?? filename,
  };
}

/**
 * 用完整相对目录优先推断刑事程序阶段；文件名只在目录无命中时补充。
 * 案件根目录不参与判断，避免案件名称污染全案分类。
 */
export function classifyCriminalStageFromPath(
  sourcePath: string,
  sourceFolder: string,
): CriminalStage | null {
  const { directories, filename } = relativePathParts(sourcePath, sourceFolder);
  for (const directory of [...directories].reverse()) {
    const stage = stageFromDirectory(directory);
    if (stage) return stage;
  }
  return stageFromKeywords(normalizeHint(filename), FILENAME_KEYWORDS);
}

export function classifyCriminalDocumentStage(
  doc: CriminalStageDocument,
  sourceFolder: string,
): CriminalStage | null {
  if (CRIMINAL_STAGE_ORDER.includes(doc.stage as CriminalStage)) {
    return doc.stage as CriminalStage;
  }
  // 兼容升级前已经写入的民事型阶段值；明确同义值可安全映射。
  if (doc.stage === "身份信息") return "委托身份材料";
  if (doc.stage === "一审") return "审判";
  return classifyCriminalStageFromPath(doc.source_path, sourceFolder);
}

export function groupCriminalDocuments<T extends CriminalStageDocument>(
  docs: T[],
  sourceFolder: string,
): CriminalStageGroups<T> {
  const groups: CriminalStageGroups<T> = {
    委托身份材料: [],
    侦查: [],
    审查起诉: [],
    审判: [],
    二审: [],
    未分类: [],
  };
  for (const doc of docs) {
    if (doc.is_ai_artifact) continue;
    const stage = classifyCriminalDocumentStage(doc, sourceFolder);
    if (stage) groups[stage].push(doc);
    else groups.未分类.push(doc);
  }
  return groups;
}
