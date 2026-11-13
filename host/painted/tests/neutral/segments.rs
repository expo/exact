use super::*;

const SEGMENTS: &str = r##"component App
  state unit = "km"
  action pick(u: string)
    unit = u
  view
    column width=400 padding=20 gap=10 align-items="flex-start"
      row role="radiogroup" aria-label="Units" testId="units"
        for u in ["km", "mi", "nmi"] key=u
          button role="radio" aria-checked=(unit == u) press=pick(u) testId=`u-${u}`
            text u
      text `Chosen: ${unit}` testId="chosen"
      column width=360
        row role="radiogroup" aria-label="Two" testId="stretched"
          button role="radio" aria-checked=true testId="s-1"
            text "One"
          button role="radio" aria-checked=false disabled=true testId="s-2"
            text "Two"
      row role="radiogroup" aria-label="Five" testId="five"
        for n in [1, 2, 3, 4, 5] key=n
          button role="radio" aria-checked=(n == 5) testId=`f-${n}`
            text `${n}`
      row role="radiogroup" aria-label="Back" direction="rtl" testId="rtl"
        button role="radio" aria-checked=true testId="r-1"
          text "One"
        button role="radio" aria-checked=false testId="r-2"
          text "Two"
      row role="radiogroup" aria-label="Own" padding=6 background-color="#abcdef" testId="own"
        button role="radio" aria-checked=true testId="o-1"
          text "One"
      row role="radiogroup" aria-label="Stacked" display="flex" flex-direction="column" testId="stacked"
        button role="radio" aria-checked=true testId="k-1"
          text "One"
        button role="radio" aria-checked=false testId="k-2"
          text "Two"
      row role="radiogroup" aria-label="Up" aria-orientation="vertical" testId="vertical"
        button role="radio" aria-checked=true testId="v-1"
          text "One"
      box width=40 height=20 background-color="-exact-tertiary-fill" testId="track-ref"
      box width=40 height=20 background-color="-exact-background" testId="chip-ref"
"##;

