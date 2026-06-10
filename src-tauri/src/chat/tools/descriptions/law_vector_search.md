law_vector_search — 用自然语言语义检索法条(关键词检索不准时兜底)

适用场景:
- 用户用一整句话或一段描述问问题,关键词不明确(如「物业擅自处分业主财产构成什么」)
- `search_laws` 用关键词试了 2 次都没命中,改用本工具语义搜
- 找「跟某行为类似但法条措辞不同」的条款,如「擅自处分」语义近的条款用词可能是「无权处分」「越权代理」等
- 想找的法条核心词比较生僻,关键词检索可能漏

不适用:
- 用户给出明确关键词 → 优先 `search_laws`(更快、更准)
- 已知条号 → `get_law_article`
- 想找案例 → `case_vector_search`(案例的语义搜)

输入字段:
- query: 必填,**自然语言描述**(一句话或一段话),不是关键词
  - 好例子:「业主把车位转租给第三人,物业能否单方收回」
  - 坏例子:「车位 转租」(这种关键词请用 `search_laws`)
- effect_level: 可选,过滤效力等级
- valid_only: 可选,默认 true
- implement_date_start / implement_date_end: 可选,YYYY-MM-DD
- top_k: 可选,默认 10

注意事项:
- **先查作者整理过的全库**:可先用 `search_local_kb` 看作者本地已整理的资料(0 积分),本地没有再语义外查
- 优先用本地缓存(法条永久缓存)
- 返回结构:**注意嵌套**,`data.extra.fatiao[]`(不是 `data[]`),每条带 `score`(语义相似度)
- 语义检索的命中可能比关键词检索更宽,LLM 拿到后**必须**结合上下文判断相关性,不要无脑全用
- 命中后建议挑 1-2 条最相关的,再用 `get_law_article` 拿这几条的精确全文(`law_vector_search` 返回的是摘要);**结果里带 `fgid` 的,务必把 `fgid` + `ftnum` 一起透传给 `get_law_article` —— 整部法规缓存一次、该法规后续条文 0 积分**
- `<CITATIONS>` 标 `type: "law"`,title 写「<fgmc> 第 <ftnum> 条」
- 积分消耗同 `search_laws`(本地命中 0,在线 1)
