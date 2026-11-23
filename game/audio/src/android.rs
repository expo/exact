//! AAudio output for an Android game canvas.
//! The same 32-voice, bounded-PCM native mixer as the Apple and Windows
//! outputs, rendered from AAudio's realtime data callback.
//! The Contract's own sounds are a separate path, `host/android/soundarm`.
use super::{Mixer, Pending};
use crate::Output;
use std::{
    ffi::{c_char, c_void, CStr},
    ptr,
    sync::atomic::{AtomicI32, Ordering},
};

type Stream = c_void;
type Builder = c_void;
type DataCallback = unsafe extern "C" fn(*mut Stream, *mut c_void, *mut c_void, i32) -> i32;
type ErrorCallback = unsafe extern "C" fn(*mut Stream, *mut c_void, i32);

// aaudio/AAudio.h (API 26; usage and content type API 28).
const DIRECTION_OUTPUT: i32 = 0;
const FORMAT_PCM_FLOAT: i32 = 2;
const SHARING_MODE_SHARED: i32 = 1;
const PERFORMANCE_MODE_LOW_LATENCY: i32 = 12;
const USAGE_GAME: i32 = 14;
const CONTENT_TYPE_SONIFICATION: i32 = 4;
const CALLBACK_RESULT_CONTINUE: i32 = 0;
const OK: i32 = 0;
const STATE_PAUSING: i32 = 5;
const STATE_FLUSHING: i32 = 7;
/// How long a suspend waits for the stream to finish pausing, then flushing.
const SETTLE_NS: i64 = 200_000_000;

#[link(name = "aaudio")]
// SAFETY: these declarations match the NDK's aaudio/AAudio.h C ABI.
unsafe extern "C" {
    fn AAudio_createStreamBuilder(builder: *mut *mut Builder) -> i32;
    fn AAudio_convertResultToText(result: i32) -> *const c_char;
    fn AAudioStreamBuilder_setDirection(builder: *mut Builder, direction: i32);
    fn AAudioStreamBuilder_setFormat(builder: *mut Builder, format: i32);
    fn AAudioStreamBuilder_setChannelCount(builder: *mut Builder, channels: i32);
    fn AAudioStreamBuilder_setSharingMode(builder: *mut Builder, mode: i32);
    fn AAudioStreamBuilder_setPerformanceMode(builder: *mut Builder, mode: i32);
    fn AAudioStreamBuilder_setUsage(builder: *mut Builder, usage: i32);
    fn AAudioStreamBuilder_setContentType(builder: *mut Builder, content: i32);
    fn AAudioStreamBuilder_setDataCallback(
        builder: *mut Builder,
        callback: DataCallback,
        user: *mut c_void,
    );
    fn AAudioStreamBuilder_setErrorCallback(
        builder: *mut Builder,
        callback: ErrorCallback,
        user: *mut c_void,
    );
    fn AAudioStreamBuilder_openStream(builder: *mut Builder, stream: *mut *mut Stream) -> i32;
    fn AAudioStreamBuilder_delete(builder: *mut Builder) -> i32;
    fn AAudioStream_requestStart(stream: *mut Stream) -> i32;
    fn AAudioStream_requestPause(stream: *mut Stream) -> i32;
    fn AAudioStream_requestFlush(stream: *mut Stream) -> i32;
    fn AAudioStream_waitForStateChange(
        stream: *mut Stream,
        input: i32,
        next: *mut i32,
        timeout_ns: i64,
    ) -> i32;
    fn AAudioStream_requestStop(stream: *mut Stream) -> i32;
    fn AAudioStream_close(stream: *mut Stream) -> i32;
    fn AAudioStream_getSampleRate(stream: *mut Stream) -> i32;
    fn AAudioStream_getChannelCount(stream: *mut Stream) -> i32;
    fn AAudioStream_getFormat(stream: *mut Stream) -> i32;
    fn AAudioStream_getFramesPerBurst(stream: *mut Stream) -> i32;
}

#[link(name = "log")]
// SAFETY: matches the NDK's android/log.h.
unsafe extern "C" {
    fn __android_log_write(priority: i32, tag: *const c_char, text: *const c_char) -> i32;
}

