//! The capture invariant in `exact_runner::canvas_capture`, through carrier turns.
use super::*;
use exact_runner::{Answer, DataError, Event, Request, Store};

struct Source(String, bool, usize);
impl DataSource for Source {
    fn app_id(&self) -> &str {
        &self.0
    }
    fn grants(&self) -> &str {
        "fs.write app:/tmp"
    }
    fn query(
        &mut self,
        name: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[exact_runner::Value],
    ) -> Result<Answer, DataError> {
        if self.0.contains("rejected") {
            self.1 = true;
        }
        Ok(Answer::Later(Request::capture_canvas(
            "ink",
            "app:/tmp/shot.png",
        )))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[exact_runner::Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let image = exact_runner::canvas_capture::captured_image(&outcome);
        if self.0.ends_with("chain") && !self.1 {
            self.1 = true;
            return Ok(Answer::Later(Request::capture_canvas(
                "ink",
                "app:/tmp/second.png",
            )));
        }
        Ok(Answer::Now(exact_runner::Value::str(
            &image.map(|i| i.path).unwrap_or_else(|e| e),
        )))
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("ink".into(), 1)]
    }
    fn draw(
        &mut self,
        q: &exact_runner::DrawRequest<'_>,
        ctx: &exact_runner::exact_canvas::Context2d,
    ) -> exact_runner::Drawn {
        if self.0.contains("rejected") && self.1 {
            return exact_runner::Drawn::Now(exact_runner::DrawReply {
                lists: vec![vec![0xff; 8]],
                ..Default::default()
            });
        }
        let result = self.draw_2d(q.surface, q.args, ctx, &q.frame);
        exact_runner::Drawn::Now(exact_runner::DrawReply {
            lists: ctx.take_lists(),
            wants_frame: matches!(result, Ok(true)),
            error: result.err().map(|e| e.to_string()),
            notes: ctx.take_notes(),
        })
    }
    fn draw_2d(
        &mut self,
        _: &str,
        args: &[exact_runner::Value],
        ctx: &exact_runner::exact_canvas::Context2d,
        frame: &exact_runner::exact_canvas::Frame,
    ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
        if frame.cause.has(exact_runner::exact_canvas::Causes::FRAME) {
            self.2 += 1;
        }
        if self.0.contains("animated") {
            ctx.set_fill_style_str(if frame.time >= 32. {
                "green"
            } else if frame.time >= 16. {
                "blue"
            } else {
                "red"
            });
            ctx.fill_rect(0., 0., 40., 40.);
            return Ok(true);
        }
        ctx.set_fill_style_str(if args[0] == exact_runner::Value::Number(40.) {
            "red"
        } else {
            "blue"
        });
        ctx.fill_rect(0., 0., 40., 40.);
        Ok(false)
    }
}
const APP: &str = r#"component App
  state width = 40
  mutation image as shape string
  action capture
    send image = capture()
  action resize
    width = 80
  action cancel
    image = none
  view
    column
      button press=capture testId="capture"
        text "Capture"
      button press=cancel testId="cancel"
        text "Cancel"
      button press=resize testId="resize"
        text "Resize"
      canvas surface=ink(width) width=width height=40
"#;
fn boot(name: &str) -> Presenter<Source> {
    boot_source(name, APP)
}
fn boot_source(name: &str, app: &str) -> Presenter<Source> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(app),
        Source(
            format!("test.canvas.{}.{name}", std::process::id()),
            false,
            0,
        ),
        (200., 200.),
        1.,
        std::path::PathBuf::new(),
        crate::presenter::PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}
