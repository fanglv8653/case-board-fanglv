fn main() {
    // 遥测 key 通过编译期环境变量(option_env!)注入。告诉 cargo:这两个变量变了要重编,
    // 否则增量编译会缓存旧值(改了 key / 切换有无遥测时不生效)。
    println!("cargo:rerun-if-env-changed=CASEBOARD_TELEMETRY_URL");
    println!("cargo:rerun-if-env-changed=CASEBOARD_TELEMETRY_KEY");
    tauri_build::build()
}
