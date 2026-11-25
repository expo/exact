//! @exact/photos's fixture on Android's Canvas host: the host's JNI over
//! the app's data source, with app storage for its image calls and SQLite for
//! its photo index, and its baked plan, which the reader loads as
//! `libexactcanvas.so`.

#[cfg(target_os = "android")]
include!(concat!(env!("OUT_DIR"), "/hatches.rs"));

#[cfg(target_os = "android")]
exact_logic::configured!(
    AppData,
    exact_data_host::Storage<photos_fixture_data::Fixture>,
    || {
        ibex2_sqlite::link();
        exact_logic::Swappable::off(exact_data_host::Storage::new(
            photos_fixture_data::Fixture::default(),
        ))
    }
);

#[cfg(target_os = "android")]
exact_painted::canvas_jni!(crate::AppData, hatches = crate::exact_hatches());