/// One line to logcat under the tag `exact-audio` (info, or warn): what the
/// game's audio did, for `adb logcat -s exact-audio`. A module's stderr is
/// not logcat's.
pub(crate) fn log(warn: bool, line: &str) {
    let Ok(text) = std::ffi::CString::new(line.replace('\0', " ")) else {
        return;
    };
    // SAFETY: both strings are valid and NUL-terminated for the call.
    unsafe {
        __android_log_write(
            if warn { 5 } else { 4 },
            c"exact-audio".as_ptr(),
            text.as_ptr(),
        )
    };
}

fn text(result: i32) -> String {
    // SAFETY: AAudio returns a static NUL-terminated string (or null).
    let name = unsafe { AAudio_convertResultToText(result) };
    if name.is_null() {
        return format!("AAudio result {result}");
    }
    // SAFETY: non-null, static and NUL-terminated per the NDK.
    let name = unsafe { CStr::from_ptr(name) };
    format!("{} ({result})", name.to_string_lossy())
}

fn check(what: &str, result: i32) -> Result<(), String> {
    if result == OK {
        Ok(())
    } else {
        Err(format!("AAudio {what}: {}", text(result)))
    }
}

/// What the realtime callback owns: the mixer, and where it reports a
/// fatal stream error (a disconnected device) for the owner to read.
struct Arm {
    mixer: Mixer,
    error: AtomicI32,
}

unsafe extern "C" fn render(
    _stream: *mut Stream,
    context: *mut c_void,
    data: *mut c_void,
    frames: i32,
) -> i32 {
    if context.is_null() || data.is_null() || frames <= 0 {
        return CALLBACK_RESULT_CONTINUE;
    }
    // SAFETY: AAudio serializes data callbacks for one stream, and the boxed
    // Arm outlives the stream (closed in Drop before the box is freed). Only
    // the mixer is borrowed: the error callback and the owner read `error`
    // from other threads meanwhile.
    let mixer = unsafe { &mut (*context.cast::<Arm>()).mixer };
    mixer.commands();
    // SAFETY: the stream was opened stereo float: `frames` interleaved pairs.
    let output = unsafe { std::slice::from_raw_parts_mut(data.cast::<f32>(), frames as usize * 2) };
    for frame in output.as_chunks_mut::<2>().0 {
        let (l, r) = mixer.frame();
        frame[0] = l;
        frame[1] = r;
    }
    CALLBACK_RESULT_CONTINUE
}

unsafe extern "C" fn failed(_stream: *mut Stream, context: *mut c_void, error: i32) {
    if context.is_null() {
        return;
    }
    // Closing from an AAudio callback is forbidden: the owner reads this
    // (`Output::failure`), drops the output and opens a new one later.
    // SAFETY: the boxed Arm outlives the stream; only its atomic is borrowed,
    // apart from the mixer the data callback holds.
    let failed = unsafe { &(*context.cast::<Arm>()).error };
    failed.store(if error == OK { -1 } else { error }, Ordering::Release);
}

/// The default output device, opened only by a live presentation owner.
pub struct AndroidOutput {
    stream: *mut Stream,
    arm: Box<Arm>,
    pending: Pending,
    suspended: bool,
}

impl AndroidOutput {
    pub fn new() -> Result<Self, String> {
        let (pending, mixer) = Pending::new();
        let mut output = Self {
            stream: ptr::null_mut(),
            arm: Box::new(Arm {
                mixer,
                error: AtomicI32::new(0),
            }),
            pending,
            suspended: false,
        };
        let mut builder: *mut Builder = ptr::null_mut();
        // SAFETY: the builder and stream slots are valid out-pointers; the
        // callbacks' context is the boxed Arm, which outlives the stream.
        unsafe {
            check(
                "createStreamBuilder",
                AAudio_createStreamBuilder(&mut builder),
            )?;
            AAudioStreamBuilder_setDirection(builder, DIRECTION_OUTPUT);
            AAudioStreamBuilder_setFormat(builder, FORMAT_PCM_FLOAT);
            AAudioStreamBuilder_setChannelCount(builder, 2);
            AAudioStreamBuilder_setSharingMode(builder, SHARING_MODE_SHARED);
            AAudioStreamBuilder_setPerformanceMode(builder, PERFORMANCE_MODE_LOW_LATENCY);
            AAudioStreamBuilder_setUsage(builder, USAGE_GAME);
            AAudioStreamBuilder_setContentType(builder, CONTENT_TYPE_SONIFICATION);
            let context = (&mut *output.arm as *mut Arm).cast::<c_void>();
            AAudioStreamBuilder_setDataCallback(builder, render, context);
            AAudioStreamBuilder_setErrorCallback(builder, failed, context);
            let opened = AAudioStreamBuilder_openStream(builder, &mut output.stream);
            AAudioStreamBuilder_delete(builder);
            check("openStream", opened)?;
            let (rate, channels, format) = (
                AAudioStream_getSampleRate(output.stream),
                AAudioStream_getChannelCount(output.stream),
                AAudioStream_getFormat(output.stream),
            );
            if format != FORMAT_PCM_FLOAT || channels != 2 || rate <= 0 {
                return Err(format!("AAudio gave format {format}, {channels} channels at {rate} Hz, not stereo float"));
            }
            // Before the stream starts: the callback reads it from now on.
            output.arm.mixer.rate = rate as f64;
            check("requestStart", AAudioStream_requestStart(output.stream))?;
            // One line in logcat says the game's audio is up, and how.
            log(
                false,
                &format!(
                    "game audio: AAudio stereo float at {rate} Hz, {} frames a burst, low latency",
                    AAudioStream_getFramesPerBurst(output.stream)
                ),
            );
        }
        Ok(output)
    }

