//! The session's app module on Apple (LLP 1067.000): the Swift half is the
//! app's module artifact, which `ExactKit` loads and owns per session; this
//! half turns a source's long native call into a call to it and takes its
//! answer. It is installed in the source's native slot as the host's
//! handler, so it outlives activations (Q6) and reaches a worker-built
//! instance through the shared slot.
#![allow(unsafe_code)] // One boxed reply crosses the C seam and comes back once.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_void;

/// Only a binding observer sends a null answer slot. A native caller always supplies its own slot.
pub fn binding_observer(call: CallFn, ctx: *mut c_void) -> exact_runner::Announce {
    let ctx = ctx as usize;
    std::sync::Arc::new(move |body: &str| {
        call(
            ctx as *mut c_void,
            body.as_ptr(),
            body.len(),
            std::ptr::null_mut(),
        );
    })
}

/// `later(ctx, body, len, reply)`: a long native call's JSON body, on the
/// executor's thread. The host copies the body, dispatches the call to the
/// main thread (Q5) and returns; `reply` is answered exactly once, from
/// any thread, with `exact_app_reply` ([`reply`]).
pub type LaterFn = extern "C" fn(*mut c_void, *const u8, usize, *mut c_void);

/// The handler for a session's app module behind `later` and its `ctx`.
pub fn handler(later: LaterFn, ctx: *mut c_void) -> exact_runner::NativeHandler {
    let ctx = ctx as usize;
    std::sync::Arc::new(move |body: Vec<u8>, reply: exact_runner::Reply| {
        let reply = Box::into_raw(Box::new(reply)) as *mut c_void;
        later(ctx as *mut c_void, body.as_ptr(), body.len(), reply);
    })
}

/// `call(ctx, body, len, slot)`: a `native.call`'s JSON body, on the
/// source's thread. The host answers it before returning (on the main
/// thread, D4), with `exact_app_answer(slot, …)` ([`answer`]).
pub type CallFn = extern "C" fn(*mut c_void, *const u8, usize, *mut c_void);

/// Where a synchronous call's answer lands: on the caller's stack.
#[derive(Default)]
struct Slot {
    answer: Option<(u32, Vec<u8>)>,
    loading: std::time::Duration,
}

/// The synchronous call for a session's app module behind `call` and `ctx`.
pub fn caller(call: CallFn, ctx: *mut c_void) -> exact_runner::NativeCall {
    let ctx = ctx as usize;
    std::sync::Arc::new(move |body: &[u8]| {
        let mut slot = Slot::default();
        call(
            ctx as *mut c_void,
            body.as_ptr(),
            body.len(),
            &mut slot as *mut Slot as *mut c_void,
        );
        exact_runner::Native::record_library_load(slot.loading);
        match slot.answer {
            Some((200, reply)) => Ok(reply),
            Some((_, message)) => Err(String::from_utf8_lossy(&message).into_owned()),
            None => Err("the app module did not answer the call".into()),
        }
    })
}

/// Answer a synchronous call, before `call` returns: status 200 carries the
/// JSON reply, any other a refusal. `slot` came from a [`CallFn`] call still
/// running; `bytes` is null or valid for `len` bytes. A second answer
/// replaces the first. Status 102 reports library-load seconds before the final answer
/// (LLP 1067.000.001 D7); it does not replace that answer.
pub fn answer(slot: *mut c_void, status: u32, bytes: *const u8, len: usize) {
    if slot.is_null() {
        return;
    }
    let body = if bytes.is_null() || len == 0 {
        Vec::new()
    } else {
        // SAFETY: the caller passes `len` valid bytes for this call.
        unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec()
    };
    // SAFETY: the slot lives on the stack of the `caller` call that is running.
    let slot = unsafe { &mut *(slot as *mut Slot) };
    if status == 102 {
        // An interim load report, in seconds, before the final answer. The C table is unchanged.
        if let Some(seconds) = std::str::from_utf8(&body)
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
        {
            if let Ok(time) = std::time::Duration::try_from_secs_f64(seconds) {
                slot.loading = slot.loading.saturating_add(time);
            }
        }
    } else {
        slot.answer = Some((status, body));
    }
}

/// Answer one long native call: `status` 200 carries the JSON reply, any
/// other status a refusal message that rejects the TypeScript promise.
/// Consumes `reply`, which came from a [`LaterFn`] call and has not been
/// answered; `bytes` is null or valid for `len` bytes. The Swift side calls
/// it once per `later` (`exact_app_reply`, [`app_module_exports!`]).
pub fn reply(reply: *mut c_void, status: u32, bytes: *const u8, len: usize) {
    if reply.is_null() {
        return;
    }
    // SAFETY: the caller hands back the box `handler` made, once.
    let reply = unsafe { Box::from_raw(reply as *mut exact_runner::Reply) };
    let body = if bytes.is_null() || len == 0 {
        Vec::new()
    } else {
        // SAFETY: the caller passes `len` valid bytes for this call.
        unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec()
    };
    reply.send(exact_runner::Outcome::Response(exact_runner::Response {
        status: u16::try_from(status).unwrap_or(500),
        headers: Vec::new(),
        body,
    }));
}

