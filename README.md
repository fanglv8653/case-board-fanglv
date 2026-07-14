# 案件看板 · CaseBoard

> 律师个人案件可视化看板,本地优先,不改动你的源文件。
> 把散落在文件夹里的诉讼材料,一眼看清「这个案件现在到哪一步」。

[![License](https://img.shields.io/badge/license-PolyForm%20Noncommercial%201.0.0-orange.svg)](./LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11%20%7C%20macOS%2011%2B-lightgrey.svg)](#安装)
[![Stack](https://img.shields.io/badge/stack-Tauri%202%20%2B%20React%2019-1abc9c.svg)](#技术栈)

---

## 是什么

CaseBoard 是单人律师的**案件进展可视化看板**,定位:取代 Excel 案件登记表。

- **打开 App 一眼看清** 案件在哪个阶段、关键信息是什么 —— 不用再去源文件夹翻
- **当前阶段的信息显示在最前** —— 在执行就显执行卡,在审理就显审理卡
- **关键信息 LLM 自动抽取** —— 用户不用手填字段
- **本地优先** —— 源文件只读不动;关键信息抽取 / OCR 默认走云端(DeepSeek / MinerU),可选配本机模型

## 界面预览

| | |
|:---:|:---:|
| ![诉讼首页:个性化问候 + 重要日期提醒 + 在办案件卡片](./.github/assets/shot-1-dashboard.png) | ![案件详情 + 案件 AI 助手:确认案情、定位争议焦点、自动检索法规与类案](./.github/assets/shot-2-ai-agent.png) |
| 诉讼首页 · 重要日期 + 在办案件卡片 | 案件详情 + AI 助手定位争议焦点 |
| ![AI 生成的类案检索分析文书,所见即所得编辑、导出 HTML / Word](./.github/assets/shot-3-doc-editor.png) | ![工具模块:法律计算工具、本地知识库共享、案件自动化](./.github/assets/shot-4-toolbox.png) |
| AI 生成文书 · 所见即所得编辑 / 导出 Word | 法律工具箱 · 计算器 / 知识库 / 自动化 |

## 核心模块

| 模块 | 内容 |
|:---|:---|
| 📋 **诉讼** | 案件看板 · 11 档智能状态机(接案 / 立案中 / 仲裁中 / 待开庭 / 审理中 / 已调解 / 上诉期 / 二审 / 再审 / 执行中 / 已结案),含仲裁→一审→二审→再审审级历程 |
| ⚖️ **执行** | 自动筛选执行中案件,独立展示:被执行人(身份证 / 地址 / 电话)/ 执行标的 / 执行节点时间轴 / 调解判决履行约定 / 执行法院联系 |
| 💼 **非诉** | 7 类业务字段框架 + 7 问访谈引导(等同事访谈回填) |
| 🛠 **工具** | 8 个法律计算/转换工具:刑事量刑辅助、全国律师费、诉讼费、利息执行款、交通事故赔偿、劳动解除赔偿、天数、数字大写；规则状态和结果均要求人工核对 |

## 关键能力

### 🪄 LLM 全局抽 — 不靠规则,模型通读全案

把案件所有文档(起诉状 / 判决书 / 调解书 / 笔录 / 合同 / 身份证 等)的文本拼起来,**一次性喂给 DeepSeek 1M 上下文模型**,让它输出:

1. **结构化 JSON 填表** — 案号 / 法院 / 案由 / 当事人 / 法官 / 关键日期 / 收费 / 调解结果 / 案件一句话概括 全部自动填到数据库
2. **完整案件分析报告 MD** — 资深律师助理视角的案件梳理,包含案件概况 / 当事人与代理 / 时间线 / 争议焦点 / 程序进展 / 关键日期提醒 / 法院联系 / 注意事项

对比传统"规则聚合":跨文档关联 / 反诉去污 / 同名合并 / 字段去重 全由 LLM 自动处理,**改字段只需改 prompt**,不再维护几百行规则代码。

实测一个 18 份文档的案件,DeepSeek v4-flash 全局抽 25 秒,成本 < ¥0.01。

### 🔍 元典查被执行人 + LLM 风险提示报告(执行模块独占)

执行案件详情页「🔍 查被执行人」按钮 → 自动从 LLM 抽出的当事人列表拿"被执行人":

- **企业**:走 14 个元典端点 — 工商画像 / 失信 / 限消 / 被执行案件 / 法律文书 / 法院公告 / 开庭公告 / 股权出质 / **股权冻结**(财产线索)/ **对外投资**(关联公司)/ 工商变更 / 担保 / 行政处罚 / 经营异常 / 严重违法
- **自然人**:走元典权威 + 普通案例库 按姓名 + 身份证号搜文书

所有原始 JSON 落 `~/Library/Application Support/CaseBoard/external/<case_id>/yuandian_raw/` 不浪费(每次查询都烧元典积分,本地保留)。

之后喂 DeepSeek 单次 LLM call → 输出**风险提示报告 MD**(参考资深律师风格:摘要 / 关键画像 / 被执行案件全景 / 失信限消 / 财产线索 / 核心差异与下一步行动)+ **深挖建议 JSON**(关联公司 / 新发现案号 / 第三方主体,供「🔬 深挖」按钮用)。

实测一个 2 个被执行人的案件:32 个 API 调用 + 1 次 LLM ≈ 60-90 秒,报告 1000-3000 字。

报告里 LLM 还会主动给出**深挖建议**(`dig_hints`)— 标出值得继续深挖的关联公司 / 新发现案号 / 第三方主体。

### 🔬 P2 深挖关联公司 · 资产路径

风险报告右上「🔬 深挖」按钮:按 LLM 给的深挖建议递归调元典 API:
- **关联公司**:跑 7 个核心端点(aggregation / executions / executed_person / out_invest / frozen_equity / pledge / writ_list)
- **新发现案号**:`search_qwal` + `search_ptal` 按案号(ah=)拿文书详情
- **第三方主体**:按姓名搜文书

P1 + P2 全部原始 JSON 拼一起 → DeepSeek 出**深查报告**,**核心差异与下一步行动**节按重要程度排序,律师直接拿来用。参考真实案件 yuandian_深查 108 行报告格式(摘要 / 主体 N · 关键画像 / 被执行案件全景 / 涉诉新发现 / 工商变更 / 财产线索 / 失信限消 / 关联公司分析 / 新发现案号 / 核心差异与下一步行动 / 数据来源)。

### ⏰ 保全续封 · 上诉期 到期提醒

LLM 抽 key_dates 时自动给出 `expires_at`(应用法律知识):
- 动产 / 资金保全 → 1 年到期
- 不动产 / 股权 → 3 年到期
- 民事一审判决书上诉期 → 15 天
- 民事裁定书上诉期 → 10 天
- 调解约定还款期 → 每期单独列

首页「重要日期」widget 自动按到期日排序,**< 7 天红色 · < 30 天橙色 · 已过期灰色**,避免错过续封 / 上诉关键期。

### 📖 案件分析报告 → HTML / Word 一键导出

详情页「案件报告」按钮 → 弹窗渲染 MD → 右上角 HTML / Word 导出:

- **HTML**:陶土红 × 羊皮纸 法律文书专业风格(衬线标题 + meta pills + 案件速览 + 报告正文 + 打印优化),内嵌 CSS 单文件可分享
- **Word**:走 macOS 原生 textutil HTML → docx,Times + 中文字体,字号符合 Word 视角(14px 正文 / 15px 一级标题 / 16px 居中大标题)

### 🚀 8 路并发抽取

OCR + LLM 抽取走 `buffer_unordered(8)` 流式并发(任意时刻最多 8 个 task,完成一个补一个,不是批处理),OCR 调用 wrap `spawn_blocking` 不阻塞 tokio worker。18 份案件实测 ~70 秒以内。

### 💬 反馈通道

App 内「💬 反馈」按钮 → 自动收集诊断信息(版本 / OS / provider / 聚合计数 / 去标识的失败文件类型摘要)→ 写桌面 MD 文件。App **不会自动发送反馈**；用户应先预览生成文件，再自行决定是否发送给项目维护者。诊断带稳定 client_id 的前 8 位，用于关联同一安装的多次反馈。

### 💰 DeepSeek 余额监控

右上角 chip 显示当日消费 + 当前余额,5 分钟周期自动刷新,< ¥5 时变橙色警示。

## 隐私铁律

- 原文件**只读、原地不动**:工具只记录路径,不复制、不移动、不重命名
- 抽取出的结构化数据、报告、数据库存在本机 `~/Library/Application Support/CaseBoard/`
- ⚠️ 但**默认走云端**:文本抽取 / OCR 会把文档内容发往 DeepSeek / MinerU 处理(介意请改用本机模型)
- API key / token 永远不进代码、不入 git;只存本机 `settings.json`(后续升 Keychain)
- 反馈通道只在本地生成 MD，**不会自动发送反馈**；用户预览后自行决定是否发送
- MCP 接入需要在 App 内显式授权,记录在 `mcp_clients` 表

更完整的数据存储、外部传输、遥测、保留与删除说明见 [PRIVACY.md](./PRIVACY.md)。

## 安装

Windows 10/11 用户从本仓库 [Releases](https://github.com/fanglv8653/case-board-fanglv/releases) 下载 NSIS `setup.exe`。安装包内置 WebView2 引导器；应用内更新包使用 Tauri updater minisign 验证完整性。

> minisign 是应用内更新制品签名，不是 Windows Authenticode。当前安装包未配置 Authenticode 发布者证书，因此 Windows 可能显示“未知发布者”或 SmartScreen 提示；请只从项目 Release 下载并核对发布页校验信息。

macOS 11+ 可从上游发布渠道获取 dmg；首次打开可能需要在 Applications 中右键选择“打开”。本二创版本的发布回归重点为 Windows。

## 技术栈

| 层 | 选型 |
|:---|:---|
| 桌面壳 | [Tauri 2](https://tauri.app/) |
| 后端核心 | Rust + Tokio |
| 数据库 | SQLite + [sqlx](https://github.com/launchbadge/sqlx) |
| 前端 | React 19 + TypeScript + [Vite 7](https://vitejs.dev/) |
| UI | Tailwind v4 + [shadcn/ui](https://ui.shadcn.com/) |
| 本地 LLM | llama.cpp + MiniCPM-V 4.6(可选) |
| 云端 LLM | DeepSeek `v4-flash` / `v4-pro`(可选) |
| OCR | MinerU 在线 / 本机视觉模型(任选) |

详细架构与决策见各模块源码注释。

## 开发

### 环境要求

- Node 22+ · pnpm 10+
- Rust 1.80+(`rustup install stable`)
- Windows:Visual Studio 2022 C++ Build Tools + WebView2；macOS:Xcode Command Line Tools

### 启动开发模式

```bash
pnpm install
pnpm tauri dev
```

启动后会弹出原生窗口,支持热重载。前端改了立即生效;Rust 改了会重新编译后重启窗口。

### 打包 dmg

```bash
bash scripts/release.sh
```

产出 `target/release/bundle/dmg/案件看板_<version>_aarch64.dmg`,首次约 2-3 分钟。

### 测试 / 质量基线

```bash
pnpm validate:source                 # 版本、单锁、许可正文和文档一致性
pnpm test:logic                      # Node 纯逻辑 / UI 契约
pnpm exec tsc --noEmit && pnpm build # TypeScript + Vite
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Windows 的 Tauri manifest 声明 `requireAdministrator`；因此通用测试门禁不执行
example 二进制。updater 验签 example 只在发布工作流生成最终安装包后显式运行。

### Windows 打包与 Linux 状态

Windows 是当前二创版本的主要发布与隔离回归平台。必须用 Tauri CLI 打包，不能以裸 `cargo build --release` 代替：

```powershell
pnpm install --frozen-lockfile
pnpm tauri build --bundles nsis
```

`createUpdaterArtifacts` 需要 `TAURI_SIGNING_PRIVATE_KEY` 与密码；缺少签名材料时发布工作流会 fail closed。Linux 尚未形成正式安装包与端到端回归证据，不列为支持发布平台。

## 项目结构

```
caseboard/
├── src/                          # React 前端
│   ├── components/               # 全局组件(ModuleTabs / HomeView / 弹窗)
│   ├── modules/                  # 三大模块
│   │   ├── litigation/           # 诉讼(包含案件状态机 inferStatus)
│   │   ├── transaction/          # 非诉(字段框架 + 访谈)
│   │   └── tools/                # 5 个法律工具(纯 React 重写)
│   └── lib/                      # 跨模块共享:api / types / utils / caseSnapshot
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── db/                   # SQLite + aggregator
│   │   ├── ingest/               # 扫描 / OCR / pipeline
│   │   ├── llm/                  # 本机 / 云端 LLM 客户端
│   │   ├── feedback/             # 反馈 MD 生成
│   │   ├── deepseek/             # DeepSeek 余额管理
│   │   ├── lifecycle/            # 本机 llama-server 自启
│   │   ├── settings.rs           # 本机配置读写
│   │   └── lib.rs                # Tauri commands
│   └── migrations/               # SQLite migrations(0001 ~ 0045)
├── scripts/release.sh            # 一键打 dmg
└── public/                       # 静态资源(图标 / svg)
```

## 状态

- ✅ 当前源码版本 v0.6.2；已发布更新元数据在最终同版制品完成前仍指向 v0.6.1
- ✅ Windows 10/11 是当前二创版本主要回归平台；macOS 保留上游支持，Linux 暂无正式发布保证

## 反馈 & 贡献

- 用户反馈:App 右下角 💬 反馈按钮 → 自动生成本地 MD → 用户自行发送给项目维护者
- 安全问题:见 [SECURITY.md](./SECURITY.md)
- 行为准则:见 [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md)
- 贡献指南:见 [CONTRIBUTING.md](./CONTRIBUTING.md)

## License · 授权

本项目以 **PolyForm Noncommercial License 1.0.0** 授权(源码公开、非 OSI 「开源」,属源码可见许可)。

- ✅ **免费**:个人、学习、研究、教育、非营利组织、政府机构等**一切非商业用途**,可自由使用、修改、分发。
- 💼 **商业用途须授权**:任何商业目的的使用,必须事先取得版权人**书面商业授权**。
  - 商业授权洽谈:通过本仓库 GitHub Issues 联系并取得书面授权。


完整条款见 [LICENSE](./LICENSE) 和 [NOTICE](./NOTICE)。

Copyright © 2026 江苏漫修(无锡)律师事务所 · 刘成律师
