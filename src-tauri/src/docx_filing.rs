//! Word 导出引擎(MD → 原生 OOXML · V0.3 起 · 2026-06-04 泛化为 base+profile)。
//!
//! **全 app 唯一的 docx 生成器**,固化 quote.law 15 份样本提炼出的法律级排版
//! (方正小标宋标题 / 黑体小标题 / 仿宋正文 / 两端对齐 / 首行缩进 2 字 / 1.5 倍行距),
//! 同一 run 上 `ascii=Times` + `eastAsia=仿宋` 的双字体是正式法院文书的灵魂 ——
//! 这正是被替代的 macOS `textutil` CSS 路径**结构上做不到**的(且 textutil 把 GFM 表格转散架)。
//! 纯 Rust、**零外部依赖**(编进二进制、跨平台,装了 app 即用)。
//!
//! ## 两档 [`Profile`](Profile)(共享上面全部排版,只差「是否忠实保留 MD 结构」)
//! - **base**([`build_report_docx_bytes`]):案件分析报告 / 风险·深挖报告 / 通用 MD 导出走这条。
//!   忠实渲染 —— 无序列表带圆点 + 嵌套左悬挂缩进、`---` 渲染成下边框分隔段。
//! - **filing**([`build_filing_docx_bytes`]):法律文书(起诉状等)走这条 = base + 法律叠加
//!   (无序列表去圆点、软/硬换行并段、`---` 丢弃),贴合法院文书惯例。
//!
//! ## 排版词汇表(从 15 份 quote.law 样本 docx XML **确定性提取**,见 docs/V0.3-文书格式规范)
//! | 角色 | eastAsia | sz(半点) | 对齐 | 首行缩进 |
//! |---|---|---|---|---|
//! | 文书标题 | 方正小标宋简体 | 32(16pt) | 居中 | 无 |
//! | 一级标题 | SimHei(黑体) | 30(15pt) | 两端 | 560twip(2字) |
//! | 二级标题 | SimHei(黑体) | 28(14pt) | 两端 | 560twip |
//! | 正文 | 仿宋_GB2312 | 28(14pt) | 两端 | 560twip |
//! | 强调正文 | 仿宋_GB2312 + 加粗 | 28 | 两端 | 560twip |
//!
//! 全局:A4(11906×16838)、四边页边距 1440twip(1 英寸)、docGrid linePitch=360、1.5 倍行距、
//! ascii=Times New Roman、**inline rPr 不靠段落样式**(quote.law 签名,本模块完全复刻)。
//!
//! ## Markdown → 角色映射约定
//!
//! - `title` 参数 → 文书标题(居中)
//! - MD 一/二级标题(`#` `##`)→ 一级标题(黑体 15pt)
//! - MD 三级及以下(`###`+)→ 二级标题(黑体 14pt)
//! - 普通段落 / 列表项 → 正文(仿宋 14pt);有序列表编号写进文本,无序列表 base 档加圆点(filing 不加)
//! - `---` 分隔线 → base 渲染成下边框段,filing 丢弃
//! - 段内 `**加粗**` → 该 run 加粗(强调正文)
//! - GFM 表格 → 仿宋正文表格(表头加粗,单线边框)
//!
//! 容器骨架(`[Content_Types]`/`_rels`/`styles`/`settings`/`sectPr`)取自真实样本,换取 Word 有效性。

use std::io::Write;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

// ───────────────────────── 角色 → 精确 OOXML 数值 ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Title,
    H1,
    H2,
    Body,
}

impl Role {
    fn east_asia(self) -> &'static str {
        match self {
            Role::Title => "方正小标宋简体",
            Role::H1 | Role::H2 => "SimHei",
            Role::Body => "仿宋_GB2312",
        }
    }
    /// 字号(半点)
    fn sz(self) -> &'static str {
        match self {
            Role::Title => "32",
            Role::H1 => "30",
            Role::H2 | Role::Body => "28",
        }
    }
    fn centered(self) -> bool {
        matches!(self, Role::Title)
    }
    fn first_line_indent(self) -> bool {
        !matches!(self, Role::Title)
    }
}

