//! 文件夹扫描器(纯规则,不调 LLM)。
//!
//! 设计基于真实案件目录结构观察(详见 conversation history + `docs/`):
//! - 顶层目录天然是阶段化的:`立案材料 / 一审 / 二审 / 执行 / 证据材料 / 身份信息`
//! - AI 产物(总览/调查/精要/汇报)单独识别,**不混在普通文档里**
//! - `_archive / 归档 / .DS_Store / node_modules` 等噪音目录/文件全部忽略

use serde::Serialize;
use std::path::Path;
use walkdir::WalkDir;

/// 扫描出来的单个文档元数据。
///
/// 注意:**只记录路径,不复制原文件**。这是 CaseBoard 的核心铁律。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScannedDoc {
    /// 原文件绝对路径(只读引用,不复制)
    pub source_path: String,
    /// 文件名(不含路径)
    pub filename: String,
    /// 阶段:立案 / 一审 / 二审 / 再审 / 执行 / 证据 / 身份信息 / None
    pub stage: Option<String>,
    /// 类别:起诉状 / 判决书 / 笔录 / ... / None
    pub category: Option<String>,
    /// 是否是 AI 生成的中间产物(.md/.html 含"总览"等关键词)
    pub is_ai_artifact: bool,
    /// 文件大小(字节)
    pub size_bytes: u64,
    /// 文件最后修改时间(ISO 8601),2026-05-23 晚十 加 — 用于缓存键
    pub modified_at: Option<String>,
}

/// 这些**文件名**直接忽略(macOS/Windows 噪音)
const IGNORED_FILES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini", ".gitkeep"];

/// 这些**目录名**整个跳过(归档、依赖、版本控制)
const IGNORED_DIRS: &[&str] = &[
    "_archive",
    "归档",
    ".git",
    "node_modules",
    ".idea",
    ".vscode",
];

/// 根据**路径**(注意是路径,不是文件名)识别阶段。
///
/// 规则:遍历路径的每一段,首次命中即返回。顺序按"具体优先"排:
/// 再审 > 二审 > 一审 > 立案 > 执行 > 证据 > 身份
fn classify_stage(path: &Path) -> Option<String> {
    // 路径里的中文段
    let segments: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    let joined = segments.join("/");

    // 注意顺序:更具体的先匹配,避免"二审"被"一审"误命中
    if joined.contains("再审") {
        return Some("再审".into());
    }
    if joined.contains("二审") || joined.contains("2审") {
        return Some("二审".into());
    }
    if joined.contains("一审") || joined.contains("1审") {
        return Some("一审".into());
    }
    if joined.contains("立案") || joined.contains("起诉材料") {
        return Some("立案".into());
    }
    if joined.contains("执行") {
        return Some("执行".into());
    }
    if joined.contains("证据") || joined.contains("物证") {
        return Some("证据".into());
    }
    if joined.contains("身份") || joined.contains("主体") {
        return Some("身份信息".into());
    }
    None
}

