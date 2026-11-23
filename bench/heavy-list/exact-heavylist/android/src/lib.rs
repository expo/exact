//! Heavy list on Android's Canvas host: the host's JNI over the data source and
//! the baked plan.

#[cfg(target_os = "android")]
include!(concat!(env!("OUT_DIR"), "/hatches.rs"));

#[cfg(target_os = "android")]
exact_painted::canvas_jni!(
    exact_heavylist_data::Heavy,
    hatches = crate::exact_hatches()
);