fn press(p: &mut Presenter<Source>, name: &str) {
    let k = p.host.kernel();
    let id = k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id;
    assert!(p.host.dispatch_at(id, Event::Press, 0.).is_none());
    assert!(p.after_commit().is_none());
}
fn cleanup(p: &mut Presenter<Source>) {
    for root in p.host.app_roots().unwrap() {
        let _ = std::fs::remove_dir_all(root);
    }
}
#[test]
fn a_display_turn_delivers_a_capture_after_drawing() {
    let mut p = boot("display");
    press(&mut p, "capture");
    assert_eq!(p.surfaces.captures.len(), 1);
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    assert!(
        p.surfaces.captures.is_empty(),
        "the DRM carrier must drain captures"
    );
    assert!(
        p.host.runner().pending().is_empty(),
        "the carrier delivers the outcome"
    );
    let path = p.host.app_roots().unwrap()[2].join("shot.png");
    let png = tiny_skia::Pixmap::load_png(path).unwrap();
    assert_eq!((png.width(), png.height()), (40, 40));
    assert_eq!(png.pixel(1, 1).unwrap().red(), 255);
    cleanup(&mut p);
}
#[test]
fn a_cancelled_capture_preserves_the_destination() {
    let mut p = boot("mutation");
    let root = p.host.app_roots().unwrap()[2].clone();
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("shot.png");
    std::fs::write(&path, b"keep this file").unwrap();
    press(&mut p, "capture");
    assert_eq!(p.surfaces.captures.len(), 1);
    press(&mut p, "cancel");
    assert!(p.host.runner().pending().is_empty());
    p.sync_surfaces();
    assert_eq!(std::fs::read(path).unwrap(), b"keep this file");
    cleanup(&mut p);
}
#[test]
fn a_replaced_runner_forgets_queued_captures() {
    let mut p = boot("reload");
    press(&mut p, "capture");
    assert_eq!(p.surfaces.captures.len(), 1);
    p.reload(&crate::test_plan(APP), Source("".into(), false, 0))
        .unwrap();
    assert!(p.surfaces.captures.is_empty());
}

#[test]
fn a_capture_waits_through_later_commits_and_resize() {
    for (timer, resize) in [(false, true), (true, true), (false, false), (true, false)] {
        let app = APP.replace(
            "  view",
            "  task timer mount\n    every(16, resize)\n  view",
        );
        let app = if resize {
            app
        } else {
            app.replace("width=width", "width=40")
        };
        let direct = if resize {
            APP.to_string()
        } else {
            APP.replace("width=width", "width=40")
        };
        let mut p = boot_source(
            if timer { "timer" } else { "resize" },
            if timer { &app } else { &direct },
        );
        p.sync_surfaces();
        let root = p.host.app_roots().unwrap()[2].clone();
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("shot.png");
        std::fs::write(&path, b"keep").unwrap();
        press(&mut p, "capture");
        if timer {
            assert!(p.host.advance(16.).is_none());
            assert!(p.after_commit().is_none());
        } else {
            press(&mut p, "resize");
        }
        p.sync_surfaces();
        let png = tiny_skia::Pixmap::load_png(&path).unwrap();
        assert_eq!(png.width(), if resize { 80 } else { 40 });
        assert_eq!(png.pixel(1, 1).unwrap().blue(), 255);
        assert!(p.host.runner().pending().is_empty());
        cleanup(&mut p);
    }
}

#[test]
fn chained_captures_finish_without_another_frame() {
    let mut p = boot("chain");
    p.dirty = false;
    press(&mut p, "capture");
    assert!(p.dirty(), "pending captures schedule a carrier turn");
    p.sync_surfaces();
    assert!(
        p.surfaces.captures.is_empty(),
        "the completion's capture must drain in this call"
    );
    assert!(p.host.runner().pending().is_empty());
    let root = p.host.app_roots().unwrap()[2].clone();
    assert!(root.join("second.png").exists());
    cleanup(&mut p);
}

// The same-advance row of runner's captures_wait_for_a_completed_drawing.
#[test]
fn a_capture_uses_the_last_commit_of_its_advance() {
    let app = APP.replace("  view", "  task save mount\n    after(1, capture)\n  task change mount\n    after(1, resize)\n  view");
    let mut p = boot_source("same-advance", &app);
    p.sync_surfaces();
    assert!(p.host.advance(1.).is_none());
    assert!(p.after_commit().is_none());
    p.sync_surfaces();
    let png = tiny_skia::Pixmap::load_png(p.host.app_roots().unwrap()[2].join("shot.png")).unwrap();
    assert_eq!((png.width(), png.height()), (80, 40));
    assert_eq!(png.pixel(1, 1).unwrap().blue(), 255);
    assert!(p.host.runner().pending().is_empty());
    cleanup(&mut p);
}