/// 导出排版档位。**base** = 忠实 MD 渲染(报告 / 通用 MD 走这条):保留无序列表圆点、
/// 嵌套缩进、分隔线;**filing** = 在 base 之上叠加法律文书的刻意简化(列表去圆点、
/// 软换行并段、不渲染分隔线),其余排版(仿宋正文 / 黑体标题 / 方正小标宋居中标题 /
/// 首行缩进 / 两端对齐 / 1.5 倍行距)两档完全一致。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Profile {
    /// 通用报告:忠实保留 MD 结构
    #[default]
    Base,
    /// 法律文书:base + 法律叠加
    Filing,
}

struct Run {
    text: String,
    bold: bool,
}

enum Block {
    /// `list_depth`:0 = 普通段落(按角色首行缩进);≥1 = 列表项嵌套层级(左悬挂缩进)
    Para {
        role: Role,
        runs: Vec<Run>,
        list_depth: u8,
    },
    Table {
        rows: Vec<TableRow>,
    },
    /// 分隔线(`---`),只在 base 档渲染
    Rule,
}

struct TableRow {
    header: bool,
    cells: Vec<Vec<Run>>,
}

// ───────────────────────── XML escape ─────────────────────────

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 去掉 HTML 注释(artifact MD 头部带 `<!-- chat artifact ... -->`),避免 pulldown 当内联 HTML。
fn strip_html_comments(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("-->") {
            rest = &rest[start + end + 3..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

// ───────────────────────── Markdown → Blocks ─────────────────────────

#[derive(Default)]
struct Walker {
    profile: Profile,
    blocks: Vec<Block>,
    cur: Option<(Role, Vec<Run>, u8)>,
    ordered: Vec<Option<u64>>,
    bold_depth: u32,
    // 表格累积
    table_rows: Option<Vec<TableRow>>,
    cur_row: Option<(bool, Vec<Vec<Run>>)>,
    cur_cell: Option<Vec<Run>>,
}

impl Walker {
    fn flush_cur(&mut self) {
        if let Some((role, runs, list_depth)) = self.cur.take() {
            // 丢弃完全空白段(只有空格)
            if runs.iter().any(|r| !r.text.trim().is_empty()) {
                self.blocks.push(Block::Para {
                    role,
                    runs,
                    list_depth,
                });
            }
        }
    }

    fn push_text(&mut self, t: &str) {
        // pulldown 对 CJK 紧邻的 `**` 不识别为加粗,会把分隔符当字面量 Text("*") 漏出来;
        // 纯星号 run 几乎不可能是正文内容(中文法律文书不含裸 `*`),直接丢弃防止 Word 里露出 `**`。
        if !t.is_empty() && t.chars().all(|c| c == '*') {
            return;
        }
        let bold = self.bold_depth > 0;
        if let Some(cell) = self.cur_cell.as_mut() {
            cell.push(Run {
                text: t.to_string(),
                bold,
            });
            return;
        }
        if self.cur.is_none() {
            self.cur = Some((Role::Body, Vec::new(), 0));
        }
        self.cur.as_mut().unwrap().1.push(Run {
            text: t.to_string(),
            bold,
        });
    }

    fn walk(&mut self, parser: Parser) {
        for ev in parser {
            match ev {
                Event::Start(tag) => self.start(tag),
                Event::End(tag) => self.end(tag),
                Event::Text(t) | Event::Code(t) => self.push_text(&t),
                // 内联/块级 HTML 当字面量文本处理(转义后输出),既不丢内容也不注入 HTML
                Event::Html(t) | Event::InlineHtml(t) => self.push_text(&t),
                // 软换行:中文文书同段内不插空格;硬换行同样并段(MVP)
                Event::SoftBreak | Event::HardBreak => {}
                // 分隔线 `---`:base 忠实渲染成下边框段,filing 沿用旧行为(丢弃)
                Event::Rule => {
                    self.flush_cur();
                    if self.profile == Profile::Base {
                        self.blocks.push(Block::Rule);
                    }
                }
                _ => {}
            }
        }
        self.flush_cur();
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush_cur();
                let role = match level {
                    HeadingLevel::H1 | HeadingLevel::H2 => Role::H1,
                    _ => Role::H2,
                };
                self.cur = Some((role, Vec::new(), 0));
            }
            Tag::Paragraph | Tag::BlockQuote(_)
                if self.cur.is_none() && self.cur_cell.is_none() =>
            {
                self.cur = Some((Role::Body, Vec::new(), 0));
            }
            Tag::List(start) => self.ordered.push(start),
            Tag::Item => {
                self.flush_cur();
                let depth = self.ordered.len().max(1) as u8;
                let mut runs = Vec::new();
                // 有序列表:编号写进文本(两档一致);无序列表:base 加圆点,filing 不加(沿用旧行为)
                let prefix = match self.ordered.last_mut() {
                    Some(Some(n)) => {
                        let s = format!("{}. ", n);
                        *n += 1;
                        s
                    }
                    Some(None) if self.profile == Profile::Base => "• ".to_string(),
                    _ => String::new(),
                };
                if !prefix.is_empty() {
                    runs.push(Run {
                        text: prefix,
                        bold: false,
                    });
                }
                // base 给列表项左悬挂缩进(按嵌套层级);filing 保持普通段落(首行缩进)
                let list_depth = if self.profile == Profile::Base {
                    depth
                } else {
                    0
                };
                self.cur = Some((Role::Body, runs, list_depth));
            }
            Tag::Strong => self.bold_depth += 1,
            Tag::Table(_) => {
                self.flush_cur();
                self.table_rows = Some(Vec::new());
            }
            Tag::TableHead => self.cur_row = Some((true, Vec::new())),
            Tag::TableRow => self.cur_row = Some((false, Vec::new())),
            Tag::TableCell => self.cur_cell = Some(Vec::new()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) | TagEnd::Paragraph | TagEnd::Item | TagEnd::BlockQuote(_) => {
                self.flush_cur()
            }
            TagEnd::List(_) => {
                self.ordered.pop();
            }
            TagEnd::Strong => self.bold_depth = self.bold_depth.saturating_sub(1),
            TagEnd::TableCell => {
                if let (Some(cell), Some(row)) = (self.cur_cell.take(), self.cur_row.as_mut()) {
                    row.1.push(cell);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let (Some((header, cells)), Some(rows)) =
                    (self.cur_row.take(), self.table_rows.as_mut())
                {
                    rows.push(TableRow { header, cells });
                }
            }
            TagEnd::Table => {
                if let Some(rows) = self.table_rows.take() {
                    self.blocks.push(Block::Table { rows });
                }
            }
            _ => {}
        }
    }
}

fn parse_blocks(body_md: &str, profile: Profile) -> Vec<Block> {
    let cleaned = strip_html_comments(body_md);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&cleaned, opts);
    let mut w = Walker {
        profile,
        ..Default::default()
    };
    w.walk(parser);
    w.blocks
}

// ───────────────────────── Blocks → document.xml ─────────────────────────

fn render_run(run: &Run, role: Role) -> String {
    if run.text.is_empty() {
        return String::new();
    }
    let b = if run.bold { "<w:b/><w:bCs/>" } else { "" };
    format!(
        "<w:r><w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:eastAsia=\"{ea}\"/>{b}\
         <w:sz w:val=\"{sz}\"/><w:szCs w:val=\"{sz}\"/></w:rPr>\
         <w:t xml:space=\"preserve\">{t}</w:t></w:r>",
        ea = role.east_asia(),
        b = b,
        sz = role.sz(),
        t = xml_escape(&run.text),
    )
}

fn render_para(role: Role, runs: &[Run], list_depth: u8) -> String {
    let mut s = String::from("<w:p><w:pPr><w:spacing w:line=\"360\" w:lineRule=\"auto\"/>");
    if list_depth > 0 {
        // 列表项:按嵌套层级左缩进 + 悬挂缩进(换行后对齐到圆点/编号之后)
        let left = 420 * list_depth as i32 + 280;
        s.push_str(&format!("<w:ind w:left=\"{}\" w:hanging=\"280\"/>", left));
    } else if role.first_line_indent() {
        s.push_str("<w:ind w:firstLine=\"560\"/>");
    }
    s.push_str(if role.centered() {
        "<w:jc w:val=\"center\"/>"
    } else {
        "<w:jc w:val=\"both\"/>"
    });
    s.push_str("</w:pPr>");
    for r in runs {
        s.push_str(&render_run(r, role));
    }
    s.push_str("</w:p>");
    s
}

/// 分隔线(`---`)→ 一个带下边框的空段(base 档)。
fn render_rule() -> &'static str {
    "<w:p><w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"auto\"/></w:pBdr></w:pPr></w:p>"
}

