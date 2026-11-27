//! Direct Linux's bounded submission queue, over real painted pictures and input.
use super::super::PanelImage;
use super::*;

type Frame = SubmittedFrame<PanelImage>;

const ROWS: &str = r##"component App
  state selected = -1
  action select(row)
    selected = row
  view
    column width=320 height=240
      scroll testId="port" width=320 height=160
        for row in [0, 1, 2, 3, 4, 5, 6, 7] key=row
          box testId=`row-${row}` press=select(row) height=40 background-color=(row % 2 == 0 ? "#cc3300" : "#225599")
      text `${selected}` testId="selected" height=20
"##;

fn shown(p: &mut Presenter<Empty>, frame: &Frame, at: f64) {
    assert!(p.panel_complete(frame));
    assert!(p.display_presented(frame, at).is_none());
}

fn ready(source: &str) -> (Fixture, Frame) {
    let (mut p, paints, fail) = boot_app(source);
    let a = p.panel_frame_ahead(true).unwrap();
    shown(&mut p, &a, 0.);
    ((p, paints, fail), a)
}

fn wheel(p: &mut Presenter<Empty>, by: f32) {
    p.wheel(id(p, "port"), 0., by).unwrap();
}

fn selected(p: &Presenter<Empty>) -> f64 {
    p.host
        .runner()
        .slot("selected")
        .unwrap()
        .as_number()
        .unwrap()
}

fn tap(p: &mut Presenter<Empty>, at: f64) {
    p.pointer_down(100., 20., at).unwrap();
    p.pointer_up(100., 20., at + 1.).unwrap();
}

#[test]
fn ahead_keeps_a_hits_until_b_then_c_really_show_and_never_queues_a_third() {
    let ((mut p, paints, _), a) = ready(ROWS);
    wheel(&mut p, 40.);
    let b = p.panel_frame_ahead(true).unwrap();
    wheel(&mut p, 40.);
    assert!(p.panel_can_prepare_ahead());
    let c = p.panel_frame_ahead(true).unwrap();
    let pixels = [
        a.mirror().unwrap(),
        b.mirror().unwrap(),
        c.mirror().unwrap(),
    ]
    .map(|p| p.data().to_vec());
    assert_ne!(pixels[0], pixels[1]);
    assert_ne!(pixels[1], pixels[2]);
    assert!(!p.panel_can_prepare_ahead());
    assert!(p.panel_frame_ahead(true).is_none());
    assert!(p.panel_frame(true, true).is_none());
    assert!(p.display_frame().is_none());
    assert_eq!(paints.get(), 3);
    assert!(!p.panel_complete(&c), "C cannot bypass B");

    tap(&mut p, 10.);
    assert_eq!(selected(&p), 0., "release uses the visible A row");
    shown(&mut p, &b, 16.);
    tap(&mut p, 20.);
    assert_eq!(selected(&p), 1., "B owns hits while C is still pending");
    assert!(!p.panel_complete(&a));
    assert!(!p.panel_complete(&b));
    shown(&mut p, &c, 32.);
    tap(&mut p, 40.);
    assert_eq!(selected(&p), 2.);
    assert!(p.dirty(), "input after C's paint still owes a picture");
    let d = p.panel_frame_ahead(true).unwrap();
    shown(&mut p, &d, 48.);
    assert!(!p.dirty(), "the final input converges without idle repaint");
    for (frame, pixels) in [&a, &b, &c].into_iter().zip(pixels) {
        assert_eq!(frame.mirror().unwrap().data(), pixels);
        assert!(frame.identity.picture.borrow().is_none());
    }
}

#[test]
fn ahead_is_opt_in_and_a_failed_first_pixel_never_opens_the_second_slot() {
    let (mut p, _, fail) = boot();
    fail.set(true);
    let failed = p.panel_frame_ahead(true).unwrap();
    assert!(!p.panel_can_prepare_ahead());
    assert!(p.panel_frame_ahead(true).is_none());
    shown(&mut p, &failed, 0.);
    assert!(!p.painted);
    fail.set(false);
    let first = p.panel_frame_ahead(true).unwrap();
    assert!(!p.panel_can_prepare_ahead());
    shown(&mut p, &first, 16.);
    for direct in [false, true] {
        let ordinary = p.panel_frame(direct, true).unwrap();
        assert!(!p.panel_can_prepare_ahead());
        assert!(p.panel_frame_ahead(true).is_none());
        shown(&mut p, &ordinary, 32.);
    }
    let ordinary = p.display_frame().unwrap();
    assert!(p.panel_frame_ahead(true).is_none());
    assert!(p.display_complete(&ordinary));
}

