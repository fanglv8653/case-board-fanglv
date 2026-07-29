//! 本地知识库“检索与维护说明”的只读、结构化事实源。
//!
//! 本模块描述当前方律实现，不复刻上游 raw/wiki 强制目录契约：
//! 关键词检索递归扫描用户绑定根目录，语义检索只读取已经建立且签名匹配的索引。
//! 生成说明不会创建目录、改写知识库或触发 embedding。

use std::path::Path;

use serde::Serialize;

use super::search::{
    KEYWORD_EXCLUDED_ROOT_PREFIX, KEYWORD_EXCLUDED_SEGMENTS, KEYWORD_FILE_EXTENSIONS, MAX_FILE_SIZE,
};

pub const GUIDE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalKbGuide {
    pub schema_version: u32,
    pub mode: &'static str,
    pub configured_root: Option<String>,
    pub root_available: bool,
    pub keyword_search: KeywordSearchGuide,
    pub semantic_search: SemanticSearchGuide,
    pub file_read: FileReadGuide,
    pub maintenance_boundaries: Vec<&'static str>,
    pub internal_ai_tools: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KeywordSearchGuide {
    pub scope: &'static str,
    pub extensions: Vec<&'static str>,
    pub max_file_bytes: u64,
    pub excluded_root_prefixes: Vec<&'static str>,
    pub excluded_segments: Vec<&'static str>,
    pub sorting: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SemanticSearchGuide {
    pub scope: &'static str,
    pub requires_embedding_credential: bool,
    pub requires_prebuilt_matching_index: bool,
    pub query_builds_or_updates_index: bool,
    pub mismatch_behavior: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileReadGuide {
    pub path_kind: &'static str,
    pub canonical_root_boundary: bool,
    pub max_file_bytes: u64,
    pub rejects_binary_nul: bool,
    pub default_max_chars: usize,
}

/// 构建当前实现的只读说明。
///
/// `root` 用于展示并只读检查当前目录是否真实可用；不会创建或修改任何文件。
pub fn build_local_kb_guide(root: Option<&Path>) -> LocalKbGuide {
    LocalKbGuide {
        schema_version: GUIDE_SCHEMA_VERSION,
        mode: "read_only",
        configured_root: root.map(|path| path.to_string_lossy().into_owned()),
        root_available: root.is_some_and(Path::is_dir),
        keyword_search: KeywordSearchGuide {
            scope: "递归检索所绑定根目录，不要求 raw/wiki 等固定目录名称",
            extensions: KEYWORD_FILE_EXTENSIONS.to_vec(),
            max_file_bytes: MAX_FILE_SIZE,
            excluded_root_prefixes: vec![KEYWORD_EXCLUDED_ROOT_PREFIX],
            excluded_segments: KEYWORD_EXCLUDED_SEGMENTS.to_vec(),
            sorting: vec!["命中次数降序", "文件修改时间降序"],
        },
        semantic_search: SemanticSearchGuide {
            scope: "使用当前根目录对应的既有向量索引；整根补扫自定义 .md/.txt，并对标准目录、元典详情和法律全文执行现有去重/过滤规则",
            requires_embedding_credential: true,
            requires_prebuilt_matching_index: true,
            query_builds_or_updates_index: false,
            mismatch_behavior: "索引为空、根目录变化或 embedding 签名不匹配时返回无结果，由调用方回退关键词检索",
        },
        file_read: FileReadGuide {
            path_kind: "知识库根目录内相对路径",
            canonical_root_boundary: true,
            max_file_bytes: MAX_FILE_SIZE,
            rejects_binary_nul: true,
            default_max_chars: 10_000,
        },
        maintenance_boundaries: vec![
            "本说明和内部 AI 说明工具只读，不创建、覆盖、移动或删除知识库文件",
            "不会生成或改写 AGENTS.md、CLAUDE.md 或其他外部 AI 入口文件",
            "不会开放 AI 写入 raw、wiki 或任意自定义目录",
            "目录切换、迁移和向量索引更新由现有独立功能及其确认边界处理",
            "DOCX、PDF、图片及其他非 Markdown/纯文本文件不会被关键词检索直接读取",
        ],
        internal_ai_tools: vec![
            "get_local_kb_guide",
            "search_local_kb",
            "semantic_search_local_kb",
            "read_kb_file",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_kb::search::{search_kb_files, SearchOptions};

    #[test]
    fn guide_matches_real_keyword_search_and_is_directory_name_agnostic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let custom = temp.path().join("我的自定义分类");
        let cache = temp.path().join("raw").join("yuandian-cache");
        let technical = temp.path().join("node_modules");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&technical).unwrap();
        std::fs::write(custom.join("材料.md"), "唯一检索词").unwrap();
        std::fs::write(cache.join("缓存.md"), "唯一检索词").unwrap();
        std::fs::write(technical.join("依赖.txt"), "唯一检索词").unwrap();
        std::fs::write(temp.path().join("材料.pdf"), "唯一检索词").unwrap();

        let hits = search_kb_files(temp.path(), "唯一检索词", SearchOptions::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].relative_path.contains("我的自定义分类"));

        let guide = build_local_kb_guide(Some(temp.path()));
        assert_eq!(guide.mode, "read_only");
        assert_eq!(guide.keyword_search.extensions, vec!["md", "txt"]);
        assert_eq!(guide.keyword_search.max_file_bytes, 5 * 1024 * 1024);
        assert!(guide
            .keyword_search
            .excluded_root_prefixes
            .contains(&"raw/yuandian-cache"));
        assert!(guide
            .keyword_search
            .excluded_segments
            .contains(&"node_modules"));
        assert!(!guide.semantic_search.query_builds_or_updates_index);
    }

    #[test]
    fn unbound_guide_is_still_available_and_has_no_write_capability() {
        let guide = build_local_kb_guide(None);
        assert!(!guide.root_available);
        assert!(guide.configured_root.is_none());
        let json = serde_json::to_string(&guide).unwrap();
        assert!(json.contains("get_local_kb_guide"));
        assert!(!json.contains("save_local_kb_material"));
        assert!(!json.contains("generate_external_ai_entry"));
    }
}
