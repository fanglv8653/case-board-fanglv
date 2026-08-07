# 20 V083-M1 验收矩阵

- [ ] 全新库与当前 61 条迁移谱系正常。
- [ ] 未知 checksum 返回 `DB_MIGRATION_CHECKSUM_UNKNOWN`，无写入。
- [ ] 未知已应用版本返回 `DB_MIGRATION_APPLIED_VERSION_UNKNOWN`，无写入。
- [ ] sentinel 缺失返回 `DB_MIGRATION_SCHEMA_SENTINEL_MISSING`，无写入。
- [ ] `success=0` 返回 `DB_MIGRATION_LINEAGE_INCOMPATIBLE`，无写入。
- [ ] 当前无历史兼容白名单，未猜测 checksum。
- [ ] setup 原生提示不依赖 WebView，且不暴露业务正文/凭据。
- [ ] 无迁移 SQL、依赖、版本、同步/飞书逻辑越权修改。
- [ ] `git diff --check`、Node 119、Vite build、source gate、Cargo check、Clippy、Windows Rust 全量全部通过。