/// 表格单元格里的段落:正文字体,不缩进(表格内),表头加粗。
fn render_cell(cell: &[Run], header: bool) -> String {
    let mut s = String::from(
        "<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr>\
         <w:p><w:pPr><w:spacing w:line=\"360\" w:lineRule=\"auto\"/><w:jc w:val=\"both\"/></w:pPr>",
    );
    // 空单元格也合法:留一个 run-less 段(上面的 <w:p> 已含),Word 才认
    for r in cell {
        let run = Run {
            text: r.text.clone(),
            bold: r.bold || header,
        };
        s.push_str(&render_run(&run, Role::Body));
    }
    s.push_str("</w:p></w:tc>");
    s
}

fn render_table(rows: &[TableRow]) -> String {
    let cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(1).max(1);
    // 文字区宽 = 11906 - 左右各 1440 = 9026 twip,均分
    let colw = 9026 / cols as i32;
    let mut grid = String::from("<w:tblGrid>");
    for _ in 0..cols {
        grid.push_str(&format!("<w:gridCol w:w=\"{}\"/>", colw));
    }
    grid.push_str("</w:tblGrid>");

    let mut s = String::from(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/>\
         <w:tblBorders>\
         <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
         <w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
         <w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
         <w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
         </w:tblBorders></w:tblPr>",
    );
    s.push_str(&grid);
    for row in rows {
        s.push_str("<w:tr>");
        for cell in &row.cells {
            s.push_str(&render_cell(cell, row.header));
        }
        s.push_str("</w:tr>");
    }
    s.push_str("</w:tbl>");
    s
}