    /// The activity stopped: the app went to the background.
    pub fn suspend(&mut self) -> Result<(), String> {
        // A pause keeps the samples the stream already holds, which the return
        // would play. A flush discards them, once the pause has finished.
        // SAFETY: this is the owned open stream.
        unsafe { check("requestPause", AAudioStream_requestPause(self.stream))? };
        self.settle(STATE_PAUSING)?;
        // SAFETY: as above; the stream is now paused.
        unsafe { check("requestFlush", AAudioStream_requestFlush(self.stream))? };
        self.settle(STATE_FLUSHING)?;
        self.suspended = true;
        let ids: Vec<_> = self.pending.live.keys().copied().collect();
        for id in ids {
            self.pending.stop(id);
        }
        self.pending.flush();
        Ok(())
    }

    /// Wait until the stream has left `passing`, an asynchronous request's
    /// transitional state.
    fn settle(&self, passing: i32) -> Result<(), String> {
        let mut next = passing;
        // SAFETY: the owned open stream and a valid out-pointer.
        let waited =
            unsafe { AAudioStream_waitForStateChange(self.stream, passing, &mut next, SETTLE_NS) };
        check("waitForStateChange", waited)
    }

    /// Back in the foreground. Increment the transport generation before sync.
    pub fn resume(&mut self) -> Result<(), String> {
        self.pending.flush();
        // SAFETY: this is the owned open stream.
        unsafe { check("requestStart", AAudioStream_requestStart(self.stream))? };
        self.suspended = false;
        Ok(())
    }
}

impl Output for AndroidOutput {
    fn capacity(&self) -> usize {
        32
    }
    fn ready(&self) -> bool {
        !self.suspended && self.arm.error.load(Ordering::Acquire) == 0
    }
    fn failure(&self) -> Option<String> {
        let code = self.arm.error.load(Ordering::Acquire);
        (code != 0).then(|| format!("AAudio stream: {}", text(code)))
    }
    fn flush(&mut self) {
        self.pending.flush();
    }
    fn owns_pcm(&self, pcm: &crate::Pcm) -> bool {
        self.pending.retained.contains_key(&pcm.address())
    }
    fn start(
        &mut self,
        id: u64,
        pcm: &crate::Pcm,
        rate: u32,
        looping: bool,
        offset: usize,
        pitch: f32,
    ) -> bool {
        self.pending.start(id, pcm, rate, looping, offset, pitch)
    }
    fn set(&mut self, id: u64, left: f32, right: f32) {
        self.pending.set(id, left, right);
    }
    fn set_pitch(&mut self, id: u64, pitch: f32) {
        self.pending.set_pitch(id, pitch);
    }
    fn finished(&self, id: u64) -> bool {
        self.pending.finished(id)
    }
    fn stop(&mut self, id: u64) {
        self.pending.stop(id);
    }
}

impl Drop for AndroidOutput {
    fn drop(&mut self) {
        if !self.stream.is_null() {
            // SAFETY: the owned stream is stopped and closed exactly once;
            // close waits for a running callback before the Arm is freed.
            unsafe {
                AAudioStream_requestStop(self.stream);
                AAudioStream_close(self.stream);
            }
            self.stream = ptr::null_mut();
        }
    }
}
