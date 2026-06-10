//! embedding/index.rs — 案件文档向量索引(切片 + 缓存 + 语义检索)。V0.3.3 阶段2-3。
//!
//! 职责:把案件「材料文档」全文(`extracts/<case_id>/<doc_id>.md`)切成 ~500 字片段、
//! embed 成向量、按余弦相似度对用户 query 做 top-N 检索。向量缓存落
//! `embeddings/<case_id>.json`,按 `documents.cache_key` 增量失效(文件变了才重 embed)。
//!
//! 设计要点:
//!   - **懒加载 + 增量**:首次检索才建索引;之后只对 cache_key 变了 / 新增的文档重 embed,
//!     未变的直接复用旧向量(命中现有 cache_key 失效模式,坑#11 同源思路)。
//!   - **模型签名**:换 embedding endpoint/model → signature 变 → 整库失效重建(维度也会变,
//!     旧向量跟新 query 维度不一致,cosine 直接返 0,必须重建)。
//!   - **材料集对齐 constitution**:只索引 `!is_ai_artifact && !归档类` 且有全文的文档,
//!     跟喂进 system prompt 的 material docs 一致(不索引 AI 产物,防自证循环)。
//!   - **没配 / 出错 → 调用方静默回退**:embed 报错透传(坑#8),由接入层 fallback 关键词/轻量,
//!     AI 无感。本模块不吞错、不用固定文案。
//!
//! 纯函数(`chunk_text` / `plan_update` / `rank_hits`)单测覆盖;走网络的 `embed` 编排薄封装。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::db::documents::Document;

/// 单个切片目标字数。bge-m3 等上限远大于此,500 字兼顾召回粒度与 embed 次数。
const CHUNK_TARGET_CHARS: usize = 500;
/// 单次 embed 请求最多带多少条文本(保守值,兼容硅基/智谱批量上限)。
const EMBED_BATCH: usize = 32;

// =============================================================================
// 数据结构(落盘 JSON)
// =============================================================================

/// 一个文本切片 + 它的向量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub text: String,
    pub vector: Vec<f32>,
}

/// 一份文档的索引条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIndex {
    pub doc_id: String,
    pub filename: String,
    pub category: Option<String>,
    /// 复用 `documents.cache_key`("<modified_at>:<size>");变了 → 重新切片 + embed。
    pub cache_key: Option<String>,
    pub chunks: Vec<Chunk>,
}

/// 整个案件的向量索引(落 `embeddings/<case_id>.json`)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaseIndex {
    /// embedding 模型签名("<endpoint>|<model>");变了 → 整库失效重建。
    pub signature: String,
    pub docs: Vec<DocIndex>,
}

/// 一条检索命中(给接入层拼进 user turn)。
#[derive(Debug, Clone)]
pub struct Hit {
    pub doc_id: String,
    pub filename: String,
    pub category: Option<String>,
    pub score: f32,
    pub text: String,
}

// =============================================================================
// 纯函数:可索引判定 / 签名 / 切片 / 增量计划 / 排序
// =============================================================================

/// 可索引文档:非 AI 产物、非归档类、有全文、未缺失/未删。
/// 跟 `constitution::build_system_prompt` 喂 LLM 的材料集对齐 —— 索引什么、喂什么一致。
pub fn is_indexable(d: &Document) -> bool {
    !d.is_ai_artifact
        && !d.missing
        && d.deleted_at.is_none()
        && d.extracted_text_path.is_some()
        && !crate::ingest::pipeline::is_archival_category(d.category.as_deref())
}

/// embedding 模型签名("<endpoint>|<model>")。留空时用默认值,跟 `embedding::embed` 兜底一致。
pub fn signature(endpoint: &str, model: &str) -> String {
    let ep = if endpoint.trim().is_empty() {
        crate::embedding::DEFAULT_ENDPOINT
    } else {
        endpoint.trim()
    };
    let md = if model.trim().is_empty() {
        crate::embedding::DEFAULT_MODEL
    } else {
        model.trim()
    };
    format!("{ep}|{md}")
}

