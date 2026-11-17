//! The widgets' JNI (LLP 1126.003 D9): `dev.exact.widgets.Native`'s methods
//! (the reader's `:widgets` module, `host/android/widgets`), which an app's
//! library defines with `exact_painted::widgets_jni!` only when its bake has
//! widget entries. Nothing else references [`super::render`] or
//! [`super::timeline`], so a library without widgets links neither.
//!
//! Every call is made on the `:widgets` process's one worker thread
//! (`host/android/widgets`'s `Worker`); each answers JSON, or `{"error"}`.

/// Define `dev.exact.widgets.Native` over the entry plans the build script
/// wrote to `OUT_DIR` (`entries = [(kind, "plans/<kind>.plan"), …]`), with
/// `data` the expression that makes the app's data source for a refresh and
/// `module` whether the bake paired a TypeScript module (`app.module.json`,
/// `app.hbc`) with them, which a TypeScript refresh runs.
#[macro_export]
macro_rules! widgets_jni {
    (data = $data:expr, module = $module:tt, entries = [$(($kind:literal, $path:literal)),* $(,)?] $(,)?) => {
        mod exact_widgets_jni {
            #![allow(clippy::missing_safety_doc, unsafe_code)]
            use super::*;
            use $crate::canvas::jni_sys::{jbyteArray, jclass, jdouble, jfloat, jfloatArray, jlong, JNIEnv};

            const ENTRIES: &[(&str, &[u8])] = &[
                $(($kind, include_bytes!(concat!(env!("OUT_DIR"), "/", $path)))),*
            ];

            $crate::widgets_jni!(@module $module);

            fn plan(kind: &str) -> Option<&'static [u8]> {
                ENTRIES.iter().find(|(k, _)| *k == kind).map(|(_, p)| *p)
            }

            /// The process's environment, as the reader's (`canvas_jni!`'s
            /// `environment`), from the app's files directory `dir`: once,
            /// before the first session reads it.
            fn environment(dir: &str) {
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| {
                    $crate::android::redirect_stdio();
                    std::env::set_var("HOME", dir);
                    std::env::set_var("XDG_DATA_HOME", format!("{dir}/data"));
                    std::env::set_var("EXACT_ASSETS", format!("{dir}/app"));
                    $crate::app::show_material_look();
                    std::env::set_var("EXACT_FONTS", "/system/fonts");
                    std::env::set_var("EXACT_FONT", "Roboto");
                });
            }

            unsafe fn optional(env: *mut JNIEnv, array: jbyteArray) -> Option<Vec<u8>> {
                (!array.is_null()).then(|| $crate::canvas::jni::utf8(env, array).into_bytes())
            }

            fn answer(env: *mut JNIEnv, result: Result<String, String>) -> jbyteArray {
                let text = result.unwrap_or_else(|e| {
                    $crate::android::log(&format!("exact: widget: {e}"));
                    $crate::snapshot::error_json(&e)
                });
                unsafe { $crate::canvas::jni::bytes(env, text.as_bytes()) }
            }

            /// Render `kind` at each (width, height) pair of `sizes` (dp),
            /// at `now` (ms since the epoch), with `fontScale` the root font
            /// size over 16 dp: `snapshot::render`'s JSON.
            #[no_mangle]
            pub unsafe extern "system" fn Java_dev_exact_widgets_Native_render(
                env: *mut JNIEnv,
                _class: jclass,
                dir: jbyteArray,
                kind: jbyteArray,
                props: jbyteArray,
                configuration: jbyteArray,
                sizes: jfloatArray,
                now: jdouble,
                font_scale: jfloat,
            ) -> jbyteArray {
                let dir = $crate::canvas::jni::utf8(env, dir);
                environment(&dir);
                $crate::canvas::secrets::attach(env);
                let kind = $crate::canvas::jni::utf8(env, kind);
                let props = optional(env, props);
                let configuration = optional(env, configuration);
                let len = ((**env).GetArrayLength.expect("jni"))(env, sizes).max(0) as usize;
                let mut flat = vec![0f32; len];
                ((**env).GetFloatArrayRegion.expect("jni"))(env, sizes, 0, len as i32, flat.as_mut_ptr());
                let sizes: Vec<(f32, f32)> = flat.chunks_exact(2).map(|p| (p[0], p[1])).collect();
                let result = match plan(&kind) {
                    None => Err(format!("no widget `{kind}`")),
                    Some(plan) => $crate::snapshot::render(
                        $crate::snapshot::Entry {
                            plan,
                            kind: &kind,
                            props: props.as_deref(),
                            configuration: configuration.as_deref(),
                        },
                        || (),
                        std::path::Path::new(&format!("{dir}/app")),
                        &sizes,
                        now,
                        font_scale,
                    ),
                };
                answer(env, result)
            }

            /// Run `kind`'s refresh at `now` within `budget` ms: its
            /// timeline, JSON.
            #[no_mangle]
            pub unsafe extern "system" fn Java_dev_exact_widgets_Native_timeline(
                env: *mut JNIEnv,
                _class: jclass,
                dir: jbyteArray,
                kind: jbyteArray,
                now: jdouble,
                budget: jlong,
            ) -> jbyteArray {
                let dir = $crate::canvas::jni::utf8(env, dir);
                environment(&dir);
                $crate::canvas::secrets::attach(env);
                let kind = $crate::canvas::jni::utf8(env, kind);
                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_millis(budget.max(0) as u64);
                let result = match plan(&kind) {
                    None => Err(format!("no widget `{kind}`")),
                    Some(plan) => $crate::snapshot::timeline(
                        $crate::snapshot::Entry { plan, kind: &kind, props: None, configuration: None },
                        $data,
                        module(),
                        now,
                        deadline,
                    ),
                };
                answer(env, result)
            }
        }
    };
    (@module true) => {
        fn module() -> Option<$crate::snapshot::Module<'static>> {
            static BYTECODE: &$crate::snapshot::Aligned<[u8]> =
                &$crate::snapshot::Aligned(*include_bytes!(concat!(env!("OUT_DIR"), "/app.hbc")));
            Some($crate::snapshot::Module {
                receipt: include_str!(concat!(env!("OUT_DIR"), "/app.module.json")),
                bytecode: &BYTECODE.0,
            })
        }
    };
    (@module false) => {
        fn module() -> Option<$crate::snapshot::Module<'static>> {
            None
        }
    };
}
