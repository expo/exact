//! @ref LLP 1123 D13 — a painted box's reach: its surface and its hits past its border box, its children and what
//! reads its geometry where they were.
use super::super::*;

const APP: &str = r##"component App
  view
    box testId="root" width="100%" height="100%" background-color="#ffffff" position="relative"
      box testId="clip" position="absolute" left=15 top=35 width=300 height=300 overflow="hidden"
        box testId="card" position="absolute" left=5 top=5 width=100 height=60 background-color="#ff0000"
          box testId="child" position="absolute" left=10 top=10 width=20 height=20 background-color="#0000ff"
"##;

fn boot() -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    p
}

fn id(p: &Presenter<()>, test_id: &str) -> ViewId {
    let kernel = p.host().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .id
}

fn pixel(p: &mut Presenter<()>, x: f32, y: f32) -> [u8; 3] {
    let c = p.frame().pixel(x as u32, y as u32).unwrap().demultiply();
    [c.red(), c.green(), c.blue()]
}

const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// The card's border box is (20, 40, 100, 60); reaching 10, 5, 15 and 20 (top right bottom left) it paints and takes
/// hits over (0, 30, 125, 85), inside its ancestor's clip, which begins at (15, 35).
#[test]
fn a_reaching_box_paints_and_takes_hits_past_its_border_box() {
    let mut p = boot();
    let (card, child, root) = (id(&p, "card"), id(&p, "child"), id(&p, "root"));
    let still = p.still();
    let empty = p.frame().data().to_vec();
    assert_eq!(pixel(&mut p, 17., 37.), WHITE);
    assert_eq!(p.hit(17., 37.), Some(id(&p, "clip")));
    assert!(p.set_box_reach(card, Some([10., 5., 15., 20.])));
    assert!(
        !p.set_box_reach(card, Some([10., 5., 15., 20.])),
        "the same reach is no change"
    );
    assert!(
        p.still() != still,
        "a frame that only scrolls would not show it"
    );
    // Its fill on each edge of the reached frame, and nothing past it
    for (x, y) in [(17., 37.), (122., 50.), (60., 112.), (17., 112.)] {
        assert_eq!(pixel(&mut p, x, y), RED, "at ({x}, {y})");
    }
    for (x, y) in [(127., 50.), (60., 117.)] {
        assert_eq!(pixel(&mut p, x, y), WHITE, "past the reach at ({x}, {y})");
    }
    // Its ancestor's clip still holds: nothing of it left of 15 or above 35
    assert_eq!(pixel(&mut p, 10., 50.), WHITE);
    assert_eq!(pixel(&mut p, 50., 32.), WHITE);
    assert_eq!(p.hit(10., 50.), Some(root));
    // Its hits in the strip, its child where it was laid out, and its geometry its laid-out box
    assert_eq!(p.hit(17., 37.), Some(card));
    assert_eq!(p.hit(122., 50.), Some(card));
    assert_eq!(pixel(&mut p, 40., 60.), BLUE);
    assert_eq!(p.hit(40., 60.), Some(child));
    assert_eq!(p.rect_of(card), Some((20., 40., 100., 60.)));
    assert_eq!(p.rect_of(child), Some((30., 50., 20., 20.)));
    // Cleared, the frame is the one it painted before
    assert!(p.set_box_reach(card, None));
    assert_eq!(p.hit(17., 37.), Some(id(&p, "clip")));
    assert!(p.frame().data() == empty, "as before the reach");
}

/// A 300-point scroller at (0, 100) over ten 100-point rows, each its own colour.
const SCROLLER: &str = r##"component App
  view
    box testId="root" width="100%" height="100%" background-color="#ffffff" position="relative"
      scroll testId="port" position="absolute" left=0 top=100 width=300 height=300
        box testId="row-0" height=100 background-color="#ff0000"
        box testId="row-1" height=100 background-color="#00ff00"
        box testId="row-2" height=100 background-color="#0000ff"
        box testId="row-3" height=100 background-color="#ffff00"
        box testId="row-4" height=100 background-color="#00ffff"
        box testId="row-5" height=100 background-color="#ff00ff"
        box testId="row-6" height=100 background-color="#800000"
        box testId="row-7" height=100 background-color="#008000"
        box testId="row-8" height=100 background-color="#000080"
        box testId="row-9" height=100 background-color="#808000"
