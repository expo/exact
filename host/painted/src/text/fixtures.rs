//! Font catalogs owned by each test, independent of installed fonts.
use super::*;

fn catalog() -> catalog::Catalog {
    catalog::Catalog::from_files(
        &[
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            )),
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fonts/NotoColorEmoji-subset.ttf"
            )),
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fonts/NotoSansCJKsc-subset.otf"
            )),
        ],
        "DejaVu Sans",
    )
}

pub(crate) fn engine() -> TextEngine {
    TextEngine::with_catalog(catalog())
}

pub(crate) fn for_assets(plan: &Plan, assets: &Assets) -> Shared {
    Rc::new(RefCell::new(TextEngine::with_catalog(
        catalog().with_assets(plan, assets),
    )))
}

pub(crate) fn shared() -> Shared {
    Rc::new(RefCell::new(engine()))
}
