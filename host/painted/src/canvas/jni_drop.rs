//! The JNI methods of drags from other apps (LLP 1094.001 D5), expanded by
//! `canvas_jni!` inside its module, where `with` reaches the thread's host.

#[doc(hidden)]
#[macro_export]
macro_rules! canvas_jni_drop {
    () => {
        /// The drag is at (`x`, `y`) px (`CanvasHost::drag_over`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_dragOver(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            x: $crate::canvas::jni_sys::jfloat,
            y: $crate::canvas::jni_sys::jfloat,
        ) {
            with(|h| h.drag_over(x, y));
        }

        /// The drag left the window or was cancelled.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_dragLeft(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
        ) {
            with(|h| h.drag_left());
        }

        /// A field took the drop at (`x`, `y`) px (`CanvasHost::drag_dropped`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_dragDropped(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            x: $crate::canvas::jni_sys::jfloat,
            y: $crate::canvas::jni_sys::jfloat,
        ) {
            with(|h| h.drag_dropped(x, y));
        }

        /// The drag was released at (`x`, `y`) px: the drop's number, 0 when
        /// nothing takes it (`CanvasHost::drop_released`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_dropReleased(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            x: $crate::canvas::jni_sys::jfloat,
            y: $crate::canvas::jni_sys::jfloat,
        ) -> $crate::canvas::jni_sys::jint {
            with(|h| h.drop_released(x, y)).unwrap_or(0) as $crate::canvas::jni_sys::jint
        }

        /// Who takes a drop Android gives field `id`'s view at (`x`, `y`) px
        /// (`CanvasHost::field_drop`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_fieldDrop(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
            x: $crate::canvas::jni_sys::jfloat,
            y: $crate::canvas::jni_sys::jfloat,
        ) -> $crate::canvas::jni_sys::jint {
            with(|h| h.field_drop(id as u32, x, y)).unwrap_or(-1)
        }

        /// Drop `id`'s items arrived, as JSON (`CanvasHost::drop_landed`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_dropLanded(
            env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
            items: $crate::canvas::jni_sys::jstring,
        ) {
            let items = string(env, items);
            with(|h| h.drop_landed(id as u32, &items));
        }
    };
}
