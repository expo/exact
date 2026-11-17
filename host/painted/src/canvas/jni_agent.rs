//! The agent's wait services the reader without re-entering its borrowed host.
#![allow(unsafe_code)]
use super::jni_extensions;
use crate::canvas::CanvasHost;
use exact_runner::DataSource;
use jni_sys::{jstring, jvalue, JNIEnv};

/// Keep JNI references within this call; the callback is removed before returning.
/// # Safety
/// `env` is this owner thread's JNI environment for the duration of the call.
pub unsafe fn run<D: DataSource + Default>(
    env: *mut JNIEnv,
    host: &mut CanvasHost<D>,
    line: &str,
) -> String {
    let class = ((**env).FindClass.expect("jni"))(env, c"dev/exact/canvas/Exact".as_ptr());
    if class.is_null() {
        return String::new(); // Preserve the Java exception for the caller.
    }
    let method = ((**env).GetStaticMethodID.expect("jni"))(
        env,
        class,
        c"agentTurn".as_ptr(),
        c"([I)Ljava/lang/String;".as_ptr(),
    );
    if method.is_null() {
        ((**env).DeleteLocalRef.expect("jni"))(env, class);
        return String::new();
    }
    let reply = host.agent_with_reader(
        line,
        Box::new(move |now, words| {
            jni_extensions::stamp(now);
            let array = words.map_or(std::ptr::null_mut(), |words| {
                let array = ((**env).NewIntArray.expect("jni"))(env, words.len() as i32);
                if !array.is_null() {
                    ((**env).SetIntArrayRegion.expect("jni"))(
                        env,
                        array,
                        0,
                        words.len() as i32,
                        words.as_ptr(),
                    );
                }
                array
            });
            let text = ((**env).CallStaticObjectMethodA.expect("jni"))(
                env,
                class,
                method,
                [jvalue { l: array }].as_ptr(),
            ) as jstring;
            ((**env).DeleteLocalRef.expect("jni"))(env, array);
            if ((**env).ExceptionCheck.expect("jni"))(env) != 0 {
                ((**env).ExceptionDescribe.expect("jni"))(env);
                ((**env).ExceptionClear.expect("jni"))(env);
                return Vec::new();
            }
            if text.is_null() {
                return Vec::new();
            }
            let json = super::super::field::text(env, text);
            ((**env).DeleteLocalRef.expect("jni"))(env, text);
            serde_json::from_str(&json)
                .expect("the reader returns what it handed the owner (`Handed`)")
        }),
    );
    ((**env).DeleteLocalRef.expect("jni"))(env, class);
    reply
}
