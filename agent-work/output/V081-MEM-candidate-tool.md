# V081-MEM：案件记忆候选原生工具补齐

## 结论

已在 `case_chat` 原生 `ToolRegistry` 注册
`propose_case_memory_candidate`。该工具必须绑定当前 `case_id`，仅接收
`type`、`title`、`content`、`source_message_id` 四个字段，调用既有
`db::case_memory::create_memory_candidate` 创建当前案件的 `pending` 候选。

工具不会创建正式记忆，不会接受、确认或启用记忆，也不会把候选注入 AI
上下文。返回值明确要求用户前往“记忆”页面接受，且接受后仍需二次确认。

## 并发和权限语义

- 工具会写入 `case_memory_candidates`，因此 `is_mutating()` 明确返回
  `true`，由原生工具调度器串行执行。
- “非激活”是本工具的业务边界，不等于数据库只读。
- `case_id` 不作为模型参数开放，只从当前 `ToolContext` 获取；未绑定案件时
  返回 `ToolError::NoCaseBound`。
- 参数 Schema 设置 `additionalProperties: false`，不允许模型传入
  `status`、`active`、`confirmed_by`、`accepted_memory_id` 等越权字段。
- `source_message_id` 如提供，仍由既有数据库复合外键校验其必须属于当前案件。

## 修改文件

- `src-tauri/src/chat/tools/memory_candidate.rs`
- `src-tauri/src/chat/tools/descriptions/propose_case_memory_candidate.md`
- `src-tauri/src/chat/tools/mod.rs`（仅模块声明和默认注册）

未修改 `lib.rs`、前端 API/UI、设备同步或记忆数据库接口。

## 定向测试

新增三项模块测试：

1. 当前对话无 `case_id` 时拒绝；
2. 调用后 `case_memory_candidates` 中 `pending = 1`，同时
   `case_memory_items` 中 `active = 0`；
3. 参数 Schema 仅暴露四个约定字段，并禁止额外字段。

同时测试工具被标记为 `mutating`。

## 验证结果

- 指定文件 `rustfmt --check --edition 2021`：通过。
- 指定文件 `git diff --check`：通过（仅提示共享工作树现有
  `tools/mod.rs` 的 LF/CRLF 转换警告，无空白错误）。
- 定向命令：
  `cargo test --manifest-path .\src-tauri\Cargo.toml memory_candidate --lib --locked`
  已进入本项目编译，但被共享工作树中与本任务无关的
  `src-tauri/src/lib.rs:796` 编译错误阻断：
  `set_package_status` 调用处声明返回 `Result<(), String>`，实际返回
  `Result<LegalSkillPackageRecord, String>`。本任务依约未修改 `lib.rs`。

主控修复该并行改动后，应原样重跑上述定向测试。