"##;

const ROW: [[u8; 3]; 6] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [0, 255, 255],
    [255, 0, 255],
];

/// Claimed with a reach of 40 above and 60 below and insets of as much, the scroller's range and its content's place
/// are as they were and the reached strips show the content beyond its box; an inset taken away moves the content and
/// shortens the range at the same CSS offset, and a claim that shortens the range past the offset clamps it.
#[test]
fn a_claimed_scroller_reaches_and_widens_its_range_keeping_its_offset() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(SCROLLER),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let port = id(&p, "port");
    p.scroll.insert(port, (0., 200.));
    p.frame();
    let max = |p: &Presenter<()>| p.scroller(port, &BTreeMap::new()).unwrap().max;
    assert_eq!(max(&p), (0., 700.));
    assert_eq!(
        pixel(&mut p, 150., 110.),
        ROW[2],
        "at 200 the box shows row 2 at its top"
    );
    assert_eq!(pixel(&mut p, 150., 70.), WHITE, "nothing above the box");
    let empty = p.frame().data().to_vec();
    assert!(p.set_scroll_claim(port, Some(([40., 0., 60., 0.], [40., 0., 60., 0.]))));
    assert_eq!(
        max(&p),
        (0., 700.),
        "insets as deep as the reach: the same range"
    );
    assert_eq!(p.scroll_of(port), (0., 200.));
    assert_eq!(
        pixel(&mut p, 150., 110.),
        ROW[2],
        "the content where it was"
    );
    assert_eq!(
        pixel(&mut p, 150., 70.),
        ROW[1],
        "row 1 in the strip above the box"
    );
    assert_eq!(
        pixel(&mut p, 150., 450.),
        ROW[5],
        "row 5 in the strip below it"
    );
    assert_eq!(pixel(&mut p, 150., 465.), WHITE, "nothing past the reach");
    assert_eq!(
        p.hit(150., 70.),
        Some(id(&p, "row-1")),
        "a row in the strip takes its hits"
    );
    assert_eq!(
        p.rect_of(port),
        Some((0., 100., 300., 300.)),
        "its geometry its laid-out box"
    );
    let under = p.scrollers_at(150., 70.);
    let told = under.chain.iter().find(|s| s.id == port).unwrap();
    assert_eq!(
        (told.port, told.offset, told.max),
        ([0., 60., 300., 400.], (0., 200.), (0., 700.)),
        "the reader's"
    );
    // The top inset taken away: the range is 40 shorter and the content 40 higher, at the same CSS offset
    assert!(p.set_scroll_claim(port, Some(([40., 0., 60., 0.], [0., 0., 60., 0.]))));
    assert_eq!(max(&p), (0., 660.));
    assert_eq!(p.scroll_of(port), (0., 200.));
    assert_eq!(pixel(&mut p, 150., 70.), ROW[2], "the content at 240");
    // At its end, a claim that shortens the range clamps the CSS offset to the new end
    p.scroll.insert(port, (0., 660.));
    assert!(p.set_scroll_claim(port, Some(([40., 0., 60., 0.], [0.; 4]))));
    assert_eq!(max(&p), (0., 600.));
    assert_eq!(p.scroll_of(port), (0., 600.));
    // Released, the scroller is its layout's again
    assert!(p.set_scroll_claim(port, None));
    assert_eq!(max(&p), (0., 700.));
    p.scroll.insert(port, (0., 200.));
    assert!(p.frame().data() == empty, "as before the claim");
}

