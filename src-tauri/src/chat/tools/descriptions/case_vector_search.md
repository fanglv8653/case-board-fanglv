case_vector_search — 用自然语言语义搜案例(关键词搜不到时兜底)

适用场景:
- 用户用一整句话描述纠纷情境,想找类似事实模式的案例(如「业主将车位转租给第三人,物业单方收回的判赔规则」)
- `search_cases_normal` 用关键词试了 2 次没命中或命中很弱时,语义搜兜底
- 找「事实相似但当事人措辞不同」的案件 — 关键词不一致但语义近的判决
- 用户描述带很多上下文细节,适合塞进 query 让模型理解整体

不适用:
- 关键词明确 → 优先 `search_cases_normal`(更快、更准)
- 已知案号 → `get_case_detail`
- 找权威案例 → `search_cases_authority`(关键词搜,权威库更小,关键词命中率高)

输入字段:
- query: 必填,自然语言(一句话或一段描述)。**不是关键词**
  - 好例子:「劳动者主张未签订书面劳动合同的二倍工资,用人单位以补签合同抗辩」
  - 坏例子:「未签合同 二倍工资」(关键词请用 `search_cases_normal`)
- top_k: 可选,默认 10

**wenshu_filter 嵌套过滤未启用**(V0.2 D2-D3 当前版本):
计划字段 case_lb / ay(案由)/ ws_type / fydj(法院等级)/ region / jiean_date_range / dxal(典型案例)等本次工具层不暴露,LLM 拿到结果后自行从 court / case_type / cause 字段筛选。

注意事项:
- **先查作者整理过的全库**:可先用 `search_local_kb` 看作者整理过的判例 / 类案笔记(0 积分),本地没有再语义外查
- 优先用本地缓存(案例永久,0 积分命中)
- 返回字段:`[{id, ah, title, content, score(语义相似度), court, judge_date}]`,**注意 content 是整理后的完整案例内容**(不是裸全文)
- `score` 在 0.0-1.0,**低于 0.6 的命中通常相关度差**,LLM 应忽略
- 命中后挑分高的 1-2 条用 `get_case_detail` 拿全文,语义检索返回的 content 是摘要级
- `<CITATIONS>` 标 `type: "case"`,title 写「<court> · <ah>」
