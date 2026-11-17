//! The Canvas gallery on the web: its TypeScript data module draws every
//! canvas (LLP 1056), replayed into the browser's own 2D context.

include!(concat!(env!("OUT_DIR"), "/module.rs"));
const PLAN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.plan"));
const COMPAT: &str = include_str!(concat!(env!("OUT_DIR"), "/compat.json"));

type ExactEmbeddedData = canvas_gallery_data::Data<exact_js_web::Module>;
fn embedded_data() -> ExactEmbeddedData {
    canvas_gallery_data::mixed(
        exact_js_web::Module::new(APP, GRANTS, REVISION)
            .with_canvas_surfaces(CANVAS_SURFACES)
            .placed(TYPESCRIPT_PLACEMENT),
        RUST_PLACEMENT,
    )
}
include!(concat!(env!("OUT_DIR"), "/logic.rs"));
exact_web::host!(
    AppData,
    PLAN,
    COMPAT,
    app_data,
    [
        include_bytes!(concat!(env!("OUT_DIR"), "/app.module.json")) as &[u8],
        include_bytes!(concat!(env!("OUT_DIR"), "/app.js")) as &[u8]
    ]
);
