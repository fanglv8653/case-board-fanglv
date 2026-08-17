# V084-N0 总契约

状态：`accepted`

本文件是 v0.8.4 后续实现的唯一上游契约。若实现与本文件冲突，必须先退回主控修改契约，不允许各线程自行漂移。

## 一、版本范围与顺序

开发顺序冻结为：`V084-U1` 更新生命周期、`V084-R1` 原子发布、`V084-T1` 待办本地模型、`V084-F1` 飞书同步、`V084-RC` 集成发布验收。

U1、R1、T1 可在非重叠文件范围内实施；F1 必须等待 T1 的 0064 和业务 API 验收；共享注册文件由集成任务串行修改。

## 二、更新生命周期

1. 废弃成功路径上的前端 `downloadAndInstall()`、`localStorage` 成功标记和 `relaunch()`。
2. Rust 只使用 updater `check()`/`download()` 完成元数据、下载和 minisign 验证，不调用插件 `install()`/`download_and_install()`。
3. 使用独立 updater helper 启动 NSIS；helper 启动前必须确认专用 shutdown coordinator OS 线程已停止本应用 sidecar、SQLite pool 已关闭、耐久屏障已原子落盘且旧应用 PID 已退出。
4. coordinator 独占 current-thread Tokio runtime；禁止在 Tokio worker 或同步 updater hook 内嵌套 `block_on`。
5. helper 等待安装器真实退出码并核验目标 EXE 版本；成功后在当前用户受限 ACL 目录原子写一次性回执，仅携非秘密 `attempt_id` 启动目标版本。
6. 新版本按 attempt、版本、phase、退出码、文件版本、时限及 ACL 原子 claim；提示至多一次。
7. 命令行、环境变量、日志、SQLite 和崩溃报告不得承载 token、密码、密钥或回执正文。本版不宣称抵御同一用户权限下的恶意进程。
8. 取消、非零退出、签名/下载/收尾/ACL/回执失败或目标版本异常均不得显示成功。

## 三、发布事务

1. Windows 外部事实资产严格为 `FanglvCaseBoard_<version>_x64-setup.exe` 及同名 `.sig`。
2. workflow 在清洁 staging 目录生成最终 ASCII 名，对最终文件重做 minisign、SHA-256 和文件名门禁。
3. 先建立或复用 draft Release，资产齐套并远端回读一致后再公开 Release。
4. 公开 Release 通过后生成 `release/version.json` 与 `release/latest.json`；两文件必须版本、tag、Release target、URL、签名一致，在同一独立提交中修改，且提交不得含其他文件。
5. push 超时先远端回读；main 漂移、同名不同内容或双清单不一致均失败关闭。
6. RC 验收前不改公开清单、不创建正式 Release。

## 四、待办业务表（0064）

1. 演进现有 `case_todos`，不新建平行业务表；保留全部旧 ID。
2. 0064 在单个 SQLx 迁移事务内重建表，将 `case_id` 改为 nullable，外键改为 `ON DELETE SET NULL`。
3. 业务字段冻结为 `id`、`case_id NULL`、`title`、`content`、`item_at NULL`、`due_date NULL`、`done/done_at`、不可变的 `source=caseboard|feishu|hermes`、`deleted_at`、`created_at/updated_at`。
4. 旧行保留原字段；`content=''`、`source='caseboard'`、`deleted_at=NULL`；旧 due_date 投影为当日 00:00。
5. 删除改为软删除；案件删除只解除关联；默认列表不含软删。
6. 0064 必须重建索引、三个设备同步 trigger，并更新 device-sync registry 新字段白名单。

## 五、复制到案件进展

1. 仅由用户触发。已关联事项固定复制到关联案件；未关联事项必须先选择案件。
2. 单事务读取事项、验证案件并写入 `case_work_items`。
3. 进展时间按 `item_at → due_date 的 Asia/Shanghai 00:00 → created_at` 回退。
4. 写入 `external_source='case_todo'`、`external_record_id=todo.id`，利用现有唯一索引防串行和并发重复。
5. 复制内容是当时快照；复制后待办和进展相互独立，不静默联动修改、删除或恢复。
6. 重复点击返回既有 work item；目标冲突或既有进展已软删时失败关闭并交人工处理。

## 六、飞书“收件箱”同步（0065+）

1. “待办事项”是案件看板模块名；“收件箱”仅是飞书表名。
2. 配置新增独立 `feishu_todo_inbox_table_id`，不得复用案件总表 ID。
3. 飞书字段冻结为：事项ID、标题、内容、事项时间、状态、完成时间、关联案件、来源、同步版本、基线哈希、内容哈希、删除状态。
4. “事项时间”列必须存在但单元格可空；空值规范为 JSON `null`。旧 due_date 仅作上海时区 00:00 兼容投影，不回填 item_at、不提升版本。
5. `case_todos.id` 是稳定 ID；同步版本、基线、哈希、冲突、远端 record ID、预览和审计放入 0065 起独立账本。
6. 自动拉取仅只读并生成候选；所有业务写入和远端写入均须用户明确确认。0.8.4 不启用后台自动远端写。
7. 第一阶段不调用飞书物理删除 API；软删除作为状态同步，远端物理缺失只生成 `remote_missing` 候选。
8. 同步采用稳定 ID、来源、版本、共同基线哈希和双方内容哈希作三方判断；同 ID 多记录全部隔离，不按标题自动合并。
9. 写前重读、写后回读；结果不确定时标记 `write_uncertain`，不推进基线、不自动重试。实现前核对官方条件更新能力；若无 CAS，RC 保留残余竞态说明。
10. 正式飞书 Base、凭据、NAS/Hermes 生产实例不参与自动测试；真实验收只使用结构隔离副本。

## 七、错误与测试门禁

实现必须沿用三份已验收报告冻结的 `UPD_*`、`REL_*`、`TODO_*`、`FEISHU_TODO_*` 稳定错误码，UI 不得根据中文全文分支。

每阶段至少运行对应专项测试、`pnpm test:logic`、`pnpm build`、`cargo check --lib -j 1`、`cargo clippy --lib -j 1 -- -D warnings` 和 `pnpm validate:source`；Windows Rust 总测试在集成与 RC 阶段执行。

## 八、来源报告

- `.agent-work/output/V084-N0-UPDATER.md`
- `.agent-work/output/V084-N0-TODO.md`
- `.agent-work/output/V084-N0-FEISHU.md`
- `.agent-work/54_v084_n0_acceptance_rubric.md`
