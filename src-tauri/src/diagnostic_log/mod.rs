//! 进程内诊断日志环形缓冲(2026-05-26 V0.1.11)。
//!
//! 给反馈通道用——朋友的 Mac 上偶尔有报错但 stderr 上不来,
//! 把最近 200 行运行时日志收在内存里,反馈 MD 带出来,作者能复现。
//!
//! 设计:
//!   - 线程安全的 `Arc<Mutex<VecDeque<String>>>`,上限 200 行,FIFO
//!   - `dlog!()` 宏 = `eprintln!()` + push 到 buffer,跟原 `eprintln!` 行为一致
//!   - `install_panic_hook()` 把 panic 也写进 buffer
//!   - **隐私**:写入的每条都过一次 `feedback::sanitize_paths`,
//!     把 `/Users/xxx/...` 替换成 `<path>/basename`(防当事人姓名出现在路径里泄漏)
//!
//! 不用 `tracing` / `log` crate 的原因:依赖小、行为透明、迁移成本只是 sed 一次,
//! 也不用跟 tokio runtime 抢 subscriber 所有权。

use std::collections::VecDeque;
use std::sync::Mutex;

const RING_CAPACITY: usize = 200;

static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// 推一条日志进 ring buffer。同时 `eprintln!` 到原 stderr(开发时仍可见)。
///
/// 内部用,生产代码请用 `dlog!()` 宏。
pub fn push_log(line: String) {
    // 先脱敏(路径里可能含当事人姓名)
    let safe = crate::feedback::sanitize_paths(&line);
    eprintln!("{}", safe);
    if let Ok(mut g) = RING.lock() {
        if g.len() >= RING_CAPACITY {
            g.pop_front();
        }
        let ts = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        g.push_back(format!("[{}] {}", ts, safe));
    }
}

/// 拿当前 ring buffer 快照(给反馈 MD 用)。
pub fn snapshot() -> Vec<String> {
    RING.lock()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default()
}

/// 安装 panic hook,把 panic 信息也写进 ring buffer(主进程崩溃前留下线索)。
///
/// 由 `lib.rs` 在启动早期调一次。
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "?".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<non-string payload>");
        push_log(format!("[panic] {} @ {}", payload, location));
        // 调原 hook,保持默认 stderr 输出 / abort 行为
        prev(info);
    }));
}

/// 跟 `eprintln!` 等价的宏 — 但同时落到 ring buffer。
/// 用法跟 `eprintln!` 一致:`dlog!("[ocr] 限流被拒,等 {}s 重试", secs)`。
#[macro_export]
macro_rules! dlog {
    () => {
        $crate::diagnostic_log::push_log(String::new())
    };
    ($($arg:tt)*) => {
        $crate::diagnostic_log::push_log(format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_snapshot_roundtrip() {
        // 隔离测试:先清空再测
        if let Ok(mut g) = RING.lock() {
            g.clear();
        }
        push_log("hello".into());
        let snap = snapshot();
        assert!(snap.iter().any(|l| l.contains("hello")));
    }

    #[test]
    fn ring_caps_at_capacity() {
        if let Ok(mut g) = RING.lock() {
            g.clear();
        }
        for i in 0..(RING_CAPACITY + 50) {
            push_log(format!("line {}", i));
        }
        let snap = snapshot();
        assert_eq!(snap.len(), RING_CAPACITY);
        // 头部已经被挤掉
        assert!(snap.first().unwrap().contains(&format!("line {}", 50)));
        assert!(snap
            .last()
            .unwrap()
            .contains(&format!("line {}", RING_CAPACITY + 49)));
    }

    #[test]
    fn sanitizes_paths_before_pushing() {
        if let Ok(mut g) = RING.lock() {
            g.clear();
        }
        push_log("打开 /Users/alice/cases/李四/foo.pdf 失败".into());
        let snap = snapshot();
        let joined = snap.join("\n");
        assert!(!joined.contains("李四"), "must drop case name: {}", joined);
        assert!(joined.contains("<path>/foo.pdf"));
    }
}