/// 生成完整的 `word/document.xml`。`pub(crate)` 供测试做结构断言。
pub(crate) fn render_document_xml(title: &str, body_md: &str, profile: Profile) -> String {
    let mut blocks = parse_blocks(body_md, profile);

    // 去重:若正文首块是与 title 同名的标题,丢掉(LLM 常在正文重复写标题)
    let title_trim = title.trim();
    if let Some(Block::Para { role, runs, .. }) = blocks.first() {
        if matches!(role, Role::H1) {
            let txt: String = runs.iter().map(|r| r.text.as_str()).collect();
            if txt.trim() == title_trim && !title_trim.is_empty() {
                blocks.remove(0);
            }
        }
    }

    let mut body = String::new();
    // 文书标题(总在最前)
    if !title_trim.is_empty() {
        body.push_str(&render_para(
            Role::Title,
            &[Run {
                text: title_trim.to_string(),
                bold: false,
            }],
            0,
        ));
    }

    let last_is_table = matches!(blocks.last(), Some(Block::Table { .. }));
    for b in &blocks {
        match b {
            Block::Para {
                role,
                runs,
                list_depth,
            } => body.push_str(&render_para(*role, runs, *list_depth)),
            Block::Table { rows } => body.push_str(&render_table(rows)),
            Block::Rule => body.push_str(render_rule()),
        }
    }
    // OOXML 要求表格后须有段落;末块是表格时补一个空段
    if last_is_table {
        body.push_str("<w:p/>");
    }

    format!(
        "{decl}{open}<w:body>{body}{sect}</w:body></w:document>",
        decl = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        open = DOC_OPEN,
        body = body,
        sect = SECTPR,
    )
}

