# V081-SKILL 完整包管理前端交付报告

日期：2026-07-29

## 完成内容

- 保留原有目录导入、启用/停用、法律领域与任务类型默认绑定。
- 新增 `.fanglv-skill.zip` 文件选择与写入前安全预检：
  - 扩展名必须为 `.fanglv-skill.zip`；
  - 压缩包不超过 1 MB；
  - ZIP 条目不超过 23 个，解包总量不超过 512 KB；
  - 拒绝绝对路径、盘符路径、路径穿越、嵌套目录、符号链接、加密条目和未知压缩方法；
  - 只接受根目录 `SKILL.md`、`manifest.json` 及 `references/` 下单层 `.md/.json/.txt`；
  - 导入前显示 manifest、文件清单和 SKILL.md 正文；
  - 后端仍执行最终的失败关闭校验。
- 新增版本历史：
  - 按 slug 读取已注册版本及审计修订；
  - 选择目标版本后必须先请求后端差异预览；
  - 显示逐文件新增、删除、修改及前后正文；
  - 查看差异后再次弹出原生确认，再调用升级或回滚命令。
- 新增导出：
  - 后端返回 `{file_name, bytes}`；
  - 前端弹出用户保存位置；
  - 使用 Tauri FS 将字节写入所选路径。
- 新增删除：
  - 仅导入包显示删除按钮；
  - 删除前原生危险确认，后端仍要求 `confirmed=true`；
  - 内置包不显示删除按钮，只允许原有启用/停用。

## 后端契约核对

已与 `src-tauri/src/lib.rs` 的实际命令签名逐项核对：

| 命令 | 参数 | 返回 |
|---|---|---|
| `import_legal_skill_archive` | `fileName, archiveBytes` | `LegalSkillRegistration` |
| `list_legal_skill_versions` | `slug` | `LegalSkillVersionHistory` |
| `preview_legal_skill_diff` | `currentSkillId, targetSkillId` | `LegalSkillDiffPreview` |
| `upgrade_legal_skill_package` | `currentSkillId,targetSkillId,confirmed:true` | `LegalSkillPackageRecord` |
| `rollback_legal_skill_package` | `currentSkillId,targetSkillId,confirmed:true` | `LegalSkillPackageRecord` |
| `export_legal_skill_package` | `skillId` | `{file_name,bytes}` |
| `delete_legal_skill_package` | `skillId,confirmed:true` | `void` |

### 契约差异及处理

1. 后端没有“压缩包只预检、不注册”的命令；`import_legal_skill_archive` 校验成功即注册。因此前端先做结构/路径/大小/正文预检并显示人工确认，确认后才调用后端；后端仍是最终权威校验。
2. 后端导出只返回字节，不接受目标路径。前端使用保存对话框取得用户路径，再通过 Tauri FS 写入。
3. 升级/回滚目标必须先成为已注册版本。因此新压缩包先以停用版本导入，再从版本历史中查看差异并切换。
4. 后端删除仅允许 imported 包；前端同步隐藏内置包删除按钮，并在误调用防线上显示“只能停用”。

## 修改范围

- `src/components/settings/LegalSkillsSettingsCard.tsx`
- `src/lib/api.ts`
- `src/lib/types.ts`
- `scripts/test-v081-legal-skills-package-ui.cjs`

未修改 Rust 或 `SettingsModal` 其他区域。

## 检查

- `node node_modules/typescript/bin/tsc --noEmit`：通过。
- `node scripts/test-v081-legal-skills-package-ui.cjs`：通过。
- `git diff --check`：无空白错误，仅有工作区 LF/CRLF 提示。
- 未运行 Rust fmt/build。

## 主控运行时验收

1. 用合法 `.fanglv-skill.zip` 验证预检、确认和停用版本注册。
2. 用路径穿越、符号链接、嵌套 ZIP、超大或超多条目包验证前后端均拒绝。
3. 同 slug 导入两个版本，验证差异预览、升级、回滚及默认绑定保留。
4. 导出后重新导入，核对内容哈希一致。
5. 验证内置包无删除入口，导入包删除需要双重确认。
