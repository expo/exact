use std::sync::Arc;

use super::Style;
use crate::sys::DefaultCheapStr;
use crate::{geometry::*, style_helpers::TaffyAuto as _};

#[test]
fn defaults_match() {
    #[cfg(feature = "grid")]
    use super::GridPlacement;

    let old_defaults: Style<DefaultCheapStr> = Style {
        dummy: core::marker::PhantomData,
        rare: super::RareStyle {
            #[cfg(feature = "grid")]
            grid_template_rows: Default::default(),
            #[cfg(feature = "grid")]
            grid_template_columns: Default::default(),
            #[cfg(feature = "grid")]
            grid_auto_rows: Default::default(),
            #[cfg(feature = "grid")]
            grid_auto_columns: Default::default(),
            #[cfg(feature = "grid")]
            grid_template_column_names: Default::default(),
            #[cfg(feature = "grid")]
            grid_template_row_names: Default::default(),
            #[cfg(feature = "grid")]
            grid_template_areas: Default::default(),
            #[cfg(feature = "grid")]
            grid_row: Line { start: GridPlacement::Auto, end: GridPlacement::Auto },
            #[cfg(feature = "grid")]
            grid_column: Line { start: GridPlacement::Auto, end: GridPlacement::Auto },
            #[cfg(feature = "block_layout")]
            multicol: None,
            ..super::RareStyle::DEFAULT
        }
        .into(),
        display: Default::default(),
        item_is_table: false,
        item_is_replaced: false,
        box_sizing: Default::default(),
        #[cfg(feature = "float_layout")]
        float: Default::default(),
        #[cfg(feature = "float_layout")]
        clear: Default::default(),
        direction: Default::default(),
        overflow: Default::default(),
        scrollbar_width: 0.0,
        contain: Default::default(),
        position: Default::default(),
        #[cfg(feature = "flexbox")]
        flex_direction: Default::default(),
        #[cfg(feature = "flexbox")]
        flex_wrap: Default::default(),
        #[cfg(feature = "flexbox_balance")]
        flex_line_count: 1,
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        align_items: Default::default(),
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        align_self: Default::default(),
        #[cfg(any(feature = "grid", feature = "block_layout"))]
        justify_items: Default::default(),
        #[cfg(any(feature = "grid", feature = "block_layout"))]
        justify_self: Default::default(),
        #[cfg(any(feature = "flexbox", feature = "grid", feature = "block_layout"))]
        align_content: Default::default(),
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        justify_content: Default::default(),
        inset: Rect::auto(),
        margin: Rect::zero(),
        padding: Rect::zero(),
        border: Rect::zero(),
        gap: Size::zero(),
        #[cfg(feature = "block_layout")]
        text_align: Default::default(),
        #[cfg(feature = "flexbox")]
        flex_grow: 0.0,
        #[cfg(feature = "flexbox")]
        flex_shrink: 1.0,
        #[cfg(feature = "flexbox")]
        flex_basis: super::Dimension::AUTO,
        size: Size::auto(),
        min_size: Size::auto(),
        max_size: Size::auto(),
        aspect_ratio: Default::default(),
        aspect_ratio_content_box: false,
        #[cfg(feature = "grid")]
        grid_auto_flow: Default::default(),
    };

    assert_eq!(Style::DEFAULT, Style::<DefaultCheapStr>::default());
    assert_eq!(Style::DEFAULT, old_defaults);
}

#[test]
#[cfg(feature = "parse")]
fn parse_contain() {
    use super::Contain;

    fn parse(input: &str) -> Contain {
        input.parse().unwrap()
    }

    assert_eq!(parse("none"), Contain::NONE);
    assert_eq!(parse("content"), Contain::LAYOUT | Contain::PAINT);
    assert_eq!(parse("layout"), Contain::LAYOUT);
    assert_eq!(parse("style"), Contain::NONE);
    assert_eq!(parse("paint"), Contain::PAINT);
    assert_eq!(parse("layout paint"), Contain::LAYOUT | Contain::PAINT);
    assert_eq!(parse("paint layout"), Contain::LAYOUT | Contain::PAINT);
    assert_eq!(parse("layout paint style"), Contain::LAYOUT | Contain::PAINT);
    assert_eq!(parse("Paint LAYOUT"), Contain::LAYOUT | Contain::PAINT);
    assert!("paint paint".parse::<Contain>().is_err());

    assert!("".parse::<Contain>().is_err());
    assert!("banana".parse::<Contain>().is_err());
    assert!("layout layout".parse::<Contain>().is_err());
    assert!("none layout".parse::<Contain>().is_err());
    assert!("layout none".parse::<Contain>().is_err());
    assert!("content layout".parse::<Contain>().is_err());
    assert!("layout content".parse::<Contain>().is_err());
}

