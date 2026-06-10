//! 多案件文件夹检测(Phase 1 · 2026-06-04 · 纯结构启发式,只读不写库)。
//!
//! 详见 `docs/提案-多案件文件夹识别-2026-06-04.md`。
//!
//! 目标:拖一个文件夹进来,判断它该拆成 1~N 个案件。覆盖三类:
//! - **单案件**(保底):文件夹直接放文档 → 1 个案件。
//! - **多案·已整理**:顶层 `02_案件A/ 03_案件B/ 01_原告与共用证据/` → N 案件 + 共用材料。
//! - **年份大文件夹**:`2026/{张三案/, 李四案/, …}` 递归展开成很多案件。
//!
//! ## 核心判别(advisor 修正:先剔除再计数,否则案件内的阶段子目录会被误拆成多案)
//! `collect_cases(D)`:
//! 1. children = D 的子目录 **减去** {阶段词 / 共用词 / 杂项词} 命中的;
//! 2. case_candidates = children 里**(递归)含文档**的;
//! 3. `len(case_candidates) >= 2` → D 是容器(递归展开每个候选);否则 D 自己是一个案件。
//!
//! 关键:`02_案件A/` 的子目录全是阶段目录(诉讼文书/法院文书/最终结果/盖章扫描)→ 被剔除 →
//! 0 个候选 → `02_案件A` 收敛成**一个**案件;而 `张三/` 下 `02_案件A`+`03_案件B` 作为候选存活
//! → 2 个 → `张三` 判为容器 → 拆成两案。

use std::path::{Path, PathBuf};

use serde::Serialize;

/// 阶段子目录词表:命中(剥前缀后 **starts_with**)= 案件**内部**的组织子目录,不单独成案。
/// 用 starts_with 而非 contains —— 否则「张三借贷材料」这类**案件名**会被「材料」误判成阶段目录。
/// 故只收**具体**阶段词,且要求目录名以其开头(`01_诉讼文书`→`诉讼文书`✓ / `张三执行案`✗)。
const STAGE_HINTS: &[&str] = &[
    "诉讼文书",
    "法院文书",
    "最终结果",
    "盖章扫描",
    "执行",
    "证据",
    "身份",
];
/// 共用材料词表:必须含「分享」语义(contains;「证据」单独不算,避免和证据阶段目录撞)。
const SHARED_HINTS: &[&str] = &["共用", "共享", "共同", "通用"];
/// 杂项/忽略目录词表(contains)。
const MISC_HINTS: &[&str] = &["后续", "待整理", "归档备份", "其他", "其它"];
/// 不进检测的目录(隐藏 / 系统 / 依赖)。
const IGNORED_DIRS: &[&str] = &[".git", "node_modules", "__MACOSX", ".idea", ".vscode"];
/// 文档型扩展名(用于"目录是否含材料"的计数)。
const DOC_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "txt", "rtf", "odt", "md", "png", "jpg", "jpeg", "webp", "tiff", "bmp",
    "gif", "jp2", "xls", "xlsx", "csv",
];

/// 递归深度上限(年份 → 案件 → 阶段 ≈ 3,留一层余量)。
const MAX_DEPTH: usize = 4;

/// 一个候选案件(对应一个子目录)。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CaseCandidate {
    /// 案件根目录绝对路径(导入时作 source_folder,天然唯一)
    pub dir: String,
    /// 建议案件名(剥掉 `NN_` 序号前缀的目录名)
    pub suggested_name: String,
    /// 目录内(递归)文档数
    pub doc_count: usize,
    /// 是否含阶段子目录(强信号:这是一个组织过的案件)
    pub has_stage_subdirs: bool,
}

/// 一个被忽略的目录及原因。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IgnoredDir {
    pub path: String,
    pub reason: String,
}

/// 拆分预案(只读检测产物,交前端确认弹窗用)。
#[derive(Debug, Clone, Serialize)]
pub struct ImportPlan {
    pub root: String,
    /// 候选案件(已按目录排序)
    pub cases: Vec<CaseCandidate>,
    /// 共用材料目录(Phase 1 先挂主案)
    pub shared_dirs: Vec<String>,
    /// 被忽略的目录(杂项 / 产物 / 空目录)
    pub ignored: Vec<IgnoredDir>,
    /// 是否建议拆分(置信度 medium+;false = 走保底单案)
    pub multi: bool,
    /// 根文件夹此前是否已作为「单个案件」导入过(命令层查 DB 填;拆分会与旧案重复 → 前端告警)
    pub root_already_imported: bool,
}

