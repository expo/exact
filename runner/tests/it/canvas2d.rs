//! Canvas 2D's protocol in the runner (LLP 1056 D4, D5): 2D surfaces never
//! reach the GPU side-output; a draw needs geometry; a new size is a new
//! generation with a fresh recorder; a throw keeps what it drew before;
//! a stale reply is discarded; frames are drawn once per clock value.
use exact_canvas::{list, Context2d, DrawError, Frame};
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    agent, DataError, DataSource, DrawReply, DrawRequest, Drawn, Geometry, Limits, Runner,
};
use std::cell::RefCell;
use std::rc::Rc;

const SOURCE: &str = "component App
  state n = 1
  state show = true
  action bump
    n = n + 1
  action hide
    show = false
  view
    column
      if show
        canvas surface=spark(n) width=100 height=50 testId=\"c\"
      canvas surface=globe(n) testId=\"g\"
";

#[derive(Default)]
struct Log {
    frames: Vec<Frame>,
    later: bool,
    /// Draw images and text instead.
    pictures: bool,
    parked: Vec<(u64, u32, u64)>,
    retired: Vec<(u64, u32)>,
    /// Each draw's request was for a `display-p3` canvas.
    p3: Vec<bool>,
    /// Reply with a list that does not read.
    malformed: bool,
    large: bool,
    /// Each draw's `currentColor`.
    colors: Vec<Option<exact_canvas::Rgba>>,
}

struct Spark(Rc<RefCell<Log>>);

impl DataSource for Spark {
    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        name: &str,
        _: &[Value],
    ) -> Result<exact_runner::Answer, DataError> {
        assert_eq!(name, "capture");
        Ok(exact_runner::Answer::Later(
            exact_runner::Request::capture_canvas("spark", "app:/tmp/shot.png"),
        ))
    }
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("unexpected {name}")
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("spark".into(), 1)]
    }
    fn draw_2d(
        &mut self,
        surface: &str,
        args: &[Value],
        ctx: &Context2d,
        frame: &Frame,
    ) -> Result<bool, DrawError> {
        assert_eq!(surface, "spark");
        self.0.borrow_mut().frames.push(*frame);
        let n = match args[0] {
            Value::Number(n) => n,
            _ => panic!(),
        };
        if self.0.borrow().pictures {
            ctx.draw_image_with_image_handle("photo.png", 0.0, 0.0)?;
            ctx.draw_image_with_image_handle("gone.png", 0.0, 0.0)?;
            ctx.fill_text("hi", 1.0, 9.0)?;
            return Ok(false);
        }
        if self.0.borrow().large {
            for _ in 0..220_000 {
                ctx.fill_rect(0., 0., 1., 1.);
            }
        }
        ctx.translate(1.0, 0.0)?;
        ctx.fill_rect(0.0, 0.0, n, n);
        if n == 3.0 {
            ctx.arc(0.0, 0.0, -1.0, 0.0, 1.0)?;
        }
        Ok(n == 4.0)
    }
    fn draw(&mut self, request: &DrawRequest<'_>, ctx: &Context2d) -> Drawn {
        self.0.borrow_mut().p3.push(request.p3);
        self.0.borrow_mut().colors.push(request.current_color);
        if self.0.borrow().later {
            self.0
                .borrow_mut()
                .parked
                .push((request.canvas, request.generation, request.seq));
            return Drawn::Later;
        }
        if self.0.borrow().malformed {
            self.0.borrow_mut().frames.push(request.frame);
            return Drawn::Now(DrawReply {
                lists: vec![vec![0xff; 8]],
                ..Default::default()
            });
        }
        let result = self.draw_2d(request.surface, request.args, ctx, &request.frame);
        Drawn::Now(DrawReply {
            lists: ctx.take_lists(),
            wants_frame: matches!(result, Ok(true)),
            error: result.err().map(|e| e.to_string()),
            notes: ctx.take_notes(),
        })
    }
    fn canvases_retired(&mut self, retired: &[(u64, u32)]) {
        self.0.borrow_mut().retired.extend_from_slice(retired);
    }
}

fn boot() -> (Runner<Spark>, Rc<RefCell<Log>>) {
    boot_source(SOURCE)
}

