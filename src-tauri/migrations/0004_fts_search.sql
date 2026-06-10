-- ============================================================================
-- 0004: FTS5 全文搜索
--
-- 顶部统一搜索框(参考产品 B 必备特性):
-- 案件名称 / 案号 / 当事人 / 法院 / 备注 一搜即得
-- ============================================================================

-- FTS5 虚拟表(关联 cases 主表)
CREATE VIRTUAL TABLE cases_fts USING fts5(
    case_id UNINDEXED,
    name,
    case_no,
    court,
    cause,
    parties_joined,    -- 原告+被告+第三人 用空格 join 后的串(由触发器维护)
    notes              -- 备注/ai_summary
);

-- 案件 INSERT/UPDATE 时同步 fts(初始版用应用层逻辑维护,触发器留给 V0.1.1)
-- 这里只建表,内容由 Rust 代码同步写入

-- 索引说明:
-- - 用 cases_fts.name MATCH 'xxx' 查询
-- - 用 cases_fts.case_no MATCH 'xxx' 查询
-- - 不支持 LIKE,需要全文 token match
