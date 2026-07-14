/**
 * LPR 离线基线与运行时数据。
 *
 * 内置基线共 83 个公布点，从 2019-08-20 到已核实的 2026-06-22。
 * 它用于离线计算，不因历史迁移来源而当然视为官方实时数据；运行时会把
 * 后端保存的人民银行官方缓存按发布日期覆盖进来。
 */

export interface LprPoint {
  date: string; // YYYY-MM-DD
  lpr1y: number; // 1 年期 LPR(%)
  lpr5y: number; // 5 年期以上 LPR(%)
}

export interface CachedLprPoint {
  publication_date: string;
  lpr_1y: number;
  lpr_5y: number;
}

export const PBOC_LPR_SOURCE_URL =
  "https://www.pbc.gov.cn/zhengcehuobisi/125207/125213/125440/3876551/index.html";

export const BUILTIN_LPR_DATA: readonly LprPoint[] = [
  { date: "2019-08-20", lpr1y: 4.25, lpr5y: 4.85 },
  { date: "2019-09-20", lpr1y: 4.2, lpr5y: 4.85 },
  { date: "2019-10-21", lpr1y: 4.2, lpr5y: 4.85 },
  { date: "2019-11-20", lpr1y: 4.15, lpr5y: 4.8 },
  { date: "2019-12-20", lpr1y: 4.15, lpr5y: 4.8 },
  { date: "2020-01-20", lpr1y: 4.15, lpr5y: 4.8 },
  { date: "2020-02-20", lpr1y: 4.05, lpr5y: 4.75 },
  { date: "2020-03-20", lpr1y: 4.05, lpr5y: 4.75 },
  { date: "2020-04-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-05-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-06-22", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-07-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-08-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-09-21", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-10-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-11-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2020-12-21", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-01-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-02-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-03-22", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-04-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-05-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-06-21", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-07-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-08-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-09-22", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-10-20", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-11-22", lpr1y: 3.85, lpr5y: 4.65 },
  { date: "2021-12-20", lpr1y: 3.8, lpr5y: 4.65 },
  { date: "2022-01-20", lpr1y: 3.7, lpr5y: 4.6 },
  { date: "2022-02-21", lpr1y: 3.7, lpr5y: 4.6 },
  { date: "2022-03-21", lpr1y: 3.7, lpr5y: 4.6 },
  { date: "2022-04-20", lpr1y: 3.7, lpr5y: 4.6 },
  { date: "2022-05-20", lpr1y: 3.7, lpr5y: 4.45 },
  { date: "2022-06-20", lpr1y: 3.7, lpr5y: 4.45 },
  { date: "2022-07-20", lpr1y: 3.7, lpr5y: 4.45 },
  { date: "2022-08-22", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2022-09-20", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2022-10-20", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2022-11-21", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2022-12-20", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2023-01-20", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2023-02-20", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2023-03-20", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2023-04-20", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2023-05-22", lpr1y: 3.65, lpr5y: 4.3 },
  { date: "2023-06-20", lpr1y: 3.55, lpr5y: 4.2 },
  { date: "2023-07-20", lpr1y: 3.55, lpr5y: 4.2 },
  { date: "2023-08-21", lpr1y: 3.45, lpr5y: 4.2 },
  { date: "2023-09-20", lpr1y: 3.45, lpr5y: 4.2 },
  { date: "2023-10-20", lpr1y: 3.45, lpr5y: 4.2 },
  { date: "2023-11-20", lpr1y: 3.45, lpr5y: 4.2 },
  { date: "2023-12-20", lpr1y: 3.45, lpr5y: 4.2 },
  { date: "2024-01-22", lpr1y: 3.45, lpr5y: 4.2 },
  { date: "2024-02-20", lpr1y: 3.45, lpr5y: 3.95 },
  { date: "2024-03-20", lpr1y: 3.45, lpr5y: 3.95 },
  { date: "2024-04-22", lpr1y: 3.45, lpr5y: 3.95 },
  { date: "2024-05-20", lpr1y: 3.45, lpr5y: 3.95 },
  { date: "2024-06-20", lpr1y: 3.45, lpr5y: 3.95 },
  { date: "2024-07-22", lpr1y: 3.35, lpr5y: 3.85 },
  { date: "2024-08-20", lpr1y: 3.35, lpr5y: 3.85 },
  { date: "2024-09-20", lpr1y: 3.35, lpr5y: 3.85 },
  { date: "2024-10-21", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2024-11-20", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2024-12-20", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2025-01-20", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2025-02-20", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2025-03-20", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2025-04-21", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2025-05-20", lpr1y: 3.1, lpr5y: 3.6 },
  { date: "2025-06-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2025-07-21", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2025-08-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2025-09-22", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2025-10-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2025-11-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2025-12-22", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2026-01-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2026-02-24", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2026-03-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2026-04-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2026-05-20", lpr1y: 3.0, lpr5y: 3.5 },
  { date: "2026-06-22", lpr1y: 3.0, lpr5y: 3.5 },
];

/** 保持数组引用稳定，供既有计算模块静态导入。 */
export const LPR_DATA: LprPoint[] = BUILTIN_LPR_DATA.map((point) => ({ ...point }));

function isIsoDate(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) && !Number.isNaN(Date.parse(`${value}T00:00:00Z`));
}

function assertValidPoint(point: LprPoint, label: string): void {
  if (!isIsoDate(point.date)) throw new Error(`${label}包含无效发布日期：${point.date}`);
  if (
    !Number.isFinite(point.lpr1y) ||
    !Number.isFinite(point.lpr5y) ||
    point.lpr1y <= 0 ||
    point.lpr5y <= 0 ||
    point.lpr1y >= 20 ||
    point.lpr5y >= 20
  ) {
    throw new Error(`${label}包含无效利率：${point.date}`);
  }
}

/**
 * 官方缓存覆盖同日基线，并按日期升序去重。
 * 缓存内部如出现同日冲突值则拒绝合并，避免前端静默选择其中一个。
 */
export function mergeLprPoints(
  baseline: readonly LprPoint[],
  cached: readonly CachedLprPoint[],
): LprPoint[] {
  const merged = new Map<string, LprPoint>();
  for (const point of baseline) {
    assertValidPoint(point, "内置基线");
    merged.set(point.date, { ...point });
  }

  const seenCache = new Map<string, LprPoint>();
  for (const point of cached) {
    const normalized = {
      date: point.publication_date,
      lpr1y: point.lpr_1y,
      lpr5y: point.lpr_5y,
    };
    assertValidPoint(normalized, "官方缓存");
    const existing = seenCache.get(normalized.date);
    if (
      existing &&
      (existing.lpr1y !== normalized.lpr1y || existing.lpr5y !== normalized.lpr5y)
    ) {
      throw new Error(`官方缓存同一发布日期存在冲突值：${normalized.date}`);
    }
    seenCache.set(normalized.date, normalized);
  }

  for (const point of seenCache.values()) merged.set(point.date, point);
  return [...merged.values()].sort((a, b) => a.date.localeCompare(b.date));
}

/** 原位更新运行时数组，使既有静态导入继续看到最新数据。 */
export function applyCachedLprPoints(cached: readonly CachedLprPoint[]): LprPoint[] {
  const next = mergeLprPoints(BUILTIN_LPR_DATA, cached);
  LPR_DATA.splice(0, LPR_DATA.length, ...next);
  return LPR_DATA;
}

export type LprTerm = "1y" | "5y+";

/**
 * 找指定日期适用的 LPR 利率(%)— 最后一个 ≤ targetDate 的 LPR 公布点。
 * @param dateStr "YYYY-MM-DD"
 * @param term 期限选择
 * @returns 利率,如 3.65;无对应数据时返 null
 */
export function getLprForDate(dateStr: string, term: LprTerm): number | null {
  if (!dateStr) return null;
  const target = new Date(dateStr);
  let lpr: number | null = null;
  for (const item of LPR_DATA) {
    const d = new Date(item.date);
    if (d <= target) {
      lpr = term === "5y+" ? item.lpr5y : item.lpr1y;
    } else {
      break;
    }
  }
  return lpr;
}
