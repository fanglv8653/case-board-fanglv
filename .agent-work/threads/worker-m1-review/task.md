# 线程任务包｜V083-M1-REVIEW

## 目标

独立只读审计 M1 实现，不修改源码、不代替主控裁决。优先寻找会导致未知数据库被写入、错误分类漂移、敏感数据泄露或启动仍无提示退出的反例。

## 必读

- `.agent-work/threads/worker-m1/task.md`；
- `.agent-work/output/V083-M1.md`；
- `.agent-work/output/V083-N0-GATE.md`；
- `.agent-work/20_m1_acceptance_rubric.md`；
- M1 四个源码文件的完整 diff；
- 主控实测：Cargo check/Clippy 通过；Windows Rust 275/0/3，device sync 23/23；Node 119/119、Vite/source gate 通过。

## 必查反例

1. 既有文件是否在 read-only 预检前产生父目录、DB header、WAL/SHM 或 schema 写入；
2. 无迁移表但有用户 schema、未知版本、checksum、success=0、history gap、sentinel 缺失的分类优先级；
3. 只读元数据查询失败是否进入结构化原生提示；
4. allowlist 是否确实为空，未来动作是否同时绑定 version/旧值/当前值/sentinel 且 CAS 更新；
5. 49/51/58—62 sentinel 是否只查结构元数据且覆盖冻结集合；
6. setup 是否只捕获兼容错误，文案/日志是否泄露业务正文、SQL 参数或凭据；
7. 7 个夹具是否可能因关闭约束、弱断言或自身 fingerprint 副作用而伪通过；
8. 是否有越权改动、递归 rustfmt 残留或报告与实测不一致。

## 允许写入

- `.agent-work/threads/worker-m1-review/`；
- `.agent-work/output/V083-M1-REVIEW.md`。

## 禁止

- 不修改任何产品/测试源码、迁移、依赖、版本或其他线程；
- 不运行 Cargo/Node/构建，不读取正式数据，不提交 Git；
- 不自行 accepted/rejected M1，只给建议和证据。

## 交付

按严重度列问题（文件/行/触发条件/影响/修正建议）；若无阻断项，也必须说明已核对的反例、可接受残余风险和主控仍需完成的视觉/隔离副本门禁。完成后 workflow submit。