/// 根据**文件名**识别类别(诉讼文书类型)。
fn classify_category(filename: &str) -> Option<String> {
    let f = filename;
    // 顺序很重要:更具体/优先级高的放前面
    // 注释里的 [R&D] 标记来自 2026-05-23 跑 5 个真实案件发现的命名习惯

    // 诉状类
    if f.contains("民事诉状") || f.contains("起诉状") || f.contains("要素式诉状") {
        return Some("起诉状".into());
    }
    if f.contains("上诉状") {
        return Some("上诉状".into());
    }
    if f.contains("反诉") {
        return Some("反诉状".into());
    }
    if f.contains("答辩") {
        return Some("答辩状".into());
    }
    if f.contains("管辖权异议") {
        return Some("管辖权异议".into()); // [R&D] 03-杰瑞典当
    }

    // 裁判文书
    if f.contains("判决书") {
        return Some("判决书".into());
    }
    if f.contains("裁定书") {
        return Some("裁定书".into());
    }
    if f.contains("调解书") {
        return Some("调解书".into());
    }
    if f.contains("调解协议") {
        return Some("调解书".into());
    }

    // 程序类
    if f.contains("受理案件通知") || f.contains("案件受理通知") || f.contains("受理通知")
    {
        return Some("受理通知".into()); // 跟 aggregator 优先级一致
    }
    if f.contains("应诉通知") {
        return Some("应诉通知".into());
    }
    if f.contains("举证通知") {
        return Some("举证通知".into());
    }
    if f.contains("传票") {
        return Some("传票".into());
    }
    if f.contains("开庭通知") || f.contains("开庭传票") {
        return Some("开庭通知".into());
    }
    if f.contains("送达地址") {
        return Some("送达地址确认书".into());
    }
    if f.contains("送达回证") {
        return Some("送达回证".into());
    }
    if f.contains("延期申请") {
        return Some("延期申请".into()); // [R&D] 03-杰瑞典当
    }
    if f.contains("出庭函") {
        return Some("出庭函".into()); // [R&D] 案件 05
    }

    // 笔录类
    if f.contains("庭审笔录") {
        return Some("庭审笔录".into());
    }
    if f.contains("询问笔录") {
        return Some("询问笔录".into());
    }
    if f.contains("笔录") || f.contains("谈话") {
        return Some("笔录".into());
    }

    // 律师工作产物 [R&D 发现的高频文档]
    if f.contains("代理合同") || f.contains("委托合同") || f.contains("委托书") {
        return Some("委托合同".into());
    }
    if f.contains("代理意见") || f.contains("代理词") {
        return Some("代理意见".into());
    }
    if f.contains("辩护词") {
        return Some("辩护词".into());
    }
    if f.contains("民事诉讼案件收案呈批") || f.contains("收案呈批") {
        return Some("收案呈批表".into()); // [R&D] 5 个案件每个都有
    }
    if f.contains("办案笔记") {
        return Some("办案笔记".into()); // [R&D] 01/03
    }
    if f.contains("风险告知") {
        return Some("风险告知".into()); // [R&D] 04/05
    }
    if f.contains("律师工作反馈") || f.contains("反馈卡") {
        return Some("反馈卡".into()); // [R&D] 04/05
    }
    if f.contains("介绍信") {
        return Some("介绍信".into()); // [R&D] 03
    }
    if f.contains("律师函") {
        return Some("律师函".into());
    }
    if f.contains("催告函") || f.contains("催款") {
        return Some("催告函".into());
    }
    if f.contains("敦促履约") || f.contains("严正通知") {
        return Some("催告函".into()); // [R&D] 05
    }
    if f.contains("诉讼思路") {
        return Some("办案笔记".into()); // [R&D] 05
    }

    // 证据类
    if f.contains("证据清单") || f.contains("证据目录") || f.contains("举证清单") {
        return Some("证据清单".into());
    }
    if f.contains("邮寄证据") || (f.contains("邮寄") && f.contains("记录")) {
        return Some("邮寄证据".into()); // [R&D] 03
    }

    // 保全 + 执行
    if f.contains("财产保全") || f.contains("保全申请") {
        return Some("财产保全".into()); // 跟 aggregator 优先级一致
    }
    if f.contains("申请执行") || f.contains("强制执行") {
        return Some("执行申请".into());
    }
    if f.contains("执行通知") {
        return Some("执行通知".into());
    }
    if f.contains("执行查询") || f.contains("查控申请") {
        return Some("查控申请".into()); // [R&D] 05
    }
    if f.contains("限消") || f.contains("限制消费") {
        return Some("限制消费令".into());
    }
    if f.contains("失信") {
        return Some("失信被执行人".into());
    }
    if f.contains("终本") {
        return Some("终本裁定".into());
    }

    // 财务
    if f.contains("诉讼费") {
        return Some("诉讼费".into());
    }
    if f.contains("缴费通知") || f.contains("缴费") {
        return Some("缴费通知".into());
    }
    if f.contains("发票") {
        return Some("发票".into());
    }
    if f.contains("收据") || f.contains("收款凭证") {
        return Some("收据".into());
    }

    // 基础合同/证据
    if f.contains("合同") {
        return Some("合同".into());
    }
    if f.contains("协议") {
        return Some("协议".into());
    }
    if f.contains("欠条") {
        return Some("欠条".into());
    }
    if f.contains("借条") {
        return Some("借条".into());
    }
    if f.contains("银行流水") {
        return Some("银行流水".into());
    }

    // 身份信息
    if f.contains("身份证") {
        return Some("身份证".into());
    }
    if f.contains("户口") {
        return Some("户口".into());
    }
    if f.contains("营业执照") {
        return Some("营业执照".into());
    }
    if f.contains("授权委托") {
        return Some("授权委托书".into());
    }

    None
}

