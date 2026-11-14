use super::*;

/// What the presenter hands the library at boot: the text measurer (LLP
/// 1008 §3) and the wake for a request's reply (LLP 1016 D2), each with an
/// opaque context the library passes back untouched.
#[derive(Clone, Copy)]
pub struct Hooks {
    /// Measures a paragraph; `None` for the monospace reference measurer.
    pub measure: Option<MeasureFn>,
    /// Passed back to `measure` and `lines`.
    pub ctx: *mut c_void,
    /// A paragraph's line boxes (LLP 1093 D6); `None` keeps paragraphs whole.
    pub lines: Option<crate::measure::LinesFn>,
    /// Each inline box's origin (LLP 1011.002 D2); `None` leaves images at
    /// their paragraph's content origin.
    pub inline_boxes: Option<crate::measure::LinesFn>,
    /// Called on the executor's thread when a reply is queued; `None`: replies wait for `exact_pump`.
    pub wake: Option<crate::executor::WakeFn>,
    /// Passed back to `wake`.
    pub wake_ctx: *mut c_void,
    /// Measures a Canvas 2D run with Core Text (LLP 1056 D8), with `ctx`.
    pub canvas_text: Option<crate::canvas_text::CanvasTextFn>,
    /// Measures a system symbol (`TextMeasurer::symbol_size`), with `ctx`.
    pub symbol: Option<crate::measure::SymbolFn>,
    /// Platform time formatting on the runner clock.
    pub time_format: Option<crate::time::TimeFormatFn>,
    /// Writes the app's `console` lines to the unified log (LLP 1027.009 D1).
    pub console: Option<ConsoleFn>,
}

/// One `console` line for the platform's log, on the runtime's thread: the
/// method that wrote it (0 log, 1 info, 2 warn, 3 error, 4 debug) and its
/// UTF-8 bytes, borrowed for the call.
pub type ConsoleFn = extern "C" fn(method: u8, line: *const u8, len: usize);

impl Hooks {
    /// No callbacks: the reference measurer, and replies on `pump` only.
    pub const fn none() -> Hooks {
        Hooks {
            measure: None,
            ctx: std::ptr::null_mut(),
            lines: None,
            inline_boxes: None,
            wake: None,
            wake_ctx: std::ptr::null_mut(),
            canvas_text: None,
            symbol: None,
            time_format: None,
            console: None,
        }
    }

    /// The app's `console` lines into the callback from now on, the ones
    /// `runner` holds first (LLP 1027.009 D2): when its host becomes the one
    /// presented, so a candidate never committed writes nothing.
    pub(super) fn console_into<D: DataSource>(&self, runner: &mut exact_runner::Runner<D>) {
        let Some(console) = self.console else {
            return;
        };
        runner.set_console(move |method, line| {
            use exact_runner::Console;
            let method = match method {
                Console::Log => 0,
                Console::Info => 1,
                Console::Warn => 2,
                Console::Error => 3,
                Console::Debug => 4,
            };
            console(method, line.as_ptr(), line.len());
        });
    }
}

#[cfg(test)]
#[path = "console_tests.rs"]
mod console_tests;