/// A topic's UTF-8 from the C seam: `bytes` is null or valid for `len` bytes.
pub fn text(bytes: *const u8, len: usize) -> String {
    if bytes.is_null() {
        return String::new();
    }
    // SAFETY: the caller passes `len` valid bytes for this call.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, len) }).into_owned()
}

/// Export the app module's reply from the application's static archive.
#[macro_export]
macro_rules! app_module_exports {
    () => {
        /// Answer one long native call, once (`include/exact.h`).
        #[no_mangle]
        pub extern "C" fn exact_app_reply(
            reply: *mut ::std::ffi::c_void,
            status: u32,
            bytes: *const u8,
            len: usize,
        ) {
            $crate::app_module::reply(reply, status, bytes, len)
        }

        /// Answer a synchronous call, before it returns (`include/exact.h`).
        #[no_mangle]
        pub extern "C" fn exact_app_answer(
            slot: *mut ::std::ffi::c_void,
            status: u32,
            bytes: *const u8,
            len: usize,
        ) {
            $crate::app_module::answer(slot, status, bytes, len)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    extern "C" fn answer(ctx: *mut c_void, body: *const u8, len: usize, reply: *mut c_void) {
        assert_eq!(ctx as usize, 7);
        let body = unsafe { std::slice::from_raw_parts(body, len) }.to_vec();
        let reply = reply as usize;
        std::thread::spawn(move || {
            let text = format!("{{\"echo\":{}}}", String::from_utf8(body).unwrap());
            super::reply(reply as *mut c_void, 200, text.as_ptr(), text.len());
        });
    }

    extern "C" fn now(ctx: *mut c_void, body: *const u8, len: usize, slot: *mut c_void) {
        assert_eq!(ctx as usize, 9);
        let body = unsafe { std::slice::from_raw_parts(body, len) };
        if body == b"refuse" {
            let text = b"refused";
            return super::answer(slot, 500, text.as_ptr(), text.len());
        }
        let text = format!("{{\"now\":{}}}", String::from_utf8_lossy(body));
        super::answer(slot, 200, text.as_ptr(), text.len());
    }

    extern "C" fn silent(_: *mut c_void, _: *const u8, _: usize, _: *mut c_void) {}

    #[test]
    fn a_call_answers_on_the_callers_thread_or_refuses() {
        let call = caller(now, 9 as *mut c_void);
        assert_eq!(call(b"1").unwrap(), br#"{"now":1}"#);
        assert_eq!(call(b"refuse").unwrap_err(), "refused");
        let silent = caller(silent, 9 as *mut c_void);
        assert!(silent(b"1").unwrap_err().contains("did not answer"));
    }

    extern "C" fn loaded(_: *mut c_void, _: *const u8, _: usize, slot: *mut c_void) {
        for report in [b"1.3".as_slice(), b"NaN", b"-1", b"invalid"] {
            super::answer(slot, 102, report.as_ptr(), report.len());
        }
        super::answer(slot, 200, b"{}".as_ptr(), 2);
    }

    #[test]
    fn a_load_report_preserves_the_answer_and_reaches_only_the_callers_clock() {
        let before = exact_runner::Native::library_load_time();
        assert_eq!(caller(loaded, std::ptr::null_mut())(b"{}").unwrap(), b"{}");
        assert_eq!(
            exact_runner::Native::library_load_time() - before,
            std::time::Duration::from_millis(1300)
        );
        std::thread::spawn(|| {
            assert_eq!(
                exact_runner::Native::library_load_time(),
                std::time::Duration::ZERO
            )
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_long_call_reaches_the_app_module_and_its_answer_comes_back_once() {
        let handler = handler(answer, 7 as *mut c_void);
        let (tx, rx) = mpsc::channel();
        handler(
            br#"{"op":"status"}"#.to_vec(),
            exact_runner::Reply::new(move |o| tx.send(o).unwrap()),
        );
        match rx.recv().unwrap() {
            exact_runner::Outcome::Response(r) => {
                assert_eq!(r.status, 200);
                assert_eq!(r.body, br#"{"echo":{"op":"status"}}"#);
            }
            other => panic!("{other:?}"),
        }
    }
}
