use exact_painted::{presenter::PainterChoice, Presenter};
use std::path::PathBuf;

const FIXTURE: &str = include_str!("../../../../runner/tests/fixtures/per-axis-scale.contract");

fn boot(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &contract::compile(source).unwrap().encode(),
        (),
        (800., 700.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    p
}
fn id(p: &Presenter<()>, name: &str) -> u32 {
    let k = p.host().kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

#[test]
fn per_axis_scale_matches_chrome_pixels_hits_and_scroll_extent() {
    let mut p = boot(FIXTURE);
    let (port, overhang) = (id(&p, "port"), id(&p, "overhang"));
    let f = p.host().kernel().node(port).unwrap().frame;
    let extent = p
        .scrollers_at(f.x + 5., f.y + 5.)
        .chain
        .into_iter()
        .find(|s| s.id == port)
        .unwrap();
    assert_eq!(extent.max, (40., 105.), "Chrome scroll size is 160 by 225");
    for (scroll, left, top) in [((0., 0.), 0., 25.), ((20., 80.), -20., -55.)] {
        assert_eq!(p.scroll_to(port, scroll), Some(scroll));
        let image = p.frame();
        for y in (5..120).step_by(10) {
            for x in (5..120).step_by(10) {
                let blue = (x as f32) >= left
                    && (x as f32) < left + 160.
                    && (y as f32) >= top
                    && (y as f32) < top + 200.;
                let point = (f.x + x as f32, f.y + y as f32);
                assert_eq!(
                    p.hit(point.0, point.1) == Some(overhang),
                    blue,
                    "hit at {point:?}, scroll {scroll:?}"
                );
                // Native scrollbars can cover the last few pixels.
                if x < 100 && y < 100 {
                    let pixel = image.pixel(point.0 as u32, point.1 as u32).unwrap();
                    assert_eq!(
                        [pixel.red(), pixel.green(), pixel.blue()],
                        if blue { [0, 0, 255] } else { [255, 255, 255] }
                    );
                }
            }
        }
    }
}

#[test]
fn per_axis_scale_negative_and_collapsed_axes_keep_finite_bounds() {
    for scale in ["-1 0.5", "1 0", "0 1"] {
        let source = FIXTURE.replace("scale=\"1 0.5\"", &format!("scale=\"{scale}\""));
        let mut p = boot(&source);
        let (port, child) = (id(&p, "port"), id(&p, "overhang"));
        let f = p.host().kernel().node(port).unwrap().frame;
        let max = p
            .scrollers_at(f.x + 5., f.y + 5.)
            .chain
            .into_iter()
            .find(|s| s.id == port)
            .map_or((0., 0.), |s| s.max);
        assert!(max.0.is_finite() && max.1.is_finite());
        assert_eq!(
            max,
            if scale == "-1 0.5" {
                (0., 105.)
            } else {
                (0., 0.)
            }
        );
        for y in (5..120).step_by(10) {
            for x in (5..120).step_by(10) {
                let hits = p.hit(f.x + x as f32, f.y + y as f32) == Some(child);
                assert_eq!(hits, scale == "-1 0.5" && x < 100 && y >= 25);
            }
        }
    }
}

#[test]
fn per_axis_scale_motion_and_press_keep_y_only_changes() {
    let mut p = boot(&FIXTURE.replace("both paused", "both"));
    assert!(p.advance(500.).is_none());
    let animated = id(&p, "animated");
    assert_eq!(p.host().presented(animated).scale, (1., 0.5));
    let toggle = id(&p, "toggle");
    p.tap(toggle).unwrap();
    assert!(p.advance(1000.).is_none());
    let transitioned = id(&p, "transitioned");
    assert_eq!(p.host().presented(transitioned).scale, (2., 0.5));
    assert!(p.advance(1500.).is_none());
    assert_eq!(p.host().presented(transitioned).scale, (3., 0.));
    let pressed = id(&p, "pressed");
    let frame = p.host().kernel().node(pressed).unwrap().frame;
    p.pointer_down(
        frame.x + frame.width / 2.,
        frame.y + frame.height / 2.,
        1500.,
    )
    .unwrap();
    // The press scale moves on the node's own `scale 1s linear`.
    assert!(p.advance(2000.).is_none());
    assert!((p.host().presented(pressed).press - 0.75).abs() < 1e-5);
    assert!(p.advance(2500.).is_none());
    let value = p.host().presented(pressed);
    assert_eq!(value.scale, (2., 0.25));
    assert!((value.press - 0.5).abs() < 1e-5);
    assert_eq!(
        (value.scale.0 * value.press, value.scale.1 * value.press),
        (1., 0.125)
    );
    p.pointer_up(
        frame.x + frame.width / 2.,
        frame.y + frame.height / 2.,
        2500.,
    )
    .unwrap();
}

#[test]
fn scroll_extent_ignores_translate_rotate_and_press_but_tracks_scale_holds() {
    use exact_motion::{HoldEnd, Property, Value};
    let source = r#"component App
  state n = 0
  action noop
    n = n + 1
  view
    view testId="port" width=120 height=120 overflow="auto"
      view testId="child" width=100 height=100 press=noop -exact-press-scale=0.5
        view width=160 height=400
"#;
    let extent = |p: &mut Presenter<()>| {
        p.frame();
        let port = id(p, "port");
        let f = p.host().kernel().node(port).unwrap().frame;
        p.scrollers_at(f.x + 5., f.y + 5.)
            .chain
            .into_iter()
            .find(|s| s.id == port)
            .unwrap()
            .max
    };
    let baseline = extent(&mut boot(source));
    assert_eq!(baseline, (40., 280.));
    for row in ["translate=\"50px 70px\"", "rotate=45"] {
        let mut p = boot(&source.replace(
            "width=100 height=100 press",
            &format!("width=100 height=100 {row} press"),
        ));
        assert_eq!(extent(&mut p), baseline, "{row}");
    }
    let mut p = boot(source);
    let child = id(&p, "child");
    let f = p.host().kernel().node(child).unwrap().frame;
    p.pointer_down(f.x + 50., f.y + 50., 0.).unwrap();
    p.advance(100.);
    assert_eq!(extent(&mut p), baseline);
    p.pointer_cancel(100.).unwrap();
    let held = p
        .host_mut()
        .hold_begin(child, Property::Scale, 100.)
        .unwrap()
        .unwrap();
    p.host_mut()
        .hold_update(held.token, Value::new(1., 0.5), 110.)
        .unwrap();
    assert_eq!(extent(&mut p), (40., 105.));
    p.host_mut()
        .hold_update(held.token, Value::new(1., 1.), 120.)
        .unwrap();
    assert_eq!(extent(&mut p), baseline);
    p.host_mut()
        .hold_end(held.token, HoldEnd::Cancel, 120.)
        .unwrap();
}
