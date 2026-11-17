//! The Interaction gallery on Android's Canvas host: the host's JNI over the
//! app's data source and its baked plan, which the reader loads as
//! `libexactcanvas.so`.

#[cfg(target_os = "android")]
include!(concat!(env!("OUT_DIR"), "/hatches.rs"));

#[cfg(target_os = "android")]
exact_painted::canvas_jni!(
    interaction_gallery_data::Gallery,
    hatches = crate::exact_hatches()
);
