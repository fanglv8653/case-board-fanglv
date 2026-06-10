//! 文档(`documents`)表的 CRUD。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::ingest::scanner::ScannedDoc;

/// 文档表行结构。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Document {
    pub id: String,
    pub case_id: String,
    pub source_path: String,
    pub filename: String,
    pub stage: Option<String>,
    pub category: Option<String>,
    pub is_ai_artifact: bool,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub modified_at: Option<String>,
    pub extracted_fields: Option<String>, // JSON 文本
    pub extraction_status: String,
    pub missing: bool,
    pub created_at: String,
    /// 2026-05-23 晚十 加(migration 0005):软删时间戳
    pub deleted_at: Option<String>,
    /// 抽出来的 .md 文件落盘路径(extracts/<case_id>/<doc_id>.md)
    pub extracted_text_path: Option<String>,
    /// 缓存键 = "<modified_at>:<size>",变了就重抽
    pub cache_key: Option<String>,
    /// 2026-05-25 加(migration 0014):最近一次抽取失败的错误信息。
    /// 三轮重试(8 → 4 → 1)全失败后才会落进来。成功 / skipped 时清 NULL。
    pub last_error: Option<String>,
    /// 2026-05-26 加(migration 0017):文档来源,区分 'scan' / 'llm_extract' / 'chat'。
    /// - 'scan':扫描原始文件夹时录入的源文件(默认)
    /// - 'llm_extract':LLM 全局抽产生的 MD 报告(案件画像 / 风险报告 / 深挖等)
    /// - 'chat':案件 AI 助手聊天面板生成的 artifact
    pub source: String,
    /// 2026-05-27 加(migration 0018,V0.2 D2-D3):置顶时间戳。
    /// 非 null 时,引用弹窗「📎 引用文件」按本字段降序优先显示;
    /// 用于让用户对常用文档(如本案合同 / 起诉状)做置顶,避免每次翻找。
    pub pinned_at: Option<String>,
}

fn make_cache_key(modified_at: Option<&str>, size_bytes: u64) -> String {
    format!("{}:{}", modified_at.unwrap_or(""), size_bytes)
}

/// 同步结果统计(给前端 Toast / 日志用)。
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncStats {
    /// 全新加入的文件
    pub added: usize,
    /// mtime + size 变了,标了 pending 等重抽
    pub updated: usize,
    /// 完全没变,直接跳过(extracted_fields 保留)
    pub unchanged: usize,
    /// 源文件夹里不存在了,本次标 deleted_at
    pub deleted: usize,
}

/// 把一次扫描的所有结果同步到 DB(2026-05-23 晚十 重写,**不再 DELETE+INSERT 全表**)。
///
/// 作者核心痛点:重扫不重抽。逻辑:
///   - 已存在 + cache_key 一致 → 跳过(unchanged++)
///   - 已存在 + cache_key 变了 → UPDATE,清 extracted_fields,status=pending(updated++)
///   - 新增 → INSERT,status=pending(added++)
///   - DB 有但 scanned 里没 → 标 deleted_at(deleted++)
///
/// 用 transaction 保证原子性。
pub async fn replace_documents_for_case(
    pool: &SqlitePool,
    case_id: &str,
    scanned: &[ScannedDoc],
) -> Result<usize, sqlx::Error> {
    let stats = sync_documents_for_case(pool, case_id, scanned).await?;
    // 兼容老 caller(返回总数 = 该案件最终活跃文档数)
    Ok(stats.added + stats.updated + stats.unchanged)
}

