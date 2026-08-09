# V083-FORMAL-BACKUP-EXECUTE 验收标准

- P0=0、P1=0。
- 正式源三件套前后字节事实不变，应用全程停止。
- 原样完整数据根与 main-only 一致性副本同时存在且哈希可追溯。
- 安装目录、rollback setup、注册表、可选旧数据/WebView 已按存在状态备份。
- 备份 run root EFS/ACL 保护有效。
- main-only 全审计满足 v0.8.3 升级前基线。
- 未安装、未启动、未切换、未删除 sidecar、未访问 NAS/凭据内容。