fn boot_source(source: &str) -> (Runner<Spark>, Rc<RefCell<Log>>) {
    let log = Rc::new(RefCell::new(Log::default()));
    let plan = contract::compile(source).unwrap();
    let r = Runner::boot(
        plan,
        Spark(log.clone()),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    (r, log)
}

fn geometry(w: f64, h: f64, scale: f64) -> Geometry {
    Geometry {
        width: w,
        height: h,
        scale,
        bitmap: None,
        settings: Default::default(),
    }
}

fn ops(bytes: &[u8]) -> Vec<String> {
    list::records(bytes)
        .unwrap()
        .iter()
        .map(|r| r.op.name().to_string())
        .collect()
}

#[test]
fn a_2d_surface_draws_by_generation_and_never_reaches_the_gpu_side_output() {
    let (mut r, log) = boot();
    let gpu = r.take_surface_updates();
    assert_eq!(gpu.len(), 1, "only globe is the GPU module's");
    assert_eq!(gpu[0].name, "globe");
    let views = r.canvas_views();
    assert_eq!(views.len(), 1);
    let view = views[0];

    // No geometry, no draw.
    r.draw_canvases(&|_| true);
    assert!(r.take_canvas_lists().is_empty());

    r.set_canvas_geometry(view, geometry(100.0, 50.0, 2.0));
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert_eq!(lists.len(), 2, "the fresh generation, then the mount draw");
    assert!(lists[0].fresh && lists[0].lists.is_empty());
    assert_eq!(
        (lists[0].pixel_width, lists[0].pixel_height, lists[0].scale),
        (200, 100, 2.0)
    );
    assert_eq!(ops(&lists[1].lists[0]), ["SetTransform", "FillRect"]);
    let f = log.borrow().frames[0];
    assert_eq!(
        (f.cause.primary(), f.width, f.pixel_width),
        ("mount", 100.0, 200)
    );

    // Args: the recorder persists — the transform is cumulative.
    r.act("bump", vec![]).unwrap();
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    let recs = list::records(&lists[0].lists[0]).unwrap();
    assert_eq!(
        recs[0].at(4),
        2.0,
        "translate(1, 0) twice in one generation"
    );
    assert_eq!(log.borrow().frames[1].cause.primary(), "args");

    // A size change: a new generation, a fresh recorder, the old retired.
    r.set_canvas_geometry(view, geometry(100.0, 50.0, 3.0));
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert!(lists[0].fresh && lists[0].generation == 1);
    let recs = list::records(&lists[1].lists[0]).unwrap();
    assert_eq!(recs[0].at(4), 1.0, "a new generation starts at identity");
    assert_eq!(log.borrow().retired.len(), 1);
    // Rounding to the same store is not a new generation.
    r.set_canvas_geometry(view, geometry(100.1, 50.0, 3.0));
    r.draw_canvases(&|_| true);
    assert!(r.take_canvas_lists().is_empty());

    // A throw keeps what was drawn before it (r3: not atomic).
    r.act("bump", vec![]).unwrap();
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert_eq!(ops(&lists[0].lists[0]), ["SetTransform", "FillRect"]);
    let state = agent::state(&r);
    assert!(state.contains("IndexSizeError"), "{state}");

    // n = 4 asks for frames: one per clock value, only while on screen.
    r.act("bump", vec![]).unwrap();
    r.draw_canvases(&|_| true);
    r.take_canvas_lists();
    assert!(r.canvas_wants_frame());
    r.canvas_frame();
    r.draw_canvases(&|_| false);
    assert!(r.take_canvas_lists().is_empty(), "held off screen");
    r.draw_canvases(&|_| true);
    assert_eq!(r.take_canvas_lists().len(), 1);
    r.canvas_frame();
    r.draw_canvases(&|_| true);
    assert!(r.take_canvas_lists().is_empty(), "once per clock value");
    r.advance(16.0).unwrap();
    r.canvas_frame();
    r.draw_canvases(&|_| true);
    assert_eq!(r.take_canvas_lists().len(), 1);
    assert_eq!(log.borrow().frames.last().unwrap().cause.primary(), "frame");

    // Unmounting drops the record and retires its generation.
    r.act("hide", vec![]).unwrap();
    assert!(r.canvas_views().is_empty());
    r.draw_canvases(&|_| true);
    assert_eq!(log.borrow().retired.len(), 2);
}

#[test]
fn a_later_reply_applies_only_with_live_stamps() {
    let (mut r, log) = boot();
    log.borrow_mut().later = true;
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(10.0, 10.0, 1.0));
    r.draw_canvases(&|_| true);
    assert!(r.take_canvas_lists()[0].fresh);
    let (canvas, generation, seq) = log.borrow().parked[0];
    // A resize before the reply lands: the reply is discarded.
    r.set_canvas_geometry(view, geometry(20.0, 10.0, 1.0));
    let ctx = Context2d::new();
    ctx.fill_rect(0.0, 0.0, 1.0, 1.0);
    let reply = DrawReply {
        lists: ctx.take_lists(),
        ..Default::default()
    };
    assert!(r
        .canvas_reply(canvas, generation, seq, reply.clone())
        .is_err());
    r.draw_canvases(&|_| true);
    let (c2, g2, s2) = log.borrow().parked[1];
    assert_eq!((c2, g2), (canvas, generation + 1));
    assert!(r.canvas_reply(c2, g2, s2, reply.clone()).is_ok());
    let lists = r.take_canvas_lists();
    assert!(lists[0].fresh && !lists[1].fresh);
    // A draw that asked for no frame does not animate (LLP 1056 §8.4).
    assert!(!lists[0].animating && !lists[1].animating);
    // The same stamps twice: the second is stale.
    assert!(r.canvas_reply(c2, g2, s2, reply.clone()).is_err());
    // One that asks for the next frame does, on the list it returns.
    r.set_canvas_geometry(view, geometry(40.0, 20.0, 2.0));
    r.draw_canvases(&|_| true);
    let (c3, g3, s3) = *log.borrow().parked.last().unwrap();
    let asks = DrawReply {
        wants_frame: true,
        ..reply
    };
    assert!(r.canvas_reply(c3, g3, s3, asks).is_ok());
    assert!(r
        .take_canvas_lists()
        .iter()
        .any(|l| !l.fresh && l.animating));
}

