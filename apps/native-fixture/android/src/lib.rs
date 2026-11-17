//! The native-module fixture on Android's Canvas host (LLP 1076 §3.3): the host's JNI
//! (`exact_painted::canvas_jni!`) over the app's data source, its baked plan and
//! its hatches. The reader (`host/android`) loads this library as
//! `libexactcanvas.so`.

#[cfg(target_os = "android")]
include!(concat!(env!("OUT_DIR"), "/hatches.rs"));

#[cfg(target_os = "android")]
exact_painted::canvas_jni!(
    native_fixture_data::Fixture,
    hatches = crate::exact_hatches()
);
