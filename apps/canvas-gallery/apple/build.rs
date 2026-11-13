fn main() {
    let platform = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        // tvOS and watchOS reuse the manifest's iOS policy, including interpreted Rust updates.
        Ok("ios" | "tvos" | "watchos") => "ios",
        _ => "macos",
    };
    exact_js_bake::build_mixed_with(
        std::path::Path::new(".."),
        platform,
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
