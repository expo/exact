//! Paragraph-stamp invalidation across Apple host batches (#1762): position
//! switches under decorated ancestors, viewport-relative inline-image boxes,
//! and lazy symbol re-measurement without frame changes.

use exact_apple::Host;
use exact_kernel::{
    MonospaceMeasurer, ParagraphStamp, SymbolRequest, TextMeasureRequest, TextMeasurer, TextMetrics,
};
use exact_runner::{DataError, DataSource, Event, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, s: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(s.into()))
    }
}

fn view<D: DataSource>(host: &Host<D>, test_id: &str) -> u32 {
    let k = host.runner().kernel();
    let key = k.find_by_test_id(test_id)[0];
    k.node_by_key(key).unwrap().id
}

#[test]
fn switching_to_absolute_while_translating_resends_paragraph_under_underline() {
    let plan = contract::compile(
        r#"component App
  state abs = false
  state x = 0
  action detach
    abs = true
    x = 10
  action slide
    x = x + 10
  view
    column text-decoration-line="underline"
      button "Detach" testId="detach" press=detach
      button "Slide" testId="slide" press=slide
      text "hello" position=(abs ? "absolute" : "relative") translate=`${x}px 0px` testId="label"
"#,
    )
    .unwrap();
    let (mut host, _) = Host::boot(
        &plan.encode(),
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.,
        300.,
    )
    .unwrap();
    let label = view(&host, "label");
    let count_paragraphs = |batch: &str| {
        let json: serde_json::Value = serde_json::from_str(batch).unwrap();
        json["ops"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["id"] == label && o["op"] == "paragraph")
            .count()
    };
    let detach = host.dispatch(view(&host, "detach"), Event::Press);
    assert_eq!(count_paragraphs(&detach), 1, "{detach}");
    let slide = host.dispatch(view(&host, "slide"), Event::Press);
    assert_eq!(count_paragraphs(&slide), 0, "{slide}");
}

#[test]
fn inline_image_geometry_updates_paragraph_on_viewport_resize() {
    // Trailing vw margin inside a fixed-size paragraph: neither the image's nor
    // the paragraph's published frame moves, both without and with an exactViewport() reader.
    for with_reader in [false, true] {
        let source = if with_reader {
            r#"shape Viewport
  width: number
component App
  resource vp = exactViewport() as shape Viewport
  view
    column width=300
      text width=300 height=40 testId="line"
        text `${vp.width > 0 ? "before " : ""}`
        image "icon.png" width=16 height=16 margin-right="10vw" testId="pic"
        text " after"
"#
        } else {
            r#"component App
  view
    column width=300
      text width=300 height=40 testId="line"
        text "before "
        image "icon.png" width=16 height=16 margin-right="10vw" testId="pic"
        text " after"
"#
        };
        let plan = contract::compile(source).unwrap();
        let (mut host, first) = Host::boot(
            &plan.encode(),
            NoData,
            Box::new(MonospaceMeasurer::default()),
            400.,
            300.,
        )
        .unwrap();
        assert!(
            first.contains("\"inlineBox\":\"56 16 0 0\""),
            "with_reader={with_reader}: {first}"
        );
        let resized = host.resize(800., 300.);
        assert!(
            resized.contains("\"inlineBox\":\"96 16 0 0\""),
            "viewport resize must resend inline image margin box even with fixed paragraph/image frames (with_reader={with_reader}): {resized}"
        );
    }
}

#[test]
fn inline_symbol_measured_with_stable_zero_frame_updates_paragraph() {
    struct UnplacedSymbolMeasurer {
        inner: MonospaceMeasurer,
        rev: Arc<AtomicU64>,
    }
    impl TextMeasurer for UnplacedSymbolMeasurer {
        fn measure_revision(&self) -> u64 {
            self.rev.load(Ordering::Relaxed)
        }
        fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
            self.inner.measure(request)
        }
        fn measure_identified(
            &mut self,
            stamp: &ParagraphStamp,
            request: &TextMeasureRequest<'_>,
        ) -> TextMetrics {
            self.inner.measure_identified(stamp, request)
        }
        fn inline_boxes(
            &mut self,
            _stamp: &ParagraphStamp,
            request: &TextMeasureRequest<'_>,
            out: &mut Vec<(f32, f32)>,
        ) {
            // Simulate a clipped/flow-excluded inline box whose published frame stays 0x0.
            let count = request.runs.iter().filter(|r| r.inline.is_some()).count();
            out.resize(count, (f32::NAN, f32::NAN));
        }
        fn symbol_size(&mut self, _request: &SymbolRequest<'_>) -> Option<(f32, f32)> {
            let side = if self.rev.load(Ordering::Relaxed) == 1 {
                12.0
            } else {
                22.0
            };
            Some((side, side))
        }
    }
    let rev = Arc::new(AtomicU64::new(1));
    let plan = contract::compile(
        r#"component App
  view
    column width=300
      text width=300 height=80 testId="line"
        text "before "
        image "symbol:star" testId="sym"
        text " after"
"#,
    )
    .unwrap();
    let (mut host, first) = Host::boot(
        &plan.encode(),
        NoData,
        Box::new(UnplacedSymbolMeasurer {
            inner: MonospaceMeasurer::default(),
            rev: Arc::clone(&rev),
        }),
        400.,
        300.,
    )
    .unwrap();
    assert!(first.contains("\"inlineBox\":\"12 12 0 0\""), "{first}");
    let line = view(&host, "line");
    let sym = view(&host, "sym");
    let sym_frame_before = host.runner().kernel().node(sym).unwrap().frame;
    assert_eq!(
        (sym_frame_before.width, sym_frame_before.height),
        (0.0, 0.0)
    );

    // Bump measurer traits so layout's measure_symbols re-measures the inline symbol
    // while both the paragraph frame (300x80) and the symbol frame (0x0) stay identical.
    rev.store(2, Ordering::Relaxed);
    let updated = host.resize(400., 300.);
    let json: serde_json::Value = serde_json::from_str(&updated).unwrap();
    let ops = json["ops"].as_array().unwrap();
    let has_measured_symbol = ops.iter().any(|op| {
        op["op"] == "paragraph"
            && op["id"] == line
            && op["runs"].as_array().is_some_and(|runs| {
                runs.iter()
                    .any(|run| run["props"]["inlineBox"] == "22 22 0 0")
            })
    });
    assert!(
        has_measured_symbol,
        "re-measuring an inline symbol with stable 0x0 frame must emit its updated inlineBox: {updated}"
    );
    let sym_frame_after = host.runner().kernel().node(sym).unwrap().frame;
    assert_eq!((sym_frame_after.width, sym_frame_after.height), (0.0, 0.0));
}
