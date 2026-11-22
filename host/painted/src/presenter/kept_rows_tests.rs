//! What a backend is told when a scroller's kept row changes (`paint/rows.rs`).
use super::*;
use crate::image::Bitmap;
use crate::paint::Shape;
use crate::text::{Paragraph, RunPaint};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tiny_skia::Transform;

mod measurement;

#[test]
fn hidden_resource_updates_bound_retained_work_without_a_paint() {
    use exact_runner::{DataError, Value};

    struct Feed {
        rows: Vec<Value>,
        revision: usize,
    }
    impl DataSource for Feed {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            if name == "changeRows" {
                let index = self.revision % 10;
                self.revision += 1;
                let mut rows = self.rows.clone();
                rows[index] = Value::record(vec![
                    Value::str(&format!("row-{index}")),
                    Value::str(&format!("Row {index} | revision {}", self.revision)),
                ]);
                self.rows = rows;
                return Ok(Value::Bool(true));
            }
            assert_eq!(name, "loadFeed");
            Ok(Value::record(vec![
                Value::list(self.rows.clone()),
                Value::Number(self.revision as f64),
            ]))
        }
    }
    let source = r#"shape Row
  id: string
  label: string
shape Feed
  rows: Row[]
  revision: number
component App
  state running = true
  resource feed = loadFeed() as shape Feed
  mutation changed as shape bool refreshes feed
  task updates when running
    every(250, pulse)
  action pulse
    send changed = changeRows()
  action stop
    running = false
  view
    column height="100%"
      button "Stop" press=stop testId="stop"
      text `Revision ${feed.revision}` testId="status" -exact-content-transition="numeric"
      list virtualized=true appearance="none" estimated-item-height=44 flex=1 min-height=0
        for item in feed.rows key=item.id
          row height=44
            text item.label
"#;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        Feed {
            rows: (0..25_000)
                .map(|index| {
                    Value::record(vec![
                        Value::str(&format!("row-{index}")),
                        Value::str(&format!("Row {index} | revision 0")),
                    ])
                })
                .collect(),
            revision: 0,
        },
        (1024., 720.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    p.frame();
    let status = p.host.kernel().find_by_test_id("status")[0];
    let accepted = Rc::downgrade(p.brush.paragraph(status).unwrap());
    let mut page = p.host.runner().page();
    page.hidden = true;
    page.background = Some(false);
    assert_eq!(p.set_page(page), None);

    for revision in 1..=384 {
        assert_eq!(p.advance(revision as f64 * 250.), None);
        assert_eq!(p.host.row_dirty.pending_nodes(), 0);
        assert!(p.brush.text.borrow().residency().cold_paragraphs <= 320);
    }
    assert_eq!(p.host.runner_mut().data().revision, 384);
    let stop = p.host.kernel().find_by_test_id("stop")[0];
    let stop = p.host.kernel().node_by_key(stop).unwrap().id;
    assert_eq!(
        p.host
            .dispatch_at(stop, exact_runner::Event::Press, p.host.now()),
        None
    );
    assert_eq!(p.after_commit(), None);
    assert_eq!(p.advance(396_000.), None);
    assert!(!p.host.runner().has_pending());
    assert_eq!(p.host.runner_mut().data().revision, 384);
    assert_eq!(p.host.runner_mut().data().rows.len(), 25_000);
    assert_eq!(
        Rc::as_ptr(p.brush.paragraph(status).unwrap()),
        accepted.as_ptr(),
        "the last accepted frame stays intact while hidden"
    );
    let residency = p.brush.text.borrow().residency();
    let pending = p.host.row_dirty.pending_nodes();
    eprintln!("hidden text storage: {residency:?}; pending row nodes: {pending}");
    assert!(
        residency.cold_paragraphs <= 320,
        "cold storage must stay within the 256-identity target plus 64 deferred identities: {residency:?}"
    );
    assert_eq!(
        pending, 0,
        "row invalidations must be consumed without painting"
    );
    let mut page = p.host.runner().page();
    page.hidden = false;
    assert_eq!(p.set_page(page), None);
    p.frame();
    let status = p.host.kernel().find_by_test_id("status")[0];
    let paragraph = p.brush.paragraph(status).unwrap();
    assert_eq!(paragraph.runs()[0].text, "Revision 384");
    assert!(
        !p.needs_animation_frame(),
        "restoring must not start a numeric roll from an old commit"
    );
}

/// What the backend heard: each row begun, with the row it was said to
/// have been, each row freed, each kept row drawn, and each drawn through a
/// transform.
#[derive(Default)]
struct Heard {
    begun: Vec<(u32, Option<u32>)>,
    freed: Vec<u32>,
    drawn: Vec<u32>,
    through: Vec<(u32, Transform)>,
    bounds: Vec<Rect4>,
    group_depth: u32,
    max_group_depth: u32,
    /// Keep each sticky section one row, as Canvas and Vello do.
    whole_sections: bool,
}

/// An Ink device, or none on a machine without one, which fails under
/// `EXACT_REQUIRED=1`.
fn ink() -> Option<crate::ink::Ink> {
    match crate::ink::Ink::new() {
        Ok(ink) => Some(ink),
        Err(e) => {
            assert_ne!(std::env::var("EXACT_REQUIRED").as_deref(), Ok("1"), "{e}");
            eprintln!("no Ink device: {e}");
            None
        }
    }
}

