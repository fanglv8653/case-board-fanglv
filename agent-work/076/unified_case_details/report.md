# 0.7.6 统一案件详情前端报告

## 结果

- 顶部案件标题与“案件信息”页签内的案件名称均调用 `getCaseDisplayName`。
- 原外层“案件基本信息”已作为 `basicInformation` 插槽移入四页签的“案件信息”，原 override 编辑/恢复能力保留。
- 外层 `CaseSnapshotView` 切换为 `supplemental` 模式，只保留审级、收费、办案时间轴、保全等非重复内容；不再重复显示基本信息、待办和联系人。
- “待办提醒”页同时呈现原 `TodosCard` 手工待办、刑事流程提醒、阶段与期限，各自继续读写原数据源，不合并或丢失记录。
- “案件通讯录”以 `case_agency_contacts` 为正式数据源；`agg_court_contacts/agg_party_contacts` 只在“材料抽取的待确认联系人”中显示，律师逐项点击“确认录入”后才写入正式通讯录。

## 刑事字段边界

- 复用 `src/lib/criminalCaseIdentity.ts` 的纯逻辑契约，不再另写阶段、称谓或阶段日期推断。
- 案件名称与罪名分开：罪名正式值只读可人工保存的 `criminal profile.suspected_charge`；`raw agg_cause` 只作“材料聚合，待确认”占位提示，不读错误的 `user override agg_cause`。
- 公诉机关、犯罪嫌疑人/被告人、委托人分为三个独立字段。委托人只用 `profile.client_name`，不从被告人或检察院推断。
- 公诉机关优先使用正式通讯录；无正式值时，`agg_plaintiffs` 中的检察院仅显示为“待核实”。
- 当前承办/审判机关独立显示 `agg_court/court`，不因公诉机关修正而丢失法院信息。
- 当前阶段使用契约生成“犯罪嫌疑人”或“被告人”称谓，并只选择当前阶段专属日期。缺失时显示“待核实”且保留可编辑日期输入，不借用通用立案日期。
- 刑事基本信息隐藏无专业意义的“案件类型=诉讼”，保留“案件领域=刑事”。

## 修改清单

- `src/modules/litigation/components/CaseView.tsx`
- `src/modules/litigation/components/snapshot/CaseSnapshotView.tsx`
- `src/modules/litigation/components/criminal/CriminalCasePanel.tsx`
- `src/modules/litigation/components/criminal/criminalManagementViewModel.ts`
- `src/modules/litigation/components/criminal/criminalManagementViewModel.test.mjs`
- `src/modules/litigation/components/criminal/unifiedCaseDetails.test.mjs`

## 验证

- 定向 Node：15 项通过（刑事身份契约、联系人待确认、统一详情 UI 契约）。
- 全量 Node：31 个文件、87 项通过。
- TypeScript：`tsc --noEmit` 通过。
- Vite 生产构建：通过；仅有仓库既有的 chunk size 警告。
- `git diff --check`：通过（仅 CRLF 提示）。

## 边界

- 未修改 Rust、数据库迁移、公共 types/api 或版本号。
- 未删除原待办、期限、工作记录、联系人或画像数据。
- 未提交、未推送 Git。