/// A list that does not read starts a new generation and draws it once
/// more; a source that keeps replying so is not asked again until
/// something else changes, so a host drawing whenever one is owed
/// (LLP 1072 §8.5) does not draw it forever.
#[test]
fn a_malformed_reply_is_retried_once() {
    let (mut r, log) = boot();
    log.borrow_mut().malformed = true;
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(10.0, 10.0, 1.0));
    let mut turns = 0;
    while r.canvas_owed(&|_| true) && turns < 10 {
        r.draw_canvases(&|_| true);
        r.take_canvas_lists();
        turns += 1;
    }
    assert!(!r.canvas_owed(&|_| true), "still owed after {turns} turns");
    assert_eq!(log.borrow().frames.len(), 2, "the first draw and one retry");
}

#[test]
fn sizes_past_the_limits_are_refused_through_state() {
    let (mut r, _) = boot();
    r.set_canvas_limits(Limits {
        max_side: 65_535,
        max_area: 67_108_864,
        budget: 1 << 20,
    });
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(1000.0, 1000.0, 1.0));
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert_eq!(
        (lists.len(), lists[0].pixel_width),
        (1, 0),
        "no bitmap, no draw"
    );
    assert!(agent::state(&r).contains("MiB budget"));
    r.set_canvas_geometry(view, geometry(9000.0, 9000.0, 1.0));
    assert!(agent::state(&r).contains("area limit"));
}

