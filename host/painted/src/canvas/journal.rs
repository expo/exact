//! The app's journal for a phone nobody drives, by two switches read once:
//!
//! - `EXACT_LOG_JOURNAL=1` (the reader's `x_log_journal=1`): every line on
//!   stderr as the host comes to hold it, with the milliseconds since its
//!   boot began. On Android stderr is logcat, whose own timestamps place the
//!   lines against the process's start, which is how what a data module adds
//!   to a launch is measured (when it was asked, when it answered).
//! - `EXACT_LOG_CONSOLE=1`, which the Android reader sets in a development
//!   build: the data module's `console` lines alone, under the tag
//!   `exact-app` (`adb logcat -s exact-app`), so a builder sees what app.ts
//!   logs without a drive.
//!
//! With neither, nothing is read or written. Off Android only [`console`]
//! is compiled, for its test.

#[cfg(target_os = "android")]
use {
    super::CanvasHost,
    exact_runner::DataSource,
    std::sync::atomic::{AtomicUsize, Ordering},
};

#[cfg(target_os = "android")]
static ON: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var("EXACT_LOG_JOURNAL").is_ok_and(|v| v == "1"));

#[cfg(target_os = "android")]
static CONSOLE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var("EXACT_LOG_CONSOLE").is_ok_and(|v| v == "1"));

/// Journal lines already written (a process has one host).
#[cfg(target_os = "android")]
static WRITTEN: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "android")]
impl<D: DataSource + Default> CanvasHost<D> {
    /// Write the journal's lines since the last call, as the switches say.
    pub(super) fn echo_journal(&self) {
        if !*ON && !*CONSOLE {
            return;
        }
        let runner = self.p.host().runner();
        let start = runner.journal_start();
        let from = WRITTEN.load(Ordering::Relaxed).max(start);
        let ms = self.started.elapsed().as_secs_f64() * 1000.0;
        let mut next = from;
        for line in runner.journal().skip(from - start) {
            if *ON {
                eprintln!("exact: journal +{ms:.1} ms: {line}");
            }
            if let Some(text) = console(line).filter(|_| *CONSOLE) {
                crate::android::log_as(c"exact-app", text);
            }
            next += 1;
        }
        WRITTEN.store(next, Ordering::Relaxed);
    }
}

/// A journal line's text (after its `t=<ms>` stamp) when it is the module's
/// `console`.
fn console(line: &str) -> Option<&str> {
    line.split_once(' ')?.1.strip_prefix("console: ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_modules_console_lines_are_its_own() {
        assert_eq!(super::console("t=12.5 console: saved 3"), Some("saved 3"));
        assert_eq!(super::console("t=0 console: "), Some(""));
        assert_eq!(super::console("t=4 boot: 12 nodes, epoch 1"), None);
        assert_eq!(
            super::console("t=4 data: unhandled rejection: console: x"),
            None
        );
    }
}
