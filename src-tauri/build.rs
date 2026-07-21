use std::hash::{Hash, Hasher};

fn main() {
    // 遥测 key 通过编译期环境变量(option_env!)注入。告诉 cargo:这两个变量变了要重编,
    // 否则增量编译会缓存旧值(改了 key / 切换有无遥测时不生效)。
    println!("cargo:rerun-if-env-changed=CASEBOARD_TELEMETRY_URL");
    println!("cargo:rerun-if-env-changed=CASEBOARD_TELEMETRY_KEY");
    // sqlx::migrate! 会内嵌迁移内容；显式监听目录，确保新增迁移文件也能让
    // 本机增量 Release 构建重新编译，而不是沿用只包含旧迁移号的缓存。
    println!("cargo:rerun-if-changed=migrations");
    let mut migration_paths = std::fs::read_dir("migrations")
        .expect("migrations directory must exist")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sql"))
        .collect::<Vec<_>>();
    migration_paths.sort();
    let mut migration_hasher = std::collections::hash_map::DefaultHasher::new();
    for path in migration_paths {
        path.file_name().hash(&mut migration_hasher);
        std::fs::read(&path)
            .unwrap_or_else(|error| panic!("failed to read migration {}: {error}", path.display()))
            .hash(&mut migration_hasher);
    }
    println!(
        "cargo:rustc-env=CASEBOARD_MIGRATION_BUILD_FINGERPRINT={:016x}",
        migration_hasher.finish()
    );
    tauri_build::build()
}
