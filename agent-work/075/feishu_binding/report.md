# 0.7.5 飞书人工绑定与名称归一化实施报告

## 结论

已完成本地人工绑定闭环，状态提交为 `submitted_for_review`。实现严格保持飞书零写入：网络层仍只有既有“在办案件只读拉取”；绑定、解除、忽略和恢复仅写入本地 `feishu_sync_links`、`feishu_sync_inbox` 与新增审计表，不更新 `cases`、工作记录、阶段、通讯录、Windows 文件夹名或飞书案件名。

## 实施内容

- 新增迁移 `0051_feishu_manual_binding.sql`：
  - `feishu_sync_inbox.auto_bind_suppressed`，解除绑定后防止下一次只读拉取自动重新绑定；
  - `feishu_sync_binding_audits`，记录 `auto_bind/manual_bind/unbind/ignore/restore` 本地审计。
- 自动匹配收紧为：只有“唯一精确案号”可在只读拉取中自动绑定。
- 本地常见日期前缀（`YYYYMMDD`、`YYYY-MM-DD`、`YYYY.MM.DD`、`YYYY/MM/DD`）只在候选匹配层剥离；不回写任何名称。
- 归一化名称推荐必须同时满足：名称一致、法律领域一致、当事人一致、案由/罪名一致；多候选不推荐默认项，必须人工选择。
- 前端新增：
  - 待绑定案件的本地案件选择与推荐理由；
  - 确认绑定、解除绑定、忽略、恢复；
  - 单独“已忽略案件”区；
  - 可逆操作后自动刷新本地预览。

## 修改文件

- `src-tauri/migrations/0051_feishu_manual_binding.sql`
- `src-tauri/src/db/feishu_sync.rs`
- `src-tauri/src/lib.rs`
- `src/lib/api.ts`
- `src/lib/types.ts`
- `src/modules/tools/FeishuSyncPreview.tsx`

## 公共接口最小追加

### Tauri 命令（`src-tauri/src/lib.rs`）

- `bind_feishu_sync_case`
- `unbind_feishu_sync_case`
- `ignore_feishu_sync_case`
- `restore_feishu_sync_case`

### 前端 API（`src/lib/api.ts`）

- `bindFeishuSyncCase`
- `unbindFeishuSyncCase`
- `ignoreFeishuSyncCase`
- `restoreFeishuSyncCase`

### 前端类型（`src/lib/types.ts`）

- `FeishuSyncInboxPreview.recommended_case_id`
- `FeishuSyncInboxPreview.recommendation_reason`
- `FeishuLocalCaseOption`
- `FeishuSyncPreview.ignored_cases`
- `FeishuSyncPreview.available_local_cases`

公共文件均为局部追加，未重排或覆盖其他 Agent 新增的合同审查字段。

## 验收证据

1. `cargo test feishu_sync --lib`
   - 结果：`5 passed; 0 failed; 105 filtered out`。
   - 覆盖：日期前缀仅用于匹配、名称只推荐不自动绑定、人工绑定/解绑/忽略/恢复、审计落库、解除后自动重绑抑制、案件业务字段不变、既有只读拉取幂等。
2. `pnpm run build`
   - 结果：TypeScript 与 Vite 生产构建通过，`2863 modules transformed`。
3. `git diff --check -- <本任务文件>`
   - 结果：通过，无空白错误。
4. `cargo fmt --all -- --check`
   - 在后端校验组合命令起始阶段通过；随后 `cargo check/clippy` 因主控要求尽快提交且共享构建耗时较长而主动停止，不据此宣称完成。主控集成轮需继续执行全库 check/clippy。

## 边界核查

- 未增加飞书写权限或飞书写 API。
- 未修改 `cases` 业务字段。
- 未修改工作记录、阶段、通讯录的外部来源字段。
- 未重命名本地文件夹或飞书记录。
- 未修改版本号、非诉功能或日历功能。
- 未提交、未推送 Git。