// ───────────────────────── 名称归一化 + 词表匹配 ─────────────────────────

/// 剥掉目录名开头的 `NN_` / `NN-` / `NN.` / `NN ` 序号前缀(参考文件夹大量使用)。
fn strip_num_prefix(name: &str) -> &str {
    let trimmed = name.trim_start_matches(|c: char| {
        c.is_ascii_digit() || c == '_' || c == '-' || c == ' ' || c == '.' || c == '、'
    });
    if trimmed.is_empty() {
        name
    } else {
        trimmed
    }
}

/// 阶段目录:剥前缀后 **以**某阶段词**开头**(精确,不误伤含该词的案件名)。
fn is_stage_dir(name: &str) -> bool {
    let n = strip_num_prefix(name);
    STAGE_HINTS.iter().any(|h| n.starts_with(h))
}
/// 共用 / 杂项:剥前缀后**包含**该词(描述性命名,如「原告与共用证据」)。
fn is_shared_dir(name: &str) -> bool {
    let n = strip_num_prefix(name);
    SHARED_HINTS.iter().any(|h| n.contains(h))
}
fn is_misc_dir(name: &str) -> bool {
    let n = strip_num_prefix(name);
    MISC_HINTS.iter().any(|h| n.contains(h))
}

// ───────────────────────── 目录遍历工具 ─────────────────────────

fn dir_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 列出直接子目录(过滤隐藏 / 系统 / 依赖)。
fn list_subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = dir_name(&p);
        if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        out.push(p);
    }
    out.sort();
    out
}

/// 是否文档型文件(按扩展名)。
fn is_doc_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| DOC_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 递归数目录内的文档数(深度受限,跳过隐藏/系统目录)。
fn doc_count_recursive(dir: &Path, depth: usize) -> usize {
    if depth > MAX_DEPTH {
        return 0;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        let name = dir_name(&p);
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            n += doc_count_recursive(&p, depth + 1);
        } else if is_doc_file(&p) {
            n += 1;
        }
    }
    n
}

/// 该目录是否有阶段子目录(强信号:组织过的案件)。
fn has_stage_subdirs(dir: &Path) -> bool {
    list_subdirs(dir).iter().any(|s| is_stage_dir(&dir_name(s)))
}

fn make_candidate(dir: &Path) -> CaseCandidate {
    let raw = dir_name(dir);
    CaseCandidate {
        dir: dir.to_string_lossy().to_string(),
        suggested_name: strip_num_prefix(&raw).to_string(),
        doc_count: doc_count_recursive(dir, 0),
        has_stage_subdirs: has_stage_subdirs(dir),
    }
}

// ───────────────────────── 递归收集案件 ─────────────────────────

/// 收集 `dir` 子树里的案件。容器 → 递归展开;否则 `dir` 自己是一个案件。
fn collect_cases(dir: &Path, depth: usize) -> Vec<CaseCandidate> {
    if depth > MAX_DEPTH {
        return leaf(dir);
    }
    // 先剔除 阶段/共用/杂项 子目录,剩下的才是"可能是子案件"的候选
    let candidate_children: Vec<PathBuf> = list_subdirs(dir)
        .into_iter()
        .filter(|s| {
            let n = dir_name(s);
            !is_stage_dir(&n) && !is_shared_dir(&n) && !is_misc_dir(&n)
        })
        .filter(|s| doc_count_recursive(s, 0) > 0)
        .collect();

    if candidate_children.len() >= 2 {
        // 容器:递归展开每个候选
        candidate_children
            .iter()
            .flat_map(|c| collect_cases(c, depth + 1))
            .collect()
    } else {
        leaf(dir)
    }
}

/// `dir` 作为单个案件(若含文档)。
fn leaf(dir: &Path) -> Vec<CaseCandidate> {
    if doc_count_recursive(dir, 0) > 0 {
        vec![make_candidate(dir)]
    } else {
        Vec::new()
    }
}

// ───────────────────────── 顶层 plan ─────────────────────────