// ───────────────────────── 打包成 docx(zip) ─────────────────────────

/// 法律文书档:base + 法律叠加(列表去圆点 / 软换行并段 / 不渲染分隔线)。
/// 排版本身(方正小标宋居中标题 / 黑体小标题 / 仿宋正文 / 首行缩进 / 两端对齐 / 1.5 行距)与 base 一致。
pub fn build_filing_docx_bytes(title: &str, body_md: &str) -> Result<Vec<u8>, String> {
    build_docx_bytes(title, body_md, Profile::Filing)
}

/// 通用报告档:忠实 MD 渲染(无序列表带圆点 / 嵌套缩进 / 分隔线 / 保留结构)。
/// 案件分析报告、风险/深挖报告、通用 MD 导出走这条(替代旧的 textutil HTML 路径)。
pub fn build_report_docx_bytes(title: &str, body_md: &str) -> Result<Vec<u8>, String> {
    build_docx_bytes(title, body_md, Profile::Base)
}

/// 把 (标题, 正文 MD, 档位) 打包成完整 .docx 字节流。纯函数,便于测试。
fn build_docx_bytes(title: &str, body_md: &str, profile: Profile) -> Result<Vec<u8>, String> {
    let document_xml = render_document_xml(title, body_md, profile);
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let mut put = |name: &str, data: &str| -> Result<(), String> {
            zip.start_file(name, opts)
                .map_err(|e| format!("zip start_file {} 失败:{}", name, e))?;
            zip.write_all(data.as_bytes())
                .map_err(|e| format!("zip write {} 失败:{}", name, e))?;
            Ok(())
        };
        put("[Content_Types].xml", CONTENT_TYPES)?;
        put("_rels/.rels", RELS_DOTRELS)?;
        put("word/_rels/document.xml.rels", DOCUMENT_RELS)?;
        put("word/styles.xml", STYLES_XML)?;
        put("word/settings.xml", SETTINGS_XML)?;
        put("word/fontTable.xml", FONT_TABLE_XML)?;
        put("word/document.xml", &document_xml)?;
        zip.finish().map_err(|e| format!("zip finish 失败:{}", e))?;
    }
    Ok(buf)
}