/// 把文本切成约 `target` 字的片段,优先在行/段落边界断;单行超长则硬切。unicode 安全(按 char 计)。
/// 空白片段丢弃。
pub fn chunk_text(text: &str, target: usize) -> Vec<String> {
    let target = target.max(1);
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;

    for line in text.lines() {
        let line_len = line.chars().count();
        // 累积后超目标 → 先把当前片段断出去
        if cur_len > 0 && cur_len + line_len > target {
            push_chunk(&mut chunks, &mut cur);
            cur_len = 0;
        }
        // 单行就超目标 → 硬切这一行(先 flush 残留,已在上面断过则 no-op)
        if line_len > target {
            push_chunk(&mut chunks, &mut cur);
            cur_len = 0;
            for piece in hard_split(line, target) {
                chunks.push(piece);
            }
            continue;
        }
        if !cur.is_empty() {
            cur.push('\n');
            cur_len += 1;
        }
        cur.push_str(line);
        cur_len += line_len;
    }
    push_chunk(&mut chunks, &mut cur);
    chunks
}

fn push_chunk(chunks: &mut Vec<String>, cur: &mut String) {
    let t = cur.trim();
    if !t.is_empty() {
        chunks.push(t.to_string());
    }
    cur.clear();
}

fn hard_split(s: &str, target: usize) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    chars
        .chunks(target)
        .map(|c| c.iter().collect::<String>())
        .filter(|p| !p.trim().is_empty())
        .collect()
}

/// 增量计划:哪些旧条目可复用、哪些要重新 embed。纯函数,便于单测。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePlan {
    /// 可直接复用的旧条目 doc_id(doc 仍在 + cache_key 一致 + signature 未变 + 有 chunks)
    pub reuse_doc_ids: Vec<String>,
    /// 需要重新切片 + embed 的 doc_id(新增 / cache_key 变 / signature 变 / 旧条目空)
    pub embed_doc_ids: Vec<String>,
}

/// 给定旧索引 + 新签名 + 当前可索引文档 `(doc_id, cache_key)`,算增量计划。
/// signature 不一致 → 全部进 embed(整库重建)。不在 `current` 里的旧文档自动丢弃(不出现在计划)。
pub fn plan_update(
    existing: &CaseIndex,
    new_signature: &str,
    current: &[(String, Option<String>)],
) -> UpdatePlan {
    let sig_ok = existing.signature == new_signature;
    let mut plan = UpdatePlan::default();
    for (doc_id, cache_key) in current {
        let prev = existing.docs.iter().find(|d| &d.doc_id == doc_id);
        let can_reuse = sig_ok
            && prev
                .map(|p| p.cache_key.as_deref() == cache_key.as_deref() && !p.chunks.is_empty())
                .unwrap_or(false);
        if can_reuse {
            plan.reuse_doc_ids.push(doc_id.clone());
        } else {
            plan.embed_doc_ids.push(doc_id.clone());
        }
    }
    plan
}

/// 给定 query 向量 + 索引,算所有片段的余弦相似度,返回 top-N 命中(降序)。纯函数。
pub fn rank_hits(index: &CaseIndex, query_vec: &[f32], top_n: usize) -> Vec<Hit> {
    let mut scored: Vec<Hit> = Vec::new();
    for d in &index.docs {
        for c in &d.chunks {
            let score = crate::embedding::cosine_similarity(query_vec, &c.vector);
            scored.push(Hit {
                doc_id: d.doc_id.clone(),
                filename: d.filename.clone(),
                category: d.category.clone(),
                score,
                text: c.text.clone(),
            });
        }
    }
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(top_n);
    scored
}

// =============================================================================
// 落盘 + 网络编排
// =============================================================================

fn index_path(case_id: &str) -> Result<PathBuf, String> {
    let base = crate::db::app_data_dir().map_err(|e| format!("无法定位 app data dir: {e}"))?;
    Ok(base.join("embeddings").join(format!("{case_id}.json")))
}

async fn load_index(case_id: &str) -> CaseIndex {
    let Ok(path) = index_path(case_id) else {
        return CaseIndex::default();
    };
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => CaseIndex::default(),
    }
}

async fn save_index(case_id: &str, index: &CaseIndex) -> Result<(), String> {
    let path = index_path(case_id)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("建 embeddings 目录失败: {e}"))?;
    }
    let json = serde_json::to_string(index).map_err(|e| format!("序列化索引失败: {e}"))?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| format!("写索引失败: {e}"))?;
    Ok(())
}

