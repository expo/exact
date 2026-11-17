fn main() {
    exact_js_bake::build_mixed_with(
        std::path::Path::new(".."),
        "linux",
        &canvas_gallery_data::Capture,
        canvas_gallery_data::RUST_SOURCES,
        |javascript| {
            Box::new(canvas_gallery_data::mixed(
                javascript,
                canvas_gallery_data::Placement::Main,
            ))
        },
    )
    .expect("bake the canvas gallery");
}