/// A `radiogroup` of radios is the web's neutral segmented control
/// (`host/web/neutral.css`, "A segmented control"): a track as wide as its
/// segments, 3 px of it round them, each segment the control's height less
/// that; the checked one a chip of the page's ground, the others on the
/// track; a press moves the chip. What the author wrote of the track wins,
/// and a vertical group stays as written.
pub(super) fn radio_groups_are_the_webs_segmented_control() {
    let plan = contract::compile(SEGMENTS).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (400.0, 600.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    // Read under its own roles: the test's other sessions live too.
    let _roles = p.host().runner().bind_roles();
    assert!(error.is_none(), "{error:?}");
    let rect = |p: &mut Presenter<()>, name: &str| p.rect_of(id(p, name)).unwrap();
    let track = pixel(&mut p, "track-ref", 5.0, 5.0);
    let chip = pixel(&mut p, "chip-ref", 5.0, 5.0);
    assert_ne!(track, chip);

    let units = rect(&mut p, "units");
    assert_eq!(units.3, 36.0, "the control's height");
    let [km, mi, nmi] = ["u-km", "u-mi", "u-nmi"].map(|n| rect(&mut p, n));
    for s in [km, mi, nmi] {
        assert_eq!(
            s.3, 30.0,
            "the control's height less the track's padding: {s:?}"
        );
        assert_eq!(s.1, units.1 + 3.0);
    }
    assert_eq!(km.0, units.0 + 3.0);
    assert_eq!(
        nmi.0 + nmi.2,
        units.0 + units.2 - 3.0,
        "as wide as its segments"
    );
    assert_ne!(km.2, nmi.2, "each its own width");
    // The track round the segments; the chip under the checked one.
    assert_eq!(pixel(&mut p, "units", 1.0, 18.0), track);
    assert_eq!(pixel(&mut p, "u-km", 3.0, 15.0), chip);
    assert_eq!(pixel(&mut p, "u-mi", 3.0, 15.0), track);
    // Its label the label's colour, an unchecked one the secondary label's:
    // the chip's darkest ink is darker.
    let darkest = |p: &mut Presenter<()>, name: &str| {
        let pixels = text_pixels(p, name);
        pixels.iter().map(|c| c[0]).min().unwrap()
    };
    assert!(darkest(&mut p, "u-km") < darkest(&mut p, "u-mi"));
    // In a column that would stretch it, it still hugs its segments.
    assert!(rect(&mut p, "stretched").2 < 200.0);
    assert_eq!(
        pixel(&mut p, "s-2", 3.0, 15.0),
        track,
        "disabled, on the track"
    );
    // Five, the last checked; right to left, the first at the right.
    assert_eq!(pixel(&mut p, "f-5", 3.0, 15.0), chip);
    assert_eq!(pixel(&mut p, "f-1", 3.0, 15.0), track);
    assert!(rect(&mut p, "r-1").0 > rect(&mut p, "r-2").0);
    assert_eq!(pixel(&mut p, "r-1", 3.0, 15.0), chip);
    // What the author wrote of the track wins.
    let own = rect(&mut p, "own");
    assert_eq!(rect(&mut p, "o-1").0, own.0 + 6.0);
    assert_eq!(pixel(&mut p, "own", 3.0, 18.0), [0xab, 0xcd, 0xef]);
    // An authored layout stays the author's, on the track.
    let (k1, k2) = (rect(&mut p, "k-1"), rect(&mut p, "k-2"));
    assert!(k2.1 >= k1.1 + k1.3, "one under the other: {k1:?} {k2:?}");
    assert_eq!(pixel(&mut p, "stacked", 1.0, 18.0), track);
    assert_eq!(pixel(&mut p, "k-1", 3.0, 15.0), chip);
    // A vertical group is no segmented control.
    assert_eq!(size(&mut p, "v-1").1, 36.0);
    assert_eq!(size(&mut p, "vertical").1, 36.0);

    // Under a pointer an unchecked label takes the label's colour, and
    // held it draws no wash: the web's sheet gives a radio none.
    let mi = rect(&mut p, "u-mi");
    let before = darkest(&mut p, "u-mi");
    p.pointer_move(mi.0 + 10.0, mi.1 + 10.0, 1000.0).unwrap();
    p.frame();
    assert!(darkest(&mut p, "u-mi") < before, "the hovered label");
    p.pointer_down(mi.0 + 10.0, mi.1 + 10.0, 1100.0).unwrap();
    p.frame();
    assert_eq!(pixel(&mut p, "u-mi", 3.0, 15.0), track, "no wash held");
    p.pointer_cancel(1150.0).unwrap();
    p.pointer_move(399.0, 599.0, 1200.0).unwrap();
    // A press moves the chip.
    p.tap(id(&p, "u-nmi")).unwrap();
    p.frame();
    let k = p.host().kernel();
    let chosen = k.node_by_key(k.find_by_test_id("chosen")[0]).unwrap();
    assert_eq!(
        chosen.props.str(exact_kernel::PropId::Text),
        Some("Chosen: nmi")
    );
    assert_eq!(pixel(&mut p, "u-nmi", 3.0, 15.0), chip);
    assert_eq!(pixel(&mut p, "u-km", 3.0, 15.0), track);
}

/// Two groups and what their layout reads, as the Material look's test
/// of the same name in `presenter/material_tests.rs` switches them.
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
const SWITCHES: usize = 8;

fn boot(source: &str) -> Presenter<()> {
    let plan = contract::compile(source).unwrap();
    let (p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (400.0, 400.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn switched(start: [bool; SWITCHES]) -> String {
    let mut source = SWITCHED.to_owned();
    for (n, on) in start.iter().enumerate() {
        source = source.replace(&format!("S{n}"), &on.to_string());
    }
    source
}

/// The boxes of both groups and their radios, and the pixels over each group.
type Picture = (Vec<Option<(f32, f32, f32, f32)>>, Vec<[u8; 3]>);

fn group_picture(p: &mut Presenter<()>) -> Picture {
    let names = ["g", "t-1", "t-2", "t-3", "g2", "u-1", "u-2"];
    let boxes: Vec<_> = names
        .iter()
        .map(|n| {
            let view = id(p, n);
            p.rect_of(view)
        })
        .collect();
    let mut pixels = text_pixels(p, "g");
    pixels.extend(text_pixels(p, "g2"));
    (boxes, pixels)
}

/// Each input the layout reads, switched either way on a mounted tree,
/// lays the groups out and paints them as a tree mounted that way does.
pub(super) fn segmented_group_follows_a_change_that_moves_its_eligibility() {
    // Every switch off, both are segmented: 30 tall on their tracks.
    let mut base = boot(&switched([false; SWITCHES]));
    let _roles = base.host().runner().bind_roles();
    assert_eq!(size(&mut base, "t-1").1, 30.0);
    assert_eq!(size(&mut base, "g").0, 300.0, "the track inside its width");
    assert!(size(&mut base, "g2").0 < 200.0);
    drop(_roles);
    for n in 0..SWITCHES {
        for from in [false, true] {
            let mut start = [false; SWITCHES];
            start[n] = from;
            let mut p = boot(&switched(start));
            let _roles = p.host().runner().bind_roles();
            p.tap(id(&p, &format!("flip-{n}"))).unwrap();
            p.pointer_move(399.0, 399.0, 5000.0).unwrap();
            let moved = group_picture(&mut p);
            drop(_roles);
            start[n] = !from;
            let mut fresh = boot(&switched(start));
            let _roles = fresh.host().runner().bind_roles();
            fresh.pointer_move(399.0, 399.0, 5000.0).unwrap();
            let mounted = group_picture(&mut fresh);
            assert_eq!(moved.0, mounted.0, "switch {n} from {from}: boxes");
            assert!(moved.1 == mounted.1, "switch {n} from {from}: paint");
        }
    }
}

/// What a segment's face holds is drawn as an ordinary button draws it: a
/// symbol beside the title, a subtitle under it, an authored alignment and
/// padding, an authored press colour. A group that is itself a grouped
/// list's row spans the card with no track, its separator to the card's
/// edge (`host/web/neutral.css`). Under a contrast palette the checked
/// radio is the highlight, every segment edged.
pub(super) fn segment_faces_are_the_buttons() {
    let mut p = boot(
        r##"component App
  view
    column width=400 padding=20 gap=10 align-items="flex-start"
      row role="radiogroup" aria-label="Faces"
        button role="radio" aria-checked=true testId="plain"
          text "Go"
        button role="radio" aria-checked=false testId="title"
          text "Go"
        button role="radio" aria-checked=false testId="symbol"
          image "symbol:star"
          text "Go"
        button role="radio" aria-checked=false testId="subtitle"
          text "Go"
          text "Later"
      row role="radiogroup" aria-label="Laid"
        button role="radio" aria-checked=true width=150 text-align="left" testId="left"
          text "Go"
        button role="radio" aria-checked=false padding-top=20 testId="low"
          text "Go"
        button role="radio" aria-checked=false testId="held"
          :active
            background-color="#ff0000"
          text "Go"
      row role="radiogroup" aria-label="Own"
        button role="radio" aria-checked=true flex-basis=120 flex-grow=0 flex-shrink=0 testId="based"
          text "Go"
        button role="radio" aria-checked=false flex=1 testId="flexed"
          text "Go"
        button role="radio" aria-checked=true -exact-corner-style="capsule" testId="capsule"
          text "Go"
      row role="radiogroup" aria-label="Off"
        button role="radio" aria-checked=true disabled=true testId="off-1"
          text "Go"
        button role="radio" aria-checked=false disabled=true testId="off-2"
          text "Go"
      list appearance="auto" listStyle="inset-grouped" width=360
        section
          text "Plain" testId="row-plain"
          row role="radiogroup" aria-label="Units" testId="row-group"
            button role="radio" aria-checked=true
              text "km"
            button role="radio" aria-checked=false
              text "mi"
          text "After" testId="row-after"
"##,
    );
    let _roles = p.host().runner().bind_roles();
    let rect = |p: &mut Presenter<()>, name: &str| p.rect_of(id(p, name)).unwrap();
    // Inked columns and rows of a box.
    let inked = |p: &mut Presenter<()>, name: &str| {
        let (_, _, w, _) = rect(p, name);
        let pixels = text_pixels(p, name);
        let w = w as usize;
        let dark: Vec<(usize, usize)> = (pixels.iter().enumerate())
            .filter(|(_, c)| c[0] < 200)
            .map(|(i, _)| (i % w, i / w))
            .collect();
        dark
    };
    // The symbol beside the title, the subtitle under it.
    let (title, symbol) = (rect(&mut p, "title"), rect(&mut p, "symbol"));
    assert!(symbol.2 > title.2 + 10.0, "{symbol:?} {title:?}");
    let (ink, title_ink) = (inked(&mut p, "symbol").len(), inked(&mut p, "title").len());
    assert!(
        ink > title_ink + 20,
        "the symbol beside the title: {ink} {title_ink}"
    );
    // The subtitle under the title: its ink spans about one line more than
    // a title's alone.
    let span = |p: &mut Presenter<()>, name: &str| {
        let rows: Vec<usize> = inked(p, name).iter().map(|d| d.1).collect();
        (rows.iter().max().unwrap() - rows.iter().min().unwrap()) as f32
    };
    let more = span(&mut p, "subtitle") - span(&mut p, "title");
    assert!(more > 10.0, "the subtitle under the title: {more}");
    // An authored alignment and an authored padding.
    let first = inked(&mut p, "left").iter().map(|d| d.0).min().unwrap();
    assert!(first < 20, "the title at the start: {first}");
    let top = inked(&mut p, "low").iter().map(|d| d.1).min().unwrap();
    assert!(top >= 20, "under the authored padding: {top}");
    // Held, the authored press colour.
    let held = rect(&mut p, "held");
    p.pointer_down(held.0 + 10.0, held.1 + 10.0, 1000.0)
        .unwrap();
    p.frame();
    assert_eq!(pixel(&mut p, "held", 4.0, 4.0), [255, 0, 0]);
    p.pointer_cancel(1050.0).unwrap();
    p.pointer_move(399.0, 599.0, 1100.0).unwrap();
    // A radio's own flex basis and corner style are its own.
    assert_eq!(size(&mut p, "based").0, 120.0);
    let capsule = rect(&mut p, "capsule");
    let track = pixel(&mut p, "capsule", capsule.2 / 2.0, -2.0);
    assert_eq!(
        pixel(&mut p, "capsule", 2.0, 2.0),
        track,
        "a capsule's corner"
    );
    // A grouped row spans the card, with no track and the row's separator.
    let (row, group) = (rect(&mut p, "row-plain"), rect(&mut p, "row-group"));
    assert_eq!((group.0, group.2), (row.0, row.2));
    let ground = pixel(&mut p, "row-plain", row.2 - 4.0, row.3 / 2.0);
    assert_eq!(
        pixel(&mut p, "row-group", group.2 - 4.0, group.3 / 2.0),
        ground
    );
    assert_eq!(
        pixel(&mut p, "row-group", group.2 - 1.0, group.3 - 0.5),
        pixel(&mut p, "row-plain", row.2 - 1.0, row.3 - 0.5),
        "the separator to the same end"
    );
    // A contrast palette: the checked one the highlight, each edged.
    let palette = exact_painted::presenter::ContrastPalette {
        window: [0, 0, 0, 255],
        text: [255; 4],
        link: [117, 233, 252, 255],
        disabled: [166, 166, 166, 255],
        highlight: [255, 255, 0, 255],
        highlighted_text: [0, 0, 0, 255],
        button: [0, 0, 0, 255],
        button_text: [255; 4],
    };
    p.set_contrast_palette(Some(palette));
    assert_eq!(pixel(&mut p, "plain", 4.0, 4.0), [255, 255, 0]);
    assert_eq!(pixel(&mut p, "subtitle", 4.0, 4.0), [0, 0, 0]);
    assert_eq!(
        pixel(&mut p, "subtitle", 0.0, 15.0),
        [255, 255, 255],
        "its edge"
    );
    // A pointer over an unchecked one gives it none of the selected pair.
    let over = rect(&mut p, "subtitle");
    p.pointer_move(over.0 + 10.0, over.1 + 10.0, 2000.0)
        .unwrap();
    p.frame();
    assert_eq!(
        pixel(&mut p, "subtitle", 4.0, 4.0),
        [0, 0, 0],
        "hovered, not selected"
    );
    p.pointer_move(399.0, 599.0, 2100.0).unwrap();
    // Disabled, the checked one is still the selected item.
    assert_eq!(pixel(&mut p, "off-1", 4.0, 4.0), [255, 255, 0]);
    assert_eq!(pixel(&mut p, "off-2", 4.0, 4.0), [0, 0, 0]);
    p.set_contrast_palette(None);
}