/// A colour-only extension paints the card's colour in a strip past its border box, once where a translucent colour
/// meets the box's own and where it meets the card's reach, takes no hits, and leaves the frame as it was once gone.
#[test]
fn a_colour_only_extension_paints_the_boxs_colour_around_it_once() {
    let source = APP.replace("#ff0000", "#ff000080");
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let (card, clip) = (id(&p, "card"), id(&p, "clip"));
    let empty = p.frame().data().to_vec();
    let inside = pixel(&mut p, 100., 90.);
    assert_ne!(inside, WHITE);
    // 20 above and 10 to the right: the card's (20, 40, 100, 60) grows to (20, 20, 110, 80), inside the clip at 35
    assert!(p.set_box_extension(card, Some([20., 10., 0., 0.])));
    for (x, y) in [(60., 37.), (125., 70.), (125., 37.)] {
        assert_eq!(pixel(&mut p, x, y), inside, "the colour once at ({x}, {y})");
    }
    assert_eq!(
        pixel(&mut p, 135., 70.),
        WHITE,
        "nothing past the extension"
    );
    assert_eq!(pixel(&mut p, 60., 32.), WHITE, "the ancestor's clip holds");
    assert_eq!(pixel(&mut p, 40., 60.), BLUE, "the child where it was");
    assert_eq!(p.hit(125., 70.), Some(clip), "the strip takes no hits");
    assert_eq!(p.rect_of(card), Some((20., 40., 100., 60.)));
    // Reaching 40 to the right as it extends 20 up: the reach paints its own strip, the extension the rest, once
    assert!(p.set_box_reach(card, Some([0., 40., 0., 0.])));
    for (x, y) in [(60., 37.), (150., 70.), (125., 37.), (125., 70.)] {
        assert_eq!(pixel(&mut p, x, y), inside, "once at ({x}, {y})");
    }
    assert_eq!(
        pixel(&mut p, 150., 37.),
        WHITE,
        "neither reaches above the reached strip"
    );
    assert!(p.set_box_reach(card, None));
    assert!(p.set_box_extension(card, None));
    assert!(p.frame().data() == empty, "as before the extension");
}

/// A 300-point scroller at (0, 100) whose first child is a 50-point top bar on its scrollport, over its rows.
const BARRED: &str = r##"component App
  view
    box testId="root" width="100%" height="100%" background-color="#ffffff" position="relative"
      scroll testId="port" position="absolute" left=0 top=100 width=300 height=300
        box testId="bar" -exact-safe-area-bar="top" height=50 background-color="#808080"
        box testId="row-0" height=100 background-color="#ff0000"
        box testId="row-1" height=100 background-color="#00ff00"
        box testId="row-2" height=100 background-color="#0000ff"
        box testId="row-3" height=100 background-color="#ffff00"
        box testId="row-4" height=100 background-color="#00ffff"
"##;

const GREY: [u8; 3] = [128, 128, 128];

/// With the kernel laying bars on scrollports, a scroller's top bar keeps its place as the content scrolls 100 under
/// it, takes the hits there, and is painted over the content; with it off, the bar is the content's first row.
#[test]
fn a_scrollers_own_bar_stays_on_its_scrollport_over_its_content() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(BARRED),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let (port, bar) = (id(&p, "port"), id(&p, "bar"));
    p.frame();
    assert!(
        p.host().kernel().scrollport_bars(port).is_empty(),
        "no bars on scrollports until the kernel lays them"
    );
    p.scroll.insert(port, (0., 100.));
    assert_ne!(
        pixel(&mut p, 150., 110.),
        GREY,
        "off, the bar scrolls away with the content"
    );
    p.host.runner_mut().kernel_mut().set_scrollport_bars(true);
    p.scroll.insert(port, (0., 0.));
    let _ = p.after_commit();
    p.frame();
    assert_eq!(p.host().kernel().scrollport_bars(port), vec![bar]);
    assert_eq!(pixel(&mut p, 150., 110.), GREY);
    let top = p.rect_of(bar).unwrap();
    p.scroll.insert(port, (0., 100.));
    assert_eq!(
        pixel(&mut p, 150., 110.),
        GREY,
        "on, the bar keeps its place"
    );
    assert_eq!(
        p.rect_of(bar),
        Some(top),
        "where it was painted before the scroll"
    );
    assert_eq!(
        p.hit(150., 110.),
        Some(bar),
        "over the content it takes the hits"
    );
    assert_ne!(
        pixel(&mut p, 150., 160.),
        GREY,
        "the content scrolls on below it"
    );
}