#[test]
fn losing_the_head_invalidates_its_ahead_but_losing_only_ahead_keeps_the_head() {
    for lose_head in [true, false] {
        let ((mut p, _, _), _) = ready(ROWS);
        wheel(&mut p, 40.);
        let b = p.panel_frame_ahead(true).unwrap();
        wheel(&mut p, 40.);
        let c = p.panel_frame_ahead(true).unwrap();
        p.panel_lost(if lose_head { &b } else { &c });
        assert!(p.dirty());
        assert!(c.identity.picture.borrow().is_none());
        assert!(!p.panel_complete(&c));
        if lose_head {
            assert!(b.identity.picture.borrow().is_none());
            assert!(!p.panel_complete(&b));
        } else {
            shown(&mut p, &b, 16.);
        }
        tap(&mut p, 20.);
        assert_eq!(selected(&p), if lose_head { 0. } else { 1. });
        let retry = p.panel_frame(false, true).unwrap();
        p.panel_lost(&b);
        p.panel_lost(&c);
        shown(&mut p, &retry, 32.);
        tap(&mut p, 40.);
        assert_eq!(selected(&p), 2., "retry paints the newest queued offset");
    }
}

#[test]
fn reload_drains_b_and_c_without_acknowledging_or_presenting_the_new_session() {
    let ((mut p, _, _), _) = ready(ROWS);
    wheel(&mut p, 40.);
    let b = p.panel_frame_ahead(true).unwrap();
    wheel(&mut p, 40.);
    let c = p.panel_frame_ahead(true).unwrap();
    assert!(p.reload(b"invalid plan", Empty).is_err());
    assert!(!p.panel_complete(&c));
    p.reload(&crate::test_plan(ROWS), Empty).unwrap();
    assert!(p.hit(100., 20.).is_none());
    shown(&mut p, &b, 16.);
    assert!(!p.painted);
    assert!(!p.panel_can_prepare_ahead());
    assert!(p.panel_frame_ahead(true).is_none());
    shown(&mut p, &c, 32.);
    assert!(!p.painted);
    assert!(p.hit(100., 20.).is_none());
    let first = p.panel_frame_ahead(true).unwrap();
    assert!(!p.panel_can_prepare_ahead());
    shown(&mut p, &first, 48.);
    tap(&mut p, 50.);
    assert_eq!(selected(&p), 0.);
    assert!(!p.panel_complete(&b));
    assert!(!p.panel_complete(&c));
}

#[test]
fn ahead_waits_for_acknowledged_bounds_before_painting_a_clamp_correction() {
    let ((mut p, _, _), _) = acknowledged_root_page();
    assert!(p.resize(900., 900.).is_none());
    let b = p.panel_frame_ahead(true).unwrap();
    assert!(!p.panel_can_prepare_ahead());
    assert!(p.panel_frame_ahead(true).is_none());
    shown(&mut p, &b, 16.);
    assert_eq!(p.page.1, 0.);
    assert!(p.box_of(id(&p, "root")).unwrap().rect.1 == 0. || p.dirty());
    if p.dirty() {
        let c = p.panel_frame_ahead(true).unwrap();
        shown(&mut p, &c, 32.);
    }
    assert_eq!(p.box_of(id(&p, "root")).unwrap().rect.1, 0.);
    assert!(!p.dirty());

    let source = ROWS
        .replace(
            "state selected = -1",
            "state selected = -1\n  state extent = 40",
        )
        .replace("selected = row", "selected = row\n    extent = 10")
        .replace("height=40", "height=extent");
    let ((mut p, _, _), _) = ready(&source);
    wheel(&mut p, 80.);
    let a = p.panel_frame_ahead(true).unwrap();
    shown(&mut p, &a, 16.);
    tap(&mut p, 20.);
    let b = p.panel_frame_ahead(true).unwrap();
    assert!(!p.panel_can_prepare_ahead(), "nested scroller shrank");
    assert!(p.panel_frame_ahead(true).is_none());
    shown(&mut p, &b, 32.);
    assert_eq!(p.scroll_of(id(&p, "port")).1, 0.);
}