/// 判断是否是 AI 跑出来的中间产物(总览/调查/精要/汇报/yuandian)。
fn is_ai_artifact(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    let is_text_doc = lower.ends_with(".md") || lower.ends_with(".html") || lower.ends_with(".htm");
    if !is_text_doc {
        return false;
    }
    // 命中任一关键词即认为是 AI 产物
    const HINTS: &[&str] = &[
        "总览", "调查", "精要", "汇报", "yuandian", "summary", "overview",
    ];
    // HINTS 全是小写 ASCII 或 CJK,lower.contains 是 filename.contains 的超集
    HINTS.iter().any(|w| lower.contains(w))
}

/// 扫描一个案件文件夹,返回所有有效文档的元数据。
///
/// 自动忽略:
/// - 噪音文件(`.DS_Store` 等)
/// - 归档目录(`_archive / 归档`)
/// - 版本控制 / 依赖 / IDE 目录
///
/// 不读取文件内容,纯元数据 + 文件名规则。
pub fn scan_folder(root: &Path) -> Vec<ScannedDoc> {
    let mut docs = Vec::new();

    let walker = WalkDir::new(root).into_iter().filter_entry(|e| {
        // 跳过整个忽略目录(不递归进去)
        let name = e.file_name().to_string_lossy();
        !IGNORED_DIRS.iter().any(|d| name.as_ref() == *d)
    });

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();
        if IGNORED_FILES.contains(&filename.as_str()) {
            continue;
        }
        // 隐藏文件也跳过(以 . 开头)
        if filename.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let meta = entry.metadata();
        let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified_at = meta.as_ref().ok().and_then(|m| m.modified().ok()).map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        });

        docs.push(ScannedDoc {
            source_path: path.to_string_lossy().to_string(),
            filename: filename.clone(),
            stage: classify_stage(path),
            category: classify_category(&filename),
            is_ai_artifact: is_ai_artifact(&filename),
            size_bytes,
            modified_at,
        });
    }

    docs
}