/// 检测一个文件夹的拆分预案(只读)。
pub fn plan_folder(root: &Path) -> ImportPlan {
    let root_str = root.to_string_lossy().to_string();
    let subdirs = list_subdirs(root);

    // 顶层分流:共用 / 杂项 / 候选
    let mut shared_dirs = Vec::new();
    let mut ignored = Vec::new();
    let mut candidate_children = Vec::new();
    for s in &subdirs {
        let n = dir_name(s);
        if is_shared_dir(&n) {
            shared_dirs.push(s.to_string_lossy().to_string());
        } else if is_misc_dir(&n) {
            ignored.push(IgnoredDir {
                path: s.to_string_lossy().to_string(),
                reason: "杂项/补充目录".to_string(),
            });
        } else if is_stage_dir(&n) {
            // 顶层就是阶段目录 → root 本身是单个案件,不参与拆分
        } else if doc_count_recursive(s, 0) > 0 {
            candidate_children.push(s.clone());
        } else {
            ignored.push(IgnoredDir {
                path: s.to_string_lossy().to_string(),
                reason: "空目录(无文档)".to_string(),
            });
        }
    }

    // 候选 < 2 → 保底:整个 root 作单个案件
    if candidate_children.len() < 2 {
        return single_case_plan(root, &root_str);
    }

    // 候选 ≥ 2 → 递归展开成案件
    let cases: Vec<CaseCandidate> = candidate_children
        .iter()
        .flat_map(|c| collect_cases(c, 1))
        .collect();

    // 置信度门控:≥2 个候选各自(有阶段子目录 或 ≥2 文档)才真拆;否则保底
    let strong = cases
        .iter()
        .filter(|c| c.has_stage_subdirs || c.doc_count >= 2)
        .count();
    if strong < 2 {
        return single_case_plan(root, &root_str);
    }

    ImportPlan {
        root: root_str,
        cases,
        shared_dirs,
        ignored,
        multi: true,
        root_already_imported: false,
    }
}

/// 保底:整个文件夹 = 一个案件(行为同现状单案导入)。
fn single_case_plan(root: &Path, root_str: &str) -> ImportPlan {
    let name = strip_num_prefix(&dir_name(root)).to_string();
    let name = if name.is_empty() {
        "未命名案件".to_string()
    } else {
        name
    };
    ImportPlan {
        root: root_str.to_string(),
        cases: vec![CaseCandidate {
            dir: root_str.to_string(),
            suggested_name: name,
            doc_count: doc_count_recursive(root, 0),
            has_stage_subdirs: has_stage_subdirs(root),
        }],
        shared_dirs: Vec::new(),
        ignored: Vec::new(),
        multi: false,
        root_already_imported: false,
    }
}