/// Under a claim whose top inset, 100, is deeper than its reach, 40, what a scroller reveals is centred in its clear
/// region, the reached port less its insets, at the CSS offset that puts it there; and the runner's geometry reads the
/// content where it is painted, its displacement, while the CSS offset stays the presenter's.
#[test]
fn a_claimed_scroller_reveals_in_its_clear_region_and_tells_the_runner_its_displacement() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(SCROLLER),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let port = id(&p, "port");
    p.frame();
    assert!(p.set_scroll_claim(port, Some(([40., 0., 60., 0.], [100., 0., 60., 0.]))));
    p.scroll.insert(port, (0., 400.));
    p.frame();
    p.reveal(id(&p, "row-0")).unwrap();
    assert_eq!(
        p.scroll_of(port),
        (0., 0.),
        "row 0 at the clear region's top: CSS offset 0"
    );
    p.scroll.insert(port, (0., 700.));
    p.frame();
    p.reveal(id(&p, "row-3")).unwrap();
    // Centred in the clear region, 60 to 300 below the box's top, it is painted at 180: CSS offset 230
    assert_eq!(p.scroll_of(port), (0., 230.));
    p.scroll.insert(port, (0., 200.));
    p.publish_scroll();
    assert_eq!(
        p.host.runner().scroll_offset(Some(port)),
        (0., 140.),
        "200 + 40 - 100"
    );
    assert_eq!(p.scroll_of(port), (0., 200.));
}

/// A zoomable scroller's range under a claim is its zoomed content's past its reached port: 1000 over 300 at a zoom of
/// 1, reaching 40 above and 60 below with no insets, is 600.
#[test]
fn a_claimed_zoomable_scroller_takes_the_claims_range() {
    let source = r##"component App
  view
    box width="100%" height="100%" position="relative"
      scroll testId="port" position="absolute" left=0 top=100 width=300 height=300 -exact-max-zoom=4
        box width=300 height=1000 background-color="#ff0000"
"##;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let port = id(&p, "port");
    let max = |p: &Presenter<()>| p.scroller(port, &BTreeMap::new()).unwrap().max;
    assert_eq!(max(&p).1, 700.);
    assert!(p.set_scroll_claim(port, Some(([40., 0., 60., 0.], [0.; 4]))));
    assert_eq!(max(&p).1, 600.);
}

/// A claimed scroller's thumb lies at its CSS offset, not its content's displacement: at the end of its range the
/// thumb ends where its track does.
#[test]
fn a_claimed_scrollers_thumb_is_at_its_css_offset() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(SCROLLER),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.brush.scrollbars.enabled = true;
    let port = id(&p, "port");
    assert!(p.set_scroll_claim(port, Some(([40., 0., 60., 0.], [100., 0., 60., 0.]))));
    p.scroll.insert(port, (0., 760.));
    p.frame();
    let bar = *p
        .brush
        .scrollbars
        .bars
        .iter()
        .find(|b| b.id == port && b.vertical)
        .unwrap();
    assert_eq!(bar.offset, 760., "the CSS offset");
    assert_eq!(
        bar.thumb.1 + bar.thumb.3,
        bar.track.1 + bar.track.3,
        "at the end of its range, the end of its track"
    );
}

/// The reach and the extension meet the box's own paint on whole device pixels: a half-transparent card reaching half
/// a point to the right and extending ten past it shows one fill across the join, and a box that clips its overflow
/// takes no extension.
#[test]
fn an_extension_meets_the_box_on_the_pixel_grid_and_skips_a_clipping_box() {
    let source = APP.replace("#ff0000", "#ff000080");
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let (card, clip) = (id(&p, "card"), id(&p, "clip"));
    let inside = pixel(&mut p, 100., 90.);
    assert!(p.set_box_reach(card, Some([0., 0.5, 0., 0.])));
    assert!(p.set_box_extension(card, Some([0., 10., 0., 0.])));
    for x in [119., 120., 121., 125.] {
        assert_eq!(pixel(&mut p, x, 90.), inside, "one fill at x {x}");
    }
    assert!(p.set_box_reach(card, None));
    assert!(p.set_box_extension(card, None));
    let before = p.frame().data().to_vec();
    assert!(p.set_box_extension(clip, Some([20., 20., 20., 20.])));
    assert!(
        p.frame().data() == before,
        "the clipping box paints no strip"
    );
}

/// The runner's geometry follows a claim at once and goes back to the CSS offset when it is released, for a scroller
/// that has never scrolled too.
#[test]
fn releasing_a_claim_gives_the_runner_back_the_css_offset() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(SCROLLER),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let port = id(&p, "port");
    assert!(p.set_scroll_claim(port, Some(([40., 0., 60., 0.], [100., 0., 60., 0.]))));
    assert_eq!(p.host.runner().scroll_offset(Some(port)), (0., -60.));
    assert!(p.set_scroll_claim(port, None));
    assert_eq!(p.host.runner().scroll_offset(Some(port)), (0., 0.));
}