// ============================================================================
// 单元测试 —— 用通用/虚构的文件名,不暴露任何真实当事人/案件信息
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn classify_stage_finds_yi_shen() {
        let p = Path::new("/tmp/案件/一审/民事判决书.pdf");
        assert_eq!(classify_stage(p), Some("一审".into()));
    }

    #[test]
    fn classify_stage_finds_er_shen_not_yi_shen() {
        // 二审目录里也可能有"一审"字样的文件,但路径段是"二审",应该返回"二审"
        let p = Path::new("/tmp/案件/二审/三被告上诉状.pdf");
        assert_eq!(classify_stage(p), Some("二审".into()));
    }

    #[test]
    fn classify_stage_finds_zhi_xing() {
        let p = Path::new("/tmp/案件/执行/执行查询原文件/限消令.pdf");
        assert_eq!(classify_stage(p), Some("执行".into()));
    }

    #[test]
    fn classify_stage_finds_li_an() {
        let p = Path::new("/tmp/案件/立案材料/民事诉状.docx");
        assert_eq!(classify_stage(p), Some("立案".into()));
    }

    #[test]
    fn classify_stage_returns_none_for_unknown() {
        let p = Path::new("/tmp/案件/some_random_dir/something.pdf");
        assert_eq!(classify_stage(p), None);
    }

    #[test]
    fn classify_category_recognizes_common_documents() {
        assert_eq!(classify_category("民事诉状.docx"), Some("起诉状".into()));
        assert_eq!(classify_category("民事判决书.pdf"), Some("判决书".into()));
        assert_eq!(classify_category("民事裁定书.pdf"), Some("裁定书".into()));
        assert_eq!(
            classify_category("开庭笔录20240101.pdf"),
            Some("笔录".into())
        );
        assert_eq!(classify_category("证据清单.docx"), Some("证据清单".into()));
        // 注意: 2026-05-23 后,分类名跟 aggregator 优先级表对齐,
        // "财产保全申请" -> "财产保全",更细的类型在文件名里区分
        assert_eq!(
            classify_category("财产保全申请.docx"),
            Some("财产保全".into())
        );
        assert_eq!(classify_category("申请执行书.doc"), Some("执行申请".into()));
        assert_eq!(classify_category("代理合同.docx"), Some("委托合同".into()));
        assert_eq!(
            classify_category("送达地址确认书.pdf"),
            Some("送达地址确认书".into())
        );
    }

    #[test]
    fn classify_category_returns_none_for_unknown() {
        assert_eq!(classify_category("一些不知道的文件.pdf"), None);
    }

    #[test]
    fn detect_ai_artifact_correctly() {
        // 正面用例
        assert!(is_ai_artifact("案件总览.md"));
        assert!(is_ai_artifact("执行调查_详细查阅版.md"));
        assert!(is_ai_artifact("yuandian_深查_20260101.md"));
        assert!(is_ai_artifact("财产线索精要.html"));
        assert!(is_ai_artifact("团队汇报简版.md"));
        assert!(is_ai_artifact("case_summary.md"));

        // 负面用例
        assert!(!is_ai_artifact("民事诉状.docx")); // 不是 md/html
        assert!(!is_ai_artifact("普通笔记.md")); // md 但无关键词
        assert!(!is_ai_artifact("民事判决书.pdf")); // 不是文本格式
    }

    /// 集成测试:在临时目录造一个迷你案件结构,验证 scan_folder 的完整产出。
    ///
    /// 用 std::env::temp_dir() 而不是依赖任何真实路径,这样测试可移植、可在 CI 跑。
    #[test]
    fn scan_folder_handles_realistic_structure() {
        // 1) 在 temp 里造一个假案件
        let tmp = std::env::temp_dir().join("caseboard_test_scan");
        let _ = fs::remove_dir_all(&tmp); // 清理上次残留
        let dirs = [
            "立案材料",
            "一审",
            "二审",
            "执行",
            "执行/执行查询原文件",
            "执行/_archive", // 这个应该被忽略
            "证据材料",
            "身份信息",
            "_archive", // 顶层归档,应该被忽略
        ];
        for d in &dirs {
            fs::create_dir_all(tmp.join(d)).unwrap();
        }
        // 造一些文件
        let files: &[(&str, &str)] = &[
            ("立案材料/民事诉状.docx", "诉状内容占位"),
            ("立案材料/财产保全申请.docx", "保全申请占位"),
            ("一审/民事判决书.pdf", "判决书占位"),
            ("一审/开庭笔录.pdf", "笔录占位"),
            ("二审/上诉状.pdf", "上诉状占位"),
            ("执行/申请执行书.doc", "执行申请占位"),
            ("执行/执行查询原文件/限消令.pdf", "限消令占位"),
            ("执行/案件总览.md", "AI 跑的总览占位"),
            ("执行/_archive/旧版调查.md", "归档里的,应该被忽略"),
            ("证据材料/证据清单.docx", "证据清单占位"),
            ("身份信息/身份证.png", "身份占位"),
            (".DS_Store", "应该被忽略"),
            ("案件总览.md", "根目录 AI 产物"),
        ];
        for (rel, content) in files {
            fs::write(tmp.join(rel), content).unwrap();
        }

        // 2) 跑扫描
        let docs = scan_folder(&tmp);

        // 3) 验证结果
        let by_name: std::collections::HashMap<&str, &ScannedDoc> =
            docs.iter().map(|d| (d.filename.as_str(), d)).collect();

        // 关键文件都扫到了
        assert!(by_name.contains_key("民事诉状.docx"));
        assert!(by_name.contains_key("民事判决书.pdf"));
        assert!(by_name.contains_key("上诉状.pdf"));
        assert!(by_name.contains_key("限消令.pdf"));
        assert!(by_name.contains_key("案件总览.md"));
        assert!(by_name.contains_key("证据清单.docx"));

        // 归档和噪音被忽略
        assert!(!by_name.contains_key(".DS_Store"));
        assert!(!by_name.contains_key("旧版调查.md")); // 在 _archive 里

        // 分类正确
        let suzhuang = by_name.get("民事诉状.docx").unwrap();
        assert_eq!(suzhuang.stage.as_deref(), Some("立案"));
        assert_eq!(suzhuang.category.as_deref(), Some("起诉状"));
        assert!(!suzhuang.is_ai_artifact);

        let panjue = by_name.get("民事判决书.pdf").unwrap();
        assert_eq!(panjue.stage.as_deref(), Some("一审"));
        assert_eq!(panjue.category.as_deref(), Some("判决书"));

        let shangsu = by_name.get("上诉状.pdf").unwrap();
        assert_eq!(shangsu.stage.as_deref(), Some("二审"));
        assert_eq!(shangsu.category.as_deref(), Some("上诉状"));

        let xiaoxiao = by_name.get("限消令.pdf").unwrap();
        assert_eq!(xiaoxiao.stage.as_deref(), Some("执行"));
        // 2026-05-23 后限消令独立成自己的分类(不再统一叫"执行查询")
        assert_eq!(xiaoxiao.category.as_deref(), Some("限制消费令"));

        let zonglan = by_name.get("案件总览.md").unwrap();
        assert!(zonglan.is_ai_artifact);

        // 清理
        let _ = fs::remove_dir_all(&tmp);

        // 让 PathBuf import 不报 unused
        let _: PathBuf = tmp;
    }
}
