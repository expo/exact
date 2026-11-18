//! Duo Lab's keyboard region (LLP 1123 D7, D4), laid out headlessly: its
//! fixture's route as a content owner, given what an iPhone 18 Pro host reports
//! (its block, its band, the first root's `interactive-widget` and the keyboard),
//! with each column ending where the fixture says it does.

use exact_kernel::{BlockAxis, HostBlock, Kernel, Keyboard, KeyboardPolicy, Offer, PropId};
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Runner};
use std::path::Path;

#[derive(Default)]
struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// The iPhone 18 Pro in portrait: its size, its band, and a docked keyboard's
/// top, measured on iOS 27.0 (LLP 1123 §"Evidence: measured on expo/exact").
const SIZE: (f32, f32) = (402.0, 874.0);
const BAND: [f32; 4] = [62.0, 0.0, 34.0, 0.0];
const KEYBOARD_TOP: f32 = 546.0;

fn plan(source: &str) -> Runner<NoData> {
    let plan = contract::compile(source).unwrap();
    let plan = contract::bake(plan, NoData).unwrap();
    Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn id_of(k: &Kernel, test_id: &str) -> exact_kernel::ViewId {
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

#[test]
fn each_column_ends_where_the_screen_says() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/duo-lab/fixtures/keyboard-region.contract");
    let mut r = plan(&std::fs::read_to_string(&path).unwrap());
    let k = r.kernel_mut();
    let (root, route) = (id_of(k, "root"), id_of(k, "route"));
    // What a host reports: the first root's policy, the route's block and band
    let widget = k.node(root).unwrap().props.str(PropId::InteractiveWidget);
    assert_eq!(widget, Some("resizes-content"));
    k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent));
    let block = HostBlock {
        tactic: Default::default(),
        width: BlockAxis::Is(SIZE.0),
        height: BlockAxis::Is(SIZE.1),
    };
    k.set_host_block(route, Some(block)).unwrap();
    k.set_owner_band(route, Some(BAND), &[]).unwrap();
    let bottoms = |k: &mut Kernel| {
        k.compute_layout(root, Offer::definite(SIZE.0, SIZE.1))
            .unwrap();
        ["neither", "container", "keyboard", "both"].map(|t| {
            let f = k.node(id_of(k, t)).unwrap().frame;
            f.y + f.height
        })
    };
    // The keyboard down: the page ends above the home indicator; ignoring the
    // safe area, or both regions, reaches the screen's bottom
    let safe = SIZE.1 - BAND[2];
    assert_eq!(bottoms(k), [safe, SIZE.1, safe, SIZE.1]);
    // The keyboard up: the page ends at its top, where ignoring the safe area
    // still ends; ignoring the keyboard reaches the safe area's bottom under
    // it, and ignoring both the screen's bottom
    let keyboard = Keyboard {
        frame: [0.0, KEYBOARD_TOP, SIZE.0, SIZE.1 - KEYBOARD_TOP],
        docked: true,
    };
    k.set_owner_keyboard(route, Some(keyboard)).unwrap();
    assert_eq!(bottoms(k), [KEYBOARD_TOP, KEYBOARD_TOP, safe, SIZE.1]);
    // Dismissed again, it lays out as before
    k.set_owner_keyboard(route, None).unwrap();
    assert_eq!(bottoms(k), [safe, SIZE.1, safe, SIZE.1]);
}

#[test]
fn the_composer_rests_on_the_keyboard_by_its_guide() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/duo-lab/fixtures/keyboard-guide.contract");
    let mut r = plan(&std::fs::read_to_string(&path).unwrap());
    let k = r.kernel_mut();
    let (root, route) = (id_of(k, "root"), id_of(k, "route"));
    let widget = k.node(root).unwrap().props.str(PropId::InteractiveWidget);
    assert_eq!(widget, Some("overlays-content"));
    k.set_keyboard_policy(Some(KeyboardPolicy::OverlaysContent));
    let block = HostBlock {
        tactic: Default::default(),
        width: BlockAxis::Is(SIZE.0),
        height: BlockAxis::Is(SIZE.1),
    };
    k.set_host_block(route, Some(block)).unwrap();
    k.set_owner_band(route, Some(BAND), &[]).unwrap();
    // The composer's bottom, and its field's, to a hundredth of a point, as
    // the field's frame sums f32 offsets
    let bottoms = |k: &mut Kernel| {
        k.compute_layout(root, Offer::definite(SIZE.0, SIZE.1))
            .unwrap();
        ["composer", "field"].map(|t| {
            let f = k.node(id_of(k, t)).unwrap().frame;
            ((f.y + f.height) * 100.0).round() / 100.0
        })
    };
    // The keyboard down: the composer and its field end above the home
    // indicator
    let safe = SIZE.1 - BAND[2];
    assert_eq!(bottoms(k), [safe, safe]);
    // The keyboard up: it lays nothing out, and the composer pads by its
    // reading, so the field rests on it
    let keyboard = Keyboard {
        frame: [0.0, KEYBOARD_TOP, SIZE.0, SIZE.1 - KEYBOARD_TOP],
        docked: true,
    };
    assert!(k.set_owner_keyboard(route, Some(keyboard)).unwrap());
    assert_eq!(bottoms(k), [safe, KEYBOARD_TOP]);
    // Dismissed again, it lays out as before
    assert!(k.set_owner_keyboard(route, None).unwrap());
    assert_eq!(bottoms(k), [safe, safe]);
}

