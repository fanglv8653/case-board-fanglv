// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|argument| argument == "--caseboard-print-version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    caseboard_lib::run()
}