/// Keeps rows and draws nothing.
struct Keeps {
    heard: Rc<RefCell<Heard>>,
    row: u32,
}
impl Backend for Keeps {
    crate::paints_nothing!();
    fn name(&self) -> &'static str {
        "keeps-rows"
    }
    fn begin(&mut self, _: f32, _: f32, _: f32) {}
    fn fill(&mut self, _: &Shape, _: [u8; 4], _: Transform) {}
    fn fill_gradient(&mut self, _: &Shape, _: &crate::paint::GradientPaint, _: Transform) {}
    fn fill_border(&mut self, _: &crate::paint::border::BorderFill, _: Transform) {}
    fn image(
        &mut self,
        _: &Arc<Bitmap>,
        _: Rect4,
        _: &[Shape],
        _: Transform,
        _: Option<[u8; 4]>,
        _: exact_kernel::ImageRendering,
    ) {
    }
    fn text(
        &mut self,
        _: &mut TextEngine,
        _: &Paragraph,
        _: &[RunPaint],
        _: (f32, f32),
        _: Transform,
    ) {
    }
    fn push_clip(&mut self, _: &Shape, _: Transform) {}
    fn pop_clip(&mut self) {}
    fn push_opacity(&mut self, _: f32) {}
    fn pop_opacity(&mut self) {}
    fn pointer(&mut self, _: f32, _: f32) {}
    fn finish(&mut self) -> Result<Pixmap, String> {
        Pixmap::new(400, 500).ok_or_else(|| "no pixmap".into())
    }
    fn rows(&self) -> bool {
        true
    }
    fn split_sticky_sections(&self) -> bool {
        !self.heard.borrow().whole_sections
    }
    fn rows_through(&self) -> bool {
        true
    }
    fn group_begin(&mut self, _: ViewId, _: (f32, f32)) {
        let mut heard = self.heard.borrow_mut();
        heard.group_depth += 1;
        heard.max_group_depth = heard.max_group_depth.max(heard.group_depth);
    }
    fn group_end(&mut self) {
        self.heard.borrow_mut().group_depth -= 1;
    }
    fn row_begin(&mut self, id: u32, _: (f32, f32), previous: Option<u32>) {
        self.row = id;
        self.heard.borrow_mut().begun.push((id, previous));
    }
    fn row_end(&mut self, bounds: Rect4) -> u32 {
        self.heard.borrow_mut().bounds.push(bounds);
        self.row
    }
    fn row_free(&mut self, id: u32) {
        self.heard.borrow_mut().freed.push(id);
    }
    fn row_draw(&mut self, id: u32, _: (f32, f32), _: Rect4) {
        self.heard.borrow_mut().drawn.push(id);
    }
    fn row_draw_through(&mut self, id: u32, ts: Transform, _: Rect4) {
        self.heard.borrow_mut().through.push((id, ts));
    }
}

#[test]
fn desktop_scrollbars_keep_rows_whose_content_fits() {
    let source = r##"component App
  view
    column width=320 height=240 overflow-y="auto"
      column width=240 height=80 overflow-y="auto" flex-shrink=0 background-color="#ffffff"
        box height=40 flex-shrink=0
      box height=450 flex-shrink=0
"##;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (320., 240.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.brush.replace_backend(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    p.set_scrollbars(false);
    p.frame();
    let expected: Vec<_> = heard.borrow().begun.iter().map(|&(id, _)| id).collect();
    assert_eq!(expected.len(), 2);
    heard.borrow_mut().begun.clear();
    heard.borrow_mut().drawn.clear();
    p.repaint();
    p.frame();
    let heard = heard.borrow();
    assert!(heard.begun.is_empty(), "{:?}", heard.begun);
    assert_eq!(heard.drawn, expected);
}

