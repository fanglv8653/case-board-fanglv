# V081 设置页卡片交付

## 范围

仅新增两个可由 `SettingsModal` 接线的前端组件，未修改共享 API、类型、设置页集成文件或 Rust：

- `src/components/settings/YuandianBalanceCard.tsx`
- `src/components/settings/LegalSkillsSettingsCard.tsx`

## 元典官方余额卡片

- 组件挂载时调用 `getYuandianBalance(true)` 一次。
- 提供独立手动刷新按钮。
- 未使用 `setInterval`、`setTimeout` 或后台轮询。
- 展示官方积分余额、官方次数余额、快照时间、缓存状态和安全错误码。
- 展示两次官方快照之间的官方消耗、本机积分账、本机调用次数与差异。
- 首次快照、充值、匹配、差异、本机账重置分别给出明确说明。
- 官方刷新失败但后端返回缓存时，明确提示当前为缓存，而不把缓存冒充实时结果。

## 全局法律 Skills 卡片

- 列出内置与导入方法包，展示版本、状态、slug 和内容哈希前缀。
- 支持启用/停用；隔离状态不能从前端直接启用。
- 解析 `manifest_json` 后，仅给出该方法包声明的领域与任务绑定选项。
- 支持把已启用方法包设为对应“领域 / 任务”的默认方法包。
- 使用目录选择器读取方法包：
  - 仅构建 `relative_path` / `content`；
  - 仅接收根目录 `SKILL.md`、`manifest.json`；
  - 仅接收 `references/` 下 `.md/.json/.txt`；
  - 缺少两个根文件时在前端阻止提交。
- 页面显著说明：方法包只提供文本方法，不授予工具权限；manifest 的工具声明仍受系统白名单和场景控制。

## 验证

- `node_modules/.bin/tsc.cmd --noEmit`：通过。
- `git diff --check -- src/components/settings/YuandianBalanceCard.tsx src/components/settings/LegalSkillsSettingsCard.tsx`：通过。
- `pnpm exec tsc --noEmit`：未进入编译，因当前 worktree 的 `node_modules` 为跨目录 junction，pnpm 在无 TTY 环境尝试重建依赖并中止；已改用同一依赖目录中的 TypeScript 编译器完成等价静态检查。

## 主控接线提示

- 数据源页渲染 `<YuandianBalanceCard />`。
- 大脑或法律 Skills 设置区渲染 `<LegalSkillsSettingsCard />`。
- 两个组件已直接使用主控提供的 API 函数名，无额外 props。