/// 单条 embed 调用按 EMBED_BATCH 分批,顺序与输入对齐。
async fn embed_batched(
    endpoint: &str,
    model: &str,
    key: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let mut out = Vec::with_capacity(texts.len());
    for batch in texts.chunks(EMBED_BATCH) {
        let v = crate::embedding::embed(endpoint, model, key, batch).await?;
        if v.len() != batch.len() {
            return Err(format!(
                "embedding 返回数量不符:期望 {} 得到 {}",
                batch.len(),
                v.len()
            ));
        }
        out.extend(v);
    }
    Ok(out)
}

/// 懒加载 + 增量建/更新案件索引,返回最新 `CaseIndex`(有变化才落盘)。
/// 没配 key / 网络错 → `embed` 报错透传,调用方静默回退。
pub async fn build_or_update_index(
    case_id: &str,
    docs: &[Document],
    endpoint: &str,
    model: &str,
    key: &str,
) -> Result<CaseIndex, String> {
    let indexable: Vec<&Document> = docs.iter().filter(|d| is_indexable(d)).collect();
    let sig = signature(endpoint, model);
    let existing = load_index(case_id).await;
    let current: Vec<(String, Option<String>)> = indexable
        .iter()
        .map(|d| (d.id.clone(), d.cache_key.clone()))
        .collect();
    let plan = plan_update(&existing, &sig, &current);

    let mut new_docs: Vec<DocIndex> = Vec::with_capacity(indexable.len());
    // 复用旧条目
    for doc_id in &plan.reuse_doc_ids {
        if let Some(prev) = existing.docs.iter().find(|d| &d.doc_id == doc_id) {
            new_docs.push(prev.clone());
        }
    }
    // 重新切片 + embed
    for doc_id in &plan.embed_doc_ids {
        let Some(d) = indexable.iter().find(|d| &d.id == doc_id) else {
            continue;
        };
        let Some(path) = &d.extracted_text_path else {
            continue;
        };
        let text = tokio::fs::read_to_string(path).await.unwrap_or_default();
        let pieces = chunk_text(&text, CHUNK_TARGET_CHARS);
        if pieces.is_empty() {
            continue;
        }
        let vectors = embed_batched(endpoint, model, key, &pieces).await?;
        let chunks = pieces
            .into_iter()
            .zip(vectors)
            .map(|(text, vector)| Chunk { text, vector })
            .collect();
        new_docs.push(DocIndex {
            doc_id: d.id.clone(),
            filename: d.filename.clone(),
            category: d.category.clone(),
            cache_key: d.cache_key.clone(),
            chunks,
        });
    }

    let index = CaseIndex {
        signature: sig,
        docs: new_docs,
    };
    // 仅在有变化时落盘:多轮 FreeChat 每轮纯复用就不重写文件。
    let changed = existing.signature != index.signature
        || !plan.embed_doc_ids.is_empty()
        || index.docs.len() != existing.docs.len();
    if changed {
        if let Err(e) = save_index(case_id, &index).await {
            crate::dlog!("[embedding] 写索引失败: {}", e);
        }
    }
    Ok(index)
}

/// 案件文档语义检索:建/更新索引 → embed query → top-N 片段。
/// 调用前应确保已配 embedding key(否则 `embed` 报错);失败透传,调用方静默回退。
pub async fn semantic_search(
    case_id: &str,
    docs: &[Document],
    query: &str,
    top_n: usize,
    endpoint: &str,
    model: &str,
    key: &str,
) -> Result<Vec<Hit>, String> {
    let index = build_or_update_index(case_id, docs, endpoint, model, key).await?;
    if index.docs.is_empty() {
        return Ok(vec![]);
    }
    let qv = crate::embedding::embed(endpoint, model, key, &[query.to_string()]).await?;
    let qv = qv.into_iter().next().ok_or("query embedding 返回空")?;
    Ok(rank_hits(&index, &qv, top_n))
}

