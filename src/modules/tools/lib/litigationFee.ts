/**
 * 诉讼费 / 保全费计算 — 100% 移植自 lawtools.top/legalfee.html。
 *
 * 依据:《诉讼费用交纳办法》2007 年 4 月 1 日施行。
 *   - 财产案件按金额分 9 档累进
 *   - 离婚案件:每件 200 元;涉及财产分割超 20 万,超出部分加 0.5%
 *   - 保全费:1000 元以下 30 元;1000-10 万按 1% + 30 元;10 万以上按 0.5%,上限 5000 元
 */

/**
 * 财产案件受理费(元)。
 * @param amountWan 标的额(万元)
 */
export function calculatePropertyFee(amountWan: number): number {
  const amount = amountWan * 10000;
  if (amount <= 10000) return 50;
  // 分段超额累进（《诉讼费用交纳办法》第十三条第（一）款）。
  // 原实现用速算扣除数（amount * rate - subtract），但第二档起 subtract
  // 应为负值却写成了正值，导致超过 10 万元的案件全部偏少。改为直接分段
  // 累加，不依赖速算扣除数，避免符号错误。
  const brackets = [
    { limit: 100000, rate: 0.025 }, // 1 万 ~ 10 万
    { limit: 200000, rate: 0.02 }, // 10 万 ~ 20 万
    { limit: 500000, rate: 0.015 }, // 20 万 ~ 50 万
    { limit: 1000000, rate: 0.01 }, // 50 万 ~ 100 万
    { limit: 2000000, rate: 0.009 }, // 100 万 ~ 200 万
    { limit: 5000000, rate: 0.008 }, // 200 万 ~ 500 万
    { limit: 10000000, rate: 0.007 }, // 500 万 ~ 1000 万
    { limit: 20000000, rate: 0.006 }, // 1000 万 ~ 2000 万
    { limit: Infinity, rate: 0.005 }, // 2000 万+
  ];
  let fee = 50; // 不超过 1 万元部分
  let prevLimit = 10000;
  for (const b of brackets) {
    if (amount <= prevLimit) break;
    const taxable = Math.min(amount, b.limit) - prevLimit;
    fee += taxable * b.rate;
    if (amount <= b.limit) break;
    prevLimit = b.limit;
  }
  return Math.round(fee);
}

/**
 * 离婚案件受理费(元)。
 * @param splitAmountWan 涉及财产分割的金额(万元),无分割传 0
 * @param hasSplit 是否涉及财产分割
 */
export function calculateDivorceFee(
  splitAmountWan: number,
  hasSplit: boolean,
): number {
  const amount = splitAmountWan * 10000;
  let fee = 200;
  if (hasSplit && amount > 200000) {
    fee += (amount - 200000) * 0.005;
  }
  return Math.round(fee);
}

/**
 * 财产保全费(元)— 上限 5000 元。
 * @param amountWan 保全标的额(万元)
 */
export function calculatePreservationFee(amountWan: number): number {
  const amount = amountWan * 10000;
  if (amount <= 1000) return 30;
  if (amount <= 100000) {
    return Math.min(Math.round(30 + (amount - 1000) * 0.01), 5000);
  }
  return Math.min(
    Math.round(30 + 99000 * 0.01 + (amount - 100000) * 0.005),
    5000,
  );
}

/** 元数 → "X.YZ 万元" 或 "X 元" */
export function formatFeeYuan(yuan: number): string {
  if (yuan >= 10000) {
    return (yuan / 10000).toFixed(2) + " 万元";
  }
  return yuan + " 元";
}