/// 从 `save_artifact` 写的元信息头 `<!-- filing · doc_type=.. · title=.. · ts=.. -->`
/// 解析出文书标题(导出 Word 时作居中大标题)。无头则返回 None,调用方用文件名兜底。
pub fn extract_filing_title(md: &str) -> Option<String> {
    let start = md.find("<!-- filing")?;
    let end = md[start..].find("-->")? + start;
    let header = &md[start..end];
    let key = "title=";
    let kpos = header.find(key)? + key.len();
    let rest = &header[kpos..];
    // title 值到下一个 ` · ` 分隔或注释尾
    let val = rest.split(" · ").next().unwrap_or(rest).trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

// ───────────────────────── 内嵌容器骨架(取自真实样本) ─────────────────────────

const DOC_OPEN: &str = r#"<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:w10="urn:schemas-microsoft-com:office:word" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" mc:Ignorable="w14 w15">"#;

/// 页面/页边距/版式网格 —— 与全部 15 份样本字节级一致(A4 / 1英寸边距 / docGrid linePitch=360)。
const SECTPR: &str = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838" w:orient="portrait"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/><w:docGrid w:linePitch="360"/></w:sectPr>"#;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#;

const RELS_DOTRELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/></Relationships>"#;

const SETTINGS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" mc:Ignorable="w14 w15"><w:evenAndOddHeaders w:val="false"/><w:compat><w:compatSetting w:val="15" w:uri="http://schemas.microsoft.com/office/word" w:name="compatibilityMode"/></w:compat></w:settings>"#;

const FONT_TABLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" mc:Ignorable="w14"/>"#;

/// styles.xml —— 取自样本(含 docDefaults + Word 默认标题样式定义)。本模块用 inline rPr,
/// 这些样式实际不引用,但保留以保证 Word 完整打开(reuse sample container)。
const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles mc:Ignorable="w14 w15" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"><w:docDefaults><w:rPrDefault/><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn xml_of(title: &str, md: &str) -> String {
        render_document_xml(title, md, Profile::Filing)
    }

    /// base 档渲染(报告/通用 MD)。
    fn xml_base(title: &str, md: &str) -> String {
        render_document_xml(title, md, Profile::Base)
    }

    #[test]
    fn title_is_fangzheng_centered_no_indent() {
        let x = xml_of("民事起诉状", "正文。");
        // 标题段:方正小标宋简体 + sz32 + 居中,且不带首行缩进
        assert!(x.contains("方正小标宋简体"), "缺标题字体");
        assert!(x.contains("<w:sz w:val=\"32\"/>"), "缺标题 16pt");
        assert!(x.contains("<w:jc w:val=\"center\"/>"), "标题应居中");
        // 标题文本所在段不应有 firstLine(取标题段片段断言)
        let title_seg = &x[x.find("方正小标宋").unwrap()..];
        let para_start = x[..x.find("方正小标宋").unwrap()].rfind("<w:p>").unwrap();
        let title_para = &x[para_start..x[para_start..].find("</w:p>").unwrap() + para_start];
        let _ = title_seg;
        assert!(
            !title_para.contains("w:firstLine"),
            "标题段不应有首行缩进:{}",
            title_para
        );
    }

    #[test]
    fn body_is_fangsong_justified_2char_indent() {
        let x = xml_of("起诉状", "这是一段正文内容。");
        assert!(x.contains("仿宋_GB2312"), "缺正文字体");
        assert!(x.contains("<w:sz w:val=\"28\"/>"), "缺正文 14pt");
        assert!(x.contains("<w:jc w:val=\"both\"/>"), "正文应两端对齐");
        assert!(
            x.contains("<w:ind w:firstLine=\"560\"/>"),
            "正文应首行缩进2字"
        );
        assert!(
            x.contains("<w:spacing w:line=\"360\" w:lineRule=\"auto\"/>"),
            "应1.5倍行距"
        );
    }

    #[test]
    fn h1_is_heiti_15pt() {
        let x = xml_of("T", "## 一、事实与理由\n\n正文。");
        // 黑体 sz30 段必须出现
        assert!(x.contains("SimHei"), "缺黑体小标题");
        assert!(x.contains("<w:sz w:val=\"30\"/>"), "一级标题应15pt");
        assert!(x.contains("一、事实与理由"), "缺标题文本");
    }

    #[test]
    fn h3_maps_to_h2_14pt_heiti() {
        let x = xml_of("T", "### （一）项目信息\n\n正文。");
        assert!(x.contains("SimHei"));
        // H2 黑体 14pt(sz28)且文本在黑体段
        assert!(x.contains("（一）项目信息"));
    }

    #[test]
    fn inline_bold_emits_b() {
        // 边界式加粗(LLM 在法律文书里的自然写法):整短语加粗
        let x = xml_of("T", "**证据1**:《合同》原件");
        assert!(x.contains("<w:b/><w:bCs/>"), "段内加粗应输出 <w:b/>");
        assert!(x.contains("证据1"));
    }

    #[test]
    fn cjk_adjacent_bold_does_not_leak_asterisks() {
        // pulldown 不识别 CJK 紧邻加粗 → 必须吞掉漏出的 `*`,内容仍保留
        let x = xml_of("T", "证据名:**《合同》**原件");
        assert!(
            !x.contains(">*<") && !x.contains("**"),
            "不得在正文露出字面 `*`:{}",
            x
        );
        assert!(x.contains("《合同》") && x.contains("原件"), "内容不得丢失");
    }

    #[test]
    fn ordered_list_numbers_into_text() {
        let x = xml_of("T", "1. 第一项\n2. 第二项");
        assert!(
            x.contains("1. 第一项") || x.contains("1. "),
            "有序列表编号应写进文本"
        );
        assert!(x.contains("第二项"));
    }

    #[test]
    fn xuanshang_bare_h1_and_two_case_numbers() {
        // 执行悬赏申请书:裸一级标题(无「一、」前缀)+ 双案号并列。
        // 锁定导出器:① 裸 `#` 仍渲染成黑体 15pt(角色不依赖序号前缀)
        // ② 标题文本原样透传、不被加「一、二、」③ 两个案号(生效文书号 / 执恢号)都不丢 ④ 不 panic。
        let md = "申请人：[姓名]，男，汉族。\n\
                  被执行人：[姓名]，男，汉族，电话：[电话]。\n\n\
                  # 申请事项\n\n\
                  1. 请求法院依法发布悬赏公告。\n\n\
                  # 事实和理由\n\n\
                  申请人与被执行人借款纠纷一案，(2024)苏0211民初123号民事判决书已生效，\
                  案号为(2025)苏0211执恢45号。\n";
        let x = xml_of("执行悬赏申请书", md);
        assert!(x.contains("SimHei"), "裸一级标题也应黑体");
        assert!(x.contains("<w:sz w:val=\"30\"/>"), "一级标题应15pt");
        assert!(
            x.contains("申请事项") && x.contains("事实和理由"),
            "裸标题文本应原样透传"
        );
        assert!(
            !x.contains("一、申请事项") && !x.contains("二、事实和理由"),
            "导出器不得自动加「一、二、」前缀"
        );
        assert!(x.contains("(2024)苏0211民初123号"), "生效文书号不得丢");
        assert!(x.contains("(2025)苏0211执恢45号"), "执行案号不得丢");
        assert!(
            build_filing_docx_bytes("执行悬赏申请书", md).is_ok(),
            "执行悬赏导出不得失败"
        );
    }

    #[test]
    fn dedup_leading_h1_equal_title() {
        // 正文首行重复标题 → 只出现一次(作为标题角色)
        let x = xml_of("民事起诉状", "# 民事起诉状\n\n原告:张三");
        let cnt = x.matches("民事起诉状").count();
        assert_eq!(cnt, 1, "重复标题应被去重,实际出现 {} 次", cnt);
        // 且标题用方正小标宋,不是黑体
        assert!(x.contains("方正小标宋简体"));
    }

    #[test]
    fn xml_escaped() {
        // pulldown 会把 < > & 拆成多个 Text run,各自转义;断言三类实体都出现且无裸 `<乙`
        let x = xml_of("T", "甲<乙>丙&丁");
        assert!(x.contains("&lt;"), "< 应转义");
        assert!(x.contains("&gt;"), "> 应转义");
        assert!(x.contains("&amp;"), "& 应转义");
        // body 区不得出现未转义的标签起始(排除 <w: OOXML 标签本身)
        assert!(!x.contains("<乙"), "不得有裸 <乙");
    }

    #[test]
    fn table_renders_tbl() {
        let md = "| 序号 | 证据名 |\n|---|---|\n| 1 | 合同 |";
        let x = xml_of("证据目录", md);
        assert!(x.contains("<w:tbl>"), "GFM 表格应转 w:tbl");
        assert!(x.contains("<w:tblBorders>"), "表格应有边框");
        assert!(x.contains("序号") && x.contains("合同"));
    }

    #[test]
    fn builds_valid_zip_with_required_parts() {
        let bytes = build_filing_docx_bytes("民事起诉状", "## 诉讼请求\n\n一、判令...").unwrap();
        assert!(bytes.len() > 500, "docx 字节过小");
        let reader = std::io::Cursor::new(bytes);
        let mut zip = zip::ZipArchive::new(reader).expect("应是合法 zip");
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        for need in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/styles.xml",
            "word/settings.xml",
        ] {
            assert!(names.iter().any(|n| n == need), "docx 缺部件 {}", need);
        }
    }

    #[test]
    fn extract_title_from_filing_header() {
        let md = "<!-- filing · doc_type=民事起诉状 · title=张三诉李四案 · ts=2026-05-31T00:00:00Z -->\n\n# 一、诉讼请求\n\n判令...";
        assert_eq!(extract_filing_title(md).as_deref(), Some("张三诉李四案"));
        // 标题不进正文(注释被 strip)
        let x = render_document_xml("张三诉李四案", md, Profile::Filing);
        assert!(!x.contains("doc_type="), "元信息头不应进 docx");
        assert!(!x.contains("ts=2026"), "时间戳不应进 docx");
    }

    #[test]
    fn extract_title_none_when_no_header() {
        assert_eq!(extract_filing_title("# 一、诉讼请求\n\n正文"), None);
    }

    #[test]
    fn document_xml_has_sectpr_with_docgrid() {
        let x = xml_of("T", "正文");
        assert!(
            x.contains("w:linePitch=\"360\""),
            "应保留 docGrid linePitch=360"
        );
        assert!(x.contains("w:w=\"11906\""), "应 A4 宽");
        assert!(x.contains("w:top=\"1440\""), "应 1 英寸上边距");
    }

    // ───────── base 档(报告/通用 MD)专属行为 ─────────

    #[test]
    fn base_unordered_list_has_bullet() {
        // base:无序列表项带圆点 + 左悬挂缩进
        let x = xml_base("报告", "- 第一点\n- 第二点");
        assert!(x.contains("•"), "base 无序列表应有圆点");
        assert!(x.contains("第一点") && x.contains("第二点"));
        assert!(x.contains("w:hanging=\"280\""), "列表项应悬挂缩进");
    }

    #[test]
    fn filing_unordered_list_no_bullet() {
        // filing:沿用旧行为,无序列表不加圆点(法律文书不用 markdown 圆点)
        let x = xml_of("起诉状", "- 第一点\n- 第二点");
        assert!(!x.contains("•"), "filing 不应加圆点");
        assert!(x.contains("第一点") && x.contains("第二点"), "内容仍保留");
    }

    #[test]
    fn base_rule_renders_border_filing_drops() {
        let md = "上文。\n\n---\n\n下文。";
        let xb = xml_base("报告", md);
        assert!(xb.contains("<w:pBdr>"), "base 应把 --- 渲染成下边框段");
        let xf = xml_of("起诉状", md);
        assert!(!xf.contains("<w:pBdr>"), "filing 应丢弃 ---(沿用旧行为)");
    }

    #[test]
    fn base_nested_list_deeper_indent() {
        // 嵌套无序列表:第二层左缩进应比第一层大
        let x = xml_base("报告", "- 一层\n  - 二层");
        assert!(x.contains("w:left=\"700\""), "一层 left=420*1+280=700");
        assert!(x.contains("w:left=\"1120\""), "二层 left=420*2+280=1120");
    }

    #[test]
    fn base_ordered_list_numbers_and_indents() {
        let x = xml_base("报告", "1. 甲\n2. 乙");
        assert!(x.contains("1. 甲") || x.contains("1. "), "有序编号写进文本");
        assert!(x.contains("乙"));
        assert!(x.contains("w:hanging=\"280\""), "有序列表项也悬挂缩进");
    }

    #[test]
    fn base_keeps_fangsong_and_table() {
        // base 与 filing 共享排版:仿宋正文 + 表格边框
        let x = xml_base(
            "案件分析报告",
            "正文一段。\n\n| 日期 | 事件 |\n|---|---|\n| 今天 | 立案 |",
        );
        assert!(x.contains("仿宋_GB2312"), "base 正文仿宋");
        assert!(
            x.contains("<w:tbl>") && x.contains("<w:tblBorders>"),
            "base 表格带边框"
        );
        assert!(x.contains("立案"));
    }

    #[test]
    fn base_report_builds_valid_docx() {
        let bytes = build_report_docx_bytes(
            "案件分析报告",
            "## 案件概况\n\n- 要点一\n- 要点二\n\n正文。",
        )
        .unwrap();
        assert!(bytes.len() > 500, "report docx 字节过小");
        let reader = std::io::Cursor::new(bytes);
        let zip = zip::ZipArchive::new(reader).expect("应是合法 zip");
        assert!(zip.len() >= 5, "缺部件");
    }
}
