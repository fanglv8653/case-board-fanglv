# V081-MEM 前端与 API 类型层交付报告

日期：2026-07-29

## 交付结果

- 顶部导航新增“记忆”入口，位于“工具”之后、“团队”之前。
- 新增全局记忆页：
  - 只读展示系统规则与证据边界；
  - 管理用户全局偏好草稿并人工确认启用；
  - 明确选择案件后才读取该案件记忆。
- 新增案件记忆面板：
  - 创建记忆草稿；
  - 接受候选项为草稿、拒绝候选项；
  - 人工确认启用、修订后重新确认、停用；
  - 逐轮选择案件记忆和全局偏好；
  - 生成注入预览、校验预览哈希并确认本轮预览。
- 新增记忆领域 TypeScript 类型及 13 个 Tauri 命令封装，并按当前 `src-tauri/src/lib.rs` 的实际参数签名核对。

## 人工门禁与默认行为

- 新建案件记忆与全局偏好均默认为草稿。
- 注入模式默认为 `archive_only`（仅归档）。
- AI 候选项“接受”后仍为草稿，不会直接启用。
- 修订产生新修订号，界面要求再次人工确认。
- 只有“已启用 + 当前修订已确认 + manual_each_turn”的条目可供逐轮选择。
- 案件切换时清空已选条目和旧预览。
- 任一选择或任务类型变化时使旧预览失效。
- 预览确认只确认该次预览记录；界面明确说明不会自动发送 AI 请求。

## 文件范围

- 修改：
  - `src/App.tsx`
  - `src/components/ModuleTabs.tsx`
  - `src/lib/api.ts`
  - `src/lib/types.ts`
- 新增：
  - `src/components/memory/MemoryView.tsx`
  - `src/components/memory/CaseMemoryPanel.tsx`
  - `scripts/test-v081-memory-frontend.cjs`

未修改设置页、主题文件或任何 Rust 文件；未运行 Rust fmt/build。

## 验收证据

1. TypeScript：
   - 命令：`node node_modules/typescript/bin/tsc --noEmit`
   - 结果：通过。
2. 专项逻辑检查：
   - 命令：`node scripts/test-v081-memory-frontend.cjs`
   - 结果：`V0.8.1 记忆前端专项检查通过`。
   - 覆盖：13 个命令名、顶部入口、全局安全提示、草稿/预览门禁、案件切换与选择变化后的状态清理。
3. Vite 生产构建：
   - 命令：`node node_modules/vite/bin/vite.js build`
   - 结果：通过，2873 个模块完成转换。
   - 备注：保留项目原有“大于 500 kB chunk”构建警告，不属于本包新增失败。
4. 差异检查：
   - 命令：`git diff --check -- <本包文件>`
   - 结果：无空白错误；仅有 Git 的 LF/CRLF 工作区提示。

## 尚待主控集成验收

- 需在完整 Tauri 运行时中验证空库、既有草稿、候选项和跨案件切换的真实 IPC 行为。
- 需由主控确认后端迁移与命令注册已随最终集成包进入同一版本。
- 本包未接入任何 AI 对话自动注入路径，这是刻意的首期安全边界。