#[test]
fn first_present_and_new_motion_wait_but_running_and_final_motion_can_go_ahead() {
    let source = r#"component App
  state faded = false
  action fade
    faded = true
  view
    column width=320 height=240
      box testId="fade" press=fade width=100 height=40
      box testId="fader" width=20 height=20 opacity=(faded ? 0 : 1) transition="opacity 200ms linear"
"#;
    let previous = exact_runner::page::set_awaits_present(true);
    let (mut p, _, _) = boot_app(source);
    exact_runner::page::set_awaits_present(previous);
    p.presents_pictures(true);
    p.set_live_clock(Some(Rc::new(|| 0.)));
    let a = p.panel_frame_ahead(true).unwrap();
    assert!(p.display_presented(&a, 8.).is_none());
    assert!(!p.host.runner().presented().seen, "submit is not a present");
    assert!(!p.panel_can_prepare_ahead());
    shown(&mut p, &a, 16.);
    assert!(p.host.runner().presented().seen);
    let fader = id(&p, "fader");
    assert!(p
        .host
        .dispatch_at(id(&p, "fade"), Event::Press, 100.)
        .is_none());
    assert!(p.after_commit().is_none());
    p.tick(150.);
    let b = p.panel_frame_ahead(true).unwrap();
    assert!(!p.panel_can_prepare_ahead());
    assert!(p.panel_frame_ahead(true).is_none());
    assert!(p.display_presented(&b, 166.).is_none());
    p.tick(180.);
    assert_eq!(
        p.host.presented(fader).opacity,
        1.,
        "early notice starts nothing"
    );
    shown(&mut p, &b, 200.);
    p.tick(250.);
    assert!((p.host.presented(fader).opacity - 0.75).abs() < 1e-3);
    let c = p.panel_frame_ahead(true).unwrap();
    p.tick(300.);
    let d = p.panel_frame_ahead(true).unwrap();
    assert!(!p.panel_can_prepare_ahead());
    assert!(!p.panel_complete(&d));
    shown(&mut p, &c, 316.);
    p.tick(450.);
    assert_eq!(p.host.presented(fader).opacity, 0.);
    assert!(p.dirty(), "settling animation still owes the final image");
    let final_frame = p.panel_frame_ahead(true).unwrap();
    shown(&mut p, &d, 466.);
    shown(&mut p, &final_frame, 482.);
    assert!(!p.dirty());
    assert!(!p.host.motion());
}

fn numeric_fixture(mode: &str) -> Fixture {
    let source = format!(
        r##"component App
  state n = 1
  action next
    n = n + 1
  action reset
    n = 1
  view
    column width=320 height=240
      box testId="target" press=next margin-left=(n * 40) width=30 height=30 background-color="#cc3300"
      text `${{n}}` font-size=48 font-variant-numeric="tabular-nums" -exact-content-transition="{mode}" testId="number"
      box testId="reset" press=reset width=0 height=0
"##
    );
    let (mut p, paints, fail) = boot_app(&source);
    assert!(p
        .set_preferences(exact_runner::Preferences {
            reduced_motion: false,
            ..Default::default()
        })
        .is_none());
    let a = p.panel_frame_ahead(true).unwrap();
    shown(&mut p, &a, 0.);
    next_number(&mut p, 100.);
    p.tick(150.);
    (p, paints, fail)
}

fn next_number(p: &mut Presenter<Empty>, at: f64) {
    assert!(p
        .host
        .dispatch_at(id(p, "target"), Event::Press, at)
        .is_none());
    assert!(p.after_commit().is_none());
}

fn shown_number(p: &Presenter<Empty>) -> String {
    let key = p.host.kernel().node(id(p, "number")).unwrap().key;
    p.brush
        .paragraph(key)
        .unwrap()
        .layout_runs()
        .map(|line| line.text)
        .collect()
}