#[test]
fn an_image_a_draw_asks_for_redraws_it_when_it_decodes() {
    let (mut r, log) = boot();
    log.borrow_mut().pictures = true;
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(10.0, 10.0, 1.0));
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert_eq!(
        ops(&lists[1].lists[0]),
        ["Font", "FillText"],
        "no image yet"
    );
    let mut asked = r.take_canvas_image_requests();
    asked.sort();
    assert_eq!(asked, ["gone.png", "photo.png"]);
    assert!(r.canvas_images_pending());
    r.canvas_image("photo.png", Ok((4, 2)), &[]);
    r.canvas_image("gone.png", Err("404".into()), &[]);
    assert!(!r.canvas_images_pending());
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert_eq!(ops(&lists[0].lists[0]), ["Image", "DrawImage", "FillText"]);
    assert_eq!(log.borrow().frames.last().unwrap().cause.primary(), "image");
    assert!(r.take_canvas_image_requests().is_empty(), "asked once");
    assert!(agent::state(&r).contains("gone.png: 404"));
    // A font that loads redraws a canvas that drew text.
    r.canvas_fonts_loaded();
    r.draw_canvases(&|_| true);
    assert_eq!(r.take_canvas_lists().len(), 1);
    assert_eq!(log.borrow().frames.last().unwrap().cause.primary(), "font");
}

/// A decoded image the host let go is requested again at its next use, and
/// draws nothing until the host answers it again.
#[test]
fn an_image_the_host_released_is_requested_again() {
    let (mut r, log) = boot();
    log.borrow_mut().pictures = true;
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(10.0, 10.0, 1.0));
    r.draw_canvases(&|_| true);
    r.take_canvas_lists();
    r.take_canvas_image_requests();
    r.canvas_image("photo.png", Ok((4, 2)), &[]);
    r.canvas_image("gone.png", Err("404".into()), &[]);
    r.draw_canvases(&|_| true);
    r.take_canvas_lists();
    r.canvas_image_released("photo.png");
    assert!(
        !r.canvas_images_pending(),
        "nothing is asked before a draw uses it"
    );
    r.canvas_fonts_loaded();
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert!(
        !ops(&lists[0].lists[0]).contains(&"DrawImage".to_string()),
        "released: drawn without it"
    );
    assert_eq!(
        r.take_canvas_image_requests(),
        ["photo.png"],
        "the broken one is not asked again"
    );
    r.canvas_image("photo.png", Ok((4, 2)), &[]);
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert!(ops(&lists[0].lists[0]).contains(&"DrawImage".to_string()));
}

