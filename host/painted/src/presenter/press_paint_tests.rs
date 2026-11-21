//! LLP 1061 D7 on the Linux host (and so on Android's and Windows's, which
//! paint through this presenter): a contact holding a node shows its
//! pressed paint, eased on the press's curve, and lets it go when it leaves
//! or lifts; the press colour reaches the text that takes the node's colour,
//! not one with its own.
use super::*;

const APP: &str = r##"component App
  state presses = 0
  action go
    presses = presses + 1
  view
    column width=400 height=400 background-color="#ffffff"
      box press=go -exact-press-opacity=0.5 background-color="#00ff00" color="#000000" width=300 height=120 testId="painted"
        :active
          background-color="#0000ff"
          color="#ff0000"
        text "IIIIIIII" font-size=40 testId="inherits"
        text "IIIIIIII" font-size=40 color="#000000" testId="keeps"
"##;

fn boot() -> Presenter<()> {
    boot_source(APP)
}

fn boot_source(source: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(source)
            .unwrap_or_else(|e| panic!("{e}"))
            .encode(),
        (),
        (400., 400.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn rect(p: &mut Presenter<()>, test_id: &str) -> (f32, f32, f32, f32) {
    let k = p.host().kernel();
    let id = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
    p.rect_of(id).unwrap()
}

fn pixel(p: &mut Presenter<()>, x: f32, y: f32) -> [u8; 3] {
    let c = p.frame().pixel(x as u32, y as u32).unwrap().demultiply();
    [c.red(), c.green(), c.blue()]
}

/// Whether any pixel of the box `r` is the press colour's red, over the
/// pressed ground at half opacity.
fn reddened(p: &mut Presenter<()>, r: (f32, f32, f32, f32)) -> bool {
    let frame = p.frame();
    (r.1 as u32..(r.1 + r.3) as u32).any(|y| {
        (r.0 as u32..(r.0 + r.2) as u32).any(|x| {
            let c = frame.pixel(x, y).unwrap().demultiply();
            c.red() > 200 && c.green() < 160 && c.blue() < 200
        })
    })
}

#[test]
fn a_held_contact_shows_the_pressed_paint_and_lets_it_go() {
    let mut p = boot();
    let b = rect(&mut p, "painted");
    let (inherits, keeps) = (rect(&mut p, "inherits"), rect(&mut p, "keeps"));
    // A point of the box's own ground, right of the texts.
    let ground = (b.0 + b.2 - 4., b.1 + 4.);
    assert_eq!(pixel(&mut p, ground.0, ground.1), [0, 255, 0]);
    assert!(!reddened(&mut p, inherits));

    assert!(p.pointer_down(ground.0, ground.1, 0.).unwrap());
    p.clock(30.);
    let part = pixel(&mut p, ground.0, ground.1);
    assert!(part[2] > 0 && part[2] < 255, "eased in: {part:?}");
    p.clock(120.);
    // Blue at half opacity over the white page.
    let held = pixel(&mut p, ground.0, ground.1);
    for (c, want) in held.into_iter().zip([127, 127, 255]) {
        assert!(c.abs_diff(want) <= 1, "{held:?}");
    }
    assert!(
        reddened(&mut p, inherits),
        "the text takes the press colour"
    );
    assert!(
        !reddened(&mut p, keeps),
        "a text with its own colour keeps it"
    );

    // Off the box it goes; back inside it shows again; the lift lets it go.
    p.pointer_move(b.0 + b.2 + 20., ground.1, 120.).unwrap();
    p.clock(240.);
    assert_eq!(pixel(&mut p, ground.0, ground.1), [0, 255, 0]);
    p.pointer_move(ground.0, ground.1, 240.).unwrap();
    p.clock(360.);
    assert_eq!(pixel(&mut p, ground.0, ground.1), held);
    p.pointer_up(ground.0, ground.1, 360.).unwrap();
    p.clock(480.);
    assert_eq!(pixel(&mut p, ground.0, ground.1), [0, 255, 0]);
    assert!(!reddened(&mut p, inherits));
    assert!(!p.host.motion(), "the paint needs no more frames");
}

/// Under the agent's clock the paint lands at once both ways (LLP 1061
/// D2): a read between `tap X down` and `tap up` sees it held.
#[test]
fn the_agents_contact_shows_the_paint_at_once() {
    let mut p = boot();
    let b = rect(&mut p, "painted");
    let ground = (b.0 + b.2 - 4., b.1 + 4.);
    let k = p.host().kernel();
    let id = k.node_by_key(k.find_by_test_id("painted")[0]).unwrap().id;
    let down = crate::agent::handle(
        &mut p,
        &format!(
            r#"{{"op":"tap","phase":"down","id":{id},"x":{},"y":{}}}"#,
            ground.0, ground.1
        ),
    );
    assert!(!down.contains("error"), "{down}");
    let held = pixel(&mut p, ground.0, ground.1);
    assert!(held[0].abs_diff(127) <= 1 && held[2] == 255, "{held:?}");
    let up = crate::agent::handle(&mut p, r#"{"op":"tap","phase":"up"}"#);
    assert!(!up.contains("error"), "{up}");
    assert_eq!(pixel(&mut p, ground.0, ground.1), [0, 255, 0]);
    assert!(!p.host.motion());
}

/// LLP 1115 D4: a platform field's text is its own colour, else FieldText;
/// a pressed ancestor's press colour does not reach it, as its `color`
/// doesn't. A field of the author's own (`appearance="none"`) takes it.
#[test]
fn a_pressed_ancestor_leaves_a_native_fields_ink() {
    const FIELDS: &str = r##"component App
  state words = "IIIIIIII"
  action go
    words = words
  action write(value: string)
    words = value
  view
    column width=400 height=400 background-color="#ffffff"
      box press=go color="#000000" width=300 height=200 testId="painted"
        :active
          color="#ff0000"
        input value=words change=write font-size=40 aria-label="Native" testId="native"
        input value=words change=write font-size=40 appearance="none" aria-label="Own" testId="own"
"##;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(FIELDS)
            .unwrap_or_else(|e| panic!("{e}"))
            .encode(),
        (),
        (400., 400.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let b = rect(&mut p, "painted");
    let (native, own) = (rect(&mut p, "native"), rect(&mut p, "own"));
    assert!(!reddened(&mut p, native) && !reddened(&mut p, own));
    let ground = (b.0 + b.2 - 4., b.1 + b.3 - 4.);
    assert!(p.pointer_down(ground.0, ground.1, 0.).unwrap());
    p.clock(200.);
    assert!(
        reddened(&mut p, own),
        "the author's field takes the press colour"
    );
    assert!(!reddened(&mut p, native), "a native field keeps its ink");
}

/// LLP 1104 D4, LLP 1115 D4: an ancestor's moving colour stops at a
/// platform field as its colour does; a field of the author's own takes it.
#[test]
fn an_ancestors_moving_colour_stops_at_a_native_field() {
    const FIELDS: &str = r##"component App
  state red = false
  state words = "IIIIIIII"
  action go
    red = !red
  action write(value: string)
    words = value
  view
    column width=400 height=400 background-color="#ffffff"
      box press=go color=(red ? "#ff0000" : "#000000") transition="color 1s" width=300 height=200 testId="painted"
        input value=words change=write font-size=40 aria-label="Native" testId="native"
        input value=words change=write font-size=40 appearance="none" aria-label="Own" testId="own"
"##;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(FIELDS)
            .unwrap_or_else(|e| panic!("{e}"))
            .encode(),
        (),
        (400., 400.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let b = rect(&mut p, "painted");
    let (native, own) = (rect(&mut p, "native"), rect(&mut p, "own"));
    let ground = (b.0 + b.2 - 4., b.1 + b.3 - 4.);
    assert!(p.pointer_down(ground.0, ground.1, 0.).unwrap());
    p.pointer_up(ground.0, ground.1, 10.).unwrap();
    p.clock(900.);
    assert!(p.host.motion(), "the colour still moves");
    assert!(
        reddened(&mut p, own),
        "the author's field takes the moving colour"
    );
    assert!(!reddened(&mut p, native), "a native field keeps its ink");
}

/// LLP 1121 D8 as amended: a link with no `press` is pressed as CSS's
/// `:active` holds on a link, its press shown while held and let go at the
/// lift; the lift still follows its `href`. A `text` link run is no target.
#[test]
fn a_link_with_no_press_shows_its_active_paint() {
    const LINK: &str = r##"component App
  view
    column width=400 height=400 background-color="#ffffff"
      link href="https://example.com/a" background-color="#00ff00" width=300 height=120 testId="link"
        :active
          background-color="#0000ff"
        text "Away"
      link href="https://example.com/b" width=300 height=120 testId="bare"
        text "Bare"
"##;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(LINK)
            .unwrap_or_else(|e| panic!("{e}"))
            .encode(),
        (),
        (400., 400.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = opened.clone();
    p.set_url_opener(move |url| {
        sink.borrow_mut().push(url.to_owned());
        Ok(())
    });
    let b = rect(&mut p, "link");
    let ground = (b.0 + b.2 - 4., b.1 + 4.);
    assert_eq!(pixel(&mut p, ground.0, ground.1), [0, 255, 0]);
    let key = |p: &Presenter<()>, name| p.host().kernel().find_by_test_id(name)[0];
    p.pointer_down(ground.0, ground.1, 0.).unwrap();
    p.clock(120.);
    assert_eq!(pixel(&mut p, ground.0, ground.1), [0, 0, 255], "held");
    assert!(p.host().pressed(key(&p, "link")));
    p.pointer_up(ground.0, ground.1, 130.).unwrap();
    p.clock(400.);
    assert_eq!(pixel(&mut p, ground.0, ground.1), [0, 255, 0], "let go");
    assert_eq!(*opened.borrow(), ["https://example.com/a"], "followed once");
    // An unsaid link is pressed too: the host's press holds it.
    let bare = rect(&mut p, "bare");
    let (x, y) = (bare.0 + 10., bare.1 + 10.);
    p.pointer_down(x, y, 500.).unwrap();
    assert!(p.host().pressed(key(&p, "bare")));
    p.pointer_up(x, y, 520.).unwrap();
    assert!(!p.host().pressed(key(&p, "bare")));
    assert_eq!(opened.borrow().len(), 2);
}

/// A link that shrinks while held (its `-exact-press-scale`) is followed
/// when a finger lifts where it went down, near its edge: the lift is
/// judged against the box it had unpressed, as any press's is.
#[test]
fn a_shrinking_link_is_followed_from_its_unpressed_box() {
    const LINK: &str = r##"component App
  view
    column width=400 height=400 background-color="#ffffff"
      link href="https://example.com/a" -exact-press-scale=0.5 background-color="#00ff00" width=300 height=120 testId="link"
        text "Away"
"##;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(LINK)
            .unwrap_or_else(|e| panic!("{e}"))
            .encode(),
        (),
        (400., 400.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.set_pointer_kind(crate::presenter::PointerKind::Touch);
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = opened.clone();
    p.set_url_opener(move |url| {
        sink.borrow_mut().push(url.to_owned());
        Ok(())
    });
    let b = rect(&mut p, "link");
    let edge = (b.0 + b.2 - 4., b.1 + 4.);
    p.pointer_down(edge.0, edge.1, 0.).unwrap();
    p.clock(300.);
    p.frame();
    assert_ne!(
        p.hit(edge.0, edge.1),
        Some(
            p.host()
                .kernel()
                .node_by_key(p.host().kernel().find_by_test_id("link")[0])
                .unwrap()
                .id
        ),
        "held, the link has shrunk away from the finger"
    );
    p.pointer_up(edge.0, edge.1, 300.).unwrap();
    assert_eq!(*opened.borrow(), ["https://example.com/a"], "followed once");
}

#[test]
fn active_translate_adds_to_the_base_and_releases_from_the_unpressed_box() {
    for direct in [false, true] {
        let mut p = boot_source(
            r##"component App
  state n = 0
  action go
    n = n + 1
  view
    view width=400 height=400 background-color="#ffffff"
      view press=go position="absolute" left=170 top=160 translate="30px 40px" width=100 height=80 background-color="#00ff00" testId="slab"
        :active
          translate = "20% 14px"
"##,
        );
        p.agent_finger(direct);
        let base = rect(&mut p, "slab");
        let point = (base.0 + 2., base.1 + 2.);
        p.pointer_down(point.0, point.1, 0.).unwrap();
        p.clock(30.);
        let partial = rect(&mut p, "slab");
        assert!(partial.0 > base.0 && partial.0 < base.0 + 20.);
        p.clock(120.);
        assert_eq!(
            rect(&mut p, "slab"),
            (base.0 + 20., base.1 + 14., base.2, base.3)
        );
        assert_eq!(pixel(&mut p, point.0, point.1), [255, 255, 255]);
        assert_eq!(pixel(&mut p, base.0 + 22., base.1 + 16.), [0, 255, 0]);
        // The stationary finger is outside the translated paint, but still inside its tracking box.
        p.pointer_move(point.0, point.1, 120.).unwrap();
        p.clock(240.);
        assert_eq!(rect(&mut p, "slab").0, base.0 + 20.);
        p.pointer_up(point.0, point.1, 240.).unwrap();
        p.clock(360.);
        assert_eq!(rect(&mut p, "slab"), base);
        assert_eq!(pixel(&mut p, point.0, point.1), [0, 255, 0]);
        let state: serde_json::Value =
            serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
        assert_eq!(state["slots"]["n"], 1, "the stationary lift activates once");
        p.pointer_down(point.0, point.1, 400.).unwrap();
        p.clock(520.);
        p.pointer_up(base.0 + base.2 + 2., point.1 + 14., 520.)
            .unwrap();
        p.clock(640.);
        let state: serde_json::Value =
            serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
        assert_eq!(state["slots"]["n"], 1, "lifting outside cancels the press");
    }
}

#[test]
fn active_translate_release_remeasures_after_a_scroll() {
    for (direct, at_new_location) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut p = boot_source(
            r##"component App
  state n = 0
  action go
    n = n + 1
  view
    scroll testId="port" width=400 height=400
      view height=160
      view press=go width=100 height=80 background-color="#00ff00" testId="slab"
        :active
          translate = "4px 4px"
      view height=800
"##,
        );
        p.agent_finger(direct);
        let base = rect(&mut p, "slab");
        let port = p
            .host
            .kernel()
            .node_by_key(p.host.kernel().find_by_test_id("port")[0])
            .unwrap()
            .id;
        p.pointer_down(base.0 + 2., base.1 + 2., 0.).unwrap();
        p.clock(120.);
        assert_eq!(p.scroll_to(port, (0., 100.)), Some((0., 100.)));
        assert_eq!(rect(&mut p, "slab").1, base.1 - 96.);
        let y = base.1 + 2. - if at_new_location { 100. } else { 0. };
        p.pointer_move(base.0 + 2., y, 120.).unwrap();
        p.clock(240.);
        assert_eq!(
            rect(&mut p, "slab").1,
            base.1 - 100. + if at_new_location { 4. } else { 0. }
        );
        p.pointer_up(base.0 + 2., y, 240.).unwrap();
        let state: serde_json::Value =
            serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
        assert_eq!(state["slots"]["n"], u32::from(at_new_location));
    }
}

#[test]
fn active_translate_none_clears_both_rows_without_an_invalid_value_and_restores_pixels() {
    let mut p = boot_source(
        r##"component App
  state offset = "20% 14px"
  action clear
    offset = "none"
  action go
    offset = offset
  view
    view width=400 height=400 background-color="#ffffff"
      view press=go position="absolute" left=100 top=100 width=100 height=80 background-color="#00ff00" testId="slab"
        :active
          translate = offset
      button "Clear" press=clear testId="clear"
"##,
    );
    p.pointer_down(102., 102., 0.).unwrap();
    p.clock(120.);
    assert_eq!(pixel(&mut p, 102., 102.), [255, 255, 255]);
    assert_eq!(pixel(&mut p, 122., 116.), [0, 255, 0]);
    let clear = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("clear")[0])
        .unwrap()
        .id;
    assert_eq!(p.host.dispatch_at(clear, Event::Press, 120.), None);
    assert_eq!(p.after_commit(), None);
    p.clock(240.);
    assert_eq!(pixel(&mut p, 102., 102.), [0, 255, 0]);
    assert_eq!(pixel(&mut p, 202., 182.), [255, 255, 255]);
    let node = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("slab")[0])
        .unwrap();
    for row in [
        exact_kernel::StyleId::PressTranslate,
        exact_kernel::StyleId::PressTranslatePercent,
    ] {
        assert!(!node.style.mask.has(row));
    }
    let logs = p.host.agent(r#"{"op":"logs"}"#);
    assert!(!logs.contains("invalid"), "{logs}");
}

#[test]
fn hit_slop_uses_the_current_safe_area() {
    let mut p = boot_source(
        r##"component App
  view
    view width=400 height=400
      view position="absolute" left=100 top=100 width=100 height=80 background-color="#cc3300" -exact-hit-slop-left="env(safe-area-inset-left)" testId="slab"
"##,
    );
    let kernel = p.host.kernel();
    let id = kernel
        .node_by_key(kernel.find_by_test_id("slab")[0])
        .unwrap()
        .id;

    for left in [0., 10., 0.] {
        assert!(p.set_safe_area([0., 0., 0., left]).is_none());
        let (x, y, ..) = p.rect_of(id).unwrap();
        assert_eq!(p.hit(x + 5., y + 5.), Some(id));
        assert_eq!(p.hit(x - 5., y + 5.) == Some(id), left > 0.);
    }
}

#[test]
fn active_translate_preserves_hit_slop_on_release() {
    for direct in [false, true] {
        let mut p = boot_source(
            r##"component App
  state n = 0
  action go
    n = n + 1
  view
    view width=400 height=400
      view press=go position="absolute" left=100 top=100 width=100 height=80 -exact-hit-slop=10 testId="slab"
        :active
          translate = "20px 20px"
"##,
        );
        p.agent_finger(direct);
        p.pointer_down(95., 120., 0.).unwrap();
        p.clock(120.);
        p.pointer_move(95., 120., 120.).unwrap();
        p.clock(240.);
        assert_eq!(rect(&mut p, "slab").0, 120.);
        p.pointer_up(95., 120., 240.).unwrap();
        let state: serde_json::Value =
            serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
        assert_eq!(state["slots"]["n"], 1);
    }
}

#[test]
fn active_translate_keeps_the_nested_button_release_ancestor_walk() {
    let mut p = boot_source(
        r##"component App
  state n = 0
  action outer
    n = n + 1
  action inner
    n = n + 10
  view
    view width=400 height=400
      view press=outer position="absolute" left=100 top=100 width=200 height=100 testId="outer"
        :active
          translate = "20px 20px"
        button "Inner" appearance="none" press=inner width=60 height=40 testId="inner"
          :active
            translate = "20px 20px"
"##,
    );
    p.agent_finger(false);
    p.pointer_down(102., 102., 0.).unwrap();
    p.clock(120.);
    p.pointer_move(250., 120., 120.).unwrap();
    p.pointer_up(250., 120., 120.).unwrap();
    let state: serde_json::Value =
        serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
    assert_eq!(state["slots"]["n"], 1);
}

#[test]
fn active_translate_projective_hits_stay_at_the_base_geometry() {
    for direct in [false, true] {
        let mut p = boot_source(
            r##"component App
  state n = 0
  action go
    n = n + 1
  view
    view width=400 height=400 perspective=500
      view press=go position="absolute" left=100 top=100 translate="0px 0px 1px" width=100 height=80 testId="slab"
        :active
          translate = "20px 20px"
        view width=50 height=50 testId="child"
"##,
        );
        p.agent_finger(direct);
        let base = rect(&mut p, "slab");
        let point = (base.0 + 2., base.1 + 2.);
        let hit = p.hit(point.0, point.1);
        p.pointer_down(point.0, point.1, 0.).unwrap();
        p.clock(120.);
        assert!(rect(&mut p, "slab").0 > base.0 + 19.);
        assert_eq!(p.hit(point.0, point.1), hit);
        p.pointer_move(point.0, point.1, 120.).unwrap();
        p.clock(240.);
        assert!(rect(&mut p, "slab").0 > base.0 + 19.);
        p.pointer_up(point.0, point.1, 240.).unwrap();
        let state: serde_json::Value =
            serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
        assert_eq!(state["slots"]["n"], 1);
    }
}

#[test]
fn active_translate_keeps_shape_clips_in_the_unshifted_hit_space() {
    for spatial in ["", "translate=\"0px 0px 1px\""] {
        let mut p = boot_source(&format!(
            r##"component App
  action go
    let value = 1
  view
    view width=400 height=400 perspective=500
      view press=go position="absolute" left=100 top=100 {spatial} width=100 height=80 clip-path="inset(0px 10px 0px 10px)" testId="slab"
        :active
          translate = "20px 20px"
        view width=100 height=80 testId="child"
"##
        ));
        let base = rect(&mut p, "slab");
        let inside = (base.0 + 12., base.1 + 12.);
        let outside = (base.0 + 2., base.1 + 12.);
        let hit = p.hit(inside.0, inside.1);
        let clipped = p.hit(outside.0, outside.1);
        assert_ne!(hit, clipped);
        p.pointer_down(inside.0, inside.1, 0.).unwrap();
        p.clock(120.);
        assert_eq!(p.hit(inside.0, inside.1), hit);
        assert_eq!(p.hit(outside.0, outside.1), clipped);
    }
}

#[test]
fn a_scaled_clipped_press_tracks_its_stationary_edge_and_current_ancestor_clip() {
    for shift in ["", "\n        :active\n          translate = \"4px 4px\""] {
        for move_first in [false, true] {
            let mut p = boot_source(&format!(
                r#"component App
  state n = 0
  state cut = false
  action go
    n = n + 1
  action clip
    cut = true
  view
    view width=400 height=400 clip-path=(cut ? "inset(0px 0px 0px 110px)" : "inset(0px)")
      view press=go position="absolute" left=100 top=100 width=100 height=80 clip-path="inset(0px)" -exact-press-scale=0.5 testId="slab"{shift}
      button "Clip" press=clip testId="clip"
"#
            ));
            p.set_pointer_kind(crate::presenter::PointerKind::Touch);
            p.pointer_down(102., 102., 0.).unwrap();
            p.clock(120.);
            p.frame();
            assert!(
                p.contact_press().is_some(),
                "the edge stays in the tracking clip"
            );
            if move_first {
                p.pointer_move(102., 102., 120.).unwrap();
                p.clock(240.);
                assert!(
                    rect(&mut p, "slab").2 < 51.,
                    "the stationary edge keeps its press"
                );
            }
            p.pointer_up(102., 102., 240.).unwrap();
            let state: serde_json::Value =
                serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
            assert_eq!(state["slots"]["n"], 1, "the stationary edge activates");
            p.clock(360.);
            p.pointer_down(102., 102., 360.).unwrap();
            p.clock(480.);
            let clip = p
                .host
                .kernel()
                .node_by_key(p.host.kernel().find_by_test_id("clip")[0])
                .unwrap()
                .id;
            assert_eq!(p.host.dispatch_at(clip, Event::Press, 480.), None);
            assert_eq!(p.after_commit(), None);
            p.frame();
            p.pointer_up(102., 102., 480.).unwrap();
            let state: serde_json::Value =
                serde_json::from_str(&p.host.agent(r#"{"op":"state"}"#)).unwrap();
            assert_eq!(
                state["slots"]["n"], 1,
                "the current ancestor clip cancels activation"
            );
        }
    }
}

#[test]
fn active_translate_keeps_its_absolute_containing_block() {
    for offset in ["\"4px 4px\"", "offset"] {
        let mut p = boot_source(&format!(
            r##"component App
  state offset = "none"
  action go
    offset = (offset == "none" ? "4px 4px" : "none")
  view
    view position="relative" width=400 height=400 background-color="#ffffff"
      view height=40
      view press=go testId="slab" width=100 height=80
        :active
          translate = {offset}
        view position="absolute" top=0 width=10 height=10 background-color="#ff0000" testId="child"
"##
        ));
        for time in [0., 240., 480.] {
            assert_eq!(rect(&mut p, "slab").1, 40.);
            assert_eq!(rect(&mut p, "child").1, 40.);
            assert_eq!(pixel(&mut p, 2., 42.), [255, 0, 0]);
            assert!(p.pointer_down(50., 60., time).unwrap());
            p.clock(time + 120.);
            assert_eq!(rect(&mut p, "child").1, rect(&mut p, "slab").1);
            let k = p.host().kernel();
            let child = k.node_by_key(k.find_by_test_id("child")[0]).unwrap();
            assert_eq!(child.frame.y, 40.);
            p.pointer_up(50., 60., time + 120.).unwrap();
            p.clock(time + 240.);
        }
    }
}
