fn main() {
    exact_js_bake::build_mixed_with(
        std::path::Path::new(".."),
        "web",
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
    .expect("bake Fieldnotes");
}
