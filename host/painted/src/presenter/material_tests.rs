//! The Material look's controls (LLP 1115.001 D9, S2): a checkbox, a radio
//! and a filled button take Material's size and colours, and what the
//! author wrote wins; each takes a touch over 48 dp, apart from its layout
//! (LLP 1115.001.001 §5 Q5). The
//! look is the process's (`app::show_material_look`), so these run in a
//! child process of their own.
use super::*;

const APP: &str = r##"component App
  view
    column padding=10 align-items="flex-start"
      input type="checkbox" checked=true testId="on"
      input type="checkbox" testId="off"
      input type="checkbox" checked=true disabled=true testId="faded"
      button buttonStyle="filled" testId="save"
        text "Save"
      button buttonStyle="filled" -exact-control-size="large" testId="large"
        text "Save"
      button buttonStyle="filled" disabled=true accent-color="#ff0000" testId="red"
        text "Save"
      button buttonStyle="filled" -exact-corner-style="small" testId="small"
        text "Save"
      button buttonStyle="filled" disabled=true testId="quiet"
        text "Save"
        text "Later"
"##;

/// Whether this is the child process that shows the Material look; the
/// parent runs the test there and returns false.
pub(super) fn material_process() -> bool {
    const CHILD: &str = "EXACT_MATERIAL_LOOK_TEST";
    if std::env::var_os(CHILD).is_some() {
        crate::app::show_material_look();
        return true;
    }
    let name = std::thread::current().name().unwrap().to_owned();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn boot() -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        (),
        (400., 600.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    p
}

pub(super) fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host().kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

/// The colour at a point of the frame, in points.
fn pixel(p: &mut Presenter<()>, x: f32, y: f32) -> [u8; 4] {
    let frame = p.frame();
    let at = |v: f32| (v * 2.4375) as u32;
    let c = frame.pixel(at(x), at(y)).unwrap().demultiply();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

/// An indeterminate `progress` is Material's indicator, not UIKit's
/// spokes: a ring in the colour, a tenth of the box thick, open between 290°
/// and 360° clockwise from the top, as its still frame (the reader turns it).
#[test]
fn materials_indeterminate_progress_is_its_arc() {
    if !material_process() {
        return;
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(
            r##"component App
  view
    column padding=10 align-items="flex-start"
      progress aria-label="Loading" width=40 height=40 color="#0000FF" testId="spinner"
"##,
        ),
        (),
        (400., 600.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let r = p.rect_of(id(&p, "spinner")).unwrap();
    let (cx, cy, ring) = (r.0 + 20.0, r.1 + 20.0, 18.0);
    let page = pixel(&mut p, 2.0, 2.0);
    let at = |degrees: f32| {
        let a = degrees.to_radians();
        (cx + ring * a.sin(), cy - ring * a.cos())
    };
    for degrees in [10.0, 90.0, 180.0, 270.0] {
        let (x, y) = at(degrees);
        assert_eq!(pixel(&mut p, x, y), [0, 0, 255, 255], "{degrees}°");
    }
    let (x, y) = at(325.0);
    assert_eq!(pixel(&mut p, x, y), page, "the gap");
    assert_eq!(pixel(&mut p, cx, cy), page, "a ring");
    assert_eq!(pixel(&mut p, cx, cy - 14.0), page, "4 thick");
}

#[test]
fn materials_checkbox_and_filled_button_take_its_size_and_colours() {
    if !material_process() {
        return;
    }
    let mut p = boot();
    let on = p.rect_of(id(&p, "on")).unwrap();
    assert_eq!(
        (on.2, on.3),
        (24.0, 24.0),
        "the mark-size row's Material 24: the 20 drawn and 2 round it"
    );
    // Checked: the accent's fill above the check, and the page in the 2
    // round it; unchecked: the page shows inside its outline.
    let accent = crate::paint::control::accents(None, false).0;
    let page = pixel(&mut p, 2.0, 2.0);
    assert_eq!(pixel(&mut p, on.0 + 12.0, on.1 + 5.0), accent);
    assert_eq!(pixel(&mut p, on.0 + 1.0, on.1 + 12.0), page, "its padding");
    let off = p.rect_of(id(&p, "off")).unwrap();
    assert_eq!(pixel(&mut p, off.0 + 12.0, off.1 + 12.0), page);
    assert_ne!(
        pixel(&mut p, off.0 + 2.5, off.1 + 12.0),
        page,
        "its outline"
    );
    // Disabled: no longer the accent.
    let faded = p.rect_of(id(&p, "faded")).unwrap();
    assert_ne!(pixel(&mut p, faded.0 + 12.0, faded.1 + 5.0), accent);
    // A size reported before (a switch that became a checkbox) is forgotten.
    let view = id(&p, "off");
    assert!(p.host.set_intrinsic(view, Some((38.0, 22.0))).is_none());
    p.size_controls();
    p.frame();
    let off = p.rect_of(view).unwrap();
    assert_eq!((off.2, off.3), (24.0, 24.0));
    // The filled button: Material's 40 tall, at least 58 wide, a pill in
    // the accent.
    let save = p.rect_of(id(&p, "save")).unwrap();
    assert_eq!(save.3, 40.0);
    assert!(save.2 >= 58.0, "{save:?}");
    assert_eq!(pixel(&mut p, save.0 + 12.0, save.1 + 20.0), accent);
    assert_eq!(
        pixel(&mut p, save.0 + 1.0, save.1 + 1.0),
        page,
        "a pill's corner"
    );
    // A larger control size is a larger button.
    let large = p.rect_of(id(&p, "large")).unwrap();
    assert!(large.3 > save.3 && large.2 > save.2, "{large:?} {save:?}");
    // An authored accent stays when disabled; an authored corner style wins.
    let red = p.rect_of(id(&p, "red")).unwrap();
    assert_eq!(pixel(&mut p, red.0 + 12.0, red.1 + 20.0), [255, 0, 0, 255]);
    let small = p.rect_of(id(&p, "small")).unwrap();
    assert_ne!(
        pixel(&mut p, small.0 + 1.5, small.1 + 1.5),
        page,
        "small corners"
    );
    // Disabled, the subtitle is fainter than the title, not stronger.
    let quiet = p.rect_of(id(&p, "quiet")).unwrap();
    let frame = (*p.frame()).clone();
    let ink = |from: f32, to: f32| {
        let at = |v: f32| (v * 2.4375) as u32;
        let mut darkest = 255u8;
        for y in at(from)..at(to) {
            for x in at(quiet.0 + 20.0)..at(quiet.0 + quiet.2 - 20.0) {
                darkest = darkest.min(frame.pixel(x, y).unwrap().demultiply().red());
            }
        }
        darkest
    };
    let middle = quiet.1 + quiet.3 / 2.0;
    assert!(ink(middle, quiet.1 + quiet.3 - 6.0) >= ink(quiet.1 + 6.0, middle));
}

const MARKS: &str = r##"component App
  state on = false
  state picked = false
  state ghost = false
  action flip(value)
    on = value
  action haunt(value)
    ghost = value
  action pick
    picked = true
  view
    column padding=16 align-items="flex-start"
      input type="radio" name="r" checked=true testId="chosen"
      input type="radio" name="r" testId="other"
      row align-items="flex-start"
        input type="checkbox" checked=on change=flip testId="near"
        button press=pick testId="beside"
          text "Beside"
      text `${on} ${picked} ${ghost}` testId="said"
      box height=40
      input type="checkbox" checked=ghost change=haunt visibility="hidden" testId="hidden"
      box height=40
      input type="checkbox" width=40 height=40 checked=true testId="large"
      box height=40
      column position="relative"
        input type="checkbox" checked=ghost change=haunt testId="covered"
        box position="absolute" inset=-12 background-color="#808080" testId="cover"
      box height=40
      column disabled=true width=24 height=24
        input type="checkbox" checked=ghost change=haunt testId="quelled"
      box height=40
      column position="relative"
        input type="checkbox" checked=ghost change=haunt testId="halved"
        box position="absolute" top=-12 left=-12 width=20 height=48 background-color="#808080" testId="half"
      box height=40
      row width=60 testId="wrapper"
        input type="checkbox" checked=ghost change=haunt testId="wrapped"
      box height=40
      column rotate="45deg" overflow="hidden"
        input type="checkbox" checked=ghost change=haunt testId="turned"
"##;

pub(super) fn said(p: &Presenter<()>) -> String {
    let k = p.host().kernel();
    let node = k.node_by_key(k.find_by_test_id("said")[0]).unwrap();
    node.props.str(PropId::Text).unwrap_or_default().to_owned()
}

/// A finger down and up at (`x`, `y`), then a frame.
fn press(p: &mut Presenter<()>, x: f32, y: f32, at: f64) {
    p.pointer_down(x, y, at).unwrap();
    p.pointer_up(x, y, at + 50.0).unwrap();
    p.frame();
}

#[test]
fn materials_radio_and_touch_target_reach_past_the_drawn_control() {
    if !material_process() {
        return;
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(MARKS),
        (),
        (400., 700.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    // A finger, as the reader's device says.
    let phone = Device {
        night: false,
        font_scale: 1.0,
        locale: "en-US".into(),
        rtl: false,
        hour24: false,
        reduced_motion: false,
        bold_text: false,
        high_contrast: false,
        touch: true,
        tv: false,
        touch_slop: 8.0,
        dates: String::new(),
        format_locale: String::new(),
    };
    assert!(p.set_device(&phone).is_none());
    p.set_pointer_kind(crate::presenter::PointerKind::Touch);
    p.frame();
    // A radio is Material's: 24 laid out, its dot the accent, the page
    // inside an unselected ring.
    let accent = crate::paint::control::accents(None, false).0;
    let page = pixel(&mut p, 2.0, 2.0);
    let chosen = p.rect_of(id(&p, "chosen")).unwrap();
    assert_eq!((chosen.2, chosen.3), (24.0, 24.0));
    assert_eq!(pixel(&mut p, chosen.0 + 12.0, chosen.1 + 12.0), accent);
    let other = p.rect_of(id(&p, "other")).unwrap();
    assert_eq!(pixel(&mut p, other.0 + 12.0, other.1 + 12.0), page);
    // A press 10 below a checkbox's box, in nothing that takes a press,
    // checks it; what is hit there is still the page's, for hover and
    // scrolling.
    let near = p.rect_of(id(&p, "near")).unwrap();
    let below = (near.0 + 12.0, near.1 + near.3 + 10.0);
    assert_ne!(p.hit(below.0, below.1), Some(id(&p, "near")));
    press(&mut p, below.0, below.1, 1000.0);
    assert_eq!(said(&p), "true false false", "the press below checked it");
    // A press in the button beside it, inside its touch box, is the
    // button's.
    let beside = p.rect_of(id(&p, "beside")).unwrap();
    assert!(
        beside.0 < near.0 + near.2 + 12.0,
        "in the checkbox's touch box: {beside:?}"
    );
    press(&mut p, beside.0 + 1.0, beside.1 + 12.0, 2000.0);
    assert_eq!(
        said(&p),
        "true true false",
        "the button's, not the checkbox's"
    );
    // A hidden checkbox takes no press round it.
    let hidden = p.rect_of(id(&p, "hidden")).unwrap();
    press(&mut p, hidden.0 + 12.0, hidden.1 - 6.0, 3000.0);
    assert_eq!(
        said(&p),
        "true true false",
        "nothing reached the hidden one"
    );
    // A checkbox under a cover, one half covered pressed on its cover, one
    // in a disabled column, one turned, and one an agent taps beside by
    // naming its wrapper take nothing.
    let cover = p.rect_of(id(&p, "cover")).unwrap();
    press(&mut p, cover.0 + 2.0, cover.1 + 2.0, 4000.0);
    let quelled = p.rect_of(id(&p, "quelled")).unwrap();
    press(
        &mut p,
        quelled.0 + 12.0,
        quelled.1 + quelled.3 + 6.0,
        5000.0,
    );
    let half = p.rect_of(id(&p, "half")).unwrap();
    press(&mut p, half.0 + 16.0, half.1 + 24.0, 5500.0);
    // Turned, outside its clip but inside the square round it.
    let turned = p.rect_of(id(&p, "turned")).unwrap();
    let (cx, cy) = (turned.0 + turned.2 / 2.0, turned.1 + turned.3 / 2.0);
    press(&mut p, cx + 15.0, cy + 15.0, 5700.0);
    let wrapper = id(&p, "wrapper");
    let _ = p.tap(wrapper);
    p.frame();
    assert_eq!(
        said(&p),
        "true true false",
        "a covered, a disabled, a turned or an unnamed control took a press"
    );
    // An authored size draws the mark across its box.
    let large = p.rect_of(id(&p, "large")).unwrap();
    assert_eq!((large.2, large.3), (40.0, 40.0));
    assert_eq!(pixel(&mut p, large.0 + 20.0, large.1 + 4.0), accent);
}

/// Segmented groups: one sized by its radios, one the author made 300
/// wide, five, an authored `display`, a disabled radio, right to left, and
/// one that says `appearance: none`.
const SEGMENTS: &str = r##"component App
  state unit = "km"
  state five = 2
  action pick(u: string)
    unit = u
  action count(n: number)
    five = n
  view
    column padding=16 align-items="flex-start" gap=8
      row role="radiogroup" aria-label="Units" testId="units"
        for u in ["km", "mi", "nmi"] key=u
          button role="radio" aria-checked=(unit == u) press=pick(u) testId=`u-${u}`
            text u
      text `Chosen: ${unit}` testId="chosen"
      row role="radiogroup" aria-label="Wide" width=300 testId="wide"
        button role="radio" aria-checked=true testId="w-1"
          text "One"
        button role="radio" aria-checked=false testId="w-2"
          text "Two"
      row role="radiogroup" aria-label="Five" testId="five"
        for n in [1, 2, 3, 4, 5] key=n
          button role="radio" aria-checked=(five == n) press=count(n) testId=`f-${n}`
            text `${n}`
      row role="radiogroup" aria-label="Flex" display="flex" testId="flex"
        button role="radio" aria-checked=true testId="x-1"
          text "A"
        button role="radio" aria-checked=false testId="x-2"
          text "Longer"
      row role="radiogroup" aria-label="Off" testId="off"
        button role="radio" aria-checked=true disabled=true testId="d-1"
          text "One"
        button role="radio" aria-checked=false testId="d-2"
          text "Two"
      row role="radiogroup" aria-label="Back" direction="rtl" testId="rtl"
        button role="radio" aria-checked=true testId="r-1"
          text "One"
        button role="radio" aria-checked=false testId="r-2"
          text "Two"
      column width=360
        row role="radiogroup" aria-label="Stretch" testId="stretched"
          button role="radio" aria-checked=true testId="s-1"
            text "One"
          button role="radio" aria-checked=false testId="s-2"
            text "Two"
      box width=360
        row role="radiogroup" aria-label="Block" testId="block"
          button role="radio" aria-checked=true testId="b-1"
            text "One"
          button role="radio" aria-checked=false testId="b-2"
            text "Two"
      row role="radiogroup" aria-label="Own" appearance="none" testId="own"
        button role="radio" aria-checked=true testId="o-1"
          text "One"
        button role="radio" aria-checked=false testId="o-2"
          text "Two"
"##;

fn segments() -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(SEGMENTS),
        (),
        (400., 700.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    p
}

/// How many pixels inside `r` (a few points in from its edges) are ink:
/// darker than half.
fn ink_count(p: &mut Presenter<()>, r: (f32, f32, f32, f32)) -> usize {
    let frame = (*p.frame()).clone();
    let at = |v: f32| (v * 2.4375) as u32;
    let mut n = 0;
    for y in at(r.1 + 4.0)..at(r.1 + r.3 - 4.0) {
        for x in at(r.0 + 4.0)..at(r.0 + r.2 - 4.0) {
            n += usize::from(frame.pixel(x, y).unwrap().demultiply().red() < 128);
        }
    }
    n
}

/// The darkest red channel in `r`, points.
fn ink_in(p: &mut Presenter<()>, r: (f32, f32, f32, f32)) -> u8 {
    darkest(p, (r.0 - 4.0, r.1 - 4.0, r.2 + 8.0, r.3 + 8.0))
}

/// The darkest red channel inside `r`, a few points in from its edges:
/// how strong its ink is.
fn darkest(p: &mut Presenter<()>, r: (f32, f32, f32, f32)) -> u8 {
    let frame = (*p.frame()).clone();
    let at = |v: f32| (v * 2.4375) as u32;
    let mut darkest = 255u8;
    for y in at(r.1 + 4.0)..at(r.1 + r.3 - 4.0) {
        for x in at(r.0 + 4.0)..at(r.0 + r.2 - 4.0) {
            darkest = darkest.min(frame.pixel(x, y).unwrap().demultiply().red());
        }
    }
    darkest
}

/// A `radiogroup` of radios, each one text, is Material's segmented buttons
/// (`SingleChoiceSegmentedButtonRow`): each 40 tall and an equal share of
/// the row, neighbours sharing one outline, the row's ends round; the
/// checked one filled, its check before its label; a press checks another,
/// and an accessibility service is told which. With no width the group is
/// as wide as its widest segment makes them all, in a column that would
/// stretch it and in a block alike. What the author wrote wins: a width is
/// shared out, a `display` keeps its own layout, `appearance: none` keeps
/// the buttons.
#[test]
fn materials_segmented_buttons_share_the_row_and_fill_the_checked_one() {
    if !material_process() {
        return;
    }
    let mut p = segments();
    let rect = |p: &mut Presenter<()>, name: &str| p.rect_of(id(p, name)).unwrap();
    let page = pixel(&mut p, 2.0, 2.0);
    let units = rect(&mut p, "units");
    let [km, mi, nmi] = ["u-km", "u-mi", "u-nmi"].map(|n| rect(&mut p, n));
    for s in [km, mi, nmi] {
        assert_eq!(s.3, 40.0, "the segmented-button-height row: {s:?}");
        assert!((s.2 - km.2).abs() <= 1.0, "equal shares: {s:?} {km:?}");
    }
    assert_eq!(mi.0, km.0 + km.2 - 1.0, "one outline between them");
    assert_eq!(units.2, km.2 + mi.2 + nmi.2 - 2.0);
    for name in ["stretched", "block"] {
        let group = rect(&mut p, name);
        assert!(group.2 < 200.0, "{name} hugs its segments: {group:?}");
    }
    // An authored width is shared out.
    let (w1, w2) = (rect(&mut p, "w-1"), rect(&mut p, "w-2"));
    assert_eq!((w1.2, w2.2), (150.0, 151.0));
    assert_eq!(w2.0 + w2.2, w1.0 + 300.0, "the row's width");
    // Five, each at least `ButtonDefaults.MinWidth`.
    let five = ["f-1", "f-2", "f-3", "f-4", "f-5"].map(|n| rect(&mut p, n));
    for s in five {
        assert!(s.2 >= 58.0 && (s.2 - five[0].2).abs() <= 1.0, "{s:?}");
    }
    // An authored `display` keeps its own layout: each its own width, no
    // shared outline.
    let (x1, x2) = (rect(&mut p, "x-1"), rect(&mut p, "x-2"));
    assert_ne!(x1.2, x2.2);
    assert_eq!(x2.0, x1.0 + x1.2);
    // Right to left, the first is at the right, still sharing an outline.
    let (r1, r2) = (rect(&mut p, "r-1"), rect(&mut p, "r-2"));
    assert_eq!(r1.0, r2.0 + r2.2 - 1.0);
    assert_eq!(
        pixel(&mut p, r1.0 + r1.2 - 1.0, r1.1 + 1.0),
        page,
        "its round start"
    );
    assert_ne!(pixel(&mut p, r1.0 + 0.5, r1.1 + 0.5), page, "a square side");
    // `appearance: none` keeps the authored buttons.
    assert_ne!(rect(&mut p, "o-1").3, 40.0);
    // The row's ends are round, between them square.
    assert_eq!(pixel(&mut p, km.0 + 1.0, km.1 + 1.0), page, "a round start");
    assert_eq!(
        pixel(&mut p, nmi.0 + nmi.2 - 1.0, nmi.1 + 1.0),
        page,
        "a round end"
    );
    assert_ne!(
        pixel(&mut p, mi.0 + 0.5, mi.1 + 0.5),
        page,
        "a square corner"
    );
    // The checked one is filled, the others show the page; its check comes
    // before its label, which an unchecked one centres.
    let inside = |s: (f32, f32, f32, f32)| (s.0 + s.2 - 6.0, s.1 + 6.0);
    let at = inside(km);
    assert_ne!(pixel(&mut p, at.0, at.1), page);
    let at = inside(mi);
    assert_eq!(pixel(&mut p, at.0, at.1), page);
    let centre = |p: &mut Presenter<()>, name: &str| {
        let text = p.field_texts(id(p, name)).and_then(|t| t.label).unwrap();
        text.rect.0 + text.rect.2 / 2.0
    };
    assert!(centre(&mut p, "u-km") > km.0 + km.2 / 2.0 + 4.0);
    assert!((centre(&mut p, "u-mi") - (mi.0 + mi.2 / 2.0)).abs() < 1.0);
    // The check is drawn between the padding and the label.
    let label = p.field_texts(id(&p, "u-km")).and_then(|t| t.label).unwrap();
    let mark = (
        km.0 + 12.0,
        km.1 + 14.0,
        label.rect.0 - 2.0 - km.0 - 12.0,
        12.0,
    );
    let fill = pixel(&mut p, km.0 + km.2 - 6.0, km.1 + 6.0)[0];
    assert!(ink_in(&mut p, mark) < fill - 60, "its check");
    let inked = ink_count(&mut p, km);
    // Disabled, its label is `onSurface` at the token's 0.38 over the
    // fill, once.
    let d1 = rect(&mut p, "d-1");
    let fill = pixel(&mut p, d1.0 + d1.2 - 6.0, d1.1 + 6.0)[0];
    let expected = f32::from(fill) * (1.0 - 0.38);
    let ink = f32::from(darkest(&mut p, d1));
    assert!((ink - expected).abs() <= 4.0, "{ink} against {expected}");
    // Held, the ripple lies over an unchecked one, at rest after 450 ms.
    p.pointer_down(mi.0 + 20.0, mi.1 + 20.0, 1000.0).unwrap();
    p.clock(1500.0);
    let at = inside(mi);
    assert_ne!(pixel(&mut p, at.0, at.1), page, "the ripple");
    p.pointer_cancel(1050.0).ok();
    // A press checks another: the fill moves, and so does what an
    // accessibility service is told.
    press(&mut p, nmi.0 + nmi.2 / 2.0, nmi.1 + 20.0, 2000.0);
    p.pointer_move(390.0, 690.0, 2200.0).unwrap();
    p.frame();
    let k = p.host().kernel();
    let chosen = k.node_by_key(k.find_by_test_id("chosen")[0]).unwrap();
    assert_eq!(chosen.props.str(PropId::Text), Some("Chosen: nmi"));
    let at = inside(nmi);
    assert_ne!(pixel(&mut p, at.0, at.1), page);
    let at = inside(km);
    assert_eq!(pixel(&mut p, at.0, at.1), page);
    // Unchecked, the label alone: the check's ink is gone.
    assert!(ink_count(&mut p, km) + 100 < inked, "no check unchecked");
    use crate::presenter::access::{flag, role};
    let (nodes, _, _) = p.access_tree();
    let radios: Vec<_> = nodes
        .iter()
        .filter(|n| n.role == role::RADIO && ["km", "mi", "nmi"].contains(&n.name.as_str()))
        .map(|n| (n.name.as_str(), n.flags & (flag::CHECKABLE | flag::CHECKED)))
        .collect();
    let checkable = flag::CHECKABLE;
    assert_eq!(
        radios,
        [
            ("km", checkable),
            ("mi", checkable),
            ("nmi", checkable | flag::CHECKED)
        ]
    );
}

/// Two groups and what their layout reads, each switch one input: the
/// group's orientation and `appearance`, a radio's button style, role,
/// `display` and label, the second group's flex direction and its parent's
/// `align-items`. `S0`..`S7` are each switch's starting value.
const SWITCHED: &str = r##"component App
  state s0 = S0
  state s1 = S1
  state s2 = S2
  state s3 = S3
  state s4 = S4
  state s5 = S5
  state word = (S5 ? "" : "One")
  state s6 = S6
  state s7 = S7
  action flip(n: number)
    s0 = n == 0 ? !s0 : s0
    s1 = n == 1 ? !s1 : s1
    s2 = n == 2 ? !s2 : s2
    s3 = n == 3 ? !s3 : s3
    s4 = n == 4 ? !s4 : s4
    s5 = n == 5 ? !s5 : s5
    word = n == 5 ? (word == "" ? "One" : "") : word
    s6 = n == 6 ? !s6 : s6
    s7 = n == 7 ? !s7 : s7
  view
    column padding=16 align-items="flex-start" gap=8
      row role="radiogroup" aria-label="T" aria-orientation=(s0 ? "vertical" : "horizontal") appearance=(s1 ? "none" : "auto") width=300 testId="g"
        button role="radio" aria-checked=true aria-label="One" testId="t-1"
          text word
        button role="radio" aria-checked=false buttonStyle=(s2 ? "plain" : "bordered") testId="t-2"
          text "Two"
        button role=(s3 ? "button" : "radio") aria-checked=false display=(s4 ? "none" : "flex") testId="t-3"
          text "Three"
      column width=360 align-items=(s7 ? "center" : "stretch") testId="holder"
        row role="radiogroup" aria-label="U" flex-direction=(s6 ? "row-reverse" : "row") testId="g2"
          button role="radio" aria-checked=true testId="u-1"
            text "One"
          button role="radio" aria-checked=false testId="u-2"
            text "Two"
      row gap=8
        for n in [0, 1, 2, 3, 4, 5, 6, 7] key=n
          button press=flip(n) testId=`flip-${n}`
            text `${n}`
"##;

/// How many switches [`SWITCHED`] has.
const SWITCHES: usize = 8;

/// [`SWITCHED`] with each switch's starting value.
fn switched(start: [bool; SWITCHES]) -> String {
    let mut source = SWITCHED.to_owned();
    for (n, on) in start.iter().enumerate() {
        source = source.replace(&format!("S{n}"), &on.to_string());
    }
    source
}

/// The boxes of both groups and their radios, and the frame's pixels over
/// each group, at `scale` pixels a point.
fn group_picture(p: &mut Presenter<()>, scale: f32) -> (Vec<Option<Rect4>>, Vec<u8>) {
    let names = ["g", "t-1", "t-2", "t-3", "g2", "u-1", "u-2"];
    let boxes: Vec<_> = names.iter().map(|n| p.rect_of(id(p, n))).collect();
    let frame = (*p.frame()).clone();
    let at = |v: f32| (v * scale) as u32;
    let mut pixels = Vec::new();
    for g in [boxes[0], boxes[4]].into_iter().flatten() {
        for y in at(g.1)..at(g.1 + g.3 + 2.0).min(frame.height()) {
            for x in at(g.0)..at(g.0 + g.2 + 2.0).min(frame.width()) {
                let c = frame.pixel(x, y).unwrap();
                pixels.extend([c.red(), c.green(), c.blue()]);
            }
        }
    }
    (boxes, pixels)
}

/// Each input the layout reads, switched either way on a mounted tree,
/// lays the groups out and paints them as a tree mounted that way does.
#[test]
fn materials_segmented_group_follows_a_change_that_moves_its_eligibility() {
    if !material_process() {
        return;
    }
    let boot = |start: [bool; SWITCHES]| {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(&switched(start)),
            (),
            (400., 400.),
            2.4375,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        p
    };
    // Every switch off, both are segmented: 40 tall, the second hugging.
    let mut base = boot([false; SWITCHES]);
    assert_eq!(base.rect_of(id(&base, "t-1")).unwrap().3, 40.0);
    assert!(base.rect_of(id(&base, "g2")).unwrap().2 < 200.0);
    for n in 0..SWITCHES {
        for from in [false, true] {
            let mut start = [false; SWITCHES];
            start[n] = from;
            let mut p = boot(start);
            p.tap(id(&p, &format!("flip-{n}"))).unwrap();
            p.pointer_move(399.0, 399.0, 5000.0).unwrap();
            let moved = group_picture(&mut p, 2.4375);
            start[n] = !from;
            let mut fresh = boot(start);
            fresh.pointer_move(399.0, 399.0, 5000.0).unwrap();
            let mounted = group_picture(&mut fresh, 2.4375);
            assert_eq!(moved.0, mounted.0, "switch {n} from {from}: boxes");
            assert!(moved.1 == mounted.1, "switch {n} from {from}: paint");
        }
    }
}

/// What the rule sets is beneath what the author wrote: a flex direction
/// written on the group keeps the author's layout, its ends where it lays
/// the segments; a radio's corner style is its own.
#[test]
fn materials_segments_keep_what_the_author_wrote_and_their_visual_order() {
    if !material_process() {
        return;
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(
            r##"component App
  view
    column padding=16 align-items="flex-start" gap=8
      row role="radiogroup" aria-label="R" flex-direction="row-reverse" testId="rev"
        button role="radio" aria-checked=true testId="v-1"
          text "One"
        button role="radio" aria-checked=false testId="v-2"
          text "Two"
      row role="radiogroup" aria-label="C" testId="corner"
        button role="radio" aria-checked=true -exact-corner-style="small" testId="c-1"
          text "One"
        button role="radio" aria-checked=false testId="c-2"
          text "Two"
"##,
        ),
        (),
        (400., 300.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let page = pixel(&mut p, 2.0, 2.0);
    let rect = |p: &mut Presenter<()>, name: &str| p.rect_of(id(p, name)).unwrap();
    // Reversed as written: the first radio at the right, each its own
    // width, no shared outline; the left end round on the second.
    let (v1, v2) = (rect(&mut p, "v-1"), rect(&mut p, "v-2"));
    assert_eq!(v1.0, v2.0 + v2.2, "{v1:?} {v2:?}");
    assert_eq!(
        pixel(&mut p, v2.0 + 1.0, v2.1 + 1.0),
        page,
        "round at the left"
    );
    assert_eq!(
        pixel(&mut p, v1.0 + v1.2 - 1.0, v1.1 + 1.0),
        page,
        "and the right"
    );
    // A corner style written on a radio is its own: small, not the end's.
    let c1 = rect(&mut p, "c-1");
    assert_ne!(pixel(&mut p, c1.0 + 1.0, c1.1 + 1.0), page);
}

/// A group that is itself a grouped list's row spans the card as the row
/// does, as the web's sheet keeps it (`host/web/neutral.css`): its row
/// box is the plain row's.
#[test]
fn materials_segmented_group_that_is_a_grouped_row_spans_the_card() {
    if !material_process() {
        return;
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(GROUPED),
        (),
        (400., 300.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let (plain, group) = (
        p.rect_of(id(&p, "plain")).unwrap(),
        p.rect_of(id(&p, "units")).unwrap(),
    );
    assert_eq!((group.0, group.2), (plain.0, plain.2));
}

/// A grouped section: a plain row, a `radiogroup` row and another row.
const GROUPED: &str = r##"component App
  view
    list appearance="auto" listStyle="inset-grouped" width=360
      section
        text "Plain" testId="plain"
        row role="radiogroup" aria-label="Units" testId="units"
          button role="radio" aria-checked=true testId="g-km"
            text "km"
          button role="radio" aria-checked=false testId="g-mi"
            text "mi"
        text "After" testId="after"
"##;

/// An authored press background is the held segment's ground, and an
/// authored press colour its content's.
#[test]
fn materials_segment_takes_an_authored_press_colour() {
    if !material_process() {
        return;
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(
            r##"component App
  state n = 0
  action go
    n = n + 1
  view
    column padding=16 align-items="flex-start"
      row role="radiogroup" aria-label="P"
        button role="radio" aria-checked=true press=go testId="p-1"
          text "One"
        button role="radio" aria-checked=false press=go testId="p-2"
          :active
            background-color="#ff0000"
            color="#0000ff"
          text "Two"
"##,
        ),
        (),
        (400., 300.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let r = p.rect_of(id(&p, "p-2")).unwrap();
    p.pointer_down(r.0 + 10.0, r.1 + 20.0, 1000.0).unwrap();
    p.frame();
    assert_eq!(pixel(&mut p, r.0 + 6.0, r.1 + 6.0), [255, 0, 0, 255]);
    // The label takes the press colour once it has eased in.
    p.clock(1300.0);
    let label = p.field_texts(id(&p, "p-2")).and_then(|t| t.label).unwrap();
    let frame = (*p.frame()).clone();
    let at = |v: f32| (v * 2.4375) as u32;
    let mut blue = false;
    for y in at(label.rect.1)..at(label.rect.1 + label.rect.3) {
        for x in at(label.rect.0)..at(label.rect.0 + label.rect.2) {
            let c = frame.pixel(x, y).unwrap().demultiply();
            blue |= c.blue() > 200 && c.red() < 60 && c.green() < 60;
        }
    }
    assert!(blue, "the label in the press colour");
}

/// A radio that sizes itself along the row keeps the author's flex layout:
/// no equal columns. A checked face laid out as a column puts its check
/// beside the column, its label where a row face's is, either direction.
#[test]
fn materials_segments_keep_a_radios_own_size_and_a_columns_check() {
    if !material_process() {
        return;
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(
            r##"component App
  view
    column padding=16 align-items="flex-start" gap=8
      row role="radiogroup" aria-label="S" width=300 testId="sized"
        button role="radio" aria-checked=true flex-basis=120 flex-grow=0 flex-shrink=0 testId="s-1"
          text "One"
        button role="radio" aria-checked=false flex=1 testId="s-2"
          text "Two"
      for d in ["ltr", "rtl"] key=d
        row role="radiogroup" aria-label="L" direction=d width=300
          button role="radio" aria-checked=true testId=`row-${d}`
            text "One"
          button role="radio" aria-checked=false
            text "Two"
        row role="radiogroup" aria-label="C" direction=d width=300
          button role="radio" aria-checked=true flex-direction="column" testId=`column-${d}`
            text "One"
          button role="radio" aria-checked=false
            text "Two"
"##,
        ),
        (),
        (400., 400.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let (s1, s2) = (
        p.rect_of(id(&p, "s-1")).unwrap(),
        p.rect_of(id(&p, "s-2")).unwrap(),
    );
    assert_eq!((s1.2, s2.2), (120.0, 180.0), "{s1:?} {s2:?}");
    for d in ["ltr", "rtl"] {
        let label = |p: &mut Presenter<()>, name: &str| {
            let r = p.rect_of(id(p, name)).unwrap();
            let t = p.field_texts(id(p, name)).and_then(|t| t.label).unwrap();
            (t.rect.0 - r.0, t.rect.2)
        };
        let row = label(&mut p, &format!("row-{d}"));
        let column = label(&mut p, &format!("column-{d}"));
        assert!((row.0 - column.0).abs() < 0.5, "{d}: {row:?} {column:?}");
    }
}

/// A group narrower than its segments' own widths shares what it has
/// equally, its last edge inside it; it hugs its segments in a grid as in
/// a column or a block, and a written width wider than what holds it is
/// clamped to it. A hidden radio's own width does not count.
#[test]
fn materials_segmented_group_fits_what_holds_it() {
    if !material_process() {
        return;
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(
            r##"component App
  view
    column padding=16 align-items="flex-start" gap=8
      row role="radiogroup" aria-label="Narrow" width=200 testId="narrow"
        for n in [1, 2, 3, 4, 5] key=n
          button role="radio" aria-checked=(n == 1) testId=`n-${n}`
            text `${n}`
      box display="grid" width=360 testId="grid-box"
        row role="radiogroup" aria-label="Grid" testId="in-grid"
          button role="radio" aria-checked=true
            text "km"
          button role="radio" aria-checked=false
            text "mi"
      box width=360
        row role="radiogroup" aria-label="Wide" width=500 testId="too-wide"
          button role="radio" aria-checked=true
            text "km"
          button role="radio" aria-checked=false
            text "mi"
      row role="radiogroup" aria-label="Hidden" testId="hidden"
        button role="radio" aria-checked=true testId="h-1"
          text "km"
        button role="radio" aria-checked=false testId="h-2"
          text "mile"
        button role="radio" aria-checked=false display="none" width=80
          text "nmi"
      row role="radiogroup" aria-label="Long" width=200 testId="long"
        button role="radio" aria-checked=true testId="l-1"
          text "A label far too long to fit"
        button role="radio" aria-checked=false
          text "B"
"##,
        ),
        (),
        (400., 600.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    let rect = |p: &mut Presenter<()>, name: &str| p.rect_of(id(p, name)).unwrap();
    let narrow = rect(&mut p, "narrow");
    let five = ["n-1", "n-2", "n-3", "n-4", "n-5"].map(|n| rect(&mut p, n));
    for s in five {
        assert!((s.2 - five[0].2).abs() <= 1.0, "equal: {s:?} {:?}", five[0]);
    }
    assert!(
        five[4].0 + five[4].2 <= narrow.0 + narrow.2,
        "{:?} in {narrow:?}",
        five[4]
    );
    for name in ["in-grid", "too-wide"] {
        let group = rect(&mut p, name);
        assert!(group.2 <= 360.0, "{name}: {group:?}");
    }
    assert!(rect(&mut p, "in-grid").2 < 200.0, "it hugs in a grid");
    assert_eq!(
        rect(&mut p, "too-wide").2,
        360.0,
        "clamped to what holds it"
    );
    let (h1, h2) = (rect(&mut p, "h-1"), rect(&mut p, "h-2"));
    assert!((h1.2 - h2.2).abs() <= 1.0, "equal: {h1:?} {h2:?}");
    // A label that no longer fits its share wraps, as an ordinary
    // button's title does, and the segment grows to hold it.
    let long = rect(&mut p, "l-1");
    assert_eq!(long.2, 100.0);
    assert!(long.3 > 40.0, "{long:?}");
}
