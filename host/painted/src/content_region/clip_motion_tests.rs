use super::*;

/// LLP 1003.003 D7: a retained region's picture of a box whose clip a
/// transition moves is taken again as the clip moves, so hits (and pixels)
/// follow the clip as it shows, not the clip the picture first captured.
#[test]
fn a_retained_region_follows_a_moving_clip() {
    let _service = crate::content_region::test_service();
    let source = r#"component App
  state grown = false
  action grow
    grown = true
  view
    view id="owner" width=200 height=200 overflow="hidden"
      view id="content" width=200 height=200
        view width=100 height=100 testId="card" press=grow clip-path=(grown ? "path('M0 0H100V100H0Z')" : "path('M0 0H100V20H0Z')") transition="clip-path 400ms linear"
      text "pending" id="pending" position="absolute"
"#;
    let (mut p, error) = Presenter::boot_region_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (200., 200.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    ready(&mut p);
    p.frame();
    let card = id(&p, "card");
    assert_eq!(p.hit(50., 10.), Some(card));
    assert_ne!(p.hit(50., 50.), Some(card), "outside the clip, 20 high");
    p.tap_at(card, Some((50., 10.))).unwrap();
    ready(&mut p);
    p.frame();
    // Each frame of the run is a publication the region takes when it lands
    p.tick(200.);
    ready(&mut p);
    p.frame();
    assert_eq!(p.hit(50., 50.), Some(card), "half way, the clip is 60 high");
    assert_ne!(p.hit(50., 80.), Some(card));
    p.tick(500.);
    ready(&mut p);
    p.frame();
    assert_eq!(p.hit(50., 80.), Some(card), "settled, 100 high");
}

/// LLP 1003.003 D7: a picture taken again as a clip moves keeps the actions
/// it captured, so a press begun on the picture a submitted repaint replaces
/// ends on the replacement.
#[test]
fn a_press_across_a_moving_clips_repaint_ends_on_it() {
    let _service = crate::content_region::test_service();
    let source = r#"component App
  state grown = false
  state marked = false
  action grow
    grown = true
  action mark
    marked = true
  view
    view id="owner" width=200 height=200 overflow="hidden"
      view id="content" width=200 height=200
        view width=100 height=100 testId="card" press=mark clip-path=(grown ? "path('M0 0H100V100H0Z')" : "path('M0 0H100V20H0Z')") transition="clip-path 400ms linear"
        view position="absolute" left=100 top=0 width=100 height=100 testId="grow" press=grow
        view position="absolute" left=100 top=100 width=100 height=100 testId="marked" display=(marked ? "flex" : "none")
      text "pending" id="pending" position="absolute"
"#;
    let (mut p, error) = Presenter::boot_region_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (200., 200.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    ready(&mut p);
    // Every frame from here is one the display shows, as a host's are
    let shown = |p: &mut Presenter<()>| {
        let frame = p.display_frame().unwrap();
        assert!(p.display_complete(&frame));
    };
    shown(&mut p);
    p.tap_at(id(&p, "grow"), None).unwrap();
    ready(&mut p);
    shown(&mut p);
    p.tick(100.);
    ready(&mut p);
    shown(&mut p);
    // The repaint for the next sample is submitted before the press begins,
    // and the display shows it before the press ends
    p.tick(200.);
    ready(&mut p);
    let next = p.display_frame().unwrap();
    assert_eq!(p.hit(50., 10.), Some(id(&p, "card")));
    p.pointer_down(50., 10., 200.).unwrap();
    assert!(p.display_complete(&next));
    p.pointer_up(50., 10., 210.).unwrap();
    ready(&mut p);
    shown(&mut p);
    assert_eq!(p.hit(150., 150.), Some(id(&p, "marked")), "the press ran");
}

/// LLP 1003.003 D7: a picture taken again as a clip moves captures the
/// actions of a node the kept picture did not reach, here a child under a
/// box that was transparent when the publication first painted.
#[test]
fn a_child_a_fade_reveals_during_a_moving_clip_is_hit() {
    let _service = crate::content_region::test_service();
    let source = r#"component App
  state shown = false
  action show
    shown = true
  view
    view id="owner" width=200 height=200 overflow="hidden"
      view id="content" width=200 height=200
        view width=100 height=100 opacity=(shown ? 1 : 0) clip-path=(shown ? "path('M0 0H100V100H0Z')" : "path('M0 0H100V20H0Z')") transition="clip-path 2s linear, opacity 1s linear 500ms"
          view width=100 height=100 testId="inside" background-color="red"
        view position="absolute" left=100 top=0 width=100 height=100 testId="show" press=show
      text "pending" id="pending" position="absolute"
"#;
    let (mut p, error) = Presenter::boot_region_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (200., 200.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    ready(&mut p);
    p.frame();
    p.tap_at(id(&p, "show"), None).unwrap();
    ready(&mut p);
    p.frame();
    // Through the fade's delay the box is transparent and its child unpainted
    p.tick(200.);
    ready(&mut p);
    p.frame();
    p.tick(1000.);
    ready(&mut p);
    p.frame();
    let inside = id(&p, "inside");
    assert_eq!(p.hit(50., 10.), Some(inside), "fading in");
    p.tick(2500.);
    ready(&mut p);
    p.frame();
    assert_eq!(p.hit(50., 80.), Some(inside), "settled");
}
