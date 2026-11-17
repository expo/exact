/// Pin each test binary's fonts before its first text engine. Every test
/// that creates an engine calls this process-wide setup first.
pub(crate) fn pin_font() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_var(
            "EXACT_FONTS",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ),
        );
        std::env::set_var("EXACT_FONT", "DejaVu Sans");
    });
}
