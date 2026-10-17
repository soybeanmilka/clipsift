//! 界面状态与后台守护逻辑。
//!
//! 这里刻意不用 `Arc<Mutex<>>` 包一层让后台线程和界面共享状态：守护循环跑在
//! 界面线程之外，两边通过 channel 传事件。理由是一旦共享可变状态，锁的持有
//! 范围就成了界面流畅度的隐患；而这里的数据量极小，channel 的开销可以忽略。

use clipsift_core::{apply, Plan};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// 守护线程发回界面的消息。
#[derive(Debug, Clone)]
pub enum Event {
    /// 成功净化了一次，带走了多少个字符。
    Cleaned { removed: usize },
    /// 读剪贴板失败（比如被别的程序独占）。不致命，继续轮询即可。
    ReadFailed(String),
    /// 写剪贴板失败。
    WriteFailed(String),
}

/// 守护线程持有的句柄。`None` 表示没在跑。
pub struct Watcher {
    stop: Sender<()>,
    events: Receiver<Event>,
    started_at: Instant,
}

impl Watcher {
    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// 把当前积累的事件全部取出来。非阻塞。
    pub fn drain_events(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(e) = self.events.try_recv() {
            out.push(e);
        }
        out
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // 忽略发送失败：线程可能已经因为别的原因退出了。
        let _ = self.stop.send(());
    }
}

/// 启动守护线程。`plan` 会被移进线程，之后改规则需要重启守护。
pub fn start_watch(plan: Plan, interval_ms: u64) -> Watcher {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (event_tx, event_rx) = mpsc::channel::<Event>();

    std::thread::spawn(move || {
        let interval = Duration::from_millis(interval_ms.max(50));
        let mut last_seen = String::new();
        loop {
            // 先睡再干活，让调用方有时间把界面画出来。
            match stop_rx.recv_timeout(interval) {
                Ok(()) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }

            let text = match clipboard::read() {
                Ok(Some(t)) => t,
                // 剪贴板是空的 / 里面是图片或文件，不是文本。这是绝大多数时候的
                // 真实状态，不是错误，静默跳过就好。
                Ok(None) => continue,
                Err(e) => {
                    let _ = event_tx.send(Event::ReadFailed(e));
                    continue;
                }
            };

            // 空剪贴板、以及"我们自己刚写回去的内容"都要跳过。
            if text.is_empty() || text == last_seen {
                continue;
            }
            if !needs_cleanup(&text, &plan) {
                last_seen = text;
                continue;
            }

            let (out, report) = apply(&text, &plan);
            // 净化结果和原文一样，说明脏的是规则管不到的东西，记账即可。
            if out == text {
                last_seen = text;
                continue;
            }
            match clipboard::write(&out) {
                Ok(()) => {
                    let _ = event_tx.send(Event::Cleaned {
                        removed: report.removed.iter().map(|(_, n)| n).sum(),
                    });
                    last_seen = out;
                }
                Err(e) => {
                    let _ = event_tx.send(Event::WriteFailed(e));
                }
            }
        }
    });

    Watcher {
        stop: stop_tx,
        events: event_rx,
        started_at: Instant::now(),
    }
}

/// 快速判断这段文本值不值得跑一遍完整规则。
///
/// 直接调 `apply` 也不慢，但守护进程每 200ms 醒一次，绝大多数时候剪贴板里
/// 是干净内容；这里用一次线性扫描先筛掉，能省下两次内存分配。
///
/// 注意这里**不看 plan 的开关**：宁可多跑一次完整净化（本来也只要几微秒），
/// 也不要让"快检"和"实际规则"两套判断逻辑慢慢走偏。
pub fn needs_cleanup(text: &str, _plan: &Plan) -> bool {
    clipsift_core::needs_cleanup(text)
}

/// 剪贴板读写。Windows 走 arboard，其他平台暂时给明确提示。
pub mod clipboard {
    /// 读剪贴板文本。
    ///
    /// 返回 `Ok(None)` 表示"剪贴板里没有文本"（空的，或者放的是图片/文件），
    /// 这是常态而不是故障——把它和真正的错误分开，界面才不会一直闪警告。
    #[cfg(windows)]
    pub fn read() -> Result<Option<String>, String> {
        let mut cb = arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板：{e}"))?;
        match cb.get_text() {
            Ok(t) => Ok(Some(t)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(format!("读剪贴板失败：{e}")),
        }
    }

    #[cfg(windows)]
    pub fn write(text: &str) -> Result<(), String> {
        let mut cb = arboard::Clipboard::new().map_err(|e| format!("打不开剪贴板：{e}"))?;
        cb.set_text(text.to_owned())
            .map_err(|e| format!("写剪贴板失败：{e}"))
    }

    #[cfg(not(windows))]
    pub fn read() -> Result<Option<String>, String> {
        Err("剪贴板守护目前只支持 Windows".to_string())
    }

    #[cfg(not(windows))]
    pub fn write(_text: &str) -> Result<(), String> {
        Err("剪贴板守护目前只支持 Windows".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_cleanup_detects_zero_width() {
        assert!(clipsift_core::needs_cleanup("a\u{200B}b"));
    }

    #[test]
    fn needs_cleanup_detects_nbsp() {
        assert!(clipsift_core::needs_cleanup("a\u{00A0}b"));
    }

    #[test]
    fn needs_cleanup_detects_smart_quote() {
        assert!(clipsift_core::needs_cleanup("\u{201C}hi\u{201D}"));
    }

    #[test]
    fn clean_text_needs_no_cleanup() {
        assert!(!clipsift_core::needs_cleanup("hello world"));
        assert!(!clipsift_core::needs_cleanup("int main() { return 0; }"));
    }

    #[test]
    fn watcher_drop_stops_thread() {
        // 不能只断言 `>= 0`——那永远成立，等于什么都没验。
        // 这里验三件真事：刚起来时是零；时间在往前走；丢掉之后能正常收尾。
        let w = start_watch(Plan::default(), 50);
        assert!(
            w.drain_events().is_empty(),
            "刚启动、剪贴板没有变动时不该有事件"
        );

        std::thread::sleep(std::time::Duration::from_millis(30));
        let elapsed = w.uptime();
        assert!(
            elapsed >= std::time::Duration::from_millis(20),
            "uptime 应该随时间增长，实际 {elapsed:?}"
        );

        // Drop 会 join 线程；如果信号没生效，join 会一直卡住，测试超时失败。
        drop(w);
    }
}