/// LLP 1100 D12a: `color-space` and `color-type` are the canvas's getContext
/// settings: the draw is told, the host's bitmap carries them, a change is
/// a new generation, and float16 counts 8 bytes a pixel.
#[test]
fn a_canvas_settings_reach_the_draw_and_the_host() {
    let (mut r, log) = boot_source(
        "component App
  state wide = true
  action narrow
    wide = false
  view
    canvas surface=spark(1) width=100 height=50 color-space=(wide ? \"display-p3\" : \"srgb\") color-type=\"float16\"
",
    );
    let view = r.canvas_views()[0];
    let settings = r.canvas_settings(view);
    assert!(settings.p3 && settings.float16);
    r.set_canvas_geometry(
        view,
        Geometry {
            settings,
            ..geometry(100.0, 50.0, 1.0)
        },
    );
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert!(lists.iter().all(|l| l.settings == settings));
    assert_eq!(log.borrow().p3, [true]);
    r.act("narrow", vec![]).unwrap();
    let settings = r.canvas_settings(view);
    assert!(!settings.p3 && settings.float16);
    r.set_canvas_geometry(
        view,
        Geometry {
            settings,
            ..geometry(100.0, 50.0, 1.0)
        },
    );
    r.draw_canvases(&|_| true);
    let lists = r.take_canvas_lists();
    assert!(lists[0].fresh && lists[0].generation == 1, "a new context");
    assert_eq!(log.borrow().p3.last(), Some(&false));
}

/// `-exact-system-green` in a scheme, as a draw's `currentColor`.
fn green(dark: bool) -> Option<exact_canvas::Rgba> {
    let role = exact_kernel::style::roles::role("-exact-system-green").unwrap();
    let [r, g, b, a] = exact_kernel::ColorValue::Role(role)
        .resolve(dark)
        .0
        .to_be_bytes();
    Some(exact_canvas::Rgba {
        r,
        g,
        b,
        a: a as f64 / 255.0,
    })
}

/// A canvas's `currentColor` is its colour in its own scheme (LLP 1056 D3):
/// a role under `color-scheme="dark"` is the role's dark value, else the
/// window's scheme's.
#[test]
fn a_canvas_current_color_is_its_schemes() {
    let (mut r, log) = boot_source(
        "component App
  view
    column
      canvas surface=spark(1) width=10 height=10 color=\"-exact-system-green\"
      column color-scheme=\"dark\"
        canvas surface=spark(1) width=10 height=10 color=\"-exact-system-green\"
",
    );
    for view in r.canvas_views() {
        r.set_canvas_geometry(view, geometry(10.0, 10.0, 1.0));
    }
    r.draw_canvases(&|_| true);
    assert_ne!(green(false), green(true), "the role has two values");
    assert_eq!(log.borrow().colors, [green(false), green(true)]);
}

/// With no subtree scheme, a canvas's `currentColor` follows the scheme the
/// app chose (`setScheme`), not the window's beneath it, and a change of
/// scheme draws it again (cause `"style"`), as nothing else would.
#[test]
fn a_canvas_current_color_follows_the_apps_scheme() {
    let (mut r, log) = boot_source(
        "component App
  action chooseLight
    setScheme(\"light\")
  view
    canvas surface=spark(1) width=10 height=10 color=\"-exact-system-green\"
",
    );
    r.set_preferences(exact_runner::Preferences {
        dark: true,
        ..Default::default()
    })
    .unwrap();
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(10.0, 10.0, 1.0));
    r.draw_canvases(&|_| true);
    r.take_canvas_lists();
    assert!(!r.canvas_owed(&|_| true), "drawn, and nothing changed");
    r.act("chooseLight", vec![]).unwrap();
    assert!(r.canvas_owed(&|_| true), "the scheme changed");
    r.draw_canvases(&|_| true);
    assert_eq!(log.borrow().colors, [green(true), green(false)]);
    assert_eq!(log.borrow().frames.last().unwrap().cause.names(), ["style"]);
}

/// A capture names one mounted 2D canvas by its surface, with a bitmap,
/// and answers its backing (LLP 1056 D9, as amended for expo/exact#188);
/// each other case is refused by a DOM exception's name before any host
/// looks at pixels.
#[test]
fn a_capture_names_one_mounted_canvas_with_a_bitmap() {
    use exact_runner::canvas_capture::captured_image;
    use exact_runner::Request;
    let (mut r, _) = boot_source(
        "component App
  state two = false
  action more
    two = true
  view
    column
      canvas surface=spark(1) width=100 height=50
      if two
        canvas surface=spark(2) width=10 height=10
",
    );
    let refusal = |r: &Runner<Spark>, surface: &str, path: &str| {
        let found = r.canvas_capture(&Request::capture_canvas(surface, path));
        captured_image(&found.expect("a capture").expect_err("refused")).unwrap_err()
    };
    let view = r.canvas_views()[0];
    assert!(
        refusal(&r, "spark", "app:/tmp/a.png").starts_with("IndexSizeError: "),
        "not laid out yet"
    );
    r.set_canvas_geometry(view, geometry(100.0, 50.0, 2.0));
    let c = r
        .canvas_capture(&Request::capture_canvas("spark", "app:/tmp/a.png"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (c.view, c.width, c.height, c.scale, c.generation),
        (view, 200, 100, 2.0, 0)
    );
    assert_eq!(c.path, "app:/tmp/a.png");
    assert!(refusal(&r, "globe", "app:/tmp/a.png").starts_with("NotFoundError: "));
    assert!(refusal(&r, "spark", "/tmp/a.png").starts_with("NotAllowedError: "));
    assert!(r.canvas_capture(&Request::get("https://x.test")).is_none());
    // Over 8,192² pixels: not written, though the canvas is within its own limits.
    r.set_canvas_geometry(view, geometry(100.0, 50.0, 200.0));
    assert!(refusal(&r, "spark", "app:/tmp/a.png").starts_with("QuotaExceededError: "));
    r.act("more", vec![]).unwrap();
    assert!(
        refusal(&r, "spark", "app:/tmp/a.png").starts_with("InvalidStateError: "),
        "two mounted"
    );
}

#[test]
fn captures_wait_for_a_completed_drawing() {
    use exact_runner::canvas_capture::captured_image;
    #[derive(Clone, Copy, Debug)]
    enum Row {
        LaterCommits,
        DrainFirst,
        Unchanged,
        SurfaceReplace,
        Later,
        SecondCanvas,
        Budget,
        LaterAdvance,
        Taken,
        Frame,
        SeveralFrames,
        Unmount,
        TakenUnmount,
        SessionEnd,
        TakenEnd,
    }
    for row in [
        Row::LaterCommits,
        Row::DrainFirst,
        Row::Unchanged,
        Row::SurfaceReplace,
        Row::Later,
        Row::SecondCanvas,
        Row::Budget,
        Row::LaterAdvance,
        Row::Taken,
        Row::Frame,
        Row::SeveralFrames,
        Row::Unmount,
        Row::TakenUnmount,
        Row::SessionEnd,
        Row::TakenEnd,
    ] {
        let tasks = match row {
            Row::LaterCommits => "  task save mount\n    after(1, capture)\n  task change mount\n    after(1, change)\n",
            Row::Frame => "  task save mount\n    every(frame, capture)\n",
            _ => "",
        };
        let source = format!("component App\n  state n = 4\n  state shown = true\n  mutation image as shape string\n  action capture\n    n = 1\n    send image = capture()\n  action change\n    n = 2\n  action hide\n    shown = false\n  action show\n    shown = true\n{tasks}  view\n    column\n      if shown\n        canvas surface=spark(n) width=40 height=40\n");
        let source = if matches!(row, Row::Frame | Row::SeveralFrames | Row::Unchanged) {
            source.replace("    n = 1\n", "")
        } else {
            source
        };
        let source = if matches!(row, Row::Unchanged) {
            source.replace("state n = 4", "state n = 1")
        } else {
            source
        };
        let source = if matches!(row, Row::SecondCanvas) {
            source.replace(
                "  view",
                "  state second = false\n  action add\n    second = true\n  view",
            ) + "      if second\n        canvas surface=spark(n) width=40 height=40\n"
        } else {
            source
        };
        let (mut r, log) = boot_source(&source);
        let view = r.canvas_views()[0];
        r.set_canvas_geometry(view, geometry(40., 40., 1.));
        log.borrow_mut().large = matches!(row, Row::Budget);
        r.draw_canvases(&|_| true);
        if !matches!(row, Row::Budget) {
            r.take_canvas_lists();
        }
        match row {
            Row::LaterCommits => assert_eq!(r.advance_timed(1.).receipts.len(), 2),
            Row::Frame => {
                r.frame(17.);
                r.canvas_frame();
            }
            _ => {
                r.act("capture", vec![]).unwrap();
            }
        }
        let request = r.take_requests().pop().unwrap().request;
        assert!(r.take_requests().is_empty(), "{row:?}: only one drain");
        assert!(request.canvas_capture_pending(), "{row:?}: no drawing yet");
        if matches!(row, Row::SessionEnd) {
            drop(r);
            assert!(request.take_canvas_capture().unwrap().is_err());
            continue;
        }
        if matches!(row, Row::LaterAdvance) {
            r.advance_timed(2.);
        }
        if matches!(row, Row::Unmount | Row::SurfaceReplace) {
            r.act("hide", vec![]).unwrap();
            if matches!(row, Row::SurfaceReplace) {
                r.act("show", vec![]).unwrap();
                let next = r.canvas_views()[0];
                r.set_canvas_geometry(next, geometry(40., 40., 1.));
            }
        }
        log.borrow_mut().later = matches!(row, Row::Later | Row::SecondCanvas);
        r.draw_canvases(&|_| true);
        if matches!(row, Row::Later | Row::SecondCanvas | Row::Budget) {
            assert!(
                request.canvas_capture_pending(),
                "{row:?}: skipped or deferred is not complete"
            );
            assert!(
                request.take_canvas_capture().unwrap().is_err(),
                "{row:?}: an early take has no completed drawing"
            );
            if matches!(row, Row::Later | Row::SecondCanvas) {
                if matches!(row, Row::SecondCanvas) {
                    r.act("add", vec![]).unwrap();
                    assert_eq!(r.canvas_views().len(), 2);
                }
                let (life, generation, seq) = log.borrow_mut().parked.pop().unwrap();
                r.canvas_reply(
                    life,
                    generation,
                    seq,
                    DrawReply {
                        lists: vec![],
                        wants_frame: false,
                        error: None,
                        notes: vec![],
                    },
                )
                .unwrap();
                assert!(
                    r.take_canvas_lists()
                        .iter()
                        .any(|l| l.seq == seq && l.lists.is_empty()),
                    "an empty completed draw reaches replay"
                );
            } else {
                r.take_canvas_lists();
                log.borrow_mut().large = false;
                r.draw_canvases(&|_| true);
            }
        }
        if matches!(row, Row::SeveralFrames) {
            for time in [20., 40., 60.] {
                r.advance_timed(time);
                r.canvas_frame();
                r.draw_canvases(&|_| true);
            }
        }
        let captured = request.take_canvas_capture().unwrap();
        if matches!(row, Row::Unmount | Row::SurfaceReplace) {
            let error = captured_image(&captured.unwrap_err()).unwrap_err();
            assert!(error.starts_with("EncodingError:"), "{row:?}: {error}");
            continue;
        }
        let c = captured.unwrap_or_else(|e| panic!("{row:?}: {e:?}"));
        assert!(
            c.sequence > 1,
            "{row:?}: a drawing after the request is identified"
        );
        if matches!(row, Row::SeveralFrames) {
            assert!(
                c.sequence >= 5,
                "several frames completed before the snapshot"
            );
        }

        match row {
            Row::Taken => {
                r.act("change", vec![]).unwrap();
                r.draw_canvases(&|_| true);
            }
            Row::TakenUnmount => {
                r.act("hide", vec![]).unwrap();
            }
            Row::TakenEnd => drop(r),
            _ => {}
        }
        if matches!(row, Row::TakenUnmount | Row::TakenEnd) {
            assert!(
                c.snapshotted().is_err(),
                "{row:?}: an unmounted or replaced canvas retires"
            );
        } else {
            c.snapshotted().unwrap();
            assert!(captured_image(&c.captured()).is_ok(), "{row:?}");
        }
    }
}

#[test]
fn a_rejected_draw_fails_its_capture_but_a_later_capture_can_succeed() {
    use exact_runner::canvas_capture::captured_image;
    for deferred in [false, true] {
        let (mut r, log) = boot_source("component App\n  mutation image as shape string\n  action capture\n    send image = capture()\n  view\n    canvas surface=spark(1) width=40 height=40\n");
        let view = r.canvas_views()[0];
        r.set_canvas_geometry(view, geometry(40., 40., 1.));
        r.draw_canvases(&|_| true);
        r.take_canvas_lists();
        r.act("capture", vec![]).unwrap();
        let first = r.take_requests().pop().unwrap().request;
        log.borrow_mut().malformed = true;
        log.borrow_mut().later = deferred;
        r.draw_canvases(&|_| true);
        if deferred {
            assert!(first.canvas_capture_pending());
            let (life, generation, seq) = log.borrow_mut().parked.pop().unwrap();
            assert!(r
                .canvas_reply(
                    life,
                    generation,
                    seq,
                    DrawReply {
                        lists: vec![vec![0xff; 8]],
                        ..Default::default()
                    }
                )
                .is_err());
        }
        assert!(
            !first.canvas_capture_pending(),
            "the rejected draw answers immediately"
        );
        r.act("capture", vec![]).unwrap();
        let next = r.take_requests().pop().unwrap().request;
        assert!(next.canvas_capture_pending());
        // Recovery answers the new request without erasing the first job's failure.
        log.borrow_mut().malformed = false;
        log.borrow_mut().later = false;
        r.draw_canvases(&|_| true);
        let error = captured_image(&first.take_canvas_capture().unwrap().unwrap_err()).unwrap_err();
        assert!(error.starts_with("EncodingError:"), "{error}");
        let capture = next.take_canvas_capture().unwrap().unwrap();
        capture.snapshotted().unwrap();
        assert!(captured_image(&capture.captured()).is_ok());
    }
}
