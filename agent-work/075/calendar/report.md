# 0.7.5 日历显示边界实施报告

## 结果

- `CalendarBoard`（飞书日历）与本地 `CalendarPanel` 均只在 `HomeView mode="workspace"` 时挂载。
- `civil` 和 `criminal` 模式不再挂载整块日历。执行模块和案件详情原本未挂载该日历，本次未改动。
- `ImportantDates` 仍按原有 `upcomingEvents` 渲染，关键日期聚合、日历数据和飞书设置均未修改。

## 代码变更

- `src/components/HomeView.tsx`
  - 通过 `shouldMountFullCalendar(mode)` 生成单一显示门禁。
  - 飞书日历和本地日历共用该门禁，防止两条渲染分支偏离。
- `src/components/homeCalendarVisibility.ts`
  - 新增纯函数显示策略：仅 `workspace` 返回 `true`。
- `src/components/homeCalendarVisibility.test.mjs`
  - 覆盖 `workspace/civil/criminal` 三种模式。
  - 静态契约检查两种日历均使用门禁，并确认 `ImportantDates` 仍然存在。

## 验证

- 定向 Node 测试：2/2 通过。
- 全量 Node 逻辑/UI 契约测试：29 个文件、73 项测试全部通过。
- TypeScript：`tsc --noEmit` 通过。
- Vite 生产构建：通过（3,000 kB 级资源包仅有既有 chunk size 警告）。

## 边界

- 未修改设置结构、版本号、飞书功能、非诉功能或 Rust 代码。
- 未提交、未推送 Git。
