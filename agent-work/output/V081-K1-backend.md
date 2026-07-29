# V081-K1 本地知识库“检索与维护说明”后端报告

> 日期：2026-07-29
> 范围：真实行为同源的只读说明数据源、方律原生 AI 只读工具
> 状态：实现完成；待主控更新共享 Cargo.lock 后执行综合 Rust 复验

## 一、完成内容

### 1. 结构化只读说明源

新增 `src-tauri/src/local_kb/guide.rs`，输出：

- 当前绑定根目录及是否可用；
- 关键词检索真实范围、文件类型、5 MB 上限、排除路径和排序规则；
- 语义检索需要 embedding 凭据及预先建立、根目录和模型签名匹配的索引；
- 语义查询本身不建立或更新索引；
- `read_kb_file` 只接受根目录内相对路径、canonical 边界校验、5 MB 上限及 NUL 二进制拒绝；
- 本说明、内部 AI 工具和目录/索引维护之间的明确边界。

该数据源没有照搬上游“Wiki 导航 → raw 正文 → 写回 L1 raw”的目录契约，明确说明方律当前行为：

- 关键词检索递归扫描用户绑定根目录；
- 不要求采用 `raw/wiki` 等固定目录名；
- 自定义目录里的 `.md/.txt` 同样可以命中；
- `raw/yuandian-cache` 默认不进入关键词检索；
- 语义索引继续沿用方律已有语料采集、法律去重和元典详情过滤规则。

### 2. 与实际关键词行为共用常量

在 `local_kb/search.rs` 将以下真实搜索常量收敛为模块内共享事实：

- `MAX_FILE_SIZE`
- `KEYWORD_FILE_EXTENSIONS`
- `KEYWORD_EXCLUDED_ROOT_PREFIX`
- `KEYWORD_EXCLUDED_SEGMENTS`

搜索实现和说明数据源共同读取这些常量，避免界面说明与代码限制漂移。

### 3. 方律原生 AI 只读工具

新增：

- `chat/tools/kb_guide.rs`
- `chat/tools/descriptions/get_local_kb_guide.md`
- 工具名：`get_local_kb_guide`

工具特征：

- 参数为空；
- `is_mutating=false`；
- 无元典积分消耗；
- 不读取知识库正文；
- 不创建目录；
- 不触发 embedding 或索引构建；
- 不生成或修改 `AGENTS.md`、`CLAUDE.md`；
- 不开放任何 AI 写入能力。

工具已加入默认 `ToolRegistry`。主控若要求它在方律三个受限 scene 中也始终可用，需要在综合集成时把工具名加入相应场景 allowlist；本子任务未修改工作流 JSON。

## 二、测试设计

新增三个定向测试：

1. 在非标准自定义目录写入 Markdown，调用真实 `search_kb_files`，证明可以命中；
2. 同时放置 `raw/yuandian-cache`、`node_modules` 和 PDF，证明真实关键词搜索只返回自定义 Markdown；
3. 验证未绑定状态仍能读取说明，且输出不包含外部 AI 入口生成或知识库写入工具；
4. 验证 `get_local_kb_guide` 无参数且为只读工具。

## 三、当前验证结果

已通过：

- 五个目标 Rust 文件的单文件 `rustfmt --check --edition 2021`；
- 目标文件 `git diff --check`；
- 源码只读核对：说明字段与 `search_kb_files`、`read_kb_file`、`semantic_search` 的当前行为一致。

Rust 定向测试命令：

```text
cargo test --manifest-path .\src-tauri\Cargo.toml guide --lib --locked
```

当前未能进入编译，原因是共享工作树另一路已在 `src-tauri/Cargo.toml` 新增
`aes-gcm`、`ed25519-dalek`、`rand`，但 `Cargo.lock` 尚未更新；`--locked`
按预期拒绝隐式更新锁文件。本任务未修改锁文件、未联网下载依赖。主控更新锁文件后应重新执行上述命令。

## 四、明确未实施

- 未开放 `save_local_kb_material` 或其他 AI 写入；
- 未生成外部 AI 入口；
- 未修改、覆盖或删除知识库文件；
- 未自动创建 `raw/wiki` 目录；
- 未触发索引重建；
- 未修改 `lib.rs`、`api.ts`、`types.ts`、`SettingsModal.tsx`；
- 未提交 Git。

## 五、实际文件范围

- `src-tauri/src/local_kb/guide.rs`
- `src-tauri/src/local_kb/search.rs`
- `src-tauri/src/local_kb/mod.rs`
- `src-tauri/src/chat/tools/kb_guide.rs`
- `src-tauri/src/chat/tools/descriptions/get_local_kb_guide.md`
- `src-tauri/src/chat/tools/mod.rs`
- `agent-work/output/V081-K1-backend.md`