// NOTE: Please feel free the update the sizes in this test as required. This test is here to prevent unintentional size changes
// and to serve as accurate up-to-date documentation on the sizes.
#[test]
fn style_sizes() {
    use super::*;
    type S = crate::sys::DefaultCheapStr;

    fn assert_type_size<T>(expected_size: usize) {
        let name = ::core::any::type_name::<T>();
        let name = name.replace("taffy::geometry::", "");
        let name = name.replace("taffy::style::dimension::", "");
        let name = name.replace("taffy::style::alignment::", "");
        let name = name.replace("taffy::style::flex::", "");
        let name = name.replace("taffy::style::grid::", "");

        assert_eq!(
            ::core::mem::size_of::<T>(),
            expected_size,
            "Expected {} for be {} byte(s) but it was {} byte(s)",
            name,
            expected_size,
            ::core::mem::size_of::<T>(),
        );
    }

    // Display and Position
    assert_type_size::<Display>(1);
    assert_type_size::<BoxSizing>(1);
    assert_type_size::<Position>(1);
    assert_type_size::<Overflow>(1);

    // Dimensions and aggregations of Dimensions
    assert_type_size::<f32>(4);
    assert_type_size::<LengthPercentage>(8);
    assert_type_size::<LengthPercentageAuto>(8);
    assert_type_size::<Dimension>(8);
    assert_type_size::<Size<LengthPercentage>>(16);
    assert_type_size::<Size<LengthPercentageAuto>>(16);
    assert_type_size::<Size<Dimension>>(16);
    assert_type_size::<Rect<LengthPercentage>>(32);
    assert_type_size::<Rect<LengthPercentageAuto>>(32);
    assert_type_size::<Rect<Dimension>>(32);

    // Alignment — `AlignContent` and `AlignItems` are structs of two `#[repr(u8)]` enums
    // (position keyword + safety modifier). Niche-packing in the safety byte (only 2 of
    // 256 values used) lets `Option<_>` stay the same size as the bare struct.
    assert_type_size::<AlignContentKeyword>(1);
    assert_type_size::<AlignItemsKeyword>(1);
    assert_type_size::<AlignmentSafety>(1);
    assert_type_size::<AlignContent>(2);
    assert_type_size::<AlignItems>(2);
    assert_type_size::<Option<AlignItems>>(2);
    assert_type_size::<Option<AlignContent>>(2);

    // Flexbox Container
    assert_type_size::<FlexDirection>(1);
    assert_type_size::<FlexWrap>(1);

    // CSS Grid Container
    assert_type_size::<GridAutoFlow>(1);
    assert_type_size::<MinTrackSizingFunction>(8);
    assert_type_size::<MaxTrackSizingFunction>(8);
    assert_type_size::<TrackSizingFunction>(16);
    assert_type_size::<Vec<TrackSizingFunction>>(24);
    assert_type_size::<Vec<GridTemplateComponent<S>>>(24);

    // String-type dependent (String)
    assert_type_size::<GridTemplateComponent<String>>(56);
    assert_type_size::<GridPlacement<String>>(32);
    assert_type_size::<Line<GridPlacement<String>>>(64);
    assert!(core::mem::size_of::<Style<String>>() <= 304);
    assert_type_size::<Rare<String>>(core::mem::size_of::<usize>());

    // String-type dependent (Arc<str>)
    assert_type_size::<GridTemplateComponent<Arc<str>>>(56);
    assert_type_size::<GridPlacement<Arc<str>>>(24);
    assert_type_size::<Line<GridPlacement<Arc<str>>>>(48);
    assert_type_size::<Style<Arc<str>>>(core::mem::size_of::<Style<String>>());
}
