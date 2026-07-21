# 0.7.5 非诉、合同审查与 DOCX 作者时间实施报告

## 结论

已完成本任务范围并提交主控验收：非诉首页保持三张主卡；合同审查增加精简背景、材料范围和复核门禁；新增律师函核心闭环；合同修订批注作者支持持久默认值与本次覆盖，OOXML 日期由后端在导出开始读取一次本机时间并以带时区 RFC3339 写入。

## 主要实现

### 合同 DOCX 作者与时间

- 新增 Settings 字段 `contract_review_comment_author`，作者优先级为：本次临时作者 > 已保存合同批注作者 > `user_display_name` > `合同审查（CaseBoard）`。
- `export_contract_redline_docx` 在后端解析最终作者，前端不传历史时间。
- `redline::build_redlined_docx` 在一次导出开始时仅获取一个 `chrono::Local::now()` 时间快照，使用 `to_rfc3339_opts(..., false)` 保留本机时区偏移；北京时间环境为 `+08:00`。
- `word/comments.xml` 及 `word/document.xml` 中全部 `w:comment`、`w:ins`、`w:del` 共用同一作者和时间字符串。
- 工作稿修订版在首段增加“待执业律师复核、不得直接对外发送或签署”批注；正式稿必须通过三项门禁。

### 合同审查精简增强

- 可折叠录入交易目的、当前阶段、可协商程度、附件/待核材料说明，未强制用户填写大型表单。
- 审查结构新增材料范围、缺失材料、主附件一致性问题、事实基础、事实状态、法源状态、律师复核状态。
- 后端强制所有 AI 风险初始为“事实待律师复核、法源待核验、律师待复核”，不信任模型自行声称已确认。
- 工作稿可直接导出并带状态标识；正式意见书和正式修订版均要求“事实已核对 + 法源已核验 + 执业律师已审核”。后端再次校验，不能仅绕过前端。

### 非诉律师函

- 非诉首页新增第三张“律师函”主卡；未加入民事起诉状或刑事法律意见书。
- 核心 intake 只要求发函方、收函方、基本事实、具体要求；双方关系、履行期限、证据、法源说明折叠为可选项。
- 原生生成律师函工作稿、待补事项和发出风险；提示词要求事实/法源分层、禁止编造、禁止声称已发送。
- 工作稿 DOCX 带待复核标识；正式稿同样执行三项后端门禁。应用不提供自动发送。
- 可由用户主动选择本地案件并创建阶段提醒，`source=local`，不调用飞书写接口。

## 新增或修改的契约/API

- Settings：`contract_review_comment_author: Option<String>` / `string | null`。
- 合同审查入参新增：`transactionGoal`、`transactionStage`、`negotiability`、`attachmentNote`。
- 合同审查结果新增：`material_review`；单项风险新增 `fact_basis`、`fact_status`、`legal_source_status`、`lawyer_review_status`。
- 合同 DOCX 导出新增：`documentStatus`、`factsConfirmed`、`sourcesVerified`、`lawyerConfirmed`。
- 新增命令/API：`generate_demand_letter` / `generateDemandLetter`、`export_demand_letter_docx` / `exportDemandLetterDocx`。
- 未新增迁移；未占用 `0051`/`0052`。

## 修改文件

- `src-tauri/src/settings.rs`
- `src/lib/types.ts`
- `src/lib/api.ts`
- `src-tauri/src/contract_review/analyze.rs`
- `src-tauri/src/contract_review/mod.rs`
- `src-tauri/src/contract_review/redline.rs`
- `src-tauri/src/contract_review/report.rs`
- `src-tauri/src/demand_letter.rs`（新增）
- `src-tauri/src/lib.rs`
- `src/modules/transaction/ContractReviewTool.tsx`
- `src/modules/transaction/TransactionModule.tsx`
- `src/modules/transaction/DemandLetterTool.tsx`（新增）

## 验证证据

1. `cargo test contract_review:: --lib`
   - 3/3 通过：作者优先级、正式稿门禁、OOXML 解包作者/统一本机带时区时间。
2. `cargo test demand_letter:: --lib`
   - 1/1 通过：核心 intake 必填门禁。
3. `cargo test export_timestamp_uses_current_machine_offset --lib`
   - 1/1 通过：导出时间 RFC3339 偏移与本机当前偏移一致。
4. `node node_modules/typescript/bin/tsc --noEmit`
   - 通过。
5. `node node_modules/vite/bin/vite.js build`
   - 通过（仅保留项目既有大 chunk 警告）。

补充：曾尝试串行 `cargo check --lib` 与并行 Vite 验证，因共享 Cargo build directory 锁等待导致该组合命令超时；随后定向 Rust 测试、TypeScript 与 Vite 已分别独立通过。未为此删除锁或终止其他 Agent 的构建。

## 边界核对

- 未修改飞书同步代码、飞书权限或飞书数据。
- 未自动发送律师函。
- 未将民事/刑事文书放入非诉首页。
- 未修改日历、版本号、Release、旧工作树或案件原材料。
- 未提交、未推送；等待主控验收。
