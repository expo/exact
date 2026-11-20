//! Bake Fieldnotes for the Canvas host through its Rust and TypeScript composer.

fn main() {
    if exact_js_bake::take_android_linked().unwrap_or_else(|e| panic!("Android bake: {e}")) {
        return;
    }
    exact_js_bake::build_android_with(std::path::Path::new(".."), |app| {
        exact_js_bake::build_mixed_with(
            app,
            "linux",
            &fieldnotes_data::Backup::default(),
            exact_js_bake::hands_out_work!(fieldnotes_data::Backup::default()),
            fieldnotes_data::RUST_SOURCES,
            |javascript| {
                Box::new(fieldnotes_data::mixed(
                    javascript,
                    fieldnotes_data::Placement::Main,
                ))
            },
        )
    })
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
}
