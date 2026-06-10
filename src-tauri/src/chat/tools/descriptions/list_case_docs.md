list_case_docs — 列当前案件所有文档(filename + category + 是否 AI 产物 + extracted_text_path 可读性)

适用场景:
- 用户问「这个案件有哪些文档」「证据材料都在哪」
- LLM 接到任务后,**第一步**就是列文档清单,了解可用材料范围
- 起草前先看文档清单决定要读哪些(配合 `read_case_doc` 拿全文)
- 给用户做案件画像 / 证据目录 / 时间线时的入口

不适用:
- 想看具体文档内容 → 用 `read_case_doc(doc_id)`
- 想在某份文档里找字符串 → 用 `find_in_document(doc_id, pattern)`
- 想搜本地知识库(不是案件文档) → 用 `search_local_kb`
- 当前没绑定案件(自由问答模式) → 本工具会报错 `NoCaseBound`

输入字段:
- 无参数,自动用 ctx.case_id

注意事项:
- **本工具不消耗元典积分**(本地 sqlite 查询)
- 返回结构:`[{id, filename, category, is_ai_artifact, source(scan/llm_extract/chat), has_extracted_text, pinned_at, size_bytes}]`
  - **id** 是文档主键,后续 `read_case_doc` / `find_in_document` 都用这个 id 引用
  - **category**:文档分类(起诉状 / 合同 / 判决 / ...),由扫描时的分类器打的
  - **is_ai_artifact**:true = AI 全局抽生成的 .md 报告(画像 / 风险报告 / 深挖等);false = 原始扫描件
  - **source**:`scan` = 原始文件,`llm_extract` = LLM 全局抽产物,`chat` = chat artifact
  - **has_extracted_text**:true = 已抽取过文字可以 `read_case_doc` 拿全文;false = 抽取未完成 / 失败
  - **pinned_at**:用户在引用弹窗里把这份文档置顶的时间(非 null 表示置顶)
- 返回按 stage(扫描阶段)+ filename 排序,置顶文档不影响排序(置顶仅前端用)
- 文档很多(几十 / 上百份)时,LLM **不要全列给用户**,挑用户问题相关的 5-10 份汇报
- 列文档时,优先告诉用户哪些是「原始证据 vs. AI 产物」,帮用户区分
