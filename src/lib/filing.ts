/**
 * filing 文书的 Markdown 元信息头解析。
 *
 * save_artifact 写盘的 .md 首行是:
 *   `<!-- filing · doc_type=民事起诉状 · title=测试起诉状 · ts=2026-05-31T... -->`
 *   `\n\n{正文 content_md}`
 *
 * 编辑器**只编辑正文**,不让这行注释头进 WYSIWYG(remark 对 HTML 注释 round-trip 不保证)。
 * 所以载入时用本模块剥头 + 取 title;保存时只回传 (title, body),由后端 save_editor_doc
 * 重建头(见 docs/V0.3-Milkdown编辑器-实施落地.md §1.4)。
 *
 * 对齐 Rust:src-tauri/src/chat/tools/artifact.rs:115 写头 / docx_filing.rs:434 解析头。
 */

export interface FilingMeta {
  docType: string | null;
  title: string | null;
}

// 容忍前后空白;doc_type/title 用懒匹配卡在 ` · ` 分隔符上;ts 直到 `-->`。
const FILING_HEADER_RE =
  /^<!--\s*filing\s*·\s*doc_type=(.*?)\s*·\s*title=(.*?)\s*·\s*ts=.*?-->\s*\n*/;

/**
 * 剥掉 filing 注释头,返回 meta(doc_type/title)+ 正文 body。
 * 没有头(非 filing 文书或老格式)时:meta 全 null,body 原样返回。
 */
export function stripFilingHeader(md: string): {
  meta: FilingMeta;
  body: string;
} {
  const m = md.match(FILING_HEADER_RE);
  if (!m) {
    return { meta: { docType: null, title: null }, body: cleanArtifactBody(md) };
  }
  return {
    meta: { docType: m[1] || null, title: m[2] || null },
    body: cleanArtifactBody(md.slice(m[0].length)),
  };
}

/**
 * 剥掉非正文垃圾:开头任意 HTML 注释头(老 `<!-- chat artifact · task=.. -->`)+ 结尾
 * `<CITATIONS>...` 协议块(闭合/未闭合都剥)。对齐 Rust `export::strip_artifact_cruft`。
 * 新文件写入端已不带(write_chat_artifact 去头 + citations.rs 剥未闭合块),这里是**存量
 * 脏文件**进编辑器时的兜底,防黑底 code block + 协议块串进 WYSIWYG。
 */
function cleanArtifactBody(s: string): string {
  let body = s.replace(/^\s*<!--[\s\S]*?-->\s*/, "");
  const cit = body.lastIndexOf("<CITATIONS>");
  if (cit >= 0) body = body.slice(0, cit);
  return body.replace(/\s+$/, "");
}

/**
 * 从文件名兜底推标题:剥掉时间戳 + 短 id 后缀 + 扩展名。
 * 文件名形如 `民事起诉状_2026-05-31_153012_abcd1234.md`。
 */
export function titleFromFilename(filename: string): string {
  return filename
    .replace(/\.(md|markdown|txt)$/i, "")
    .replace(/_\d{4}-\d{2}-\d{2}_\d{6}_[0-9a-f]{8}$/i, "")
    .replace(/_\d{4}-\d{2}-\d{2}_\d{6}$/i, "");
}
