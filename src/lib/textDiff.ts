/**
 * 轻量 token 级文本 diff(ADR-0003 Phase 2 · 文书改动审阅用)。
 *
 * 为什么自己写不引库:法律文书是中文长段落,行级 diff 会把整段标红;需要 token 级
 * (单个汉字 / 拉丁词 / 空白 / 标点为 token)才能只高亮「改的那几个字」。算法 = 剪公共
 * 前后缀 + 中段 LCS;中段过大时退化成「整段删+整段增」一处,封顶防卡。零依赖。
 *
 * 输出 `DiffPart[]` 给审阅 UI:`equal` 段原样,`change` 段含 del(旧/删除线)+ add(新/绿底),
 * 每个 change 可被用户接受(用 add)或拒绝(用 del),最终拼回正文。
 */

export interface DiffPart {
  kind: "equal" | "change";
  /** kind==="equal" 时的文本 */
  text?: string;
  /** kind==="change" 时被删除的旧文本(可空 = 纯新增) */
  del?: string;
  /** kind==="change" 时新增的新文本(可空 = 纯删除) */
  add?: string;
}

type Seg = { type: "equal" | "del" | "add"; text: string };

/** 中段 LCS 的规模上限(token 数乘积);超过就退化成单块删+增,防 O(n·m) 卡死。 */
const LCS_CAP = 2_000_000;

/** 分词:单个 CJK 字 / 拉丁数字串 / 空白串 / 其它单字符(标点等)各为一个 token。 */
function tokenize(s: string): string[] {
  const re = /[一-鿿㐀-䶿豈-﫿]|[A-Za-z0-9]+|\s+|[\s\S]/gu;
  return s.match(re) ?? [];
}

/** 往 segs 尾部追加,合并相邻同类型。 */
function push(segs: Seg[], type: Seg["type"], text: string) {
  const last = segs[segs.length - 1];
  if (last && last.type === type) last.text += text;
  else segs.push({ type, text });
}

/** 对两个 token 数组做 LCS diff,产出 equal/del/add 段序列。 */
function lcsDiff(a: string[], b: string[]): Seg[] {
  const n = a.length;
  const m = b.length;
  const segs: Seg[] = [];
  if (n === 0 && m === 0) return segs;
  if (n === 0) {
    push(segs, "add", b.join(""));
    return segs;
  }
  if (m === 0) {
    push(segs, "del", a.join(""));
    return segs;
  }
  if (n * m > LCS_CAP) {
    // 中段过大:不细 diff,整段删+整段增
    push(segs, "del", a.join(""));
    push(segs, "add", b.join(""));
    return segs;
  }
  // dp[i][j] = a[i..] 与 b[j..] 的 LCS 长度
  const dp: Int32Array[] = Array.from(
    { length: n + 1 },
    () => new Int32Array(m + 1),
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] =
        a[i] === b[j]
          ? dp[i + 1][j + 1] + 1
          : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      push(segs, "equal", a[i]);
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      push(segs, "del", a[i]);
      i++;
    } else {
      push(segs, "add", b[j]);
      j++;
    }
  }
  while (i < n) push(segs, "del", a[i++]);
  while (j < m) push(segs, "add", b[j++]);
  return segs;
}

/**
 * 计算 old → new 的 diff,折叠成 `DiffPart[]`。
 * 相邻的 del/add 合并成一个 `change`(便于逐处接受/拒绝)。
 */
export function diffParts(oldStr: string, newStr: string): DiffPart[] {
  if (oldStr === newStr) return [{ kind: "equal", text: oldStr }];
  const a = tokenize(oldStr);
  const b = tokenize(newStr);

  // 剪公共前缀
  let p = 0;
  while (p < a.length && p < b.length && a[p] === b[p]) p++;
  // 剪公共后缀(不与前缀重叠)
  let sa = a.length;
  let sb = b.length;
  while (sa > p && sb > p && a[sa - 1] === b[sb - 1]) {
    sa--;
    sb--;
  }
  const prefix = a.slice(0, p).join("");
  const suffix = a.slice(sa).join("");
  const midSegs = lcsDiff(a.slice(p, sa), b.slice(p, sb));

  const segs: Seg[] = [];
  if (prefix) push(segs, "equal", prefix);
  for (const s of midSegs) push(segs, s.type, s.text);
  if (suffix) push(segs, "equal", suffix);

  // 折叠成 DiffPart:连续 del/add 并成一个 change
  const parts: DiffPart[] = [];
  let del = "";
  let add = "";
  const flush = () => {
    if (del || add) {
      parts.push({ kind: "change", del, add });
      del = "";
      add = "";
    }
  };
  for (const s of segs) {
    if (s.type === "equal") {
      flush();
      parts.push({ kind: "equal", text: s.text });
    } else if (s.type === "del") {
      del += s.text;
    } else {
      add += s.text;
    }
  }
  flush();
  return parts;
}

/** change 块数量(审阅 UI 显示「N 处改动」用)。 */
export function countChanges(parts: DiffPart[]): number {
  return parts.filter((p) => p.kind === "change").length;
}

/**
 * 按每个 change 的「接受/拒绝」选择拼回最终正文。
 * accepts[k] 对应第 k 个 change:true=用 add(接受 AI 改动),false=用 del(还原旧文本)。
 */
export function reconstruct(parts: DiffPart[], accepts: boolean[]): string {
  let ci = 0;
  let out = "";
  for (const p of parts) {
    if (p.kind === "equal") {
      out += p.text ?? "";
    } else {
      out += (accepts[ci] ? p.add : p.del) ?? "";
      ci++;
    }
  }
  return out;
}