#[test]
fn a_row_includes_a_removed_branches_paint_bounds() {
    let source = stage::tests::SHRINKING_PANELS
        .replace("EFFECT", "overflow=\"scroll\"")
        .replace("SCALER", "");
    let mut p = stage::tests::phone_source(&source);
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.brush.replace_backend(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    stage::tests::after(&mut p, 0.0);
    heard.borrow_mut().bounds.clear();
    stage::tests::tap(&mut p, "swap");
    stage::tests::after(&mut p, 0.0);
    assert!(p.stage_moving());
    let heard = heard.borrow();
    assert!(
        heard.bounds.iter().any(|r| r.1 + r.3 >= 600.0),
        "{:?}",
        heard.bounds
    );
}

/// A row a commit bound to another item (LLP 1078) is a new row to the
/// backend: not the row it was, whose recording is the other item's (a
/// reader told otherwise draws that in its place until the new one is
/// made), and the old recording is freed. A row that only changed is still
/// the row it was.
#[test]
fn a_renewed_row_is_not_the_row_it_was() {
    let cells: String = (0..6)
        .map(|i| format!("        box height=50 width=200 testId=\"c{i}\"\n"))
        .collect();
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(&format!(
            "component App\n  view\n    scroll testId=\"port\" width=200 height=400\n      column\n{cells}"
        ))
        .unwrap()
        .encode(),
        (),
        (400., 500.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.brush.replace_backend(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    let _ = p.frame();
    let first = std::mem::take(&mut *heard.borrow_mut());
    assert_eq!(first.begun.len(), 6, "each cell a kept row");
    assert!(first.begun.iter().all(|(_, previous)| previous.is_none()));
    // () changed: nothing is recorded again.
    let _ = p.frame();
    assert!(heard.borrow().begun.is_empty());
    // One row changed, one was bound to another item.
    let key = |name: &str| p.host.kernel().find_by_test_id(name)[0];
    let (changed, renewed) = (key("c1"), key("c4"));
    p.host.row_dirty.commit(
        &exact_kernel::CommitReceipt {
            touched: vec![changed, renewed],
            renewed: vec![renewed],
            ..Default::default()
        },
        0.0,
    );
    let _ = p.frame();
    let heard = heard.borrow();
    let (was_changed, was_renewed) = (first.begun[1].0, first.begun[4].0);
    assert_eq!(heard.begun.len(), 2);
    assert_eq!(
        heard.begun[0].1,
        Some(was_changed),
        "a changed row is the row it was"
    );
    assert_eq!(heard.begun[1].1, None, "a renewed row is a new one");
    assert!(heard.freed.contains(&was_renewed), "its old recording goes");
}

/// A reader made again holds none of the last one's rows (LLP 1115.001
/// D15.1): the backend that records for it is told every row as a new one,
/// and of none to free, though nothing in the app changed.
#[test]
fn a_new_reader_is_sent_every_row_again() {
    let cells: String = (0..6)
        .map(|i| format!("        box height=50 width=200 testId=\"c{i}\"\n"))
        .collect();
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &contract::compile(&format!(
            "component App\n  view\n    scroll testId=\"port\" width=200 height=400\n      column\n{cells}"
        ))
        .unwrap()
        .encode(),
        (),
        (400., 500.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let first = Rc::new(RefCell::new(Heard::default()));
    p.brush.replace_backend(Box::new(Keeps {
        heard: first.clone(),
        row: 0,
    }));
    let _ = p.frame();
    assert_eq!(first.borrow().begun.len(), 6, "each cell a kept row");
    let _ = p.frame();
    assert!(!p.dirty(), "nothing is owed once it has painted");
    let second = Rc::new(RefCell::new(Heard::default()));
    p.reader_replaced(Box::new(Keeps {
        heard: second.clone(),
        row: 0,
    }));
    assert!(p.dirty(), "the new reader is owed a paint");
    let _ = p.frame();
    let second = second.borrow();
    assert_eq!(second.begun.len(), 6, "every row is recorded for it");
    assert!(second.begun.iter().all(|(_, previous)| previous.is_none()));
    assert!(second.freed.is_empty(), "it was sent no row to free");
}

/// A new host under the presenter (an activated update, a development
/// reload) numbers its nodes as a boot does, so its rows have the old tree's
/// keys: none of them is the row kept under that key, whose recording
/// draws the old tree (LLP 1030.006 D6). Each is recorded anew and the old
/// recordings are freed, though the plan is the same.
#[test]
fn a_replaced_host_records_every_row_anew() {
    let cells: String = (0..6)
        .map(|i| format!("        box height=50 width=200 testId=\"c{i}\"\n"))
        .collect();
    let plan = contract::compile(&format!(
        "component App\n  view\n    scroll testId=\"port\" width=200 height=400\n      column\n{cells}"
    ))
    .unwrap()
    .encode();
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (400., 500.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.brush.replace_backend(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    let _ = p.frame();
    let first = std::mem::take(&mut *heard.borrow_mut());
    assert_eq!(first.begun.len(), 6, "each cell a kept row");
    p.set_updates(Some(Box::new(super::delivery::tests::StagedPlan(
        plan.into(),
    ))));
    assert!(p.activate_update(()).unwrap());
    let _ = p.frame();
    let heard = heard.borrow();
    assert_eq!(heard.begun.len(), 6, "every row is recorded again");
    assert!(heard.begun.iter().all(|(_, previous)| previous.is_none()));
    let mut freed = heard.freed.clone();
    freed.sort_unstable();
    let mut old: Vec<u32> = first.begun.iter().map(|(id, _)| *id).collect();
    old.sort_unstable();
    assert_eq!(freed, old, "the old tree's recordings go");
}

/// A row is recorded again when a layout changes what a node in it
/// publishes beside a frame that stayed: a percentage padding whose
/// containing block narrowed (CSS Box Model 3 §4.1) moves the row's content,
/// which input reads anew, while its frame does not.
#[test]
fn a_row_whose_padding_alone_changed_is_recorded_again() {
    for moved in [false, true] {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &contract::compile(
                &r##"component App
  state narrow = false
  action shrink
    narrow = true
  view
    column
      box testId="shrink" press=shrink width=40 height=20
      scroll testId="port" width=400 height=300 margin-top=(narrow ? SHIFT : 0)
        column width=(narrow ? 300 : 400)
          box testId="padded" width=200 height=50 box-sizing="border-box" padding-left="5%"
          box testId="plain" width=200 height=50
"##
                .replace("SHIFT", if moved { "10" } else { "0" }),
            )
            .unwrap()
            .encode(),
            (),
            (400., 500.),
            1.,
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let heard = Rc::new(RefCell::new(Heard::default()));
        p.brush.replace_backend(Box::new(Keeps {
            heard: heard.clone(),
            row: 0,
        }));
        let _ = p.frame();
        let first = std::mem::take(&mut *heard.borrow_mut());
        assert_eq!(first.begun.len(), 2, "each box a kept row");
        let k = p.host.kernel();
        let (shrink, padded) = (
            k.find_by_test_id("shrink")[0],
            k.find_by_test_id("padded")[0],
        );
        let frame = k.node_by_key(padded).unwrap().frame;
        let shrink = k.node_by_key(shrink).unwrap().id;
        let reply = crate::agent::handle(&mut p, &format!(r#"{{"op":"tap","id":{shrink}}}"#));
        assert!(!reply.contains("error"), "{reply}");
        let k = p.host.kernel();
        let next = k.node_by_key(padded).unwrap().frame;
        assert_eq!(next.x, frame.x);
        assert_eq!(next.y, frame.y + if moved { 10. } else { 0. });
        assert_eq!((next.width, next.height), (frame.width, frame.height));
        assert_eq!(k.resolved_padding(padded).map(|p| p.0), Some(15.));
        let _ = p.frame();
        let heard = heard.borrow();
        let again: Vec<_> = heard.begun.iter().map(|(_, previous)| *previous).collect();
        assert_eq!(again, [Some(first.begun[0].0)], "the padded row alone");
    }
}

#[test]
fn sticky_sections_keep_body_rows_and_hits_when_the_heading_is_pushed_off() {
    let plan = crate::test_plan(
        r#"component App
  view
    scroll testId="port" width=200 height=100
      column
        box width=200 height=40
        column width=200 height=180
          column testId="heading" position="sticky" top=0 z-index=1 width=200 height=20
            scroll width=40 height=20
              box width=40 height=80
          box testId="first" width=200 height=80
          box testId="second" width=200 height=80
        box testId="below" width=200 height=120
"#,
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (200., 120.),
        1.25,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let id = |name| {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let (port, heading, first, second, below) = (
        id("port"),
        id("heading"),
        id("first"),
        id("second"),
        id("below"),
    );
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.reader_replaced(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));

    p.frame();
    assert_eq!(heard.borrow().begun.len(), 4);
    assert_eq!(p.rect_of(heading).unwrap().1, 40.);

    p.scroll_to(port, (0., 80.)).unwrap();
    p.frame();
    assert_eq!(p.rect_of(heading).unwrap().1, 0.);
    assert_eq!(p.hit(100., 5.), Some(heading));
    assert_eq!(p.hit(100., 30.), Some(first));
    assert_eq!(p.hit(100., 75.), Some(second));

    p.scroll_to(port, (0., 205.)).unwrap();
    p.frame();
    assert_eq!(p.rect_of(heading).unwrap().1, -5.);
    assert_eq!(p.hit(100., 5.), Some(heading));
    assert_eq!(p.hit(100., 20.), Some(below));
    let heard = heard.borrow();
    assert_eq!(heard.begun.len(), 4, "body rows stay recorded");
    assert_eq!(heard.max_group_depth, 1);
    assert_eq!(heard.group_depth, 0);
}

#[test]
fn a_lowered_sticky_section_paints_its_body_inside_the_layer() {
    use crate::host::lower::{LOWERED, LOWER_OPACITY};

    let plan = crate::test_plan(
        r#"component App
  state faded = false
  action fade
    faded = true
  view
    column
      button "Fade" testId="fade" press=fade width=160 height=10
      scroll width=160 height=100
        column
          box width=160 height=5
          column testId="section" width=160 opacity=(faded ? 0.5 : 1) transition="opacity 1s linear"
            box position="sticky" top=0 width=160 height=20
            box testId="body" width=160 height=50 background-color=(faded ? "blue" : "red")
            box width=160 height=80
"#,
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (160., 120.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.host_mut().lowering = true;
    p.host_mut().engine.set_lowered_properties(&LOWERED);
    let Some(ink) = ink() else {
        return;
    };
    p.reader_replaced(Box::new(ink));
    p.frame();
    let section = p.host.kernel().find_by_test_id("section")[0];
    let fade = p.host.kernel().find_by_test_id("fade")[0];
    let fade = p.host.kernel().node_by_key(fade).unwrap().id;
    let body = p.host.kernel().find_by_test_id("body")[0];
    let body = p.host.kernel().node_by_key(body).unwrap().id;
    let (_, y, _, h) = p.rect_of(body).unwrap();
    assert_eq!(
        p.host.dispatch_at(fade, exact_runner::Event::Press, 0.),
        None
    );
    assert_eq!(p.advance(250.), None);
    assert_eq!(p.host.lowered_mask(section), LOWER_OPACITY);
    // The press changed the body too, so it paints again inside the layer,
    // where a row recording would nest in the layer capture and panic.
    let pixel = p.frame().pixel(80, (y + h / 2.0) as u32).unwrap();
    assert!(
        pixel.blue() > pixel.red() && pixel.red() > 0,
        "a translucent blue body: {pixel:?}"
    );
}

#[test]
fn a_rotated_sticky_section_does_not_open_nested_row_groups() {
    let plan = crate::test_plan(
        r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=40
        column width=200 height=180 rotate=1
          column position="sticky" top=0 z-index=1 width=200 height=20
            scroll width=40 height=20
              box width=40 height=80
          box width=200 height=80
          box width=200 height=80
"#,
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (200., 120.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.reader_replaced(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    p.frame();
    let heard = heard.borrow();
    assert_eq!(heard.max_group_depth, 1);
    assert_eq!(heard.group_depth, 0);
}

#[test]
fn a_column_that_clips_by_hiding_is_one_row() {
    // `hidden` makes a scroll container, which its sticky child sticks to.
    let plan = crate::test_plan(
        r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=5
        column overflow="hidden"
          box width=200 height=10 position="sticky" top=0
          box width=200 height=10
          box width=200 height=10
"#,
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (200., 120.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.reader_replaced(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    p.frame();
    assert_eq!(heard.borrow().begun.len(), 2, "the box and the column");
}

#[test]
fn a_list_with_a_sticky_item_is_one_row() {
    let plan = crate::test_plan(
        r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=5
        ol display="flex" flex-direction="column"
          li "One" position="sticky" top=0
          li "Two"
          li "Three"
"#,
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (200., 120.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.reader_replaced(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    p.frame();
    // An item's number counts its siblings, which its own row would not see.
    assert_eq!(heard.borrow().begun.len(), 2, "the box and the list");
}

#[test]
fn a_pressed_split_section_records_its_body_rows_again() {
    // A split section, and a section a scroller wraps as its only child,
    // whose `hidden` clip, on both axes or on y, does not let it split; a
    // scroller's own background is in none of its rows.
    for (source, recorded) in [
        (
            r#"component App
  state presses = 0
  action go
    presses = presses + 1
  view
    scroll width=200 height=100
      column
        box width=200 height=5
        column testId="section" press=go color="red"
          :active
            color = "blue"
          text "Heading" position="sticky" top=0 width=200 height=20
          text "First" width=200 height=30
          column
            text "Inner" position="sticky" top=20 width=200 height=20
            text "Third" width=200 height=30
            text "Fourth" width=200 height=30
"#,
            3,
        ),
        (
            r#"component App
  state presses = 0
  action go
    presses = presses + 1
  view
    scroll width=200 height=100
      column testId="section" press=go color="red" overflow="hidden"
        :active
          color = "blue"
        text "Heading" position="sticky" top=0 width=200 height=20
        text "First" width=200 height=30
        text "Second" width=200 height=30
"#,
            2,
        ),
        (
            r#"component App
  state presses = 0
  action go
    presses = presses + 1
  view
    scroll width=200 height=100
      column testId="section" press=go color="red" overflow-y="hidden"
        :active
          color = "blue"
        text "Heading" position="sticky" top=0 width=200 height=20
        text "First" width=200 height=30
        text "Second" width=200 height=30
"#,
            2,
        ),
        (
            r#"component App
  state presses = 0
  action go
    presses = presses + 1
  view
    column testId="section" width=200 height=100 overflow-y="auto" press=go background-color="white"
      :active
        background-color = "gray"
      text "Heading" position="sticky" top=0 width=200 height=20
      text "First" width=200 height=30
      text "Second" width=200 height=30
"#,
            0,
        ),
    ] {
        let plan = crate::test_plan(source);
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &plan,
            (),
            (200., 120.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let heard = Rc::new(RefCell::new(Heard::default()));
        p.reader_replaced(Box::new(Keeps {
            heard: heard.clone(),
            row: 0,
        }));
        p.frame();
        let section = p.host.kernel().find_by_test_id("section")[0];
        let section = p.host.kernel().node_by_key(section).unwrap().id;
        let (x, y, _, _) = p.rect_of(section).unwrap();
        let before = heard.borrow().begun.len();
        assert!(p.pointer_down(x + 10., y + 30., 0.).unwrap());
        // The body text takes the section's `:active` color: First, and Third
        // and Fourth in the section split within it; in either clipped
        // section, First and Second. The scroller's `:active` background is
        // in none of its rows.
        assert_eq!(heard.borrow().begun.len() - before, recorded, "{source}");
    }
}

#[test]
fn a_rotated_scroller_keeps_a_nested_scrollers_group_on_whole_section_backends() {
    let plan = crate::test_plan(
        r#"component App
  view
    column rotate=1
      scroll width=200 height=100
        column
          box width=200 height=40
          column width=200 height=180
            scroll width=40 height=60
              box width=40 height=200
            box width=200 height=80
"#,
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (200., 120.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let heard = Rc::new(RefCell::new(Heard {
        whole_sections: true,
        ..Heard::default()
    }));
    p.reader_replaced(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    p.frame();
    let heard = heard.borrow();
    // The nested scroller records its own rows, which a move-only frame shows.
    assert_eq!(heard.max_group_depth, 2);
    assert_eq!(heard.group_depth, 0);
}

#[test]
fn sticky_sections_draw_shadows_reaching_into_the_port_from_below() {
    for row in [
        r#"box width=200 height=25 box-shadow="0 -100px 0 red""#,
        r#"text "HHHH" width=200 height=25 text-shadow="0 -100px 0 red""#,
    ] {
        let plan = crate::test_plan(&format!(
            r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=5
        column width=200
          box position="sticky" top=0 width=200 height=20
          box width=200 height=150
          {row}
"#
        ));
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &plan,
            (),
            (200., 120.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let (at, expected) = p
            .frame()
            .pixels()
            .iter()
            .copied()
            .enumerate()
            .find(|(_, pixel)| (pixel.red(), pixel.green(), pixel.blue()) == (255, 0, 0))
            .expect("the shadow casts into the scroll port");

        let Some(ink) = ink() else {
            return;
        };
        p.reader_replaced(Box::new(ink));
        assert_eq!(p.frame().pixels()[at], expected, "{row}");
    }
}

#[test]
fn overflowing_lines_still_draw_when_their_sticky_section_row_is_offscreen() {
    let plan = crate::test_plan(
        r#"component App
  view
    scroll testId="port" width=200 height=100
      column
        box width=200 height=5
        column width=200
          box position="sticky" top=0 width=200 height=20
          text "MMMM MMMM MMMM MMMM MMMM MMMM MMMM MMMM MMMM MMMM MMMM MMMM" width=120 height=10 overflow="visible" font-size=20 line-height="20px" color="red"
          box width=200 height=300
"#,
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (200., 120.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let k = p.host.kernel();
    let port = k.node_by_key(k.find_by_test_id("port")[0]).unwrap().id;
    p.scroll_to(port, (0., 120.)).unwrap();
    let red = |pixel: &tiny_skia::PremultipliedColorU8| {
        pixel.red() > 200 && pixel.green() < 50 && pixel.blue() < 50
    };
    let expected: Vec<_> = p.frame().pixels().iter().map(red).collect();
    assert!(
        expected.iter().any(|red| *red),
        "the ungrouped text is in view"
    );
    let Some(ink) = ink() else {
        return;
    };
    p.reader_replaced(Box::new(ink));
    assert!(
        p.frame()
            .pixels()
            .iter()
            .zip(expected)
            .any(|(pixel, expected)| expected && red(pixel)),
        "Ink draws the text that overflows its row"
    );
}

#[test]
fn a_retained_row_does_not_hold_a_sticky_descendant_at_its_old_position() {
    let source = r#"component App
  view
    scroll testId="port" width=200 height=100
      column
        box width=200 height=40
        column width=200 height=180
          box testId="heading" position="sticky" top=0 z-index=1 width=200 height=20
          box testId="body" width=200 height=120
        box width=200 height=120
"#;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (200., 120.),
        1.25,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let id = |name| {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let (port, heading, body) = (id("port"), id("heading"), id("body"));
    p.reader_replaced(Box::new(Keeps {
        heard: Rc::default(),
        row: 0,
    }));
    p.frame();
    p.scroll_to(port, (0., 80.)).unwrap();
    p.frame();
    assert_eq!(p.hit(5., 5.), Some(heading));
    assert_eq!(p.hit(5., 30.), Some(body));
}

#[test]
fn inline_frame_retained_rows_keep_initial_and_scrolled_viewport_origins() {
    use exact_runner::geometry::InlineGeometry;

    for transform in ["", "scale=1.25 transform-origin=\"0 0\""] {
        let source = format!(
            r#"component App
  view
    column padding-left=30 padding-top=100 {transform}
      scroll testId="port" width=200 height=200
        column
          text height=80
            text "first" testId="first"
          text height=80
            text "second" testId="second"
          box height=400
"#
        );
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(&source),
            (),
            (400., 500.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let id = |name: &str| {
            let k = p.host.kernel();
            k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
        };
        let port = id("port");
        let runs = [id("first"), id("second")];
        let ordinary = runs.map(|id| p.ranges.frame(id));
        assert!(ordinary
            .iter()
            .all(|r| !r.unavailable && r.x >= 30. && r.y >= 100.));
        let heard = Rc::new(RefCell::new(Heard::default()));
        p.reader_replaced(Box::new(Keeps {
            heard: heard.clone(),
            row: 0,
        }));
        p.frame();
        assert_eq!(heard.borrow().begun.len(), 3, "each child is recorded");
        let recorded = runs.map(|id| p.ranges.frame(id));
        heard.borrow_mut().begun.clear();
        p.scroll_to(port, (0., 20.)).unwrap();
        p.frame();
        assert!(
            heard.borrow().begun.is_empty(),
            "scroll replays the recordings"
        );
        let replayed = runs.map(|id| p.ranges.frame(id));
        let shift = if transform.is_empty() { 20. } else { 25. };
        for ((expected, got), moved) in ordinary.into_iter().zip(recorded).zip(replayed) {
            assert!(!got.unavailable && !moved.unavailable);
            assert_eq!(
                (got.x, got.y, got.width, got.height),
                (expected.x, expected.y, expected.width, expected.height),
                "recorded {transform}"
            );
            assert_eq!(
                (moved.x, moved.y, moved.width, moved.height),
                (
                    expected.x,
                    expected.y - shift,
                    expected.width,
                    expected.height
                ),
                "replayed {transform}"
            );
        }
    }
}

#[test]
fn fractional_row_scroll_matches_a_fresh_walk_with_clips_and_hinted_text() {
    let source = r##"component App
  view
    scroll testId="port" width=120 height=80
      column
        view width=120 height=70 clip-path="path('M 5 5 Q 115 0 115 65 L 5 65 Z')" background-color="#2488bb"
          text "Hinted text" font-size=17 color="#ffffff"
        view width=120 height=200 background-color="#44aa33"
"##;
    let (mut p, _) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (140., 100.),
        1.,
        PathBuf::new(),
        PainterChoice::Ink,
    )
    .unwrap();
    p.frame();
    let k = p.host.kernel();
    let port = k.node_by_key(k.find_by_test_id("port")[0]).unwrap().id;
    for offset in [0.25, 0.5, 1., 1.25] {
        p.scroll_to(port, (0., offset)).unwrap();
        let kept = p.frame();
        p.brush.forget_rows();
        p.repaint();
        let fresh = p.frame();
        assert!(kept.data() == fresh.data(), "scroll {offset}");
    }
}

/// Tabs whose pages each hold a list of twenty red rows, half as wide as the
/// list: a plain page, and one that declares its own entrance and exit (LLP
/// 1013.001).
fn lists() -> String {
    let rows: String = (0..20)
        .map(|i| {
            format!("              box height=50 width=100 background-color=\"#ff0000\" testId=\"row-{i}\"\n")
        })
        .collect();
    let page = |when: &str, class: &str| {
        format!(
            "        if {when}
          column navigationKey=`${{e.id}}` {class}testId=`route-${{e.name}}` position=\"absolute\" inset=0 background-color=\"#ffffff\"
            button press=pick(\"people\") testId=`people-${{e.name}}` height=40
              text \"People\"
            button press=open(\"/plain\") testId=`plain-${{e.name}}` height=40
              text \"Plain\"
            button press=open(\"/slide\") testId=`slide-${{e.name}}` height=40
              text \"Slide\"
            scroll testId=`list-${{e.name}}` width=200 height=400
              column
{rows}"
        )
    };
    format!(
        r#"routes nav
  tab home "/"
    plain "/plain"
    slide "/slide"
  tab people "/people"
keyframes enter
  from translate="50% 0"
keyframes leave
  to opacity=0 rotate="10deg"
style Slides
  ::view-transition-new
    animation="enter 400ms linear both"
  ::view-transition-old
    animation="leave 400ms linear both"
component App
  action pick(name: string)
    nav = select(nav, name)
  action open(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${{top(nav).id}}` navigationBack="back" viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
{}{}"#,
        page("e.name != \"slide\"", ""),
        page("e.name == \"slide\"", "class=Slides "),
    )
}

/// The tabs of [`lists`] in a root that a switch away from home scales.
fn scaled_lists() -> String {
    lists().replace(
        "viewport-fit=\"cover\"",
        "testId=\"scaler\" scale=(nav.tab == \"home\" ? 1 : 0.8) viewport-fit=\"cover\"",
    )
}

/// A phone whose painter keeps rows, painted once.
fn kept_phone() -> (Presenter<()>, Rc<RefCell<Heard>>) {
    kept_phone_of(&lists())
}

fn kept_phone_of(source: &str) -> (Presenter<()>, Rc<RefCell<Heard>>) {
    let mut p = stage::tests::phone_source(source);
    let heard = Rc::new(RefCell::new(Heard::default()));
    p.brush.replace_backend(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    stage::tests::after(&mut p, 16.0);
    (p, heard)
}

fn route(p: &Presenter<()>, name: &str) -> exact_kernel::NodeKey {
    let id = stage::tests::id(p, &format!("route-{name}"));
    p.host().kernel().node(id).unwrap().key
}

/// A page the stage keeps a drawing of holds its rows by reference: while
/// its list scrolls, the drawing is kept on every frame and walks no row a
/// second time, and the walk records none again.
#[test]
fn a_kept_page_holds_its_rows_while_they_scroll() {
    let (mut p, heard) = kept_phone();
    let list = stage::tests::id(&p, "list-home");
    let (referred, walked) = p.brush.rows_referred();
    let recorded = p.brush.kept_recorded();
    let rows = std::mem::take(&mut heard.borrow_mut().begun).len();
    assert!(rows > 20, "each list row a kept row: {rows}");
    for step in 1..=10 {
        p.scroll_to(list, (0.0, step as f32 * 20.0));
        stage::tests::after(&mut p, 16.0);
    }
    assert_eq!(
        p.brush.kept_recorded(),
        recorded + 10,
        "kept on every frame"
    );
    let now = p.brush.rows_referred();
    assert_eq!(now.1, walked, "no row walked a second time");
    assert_eq!(now.0 - referred, 10 * rows, "every row, by reference");
    assert!(heard.borrow().begun.is_empty(), "no row recorded again");
}

/// A page a switch removed keeps its rows until its crossing ends: the
/// backend is told to free none while the page's drawing fades, which
/// draws them, and every one once it has.
#[test]
fn a_removed_pages_rows_are_freed_once_its_crossing_ends() {
    let (mut p, heard) = kept_phone();
    let rows: Vec<u32> = std::mem::take(&mut heard.borrow_mut().begun)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(rows.len() > 20, "each list row a kept row");
    let home = route(&p, "home");
    stage::tests::tap(&mut p, "people-home");
    stage::tests::after(&mut p, 20.0);
    assert!(p.host().kernel().node_by_key(home).is_none());
    assert!(p.stage_moving());
    {
        let heard = heard.borrow();
        assert!(rows.iter().all(|id| !heard.freed.contains(id)), "held");
        assert!(rows.iter().all(|id| heard.drawn.contains(id)), "drawn");
    }
    stage::tests::after(&mut p, 300.0);
    assert!(!p.stage_moving());
    assert!(!p.brush.kept(home));
    let heard = heard.borrow();
    assert!(rows.iter().all(|id| heard.freed.contains(id)), "freed");
}

/// A pop committed before any frame showed Back's preview, and a declared
/// exit, pose a drawing that holds its rows by reference: the rows are
/// drawn through the pose, scaled or turned.
#[test]
fn a_posed_drawings_rows_are_drawn_through_its_pose() {
    let (mut p, heard) = kept_phone();
    stage::tests::tap(&mut p, "plain-home");
    stage::tests::after(&mut p, 400.0);
    let now = p.host().now();
    assert!(p.back_progress(0, 0.0, 0, now));
    assert!(p.back_progress(1, 0.3, 0, now + 8.0));
    assert!(p.system_back(now + 8.0));
    p.run_commands(|| ());
    heard.borrow_mut().through.clear();
    stage::tests::after(&mut p, 16.0);
    assert!(p.stage_moving(), "the pop slides from the preview's pose");
    let scaled = |ts: &Transform| ts.sx < 0.999 && ts.kx == 0.0 && ts.ky == 0.0;
    assert!(heard.borrow().through.iter().any(|(_, ts)| scaled(ts)));
    stage::tests::after(&mut p, 400.0);
    stage::tests::tap(&mut p, "slide-home");
    stage::tests::after(&mut p, 500.0);
    assert!(!p.stage_moving());
    assert!(p.system_back(p.host().now()));
    p.run_commands(|| ());
    heard.borrow_mut().through.clear();
    stage::tests::after(&mut p, 100.0);
    assert!(p.stage_moving(), "the exit plays");
    assert!(heard.borrow().through.iter().any(|(_, ts)| ts.kx != 0.0));
}

/// A switch that scales an ancestor the removed page shared with the page
/// it shows draws the removed page's rows at the new scale.
#[test]
fn a_removed_pages_rows_take_a_surviving_ancestors_scale() {
    let (mut p, heard) = kept_phone_of(&scaled_lists());
    let home = route(&p, "home");
    let referred = p.brush.rows_referred().0;
    assert!(referred > 0, "the page holds its rows by reference");
    stage::tests::tap(&mut p, "people-home");
    stage::tests::after(&mut p, 20.0);
    assert!(p.host().kernel().node_by_key(home).is_none());
    let heard = heard.borrow();
    assert!(heard.through.len() > 5, "{}", heard.through.len());
    let near = |a: f32| (a - 0.8).abs() < 1e-4;
    assert!((heard.through.iter()).all(|(_, ts)| near(ts.sx) && near(ts.sy) && ts.kx == 0.0));
}

/// The same on backends that draw: the removed page's rows, half as wide
/// as their list, end where they scale to, not a scale-1 row's width past
/// their origin. vello holds them by reference; Ink records their draw
/// calls into the drawing.
#[test]
fn a_removed_pages_rows_scale_in_pixels() {
    let backends: [(Box<dyn Backend>, bool); 2] = [
        (
            Box::new(crate::ink::Ink::new().expect("an Ink device")),
            false,
        ),
        (Box::new(crate::gpu::Gpu::new().expect("a GPU")), true),
    ];
    for (backend, referred) in backends {
        let mut p = stage::tests::phone_source(&scaled_lists());
        p.brush.replace_backend(backend);
        stage::tests::after(&mut p, 16.0);
        assert_eq!(p.brush.rows_referred().0 > 0, referred);
        let mut rect = |name: &str| {
            let id = stage::tests::id(&p, name);
            p.rect_of(id).unwrap()
        };
        let (scaler, list) = (rect("scaler"), rect("list-home"));
        let centre = (scaler.0 + scaler.2 / 2.0, scaler.1 + scaler.3 / 2.0);
        let scaled = |x: f32, c: f32| c + (x - c) * 0.8;
        let y = scaled(list.1 + list.3 / 2.0, centre.1) as u32;
        let left = scaled(list.0, centre.0);
        stage::tests::tap(&mut p, "people-home");
        stage::tests::after(&mut p, 20.0);
        let frame = p.frame();
        let row = frame.pixel((left + 40.0) as u32, y).unwrap();
        assert!(row.red() > 150 && row.green() < 150, "a row: {row:?}");
        let past = frame.pixel((left + 90.0) as u32, y).unwrap();
        assert!(past.green() > 150, "the list past its rows: {past:?}");
    }
}

mod reach;
