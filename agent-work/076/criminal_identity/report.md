# 刑事案件信息纯逻辑契约验收报告

## 任务边界

- 新增 `src/lib/criminalCaseIdentity.ts`：提供刑事案件名称、罪名、程序身份、阶段日期和当事人字段的纯函数契约。
- 新增 `src/lib/criminalCaseIdentity.test.mjs`：覆盖契约的正常、缺失、冲突和禁止推断场景。
- 未修改 `CaseSnapshotView`、`CriminalCasePanel`、Rust、数据库迁移或飞书写入逻辑。
- 共享工作树中其他界面及 view-model 变更属于并行任务，本报告不将其计入本任务交付。

## 已冻结规则

1. 案件显示名称与纯罪名分离：纯罪名只读取 `suspectedCharge`，不从案件名或文件夹名反向切割。
2. 显示名称优先级：人工 `display_name_override` > 当事人姓名加罪名 > 罪名 > 已存名称 > 未命名刑事案件。
3. 已有人工名称不得被重新识别、飞书或历史候选覆盖；只有人工操作可以替换或清空。
4. 刑事程序称谓：侦查、审查起诉阶段为“犯罪嫌疑人”；一审、二审及已明确进入审判但审级不明时为“被告人”；程序阶段未知时明确显示未决组合称谓，不擅自判断。
5. 阶段日期严格按当前阶段选择：
   - 侦查：`detention_date`
   - 审查起诉：`prosecution_received_date`
   - 一审：`first_instance_accepted_date`
   - 二审：`second_instance_accepted_date`
6. 阶段明确但专属日期缺失时，返回对应 `field`、`value: null`、`displayValue: 待核实` 和 `status: missing`，便于界面定位人工补录。
7. `filed_at` 仅出现在输入类型中用于声明禁止边界；任何情形都不作为刑事阶段事实回退值，也不借用其他阶段日期。
8. 公诉机关、犯罪嫌疑人/被告人和委托人为三个独立字段，缺失时保持空值，不互相补位。

## 正式库证据的处理

主控提供的核对结果显示：目标案件尚无刑事画像记录，虽存在 `agg_filed_at=2025-05-30`，但提取材料中没有该日期的精确来源。因此契约将其视为无可靠来源的通用日期，不展示为确定的刑事程序日期；应显示对应阶段字段“待核实”并允许人工补录。

## 验证结果

- `node --test src/lib/criminalCaseIdentity.test.mjs src/modules/litigation/components/criminal/partyTerminology.test.mjs`
  - 11 项测试通过，0 失败。
- `node node_modules/typescript/bin/tsc --noEmit`
  - 通过。
- `node node_modules/vite/bin/vite.js build`
  - 通过，2863 个模块完成转换；仅保留项目既有的大 chunk 提示。
- `git diff --check -- src/lib/criminalCaseIdentity.ts src/lib/criminalCaseIdentity.test.mjs`
  - 通过。

## 集成提示

- 界面层应使用 `stageDate.field` 定位对应输入项；`status === "missing"` 时显示“待核实”，不得自行读取 `agg_filed_at`。
- 自动识别和飞书入站在写入名称前应调用 `mergeCriminalDisplayNameOverride`，确保人工名称不可被覆盖。
- 本任务没有提交、推送或标记 accepted，等待主控审查和集成。