#[test]
fn the_safe_area_attributes_lower_to_their_rows() {
    let source = "component A\n  view\n    column\n      column testId=\"a\" -exact-safe-area-ignore=\"keyboard bottom align top\" -exact-additional-safe-area=\"0 0 20 0\" -exact-safe-area-padding=\"auto 0\"\n      column testId=\"b\" -exact-safe-area-ignore=\"align center\"\n";
    let mut r = plan(source);
    let k = r.kernel_mut();
    let style = |t: &str| k.node(id_of(k, t)).unwrap().style.clone();
    let (a, b) = (style("a"), style("b"));
    // The regions and edges to one row, `align`'s alignment to the other; with
    // `align` alone both regions on every edge
    assert_eq!(
        exact_kernel::SafeAreaIgnore::css(a.rare.safe_area_ignore),
        "keyboard bottom"
    );
    assert_eq!(format!("{:?}", &a.rare.safe_area_ignore_align), "Top");
    assert_eq!(
        exact_kernel::SafeAreaIgnore::css(b.rare.safe_area_ignore),
        "container keyboard"
    );
    assert_eq!(format!("{:?}", &b.rare.safe_area_ignore_align), "Center");
    // CSS's one-to-four-value expansion, `auto` the platform's padding
    let added = [
        a.rare.additional_safe_area_top.clone(),
        a.rare.additional_safe_area_bottom.clone(),
    ];
    assert_eq!(
        added,
        [
            exact_kernel::Dimension::Points(0.0),
            exact_kernel::Dimension::Points(20.0)
        ]
    );
    assert_eq!(a.rare.safe_area_padding_top, exact_kernel::Dimension::Auto);
    assert_eq!(
        a.rare.safe_area_padding_left,
        exact_kernel::Dimension::Points(0.0)
    );
}

#[test]
fn the_keyboard_readings_lower_with_or_without_a_fallback() {
    // UIKit's default guide composed from the readings (its safe-area term
    // reads the content owner's inset until activation), the guide that
    // follows an undocked keyboard too, and a reading with a fallback, which
    // the kernel takes where the reading is unavailable (LLP 1123 D5, D9)
    let guide = "max(var(--exact-keyboard-inset-bottom), var(--exact-safe-area-inset-bottom))";
    let followed = "var(--exact-keyboard-undocked-inset-top)";
    let fallback = "var(--exact-keyboard-inset-bottom, 8px)";
    let source = format!("component A\n  view\n    column\n      column testId=\"a\" padding-bottom=\"{guide}\"\n      column testId=\"b\" padding-top=\"{followed}\" padding-bottom=\"{fallback}\"\n");
    let mut r = plan(&source);
    let k = r.kernel_mut();
    let style = |t: &str| k.node(id_of(k, t)).unwrap().style.clone();
    let (a, b) = (style("a"), style("b"));
    let text = |d: &exact_kernel::Dimension| format!("{d:?}");
    let lowered = |v: &str| format!("Compare(Comparison({v:?}))");
    assert_eq!(text(&a.padding_bottom), lowered(guide));
    assert_eq!(text(&b.padding_top), lowered(followed));
    assert_eq!(text(&b.padding_bottom), lowered(fallback));
    // A fallback's own names are checked as an author property's are
    let unknown = "component A\n  view\n    column padding-bottom=\"var(--exact-keyboard-inset-bottom, var(--gpa))\"\n";
    let refused = contract::compile(unknown).unwrap_err();
    assert!(refused.message.contains("--gpa"), "{}", refused.message);
}
