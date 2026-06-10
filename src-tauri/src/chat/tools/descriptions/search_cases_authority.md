search_cases_authority — 权威案例库检索(最高法指导案例 + 公报案例 + 典型案例 + 参考案例)

适用场景:
- 想看最权威的裁判口径(最高法 / 公报案例对下级法院有事实上的指引力)
- 给法官 / 对方律师论证时,引用权威案例比普通案例更有说服力
- 用户问「这类纠纷最高院怎么定调」「公报案例对 X 的态度」
- 用 `search_cases_normal` 拿到一堆普通案例后,想再看有没有权威级别的对应案例
- 给客户做法律风险评估,要引用「行业标杆」级判决

不适用:
- 找普通案件 / 同类基层判决 → 用 `search_cases_normal`(覆盖面更广)
- 已知案号拿全文 → `get_case_detail` 传 `type="qwal"`
- 关键词不准 → `case_vector_search`

输入字段:
- qw: 必填,中文关键词,**支持「+」组合**(如「合同解除+违约金」)
- top_k: 可选,默认 20

**高级过滤未启用**(V0.2 D2-D3 当前版本):court / cause / region 等过滤参数本次工具层暂不暴露,LLM 自行从结果里筛选。

注意事项:
- **先查作者整理过的全库**:可先用 `search_local_kb` 看作者整理过的判例 / 类案笔记(0 积分),本地没有再调本工具外查
- 优先用本地缓存(权威案例不过期,0 积分命中)
- 返回字段:`{id, ah, title, court, case_type(指导/公报/典型/参考), judge_date, content(摘要)}`
  - **特别注意** `case_type` — 指导性案例的引用价值 > 公报 > 典型 > 参考,LLM 在 final answer 里要把这个等级告诉用户
- 命中后用 `get_case_detail` (type="qwal") 拿全文,里面有完整裁判要旨 + 法官释法
- `<CITATIONS>` 标 `type: "case"`,title 写「<court> · <ah>(<case_type>)」