/// 2026-05-23 晚十 加 — 真正的 diff sync,返回详细统计。
pub async fn sync_documents_for_case(
    pool: &SqlitePool,
    case_id: &str,
    scanned: &[ScannedDoc],
) -> Result<SyncStats, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 1) 拉 DB 里现有的**所有**活跃文档(含 chat / llm_extract artifact)。
    //
    // 2026-05-27 V0.1.13+ 重写:之前限定 `source = 'scan'` 引发两个坑 —
    //   a) 老 AI artifact 用户复制到源文件夹时,scanner 扫到 → INSERT 撞唯一索引
    //      (源文件 source_path 在本案已有行,不分 source;0019 后唯一键是 (case_id, source_path))
    //   b) 软删环节会把 chat artifact 当"扫不到的文件"误标 deleted_at
    // 修法:existing 包含全部活跃行(避免 INSERT 撞),但**软删时只动 source='scan'**
    //      (chat / llm_extract artifact 不在源文件夹,本来就不该被 sync 影响)。
    let existing: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, source_path, cache_key, source FROM documents \
         WHERE case_id = ? AND deleted_at IS NULL",
    )
    .bind(case_id)
    .fetch_all(&mut *tx)
    .await?;

    // 索引:source_path → (id, old_cache_key, source)
    let mut existing_map: std::collections::HashMap<String, (String, Option<String>, String)> =
        std::collections::HashMap::with_capacity(existing.len());
    for (id, sp, ck, src) in existing {
        existing_map.insert(sp, (id, ck, src));
    }

    // 当前扫到的 source_path 集合(用于检测 deleted)
    let mut current_paths = std::collections::HashSet::with_capacity(scanned.len());
    let mut stats = SyncStats::default();

    // 2) 遍历当前扫到的文件,upsert
    for doc in scanned {
        current_paths.insert(doc.source_path.clone());
        let new_cache_key = make_cache_key(doc.modified_at.as_deref(), doc.size_bytes);

        match existing_map.get(&doc.source_path) {
            Some((existing_id, old_ck, _src))
                if old_ck.as_deref() == Some(new_cache_key.as_str()) =>
            {
                // 不变 — 只刷一下 stage/category/is_ai_artifact(因为关键词表可能更新过)
                sqlx::query(
                    "UPDATE documents SET stage = ?, category = ?, is_ai_artifact = ?, \
                     deleted_at = NULL WHERE id = ?",
                )
                .bind(&doc.stage)
                .bind(&doc.category)
                .bind(doc.is_ai_artifact)
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
                stats.unchanged += 1;
            }
            Some((existing_id, _, _)) => {
                // 文件变了 — 清抽取产物,重排队
                sqlx::query(
                    "UPDATE documents SET \
                       filename = ?, stage = ?, category = ?, is_ai_artifact = ?, \
                       size_bytes = ?, modified_at = ?, cache_key = ?, \
                       extracted_fields = NULL, extracted_text_path = NULL, \
                       extraction_status = 'pending', deleted_at = NULL \
                     WHERE id = ?",
                )
                .bind(&doc.filename)
                .bind(&doc.stage)
                .bind(&doc.category)
                .bind(doc.is_ai_artifact)
                .bind(doc.size_bytes as i64)
                .bind(&doc.modified_at)
                .bind(&new_cache_key)
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
                stats.updated += 1;
            }
            None => {
                // 全新
                let id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO documents \
                     (id, case_id, source_path, filename, stage, category, is_ai_artifact, \
                      size_bytes, modified_at, cache_key) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(case_id)
                .bind(&doc.source_path)
                .bind(&doc.filename)
                .bind(&doc.stage)
                .bind(&doc.category)
                .bind(doc.is_ai_artifact)
                .bind(doc.size_bytes as i64)
                .bind(&doc.modified_at)
                .bind(&new_cache_key)
                .execute(&mut *tx)
                .await?;
                stats.added += 1;
            }
        }
    }

    // 3) 软删:仅删 source='scan' 且不在 current_paths 的文档。
    //    chat / llm_extract artifact 活在 app data 目录,不参与 scan-folder diff。
    let deleted_paths: Vec<String> = existing_map
        .iter()
        .filter(|(p, (_, _, src))| !current_paths.contains(p.as_str()) && src == "scan")
        .map(|(p, _)| p.clone())
        .collect();
    for sp in &deleted_paths {
        sqlx::query(
            "UPDATE documents SET deleted_at = datetime('now') \
             WHERE case_id = ? AND source_path = ? AND deleted_at IS NULL AND source = 'scan'",
        )
        .bind(case_id)
        .bind(sp)
        .execute(&mut *tx)
        .await?;
        stats.deleted += 1;
    }

    tx.commit().await?;
    Ok(stats)
}

