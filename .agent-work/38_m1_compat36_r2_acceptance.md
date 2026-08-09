# V083-M1-COMPAT36-R2 验收标准

- P0=0、P1=0。
- 不存在 `set_ignore_missing(true)` 或等效全局放行。
- 兼容 migrator 仅补入 version 36 固定元数据，并保持 `ignore_missing=false`。
- 任意其他 unknown applied version 仍失败关闭。
- version 36 精确 schema 的五类绕过均有生产路径负例及物理不写证明。
- 正例升级至 0063，业务标记、`quick_check`、`foreign_key_check` 通过，version 36 原始记录完整不变。
- 无迁移文件变更，无正式数据/凭据/NAS/发布写入。
- 主控串行完成格式、check、clippy、Windows 全量 Rust 门禁后，另派独立只读复核；在此之前不得标记最终通过。