#[test]
fn animation_captures_follow_the_frame_draw_on_both_carriers() {
    for display in [false, true] {
        let mut p = boot(if display {
            "animated-display"
        } else {
            "animated-paint"
        });
        p.frame();
        press(&mut p, "capture");
        p.set_live_clock(Some(std::rc::Rc::new(|| 16.)));
        let before = p.host.runner().data_ref().2;
        if display {
            let frame = p.display_frame().unwrap();
            p.display_complete(&frame);
        } else {
            p.frame();
        }
        assert_eq!(p.host.runner().data_ref().2 - before, 1);
        p.set_live_clock(None);
        p.host.advance(32.);
        p.frame();
        p.sync_surfaces();
        let png =
            tiny_skia::Pixmap::load_png(p.host.app_roots().unwrap()[2].join("shot.png")).unwrap();
        assert_eq!(png.pixel(1, 1).unwrap().blue(), 255);
        assert!(p.host.runner().pending().is_empty());
        cleanup(&mut p);
    }
}

#[test]
fn an_unmounted_canvas_preserves_the_destination() {
    let app = APP.replace("  state width", "  state shown = true\n  action hide\n    shown = false\n  state width")
        .replace("      canvas surface", "      button press=hide testId=\"hide\"\n        text \"Hide\"\n      if shown\n        canvas surface");
    let mut p = boot_source("unmount", &app);
    p.frame();
    let path = p.host.app_roots().unwrap()[2].join("shot.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"keep").unwrap();
    press(&mut p, "capture");
    press(&mut p, "hide");
    p.sync_surfaces();
    assert_eq!(std::fs::read(path).unwrap(), b"keep");
    assert!(p.host.runner().pending().is_empty());
    cleanup(&mut p);
}

#[test]
fn several_completed_frames_can_supply_a_pending_capture() {
    let mut p = boot("animated-delayed");
    p.frame();
    press(&mut p, "capture");
    for time in [16., 32., 48.] {
        p.host.advance(time);
        p.host.sync_canvases(1., true, &p.assets);
    }
    p.sync_surfaces();
    let png = tiny_skia::Pixmap::load_png(p.host.app_roots().unwrap()[2].join("shot.png")).unwrap();
    assert_eq!(png.pixel(1, 1).unwrap().green(), 128);
    assert!(p.host.runner().pending().is_empty());
    cleanup(&mut p);
}

#[test]
fn a_display_frame_ticks_canvas_once_even_when_the_caret_repaints() {
    let app = APP.replace("      canvas", &format!(
        "      input testId=\"editor\" value=\"{}\" width=80 height=40 appearance=\"none\"\n      canvas",
        "abcdefghij ".repeat(12)
    ));
    let mut p = boot_source("animated-cadence", &app);
    p.frame();
    let k = p.host.kernel();
    let editor = k.node_by_key(k.find_by_test_id("editor")[0]).unwrap().id;
    let clock = std::cell::Cell::new(0.);
    p.set_live_clock(Some(std::rc::Rc::new(move || {
        clock.set(clock.get() + 1.);
        clock.get()
    })));
    let before = p.host.runner().data_ref().2;
    p.set_focus(Some(editor), 0.);
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    assert!(
        p.scroll_of(editor).0 > 0.,
        "the caret needed a second paint"
    );
    let painted = p.host.runner().data_ref().2 - before;
    p.sync_surfaces();
    let synced = p.host.runner().data_ref().2 - before;
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    let next = p.host.runner().data_ref().2 - before;
    assert_eq!((painted, synced, next), (1, 1, 2));
}

#[test]
fn a_rejected_capture_draw_preserves_the_file_and_stops_framing() {
    let mut p = boot("rejected");
    p.frame();
    let path = p.host.app_roots().unwrap()[2].join("shot.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"keep").unwrap();
    press(&mut p, "capture");
    for _ in 0..2 {
        let frame = p.display_frame().unwrap();
        assert!(p.display_complete(&frame));
    }
    assert!(
        p.host.runner().pending().is_empty(),
        "a rejected drawing must answer its capture"
    );
    assert!(exact_runner::agent::state(p.host.runner()).contains("EncodingError"));
    assert!(p.surfaces.captures.is_empty());
    assert!(
        !p.dirty(),
        "a rejected capture must not keep scheduling frames"
    );
    assert_eq!(std::fs::read(path).unwrap(), b"keep");
    cleanup(&mut p);
}
