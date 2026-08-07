# 15 第一轮验收命令

至少执行：

```powershell
git diff --check
pnpm test:logic
pnpm build
$env:PATH='C:\Users\William Feng\.cargo\bin;' + $env:PATH
$env:CARGO_INCREMENTAL='0'
cargo check --lib -j 1
cargo clippy --lib -j 1 -- -D warnings
powershell.exe -ExecutionPolicy Bypass -File .\scripts\run-windows-rust-tests.ps1
```

主控可先运行定向测试；N0 关闭前必须完成上述全量门禁。
