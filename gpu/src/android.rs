//! Android's initialization entry (LLP 1009 D11): what a GPU artifact that
//! calls into Java needs from the app, once, before its first surface.
//!
//! The host opens the artifact with `dlopen`, which runs no `JNI_OnLoad`, so
//! an artifact that exports `gpu_android_init` is given the process's Java
//! VM, the calling thread's JNI environment and the application `Context`
//! there instead ([`android_init!`](crate::android_init)).

use std::ffi::c_void;

#[cfg(any(exact_android_draw_trace, test))]
#[derive(Default)]
pub(crate) struct DrawTrace {
    extent: Option<(f32, f32, f32)>,
    pub(crate) new_target: bool,
    wants: bool,
    pub(crate) polled: bool,
    pub(crate) invalidated: bool,
    attempts: u32,
}

#[cfg(any(exact_android_draw_trace, test))]
impl DrawTrace {
    pub(crate) fn empty(&mut self) {
        *self = Self {
            extent: self.extent,
            new_target: self.new_target,
            ..Self::default()
        };
    }

    pub(crate) fn attempt(&mut self) {
        self.attempts = self.attempts.saturating_add(1);
    }

    fn admissions(&self, extent: (f32, f32, f32), record: &mut dyn FnMut(&str)) {
        if self.new_target || self.extent.is_none() {
            record("surface");
        }
        if self.extent.is_some_and(|previous| previous != extent) {
            record("resize");
        }
        if self.attempts > 1 {
            record("retry");
        }
        if self.wants {
            record("continuation");
        }
        if self.polled {
            record("input-poll");
        }
        if self.invalidated {
            record("invalidation");
        }
    }

    fn rendered(&mut self, extent: (f32, f32, f32), wants: bool, drawn: bool) {
        self.polled = false;
        self.invalidated = false;
        self.wants = wants;
        if drawn {
            self.attempts = 0;
            self.new_target = false;
            self.extent = Some(extent);
        }
    }
}

#[cfg(exact_android_draw_trace)]
#[link(name = "android")]
extern "C" {
    fn ATrace_isEnabled() -> bool;
    fn ATrace_beginSection(name: *const std::ffi::c_char);
    fn ATrace_endSection();
}

#[cfg(exact_android_draw_trace)]
impl crate::Instance {
    pub(crate) fn trace_draw_begin(&self, id: u32, frame: &crate::Frame) -> Option<String> {
        // SAFETY: the NDK reads this process's tracing flag.
        if !unsafe { ATrace_isEnabled() } {
            return None;
        }
        let mut line = format!("exact canvas {id} {} draw", self.name);
        let mut record = |why: &str| {
            line.push(' ');
            line.push_str(why);
        };
        self.draw_trace
            .admissions((frame.width, frame.height, frame.scale), &mut record);
        if self.input_dirty {
            record("device-input");
        }
        if self.wake.is_woken() {
            record("wake");
        }
        if self.surface.preparing() {
            record("preparation");
        } else if !self.draw_trace.wants
            && !self.dirty
            && !self.input_dirty
            && !self.wake.is_woken()
            && self.draw_trace.attempts == 1
            && !self.draw_trace.polled
        {
            record("host-request");
        }
        if frame.seekable {
            record("driver-clock");
        }
        self.surface
            .draw_reasons(self.draw_trace.polled, &mut record);
        Some(line)
    }

    pub(crate) fn trace_draw_end(
        &mut self,
        frame: &crate::Frame,
        id: u32,
        line: Option<String>,
        wants: bool,
        drawn: bool,
    ) {
        self.draw_trace
            .rendered((frame.width, frame.height, frame.scale), wants, drawn);
        if !drawn {
            return;
        }
        let samples = self.surface.finish_draw_trace(line.is_some());
        if let Some(mut line) = line {
            let count_at = line.find(" draw ").map_or(line.len(), |i| i + 5);
            if let Some(samples) = samples {
                line.insert_str(count_at, &format!(" samples={samples}"));
            }
            trace_sections(id, &line, |part| {
                if let Ok(name) = std::ffi::CString::new(part) {
                    // SAFETY: the NDK copies the live name during begin. The same thread closes the section.
                    unsafe {
                        ATrace_beginSection(name.as_ptr());
                        ATrace_endSection();
                    }
                }
            });
        }
    }
}

// Leave room for atrace's pid and record header in its 1024-byte trace marker.
#[cfg(any(exact_android_draw_trace, test))]
fn trace_sections(id: u32, mut line: &str, mut emit: impl FnMut(&str)) {
    let mut first = true;
    while !line.is_empty() {
        let end = line.floor_char_boundary(900);
        let (part, rest) = line.split_at(end);
        if first {
            emit(part);
            first = false;
        } else {
            emit(&format!("exact canvas {id} detail {part}"));
        }
        line = rest;
    }
}

#[cfg(test)]
mod draw_trace_tests {
    use super::*;