// =============================================================================
// 测试(纯函数,无网络)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, ck: Option<&str>, chunks: usize) -> DocIndex {
        DocIndex {
            doc_id: id.into(),
            filename: format!("{id}.pdf"),
            category: None,
            cache_key: ck.map(String::from),
            chunks: (0..chunks)
                .map(|i| Chunk {
                    text: format!("chunk{i}"),
                    vector: vec![i as f32, 1.0],
                })
                .collect(),
        }
    }

    #[test]
    fn chunk_empty_yields_nothing() {
        assert!(chunk_text("", 100).is_empty());
        assert!(chunk_text("   \n\n  ", 100).is_empty());
    }

    #[test]
    fn chunk_short_is_single() {
        let c = chunk_text("一句话证据", 100);
        assert_eq!(c, vec!["一句话证据".to_string()]);
    }

    #[test]
    fn chunk_splits_when_over_target() {
        // 三段,每段 ~10 字,target=12 → 应切成多段且每段不超 target(+边界)
        let text = "第一段内容大约十字\n第二段内容大约十字\n第三段内容大约十字";
        let c = chunk_text(text, 12);
        assert!(c.len() >= 2, "应切成多段,得到 {:?}", c);
        for piece in &c {
            assert!(
                piece.chars().count() <= 24,
                "单段不应远超 target: {}",
                piece
            );
        }
        // 内容不丢:拼回去含所有原始字
        let joined: String = c.join("");
        assert!(joined.contains("第一段"));
        assert!(joined.contains("第三段"));
    }

    #[test]
    fn chunk_hard_splits_overlong_single_line() {
        // 单行 30 字、无换行、target=10 → 硬切成 3 段
        let line = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥角亢氐房心尾箕斗";
        assert_eq!(line.chars().count(), 30);
        let c = chunk_text(line, 10);
        assert_eq!(c.len(), 3);
        for piece in &c {
            assert_eq!(piece.chars().count(), 10);
        }
    }

    #[test]
    fn chunk_unicode_no_panic() {
        // emoji + 混合,不 panic,内容保留
        let text = "证据🔑材料📎合同💰".repeat(20);
        let c = chunk_text(&text, 7);
        assert!(!c.is_empty());
        assert!(c.iter().all(|p| !p.is_empty()));
    }

    #[test]
    fn plan_reuses_on_cache_key_match() {
        let existing = CaseIndex {
            signature: "sig-a".into(),
            docs: vec![doc("d1", Some("ck1"), 2)],
        };
        let plan = plan_update(&existing, "sig-a", &[("d1".into(), Some("ck1".into()))]);
        assert_eq!(plan.reuse_doc_ids, vec!["d1".to_string()]);
        assert!(plan.embed_doc_ids.is_empty());
    }

    #[test]
    fn plan_reembeds_on_cache_key_change() {
        let existing = CaseIndex {
            signature: "sig-a".into(),
            docs: vec![doc("d1", Some("ck1"), 2)],
        };
        let plan = plan_update(&existing, "sig-a", &[("d1".into(), Some("ck2".into()))]);
        assert!(plan.reuse_doc_ids.is_empty());
        assert_eq!(plan.embed_doc_ids, vec!["d1".to_string()]);
    }

    #[test]
    fn plan_signature_change_rebuilds_all() {
        let existing = CaseIndex {
            signature: "sig-a".into(),
            docs: vec![doc("d1", Some("ck1"), 2), doc("d2", Some("ck2"), 1)],
        };
        // 签名变(换模型)→ 即便 cache_key 没变也全部重 embed
        let plan = plan_update(
            &existing,
            "sig-b",
            &[
                ("d1".into(), Some("ck1".into())),
                ("d2".into(), Some("ck2".into())),
            ],
        );
        assert!(plan.reuse_doc_ids.is_empty());
        assert_eq!(plan.embed_doc_ids.len(), 2);
    }

    #[test]
    fn plan_new_doc_goes_to_embed() {
        let existing = CaseIndex {
            signature: "sig-a".into(),
            docs: vec![doc("d1", Some("ck1"), 2)],
        };
        let plan = plan_update(
            &existing,
            "sig-a",
            &[
                ("d1".into(), Some("ck1".into())),
                ("d2".into(), Some("ckX".into())),
            ],
        );
        assert_eq!(plan.reuse_doc_ids, vec!["d1".to_string()]);
        assert_eq!(plan.embed_doc_ids, vec!["d2".to_string()]);
    }

    #[test]
    fn plan_drops_doc_not_in_current() {
        // d2 从 current 消失(被删)→ 既不复用也不 embed,自然丢弃
        let existing = CaseIndex {
            signature: "sig-a".into(),
            docs: vec![doc("d1", Some("ck1"), 2), doc("d2", Some("ck2"), 1)],
        };
        let plan = plan_update(&existing, "sig-a", &[("d1".into(), Some("ck1".into()))]);
        assert_eq!(plan.reuse_doc_ids, vec!["d1".to_string()]);
        assert!(plan.embed_doc_ids.is_empty());
        assert!(!plan.reuse_doc_ids.contains(&"d2".to_string()));
    }

    #[test]
    fn plan_empty_chunks_forces_reembed() {
        // 旧条目 chunks 空(上次 embed 没产物)→ 不该复用,要重 embed
        let existing = CaseIndex {
            signature: "sig-a".into(),
            docs: vec![doc("d1", Some("ck1"), 0)],
        };
        let plan = plan_update(&existing, "sig-a", &[("d1".into(), Some("ck1".into()))]);
        assert!(plan.reuse_doc_ids.is_empty());
        assert_eq!(plan.embed_doc_ids, vec!["d1".to_string()]);
    }

    #[test]
    fn rank_orders_by_cosine_and_truncates() {
        // query 跟 [1,0] 同向 → 偏 [1,*] 的片段分高
        let index = CaseIndex {
            signature: "s".into(),
            docs: vec![DocIndex {
                doc_id: "d1".into(),
                filename: "f.pdf".into(),
                category: None,
                cache_key: None,
                chunks: vec![
                    Chunk {
                        text: "近".into(),
                        vector: vec![1.0, 0.0],
                    },
                    Chunk {
                        text: "远".into(),
                        vector: vec![0.0, 1.0],
                    },
                    Chunk {
                        text: "中".into(),
                        vector: vec![1.0, 1.0],
                    },
                ],
            }],
        };
        let hits = rank_hits(&index, &[1.0, 0.0], 2);
        assert_eq!(hits.len(), 2, "应截断到 top-2");
        assert_eq!(hits[0].text, "近", "最同向的排第一");
        assert!(hits[0].score >= hits[1].score);
    }

    #[test]
    fn case_index_serde_round_trip() {
        let idx = CaseIndex {
            signature: "ep|model".into(),
            docs: vec![doc("d1", Some("ck1"), 2)],
        };
        let json = serde_json::to_string(&idx).unwrap();
        let back: CaseIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.signature, idx.signature);
        assert_eq!(back.docs.len(), 1);
        assert_eq!(back.docs[0].chunks.len(), 2);
        assert_eq!(back.docs[0].chunks[0].vector, vec![0.0, 1.0]);
    }

    #[test]
    fn signature_falls_back_to_defaults() {
        let s = signature("", "");
        assert!(s.contains(crate::embedding::DEFAULT_MODEL));
        assert!(s.contains(crate::embedding::DEFAULT_ENDPOINT));
        // 同输入稳定(决定缓存命中)
        assert_eq!(signature("ep", "md"), "ep|md");
    }

    #[test]
    fn indexable_filters_ai_and_archival() {
        use crate::db::documents::Document;
        let mk = |id: &str, ai: bool, cat: Option<&str>, text: Option<&str>| Document {
            id: id.into(),
            case_id: "c".into(),
            source_path: "/tmp/x".into(),
            filename: format!("{id}.pdf"),
            stage: None,
            category: cat.map(String::from),
            is_ai_artifact: ai,
            mime_type: None,
            size_bytes: 0,
            modified_at: None,
            extracted_fields: None,
            extraction_status: "done".into(),
            missing: false,
            created_at: "2026-06-01".into(),
            deleted_at: None,
            extracted_text_path: text.map(String::from),
            cache_key: None,
            last_error: None,
            source: "scan".into(),
            pinned_at: None,
        };
        // 普通证据有全文 → 可索引
        assert!(is_indexable(&mk("ev", false, Some("借条"), Some("/p.md"))));
        // AI 产物 → 不索引
        assert!(!is_indexable(&mk("ai", true, Some("借条"), Some("/p.md"))));
        // 归档类(风险告知)→ 不索引
        assert!(!is_indexable(&mk(
            "ar",
            false,
            Some("风险告知"),
            Some("/p.md")
        )));
        // 没全文 → 不索引
        assert!(!is_indexable(&mk("no", false, Some("借条"), None)));
    }
}
