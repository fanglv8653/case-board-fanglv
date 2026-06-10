find_in_document — 在指定案件文档里 Ctrl+F 搜字符串(带行号 + 片段)

适用场景:
- 文档很长(>8000 字),用户问的关键词只占其中一小段,直接 `read_case_doc` 读全文浪费 token
- 用户问「合同里有没有提到 X」「起诉状里被告地址是什么」
- 用模糊匹配定位关键段落,再用 `read_case_doc` 配合 offset 精确读该段
- 校验对方主张:「对方说合同里有 Y 约定,我看看在哪一段」

不适用:
- 不知道在哪份文档里找 → 先用 `list_case_docs` 看候选
- 想读完整内容 → `read_case_doc`(整段读)
- 搜本地 KB → `search_local_kb`(整库搜)
- 搜法条 → `search_laws`
- 文档没抽取过文字 → 本工具会报错

输入字段:
- doc_id: 必填,从 `list_case_docs` 拿
- pattern: 必填,搜索关键词或 regex。**默认不区分大小写**;特殊字符自动 escape
- case_sensitive: 可选,布尔,默认 false
- max_hits: 可选,返回上限,默认 10

注意事项:
- **本工具不消耗元典积分**(本地 regex grep)
- 返回结构:`[{line_no, snippet, match_start, match_end}]`,按 line_no asc 排
  - `line_no`:文档里的行号(1-based)
  - `snippet`:命中位置前后 200 字符,命中部分用 `**` 包裹
  - `match_start / match_end`:命中在 snippet 里的 char 偏移
- 拿到 line_no 后,**下一步通常用 `read_case_doc` + offset** 精确读该段上下文
- pattern 用关键词即可,**不要用复杂 regex**(LLM 写 regex 容易错,且元典检索语义更适合通用工具)
- 命中数多时(>10),建议先精确化 pattern,而不是无脑 max_hits=100
- 引用具体段落时,`<CITATIONS>` 标 `type: "case_doc"`,title 写「<filename> · 第 <line_no> 行」
- 当前对话没绑定 case_id 时报错 `NoCaseBound`