// ───────────────────────── 测试 ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 在临时目录里建一棵目录树。`files` 是相对路径列表,自动建中间目录 + 空文件。
    fn make_tree(files: &[&str]) -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        for rel in files {
            let p = td.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, b"x").unwrap();
        }
        td
    }

    /// 案件内含阶段子目录 → **收敛成 1 个案件**(advisor 指定的头号测试,防误拆)。
    #[test]
    fn stage_organized_case_stays_single() {
        let td = make_tree(&[
            "01_诉讼文书/起诉状.pdf",
            "01_诉讼文书/授权委托书.pdf",
            "02_法院文书/受理通知.pdf",
            "03_最终结果/调解书.pdf",
            "99_盖章扫描备份/全套.pdf",
        ]);
        let plan = plan_folder(td.path());
        assert!(!plan.multi, "阶段子目录不该把单案拆开");
        assert_eq!(plan.cases.len(), 1, "应收敛成 1 个案件");
    }

    /// 参考文件夹结构:01_共用 + 02_案件A + 03_案件B + 00_总览 + 04_后续 → 2 案 + 1 共用。
    #[test]
    fn reference_two_case_folder_splits() {
        let td = make_tree(&[
            "00_案件总览.md",
            "案件看板.html",
            "01_原告与共用证据/身份证.png",
            "01_原告与共用证据/流水.pdf",
            "02_案件A_张三+李四(50万)/01_诉讼文书/起诉状.pdf",
            "02_案件A_张三+李四(50万)/02_法院文书/调解书.pdf",
            "03_案件B_张三+王五(20万)/01_诉讼文书/起诉状.pdf",
            "03_案件B_张三+王五(20万)/04_执行/强制执行申请书.pdf",
            "04_后续补充材料/新材料.pdf",
        ]);
        let plan = plan_folder(td.path());
        assert!(plan.multi, "应判为多案");
        assert_eq!(plan.cases.len(), 2, "应拆成 2 个案件");
        assert_eq!(plan.shared_dirs.len(), 1, "应识别 1 个共用目录");
        assert!(plan.shared_dirs[0].contains("共用"));
        // 案件名剥掉序号前缀
        let names: Vec<&str> = plan
            .cases
            .iter()
            .map(|c| c.suggested_name.as_str())
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("案件A")),
            "名应去 02_ 前缀: {:?}",
            names
        );
        assert!(names.iter().any(|n| n.starts_with("案件B")));
        // 04_后续 进 ignored
        assert!(plan.ignored.iter().any(|i| i.path.contains("后续")));
    }

    /// 年份大文件夹:2026/{张三案(含阶段), 李四案(含阶段), 王五案(含阶段)} → 3 案。
    #[test]
    fn year_folder_expands_to_many_cases() {
        let td = make_tree(&[
            "张三借贷案/01_诉讼文书/起诉状.pdf",
            "张三借贷案/02_法院文书/判决书.pdf",
            "李四合同案/01_诉讼文书/起诉状.pdf",
            "李四合同案/02_法院文书/判决书.pdf",
            "王五离婚案/01_诉讼文书/起诉状.pdf",
            "王五离婚案/03_最终结果/调解书.pdf",
        ]);
        let plan = plan_folder(td.path());
        assert!(plan.multi);
        assert_eq!(plan.cases.len(), 3, "年份文件夹应展开成 3 案");
    }

    /// 嵌套容器:年份/{张三系列(双案), 周九单案} → 张三系列再展开成 2,合计 3。
    #[test]
    fn nested_container_recurses() {
        let td = make_tree(&[
            "张三民间借贷/01_共用证据/流水.pdf",
            "张三民间借贷/02_案件A/01_诉讼文书/起诉状.pdf",
            "张三民间借贷/02_案件A/02_法院文书/调解书.pdf",
            "张三民间借贷/03_案件B/01_诉讼文书/起诉状.pdf",
            "张三民间借贷/03_案件B/04_执行/执行申请.pdf",
            "周九独立案/01_诉讼文书/起诉状.pdf",
            "周九独立案/02_法院文书/判决书.pdf",
        ]);
        let plan = plan_folder(td.path());
        assert!(plan.multi);
        assert_eq!(plan.cases.len(), 3, "张三系列(2)+周九(1)=3");
    }

    /// 单案件文件夹(直接放文档)→ 保底 1 案,不拆。
    #[test]
    fn flat_single_case_folder() {
        let td = make_tree(&["起诉状.pdf", "判决书.pdf", "证据1.png"]);
        let plan = plan_folder(td.path());
        assert!(!plan.multi);
        assert_eq!(plan.cases.len(), 1);
        assert_eq!(plan.cases[0].dir, td.path().to_string_lossy());
    }

    /// 弱信号:两个非词表子目录、无阶段子目录、各自只有 1 个文档 → 置信度门控不过 → 保底单案。
    #[test]
    fn weak_signal_falls_back_to_single() {
        let td = make_tree(&["客户甲/a.pdf", "客户乙/b.pdf"]);
        let plan = plan_folder(td.path());
        assert!(
            !plan.multi,
            "弱信号(各 1 文档无阶段)应保底单案,避免过度拆分"
        );
        assert_eq!(plan.cases.len(), 1);
    }

    /// 案件名含阶段词但不以其开头(如「张三借贷材料」)→ 不被误判成阶段目录。
    #[test]
    fn case_name_containing_stage_word_not_filtered() {
        let td = make_tree(&[
            "张三借贷材料/01_诉讼文书/起诉状.pdf",
            "张三借贷材料/02_法院文书/判决书.pdf",
            "李四合同证据卷/01_诉讼文书/起诉状.pdf",
            "李四合同证据卷/02_法院文书/判决书.pdf",
        ]);
        let plan = plan_folder(td.path());
        assert!(
            plan.multi,
            "「…材料」「…证据卷」是案件名不该被当阶段目录吞掉"
        );
        assert_eq!(plan.cases.len(), 2);
    }

    /// 序号前缀剥离 + 词表匹配。
    #[test]
    fn vocab_and_prefix() {
        assert_eq!(strip_num_prefix("02_案件A_张三"), "案件A_张三");
        assert_eq!(strip_num_prefix("01_诉讼文书"), "诉讼文书");
        assert_eq!(strip_num_prefix("2025"), "2025"); // 纯数字保留
        assert!(is_stage_dir("01_诉讼文书"));
        assert!(is_stage_dir("证据")); // 证据是阶段,不是共用
        assert!(!is_shared_dir("01_原告与证据")); // 无分享词 → 不算共用
        assert!(is_shared_dir("01_原告与共用证据")); // 含"共用" → 共用
        assert!(is_misc_dir("04_后续补充材料"));
    }
}
