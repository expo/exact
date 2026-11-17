//! The Linux host's integration tests that pin the fixture font: one binary.
//! `tests/it/` holds the ones that leave the font environment alone.

mod ahead;
mod backdrop;
mod borders;
mod color_scheme;
mod field_align;
mod gradients;
mod host;
mod materials;
mod motion_paint;
mod native_buttons;
mod native_fields;
mod paint;
mod text;
mod tint;
mod visual;

#[path = "../fonts/mod.rs"]
mod test_fonts;
use test_fonts::pin_font;

/// Caltrain's plan, compiled and baked once for the binary: its boots each make their own
/// presenter, data source and images from the same bytes.
fn caltrain_plan() -> &'static [u8] {
    static PLAN: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    PLAN.get_or_init(|| caltrain::build().unwrap().encode())
}