/// 列出某案件下的所有活跃文档(2026-05-23 晚十:过滤软删),按 stage 顺序 + filename 字典序。
pub async fn list_documents_by_case(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<Document>, sqlx::Error> {
    sqlx::query_as::<_, Document>(
        "SELECT * FROM documents WHERE case_id = ? AND deleted_at IS NULL \
         ORDER BY stage, filename",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
}

/// 按 id 取单个文档(过滤软删)。单文档操作(重抽等)用。
pub async fn get_document_by_id(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<Document>, sqlx::Error> {
    sqlx::query_as::<_, Document>("SELECT * FROM documents WHERE id = ? AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// 把文档抽取状态重置为 `pending`(并清 `last_error`),用于强制重抽。
/// run_extraction 只处理 pending,故重置后再 spawn_extraction 即会重抽该文档。返回受影响行数。
pub async fn reset_for_reextract(pool: &SqlitePool, id: &str) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE documents SET extraction_status = 'pending', last_error = NULL \
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// 软删一个文档(置 `deleted_at`):用户手动从材料列表移除(主要给 AI artifact 用)。
/// 只软删 DB 行(列表/LLM corpus 都过滤 `deleted_at`),**不动磁盘文件**。返回受影响行数。
pub async fn soft_delete_document(
    pool: &SqlitePool,
    id: &str,
    now: &str,
) -> Result<u64, sqlx::Error> {
    let res =
        sqlx::query("UPDATE documents SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
    Ok(res.rows_affected())
}

/// 统计某案件下文档数量(用于案件列表卡片显示)。
///
/// V0.1 暂未在命令层暴露,留给 task #4 真正做案件列表时用。
#[allow(dead_code)]
pub async fn count_documents_for_case(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<i64, sqlx::Error> {
    // 2026-05-23 晚十:过滤软删,跟 list_documents_by_case 一致
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM documents WHERE case_id = ? AND deleted_at IS NULL")
            .bind(case_id)
            .fetch_one(pool)
            .await?;
    Ok(n)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::cases::{create_case, NewCase};
    use crate::db::init_pool;

    fn fake_scanned(filename: &str, stage: Option<&str>, category: Option<&str>) -> ScannedDoc {
        ScannedDoc {
            source_path: format!("/tmp/fake/{}", filename),
            filename: filename.into(),
            stage: stage.map(String::from),
            category: category.map(String::from),
            is_ai_artifact: false,
            size_bytes: 1024,
            modified_at: Some("2026-01-01T00:00:00Z".into()),
        }
    }

    async fn fresh_pool_with_case() -> (SqlitePool, String) {
        let pool = init_pool(":memory:").await.unwrap();
        let case = create_case(
            &pool,
            NewCase {
                name: "测试案".into(),
                case_type: "诉讼".into(),
                source_folder: "/tmp/fake".into(),
            },
        )
        .await
        .unwrap();
        (pool, case.id)
    }

    #[tokio::test]
    async fn replace_inserts_all_docs() {
        let (pool, case_id) = fresh_pool_with_case().await;
        let scanned = vec![
            fake_scanned("民事诉状.docx", Some("立案"), Some("起诉状")),
            fake_scanned("民事判决书.pdf", Some("一审"), Some("判决书")),
            fake_scanned("上诉状.pdf", Some("二审"), Some("上诉状")),
        ];

        let n = replace_documents_for_case(&pool, &case_id, &scanned)
            .await
            .unwrap();
        assert_eq!(n, 3);

        let docs = list_documents_by_case(&pool, &case_id).await.unwrap();
        assert_eq!(docs.len(), 3);
    }

    #[tokio::test]
    async fn reextract_resets_failed_doc_to_pending_and_clears_error() {
        let (pool, case_id) = fresh_pool_with_case().await;
        replace_documents_for_case(
            &pool,
            &case_id,
            &[fake_scanned("离婚补偿协议.pdf", Some("立案"), Some("协议"))],
        )
        .await
        .unwrap();
        let id = list_documents_by_case(&pool, &case_id).await.unwrap()[0]
            .id
            .clone();
        // 模拟抽取失败
        sqlx::query(
            "UPDATE documents SET extraction_status='failed', last_error='LLM 抽取失败' WHERE id=?",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();

        // get_document_by_id 取得到失败态
        let d = get_document_by_id(&pool, &id).await.unwrap().unwrap();
        assert_eq!(d.extraction_status, "failed");

        // 重置后:pending + last_error 清空
        assert_eq!(reset_for_reextract(&pool, &id).await.unwrap(), 1);
        let d2 = get_document_by_id(&pool, &id).await.unwrap().unwrap();
        assert_eq!(d2.extraction_status, "pending");
        let err: Option<String> = sqlx::query_scalar("SELECT last_error FROM documents WHERE id=?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(err.is_none(), "last_error 应被清空");
    }

    #[tokio::test]
    async fn replace_truly_replaces() {
        let (pool, case_id) = fresh_pool_with_case().await;

        // 第一次:3 个文档
        replace_documents_for_case(
            &pool,
            &case_id,
            &[
                fake_scanned("a.pdf", Some("立案"), None),
                fake_scanned("b.pdf", Some("立案"), None),
                fake_scanned("c.pdf", Some("立案"), None),
            ],
        )
        .await
        .unwrap();
        assert_eq!(count_documents_for_case(&pool, &case_id).await.unwrap(), 3);

        // 第二次:只剩 1 个(模拟用户删了 b 和 c)
        replace_documents_for_case(
            &pool,
            &case_id,
            &[fake_scanned("a.pdf", Some("立案"), None)],
        )
        .await
        .unwrap();
        assert_eq!(count_documents_for_case(&pool, &case_id).await.unwrap(), 1);

        let docs = list_documents_by_case(&pool, &case_id).await.unwrap();
        assert_eq!(docs[0].filename, "a.pdf");
    }

    #[tokio::test]
    async fn count_is_zero_for_empty_case() {
        let (pool, case_id) = fresh_pool_with_case().await;
        assert_eq!(count_documents_for_case(&pool, &case_id).await.unwrap(), 0);
    }

    /// 回归测试(2026-05-27 老板手测发现的 UNIQUE 冲突):
    /// 用户案件文件夹里如果原本就有 AI 生成的 MD(比如老的「案件总览.md」),
    /// 那 DB 里这行的 source 可能是 'llm_extract'(backfill 设置)。
    /// 下次 scanner 扫到同一个 source_path,sync 应走 UPDATE 而不是 INSERT,
    /// 否则会撞 (case_id, source_path) 复合唯一索引 → 整个 sync 失败。
    #[tokio::test]
    async fn sync_updates_existing_non_scan_row_instead_of_insert() {
        let (pool, case_id) = fresh_pool_with_case().await;
        let path = "/tmp/fake/案件总览.md";

        // 1) 模拟历史数据:DB 里有一行 source='llm_extract',source_path 在源文件夹
        sqlx::query(
            "INSERT INTO documents (id, case_id, source_path, filename, \
             is_ai_artifact, source, extraction_status) \
             VALUES ('llm-row', ?, ?, '案件总览.md', 1, 'llm_extract', 'done')",
        )
        .bind(&case_id)
        .bind(path)
        .execute(&pool)
        .await
        .unwrap();

        // 2) scanner 现在扫到这个文件 → 应走 UPDATE,不应 INSERT 撞 UNIQUE
        let mut scanned = fake_scanned("案件总览.md", None, None);
        scanned.source_path = path.to_string();
        scanned.is_ai_artifact = true;

        let result = sync_documents_for_case(&pool, &case_id, &[scanned]).await;
        assert!(result.is_ok(), "sync 不应撞 UNIQUE:{:?}", result.err());

        // 3) 仍只有这一行(UPDATE 不创建新行)
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM documents WHERE case_id = ? AND deleted_at IS NULL",
        )
        .bind(&case_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "应只有 1 行,不应 INSERT 新行");

        // 4) 这行还在(没被软删,source 保留 'llm_extract')
        let (still_alive, src): (Option<String>, String) =
            sqlx::query_as("SELECT deleted_at, source FROM documents WHERE id = 'llm-row'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(still_alive.is_none(), "原 llm-row 不应被软删");
        assert_eq!(src, "llm_extract", "source 应保留 llm_extract");
    }

    /// 回归测试(2026-05-27,advisor 抓到的 ship-blocker):
    /// chat artifact 和 LLM 全局抽产物 source != 'scan',活在 app data 目录而非源
    /// 文件夹。`sync_documents_for_case` 软删环节必须**只**针对 source='scan',
    /// 否则用户点"更新源文件"会把 chat artifact 误删。
    #[tokio::test]
    async fn sync_does_not_soft_delete_chat_or_llm_artifacts() {
        let (pool, case_id) = fresh_pool_with_case().await;

        // 1) 先扫一份普通文件入库
        replace_documents_for_case(
            &pool,
            &case_id,
            &[fake_scanned("民事诉状.docx", Some("立案"), Some("起诉状"))],
        )
        .await
        .unwrap();
        assert_eq!(count_documents_for_case(&pool, &case_id).await.unwrap(), 1);

        // 2) 模拟 chat artifact 入库(source='chat',路径在 app data 外)
        sqlx::query(
            "INSERT INTO documents (id, case_id, source_path, filename, \
             is_ai_artifact, source, extraction_status) \
             VALUES ('chat-art-1', ?, ?, ?, 1, 'chat', 'done')",
        )
        .bind(&case_id)
        .bind("/Users/x/Library/Application Support/CaseBoard/extracts/case-1/chat_artifacts/overview.md")
        .bind("overview.md")
        .execute(&pool)
        .await
        .unwrap();

        // 3) 模拟 LLM 全局抽 artifact(source='llm_extract')
        sqlx::query(
            "INSERT INTO documents (id, case_id, source_path, filename, \
             is_ai_artifact, source, extraction_status) \
             VALUES ('llm-art-1', ?, ?, ?, 1, 'llm_extract', 'done')",
        )
        .bind(&case_id)
        .bind("/Users/x/Library/Application Support/CaseBoard/extracts/case-1/llm_report.md")
        .bind("llm_report.md")
        .execute(&pool)
        .await
        .unwrap();

        // 4) 现在再 sync 一次,但源文件夹"扫不到任何文件"(模拟用户点"更新源文件",
        //    源文件夹空了 / 已存在的诉状文件被改了路径)
        let stats = sync_documents_for_case(&pool, &case_id, &[]).await.unwrap();
        // 普通扫描型文档应被软删
        assert_eq!(stats.deleted, 1, "scan 型文档应被软删");

        // 但 chat / llm_extract artifact 必须**保留**(deleted_at 仍为 NULL)
        let (chat_deleted_at,): (Option<String>,) =
            sqlx::query_as("SELECT deleted_at FROM documents WHERE id = 'chat-art-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            chat_deleted_at.is_none(),
            "chat artifact 不应被 sync 误删!chat_deleted_at = {:?}",
            chat_deleted_at
        );
        let (llm_deleted_at,): (Option<String>,) =
            sqlx::query_as("SELECT deleted_at FROM documents WHERE id = 'llm-art-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            llm_deleted_at.is_none(),
            "LLM 全局抽 artifact 不应被 sync 误删!llm_deleted_at = {:?}",
            llm_deleted_at
        );
    }

    #[tokio::test]
    async fn list_sorts_by_stage_then_filename() {
        let (pool, case_id) = fresh_pool_with_case().await;
        replace_documents_for_case(
            &pool,
            &case_id,
            &[
                fake_scanned("z.pdf", Some("一审"), None),
                fake_scanned("a.pdf", Some("一审"), None),
                fake_scanned("m.pdf", Some("执行"), None),
                fake_scanned("b.pdf", Some("执行"), None),
            ],
        )
        .await
        .unwrap();

        let docs = list_documents_by_case(&pool, &case_id).await.unwrap();
        // ORDER BY stage, filename → 一审 (a, z),然后 执行 (b, m)
        assert_eq!(docs[0].filename, "a.pdf"); // 一审 a
        assert_eq!(docs[1].filename, "z.pdf"); // 一审 z
        assert_eq!(docs[2].filename, "b.pdf"); // 执行 b
        assert_eq!(docs[3].filename, "m.pdf"); // 执行 m
    }
}
