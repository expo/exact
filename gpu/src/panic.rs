//! A GPU module's panic report.

/// The panic line a GPU module reports before it aborts, from the hook's
/// `PanicHookInfo` ("panicked at <file>:<line>:<column>:\n<message>").
fn panic_line(info: &dyn std::fmt::Display) -> String {
    format!("GPU module panicked: {info}")
}

/// Release GPU modules abort on panic. On the web that surfaces only as
/// `RuntimeError: unreachable`, so the message goes to the console first; a
/// native module repeats it after the default report, whose backtrace would
/// otherwise push it out of a log's tail. Installed once, at load.
pub(crate) fn report_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        #[cfg(target_arch = "wasm32")]
        std::panic::set_hook(Box::new(|info| {
            crate::web::console_error(&panic_line(info))
        }));
        #[cfg(not(target_arch = "wasm32"))]
        {
            let default = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                default(info);
                eprintln!("{}", panic_line(info));
            }));
        }
    });
}

#[cfg(test)]
mod panic_tests {
    // The formatter alone: the process-wide hook is never swapped under
    // concurrently running tests.
    #[test]
    fn a_panic_line_names_the_message_and_location() {
        let info = "panicked at gpu/src/frame.rs:12:5:\nslot has not been initialized";
        assert_eq!(
            super::panic_line(&info),
            "GPU module panicked: panicked at gpu/src/frame.rs:12:5:\nslot has not been initialized"
        );
    }
}