    #[test]
    fn a_retry_keeps_target_changes_and_consumes_each_admission_once() {
        let old = (10., 10., 1.);
        let new = (12., 10., 1.);
        let mut trace = DrawTrace::default();
        trace.rendered(old, false, true);
        trace.polled = true;
        trace.attempt();
        trace.rendered(old, true, false);
        trace.invalidated = true;
        trace.new_target = true;
        trace.attempt();
        let reasons = |trace: &DrawTrace| {
            let mut seen = Vec::new();
            trace.admissions(new, &mut |r| seen.push(r.to_owned()));
            seen
        };
        assert_eq!(
            reasons(&trace),
            ["surface", "resize", "retry", "continuation", "invalidation"]
        );
        trace.polled = true;
        assert!(reasons(&trace).contains(&"input-poll".to_owned()));
        assert!(reasons(&trace).contains(&"invalidation".to_owned()));
        trace.rendered(new, false, true);
        trace.attempt();
        assert!(reasons(&trace).is_empty());
        trace.polled = true;
        trace.empty();
        assert!(reasons(&trace).is_empty());
    }

    #[test]
    fn long_hook_names_leave_all_frame_reasons_in_the_trace() {
        let line = format!(
            "exact canvas 7 world draw hook:{} world:Transform",
            "長".repeat(1000)
        );
        let mut joined = String::new();
        trace_sections(7, &line, |part| {
            assert!(part.len() < 1000);
            joined.push_str(part.strip_prefix("exact canvas 7 detail ").unwrap_or(part));
        });
        assert_eq!(joined, line);
    }
}

/// What the host gives the entry. The pointers are JNI's own types, untyped
/// here so that an artifact chooses its JNI binding, or none.
#[derive(Clone, Copy, Debug)]
pub struct Android {
    /// The process's `JavaVM*`: attach a thread the artifact starts with it.
    pub vm: *mut c_void,
    /// The calling thread's `JNIEnv*`, valid for this call only. The thread
    /// may be one the host attached, where `FindClass` sees the system class
    /// loader alone: an app's class is loaded through
    /// `context.getClassLoader()`.
    pub env: *mut c_void,
    /// The application `Context` (never an `Activity`): a global reference
    /// the host keeps for the process, usable on any attached thread.
    pub context: *mut c_void,
}

/// `gpu_android_init`'s body: 0 when `initialize` succeeds, else 1 with its
/// error where `gpu_error` reads it on this thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn init<E: std::fmt::Display>(
    android: Android,
    initialize: impl FnOnce(Android) -> Result<(), E>,
) -> u32 {
    crate::report_panics();
    match initialize(android) {
        Ok(()) => 0,
        Err(error) => {
            crate::native::refuse(&error.to_string());
            1
        }
    }
}

/// Define the artifact's Android initialization entry, in the crate that
/// calls [`module!`](crate::module): one expression that takes an
/// [`Android`] and returns a `Result` whose error is `Display`, composing the
/// initializers of the crate's dependencies. Every target has the macro, so
/// the line needs no `cfg`; off Android it compiles to nothing.
///
/// ```ignore
/// exact_gpu::android_init!(|android| unsafe {
///     mln_storage::android::init(android.env, android.context)
/// });
/// ```
///
/// The host calls it once for each loaded image in a process, on the thread
/// that opens the artifact, after it has verified the artifact's baked
/// identity and before any other export (so before `gpu_prepare`, a device or
/// a surface factory). A failure, or a Java exception left pending, refuses
/// the artifact: its canvases report the error and stay unpainted. An
/// initialized artifact stays loaded for the life of the process, as a
/// library the VM loaded does.
#[macro_export]
macro_rules! android_init {
    ($initialize:expr) => {
        #[cfg(target_os = "android")]
        $crate::android_init!(@entry $initialize);
    };
    (@entry $initialize:expr) => {
        /// Android's initialization entry (LLP 1009 D11). 0 on success.
        ///
        /// # Safety
        /// `vm`, `env` and `context` are the process's `JavaVM*`, the calling
        /// thread's `JNIEnv*` and a live reference to the application
        /// `Context`.
        #[no_mangle]
        pub unsafe extern "C" fn gpu_android_init(
            vm: *mut ::std::ffi::c_void,
            env: *mut ::std::ffi::c_void,
            context: *mut ::std::ffi::c_void,
        ) -> u32 {
            $crate::android::init($crate::android::Android { vm, env, context }, $initialize)
        }
    };
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::ffi::c_void;

    // The entry as an Android artifact exports it, on every test host.
    crate::android_init!(@entry |android| {
        if android.context.is_null() {
            return Err("no application Context");
        }
        GIVEN.set(Some([android.vm, android.env, android.context].map(|p| p as usize)));
        Ok(())
    });

    thread_local! {
        static GIVEN: std::cell::Cell<Option<[usize; 3]>> = const { std::cell::Cell::new(None) };
    }

    #[test]
    fn the_entry_gives_the_initializer_its_pointers_and_reports_its_error_once() {
        let [vm, env, context] = [1, 2, 3].map(|p: usize| p as *mut c_void);
        // SAFETY: the test's initializer reads the pointers as numbers.
        assert_eq!(unsafe { gpu_android_init(vm, env, context) }, 0);
        assert_eq!(GIVEN.get(), Some([1, 2, 3]));
        assert_eq!(crate::native::error(), "");
        // SAFETY: as above.
        assert_eq!(
            unsafe { gpu_android_init(vm, env, std::ptr::null_mut()) },
            1
        );
        assert_eq!(crate::native::error(), "no application Context");
        assert_eq!(crate::native::error(), "", "reported once");
    }
}
