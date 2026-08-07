# 14 第一轮主控节奏

1. 派发 MIG、SYNC、GATE；
2. 等待 Worker 自行 start/submit；
3. 优先审查 MIG 与 SYNC 的可执行夹具；
4. 最后用 GATE 独立报告核对缺口；
5. 驳回项原线程修复；
6. 三项 accepted 后关闭 N0，串行创建 M1。
