read_case_doc — 读单份案件文档的完整抽取文本(支持 offset + length 分段读)

适用场景:
- 用户问到具体文档内容(「合同里关于违约金的条款怎么写的」「判决书的本院认为段落」)
- 写文书前先精读关键证据(合同 / 判决 / 起诉状),拿完整文本理解事实
- 配合 `list_case_docs` 先选出要读的 doc_id,本工具拿全文
- 看 AI 产物(画像 / 风险报告)的完整内容

不适用:
- 不知道文档 id → 先用 `list_case_docs` 拿清单
- 只想在文档里搜字符串 → 用 `find_in_document`(grep 风格,带行号)
- 文档没抽取过文字(has_extracted_text=false) → 本工具会报错,提示用户去文档详情页重新抽取
- 想读 KB 文件 → 用 `read_kb_file`

输入字段:
- doc_id: 必填,文档标识。**两种写法都行**:① `list_case_docs` 拿的 id(UUID,最稳,零歧义);② 直接填文件名(如「5、民事起诉状.docx」)。拿不准就先 `list_case_docs` 看清单
- offset: 可选,从第几个 char 开始读(默认 0)
- length: 可选,读多少 char(默认 8000)。**单次最大 30000**,超过用 offset 翻页

> ⚠️ 注意:部分材料(证据类合同、催告函等)在导入时**被跳过抽取**(省成本),它们 `has_extracted_text=false`,本工具读不到内容会报错并列出可用文档。这类需要内容时提示用户在详情页对该文件单独重抽。

注意事项:
- **本工具不消耗元典积分**(本地文件读)
- 返回结构:`{filename, category, total_chars, content, has_more}`
  - `total_chars` = 文档全部字符数,用于分页判断
  - `has_more` = true 表示 offset+length 还没读到结尾,可以继续 offset = 当前 offset + length
- 长文档(>8000 字)默认只读前 8000,LLM 看到 has_more=true 决定是否要继续读
- 文档过大(>30000 字)时,建议先用 `find_in_document` 定位关键段落,再用 offset 精确读该段
- 文档内容**不要全塞进 final answer**,引用时复述关键句 + 落 `<CITATIONS>` `type: "case_doc"` + source = doc_id + title = filename
- 当前对话没绑定 case_id 时报错 `NoCaseBound`