#[test]
fn ahead_numeric_text_matches_single_flight_without_changing_shown_hits() {
    for mode in ["none", "numeric"] {
        for change_again in [false, true] {
            let (mut ahead, _, _) = numeric_fixture(mode);
            let (mut serial, _, _) = numeric_fixture(mode);
            let b = ahead.panel_frame_ahead(true).unwrap();
            let serial_b = serial.panel_frame_ahead(true).unwrap();
            assert!(b.mirror().unwrap().data() == serial_b.mirror().unwrap().data());
            shown(&mut serial, &serial_b, 166.);

            for p in [&mut ahead, &mut serial] {
                if change_again {
                    next_number(p, 250.);
                }
                p.tick(300.);
            }
            assert!(ahead.panel_can_prepare_ahead());
            let c = ahead.panel_frame_ahead(true).unwrap();
            let serial_c = serial.panel_frame_ahead(true).unwrap();
            let different = c
                .mirror()
                .unwrap()
                .data()
                .iter()
                .zip(serial_c.mirror().unwrap().data())
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(different, 0, "{mode}, change again: {change_again}");
            assert_eq!(shown_number(&ahead), "1");
            assert_eq!(ahead.hit(45., 5.), Some(id(&ahead, "target")));
            assert_ne!(ahead.hit(85., 5.), Some(id(&ahead, "target")));
            shown(&mut ahead, &b, 316.);
            assert_eq!(shown_number(&ahead), "2");
            assert_eq!(ahead.hit(85., 5.), Some(id(&ahead, "target")));
            shown(&mut ahead, &c, 332.);
            shown(&mut serial, &serial_c, 332.);
            for p in [&mut ahead, &mut serial] {
                p.tick(1000.);
            }
            let settled = ahead.panel_frame_ahead(true).unwrap();
            let serial_settled = serial.panel_frame_ahead(true).unwrap();
            assert!(
                settled.mirror().unwrap().data() == serial_settled.mirror().unwrap().data(),
                "the final numeral must match after both acknowledgements"
            );
            assert!(!ahead.needs_animation_frame());
        }
    }
}

fn numeric_recovery(failed_paint: bool) {
    for (change_again, revert, newer) in [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let (mut ahead, _, fail) = numeric_fixture("numeric");
        let (mut serial, _, serial_fail) = numeric_fixture("numeric");
        let b = ahead.panel_frame_ahead(true).unwrap();
        let serial_b = serial.panel_frame_ahead(true).unwrap();
        if failed_paint {
            shown(&mut serial, &serial_b, 166.);
        }
        for p in [&mut ahead, &mut serial] {
            if change_again {
                next_number(p, 250.);
            }
            if revert {
                assert!(p
                    .host
                    .dispatch_at(id(p, "reset"), Event::Press, 275.)
                    .is_none());
                assert!(p.after_commit().is_none());
            }
            p.tick(300.);
        }
        fail.set(failed_paint);
        serial_fail.set(failed_paint);
        let c = ahead.panel_frame_ahead(true).unwrap();
        assert_eq!(shown_number(&ahead), "1");
        let serial_c = failed_paint.then(|| serial.panel_frame_ahead(true).unwrap());
        if newer {
            for p in [&mut ahead, &mut serial] {
                next_number(p, 350.);
            }
        }
        if failed_paint {
            assert!(!ahead.last_frame_succeeded());
            assert!(!serial.last_frame_succeeded());
            shown(&mut ahead, &b, 316.);
            shown(&mut ahead, &c, 332.);
            shown(&mut serial, &serial_c.unwrap(), 332.);
        } else {
            ahead.panel_lost(&b);
            ahead.panel_lost(&c);
            serial.panel_lost(&serial_b);
        }
        let visible = if failed_paint { "2" } else { "1" };
        for p in [&ahead, &serial] {
            assert_eq!(shown_number(p), visible);
        }
        fail.set(false);
        serial_fail.set(false);
        for p in [&mut ahead, &mut serial] {
            p.tick(400.);
        }
        let retry = ahead.panel_frame(false, true).unwrap();
        let serial_retry = serial.panel_frame(false, true).unwrap();
        let different = retry
            .mirror()
            .unwrap()
            .data()
            .iter()
            .zip(serial_retry.mirror().unwrap().data())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            different, 0,
            "failed paint: {failed_paint}, change again: {change_again}, revert: {revert}, newer: {newer}"
        );
        shown(&mut ahead, &retry, 416.);
        shown(&mut serial, &serial_retry, 416.);
        let expected = match (newer, revert, change_again) {
            (true, _, _) => "4",
            (_, true, _) => "1",
            (_, _, true) => "3",
            _ => "2",
        };
        assert_eq!(shown_number(&ahead), expected);
        if revert && !failed_paint {
            assert!(
                !ahead.needs_animation_frame(),
                "an unshown roll cannot survive loss"
            );
        }
    }
}

#[test]
fn lost_ahead_numeric_text_matches_single_flight_recovery() {
    numeric_recovery(false);
}

#[test]
fn failed_ahead_numeric_text_matches_single_flight_recovery() {
    numeric_recovery(true);
}
