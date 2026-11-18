//! A box's CSS `filter` for the reader to run on the GPU (`FILTER`, op 53;
//! LLP 1172.023.001 D1). The painter otherwise rasterizes the box into a
//! window-sized picture and runs the chain on the CPU (`paint/box_filter.rs`)
//! on every frame that walks it, and a picture keeps no row. Here the box's
//! own ops go between `FILTER` and `FILTER_END`; the reader records them into
//! a `RenderNode` whose `RenderEffect` is the chain, which HWUI runs on the
//! GPU and keeps until the row is recorded again, as Compose's `Modifier.blur`
//! does.
use super::{Recorder, Rect4};
use exact_kernel::svg::filter::{Input, Op, Primitive, Transfer};
use tiny_skia::Transform;

/// `[53, len, x y w h, n, (kind args…)×n]`: the region in local units under
/// the MATRIX before it, then the functions in order, each over the last's
/// result. Kinds: 0 a blur `σx σy` (local units); 1 a colour matrix, 20 words
/// as Android's `ColorMatrix` (over unpremultiplied colour, offsets 0–255);
/// 2 a drop shadow `σ dx dy color` (local units, ARGB). `len` counts the words
/// after it.
pub(super) const FILTER: u32 = 53;
/// The end of what the innermost open `FILTER` filters.
pub(super) const FILTER_END: u32 = 54;

/// `EXACT_FILTERS=0` leaves filters to the painter's CPU chain, to compare;
/// the reader sets it below Android 12, which has no `RenderEffect`.
static FILTERS: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| !std::env::var("EXACT_FILTERS").is_ok_and(|v| v == "0"));

/// HWUI draws an effect's input into a texture the size of its node and keeps
/// it while the node is drawn. A box larger than this on a side (device
/// pixels), larger than `FILTER_WINDOWS` windows, or one that would take what
/// kept rows and the page hold past `HELD_WINDOWS` windows stays on the CPU,
/// which only filters what the window shows (LLP 1172.023 E6).
const MAX_SIDE: f32 = 8192.0;
const FILTER_WINDOWS: f32 = 2.0;
const HELD_WINDOWS: f32 = 4.0;

/// Frames a freed row's layers count, the one that frees it included: the
/// reader lets a row's node go as it takes the stream after the one that
/// frees it (`ExactView.free`). While the row replacing it is still recorded
/// on the reader's worker it holds the node until that lands, which the
/// stream does not report.
const RELEASE: u64 = 2;

impl Recorder {
    /// Open a filter over `region` in `ts`'s space; whether its chain is one
    /// the reader runs.
    pub(super) fn filter_open(
        &mut self,
        primitives: &[Primitive],
        region: Rect4,
        ts: Transform,
    ) -> bool {
        // On either path, a row's bounds hold the filter's reach (`row_end`).
        if let Some(row) = &mut self.row {
            row.filtered = true;
        }
        if !*FILTERS || !self.planes.is_empty() {
            return false;
        }
        // A turned or skewed box keeps the CPU chain, which blurs in its space.
        let Some(page) = self.on_page(region, ts) else {
            return false;
        };
        let s = self.scale;
        if page.2 * s > MAX_SIDE || page.3 * s > MAX_SIDE {
            return false;
        }
        let window = self.direct.size.0 * self.direct.size.1 * s * s;
        let area = page.2 * page.3 * s * s;
        // The row this one replaces still counts: the reader draws it until
        // this one is made. So do rows freed and not yet let go.
        let kept: f32 = self.filter_areas.values().sum();
        let leaving: f32 = self.filter_leaving.iter().map(|(a, _)| a).sum();
        let held = kept + leaving + self.page_area + self.row.as_ref().map_or(0.0, |r| r.area);
        if area > FILTER_WINDOWS * window || held + area > HELD_WINDOWS * window {
            return false;
        }
        let Some((count, words)) = encode(primitives, (ts.sx * s, ts.sy * s)) else {
            return false;
        };
        match &mut self.row {
            Some(row) => row.area += area,
            None => self.page_area += area,
        }
        // CSS clips the filtered result: every pending clip goes before it.
        self.need(None);
        self.transform(ts);
        self.direct.isolation += 1;
        self.ops.extend([FILTER, 5 + words.len() as u32]);
        for v in [region.0, region.1, region.2, region.3] {
            self.f(v);
        }
        self.ops.push(count);
        self.ops.extend(words);
        true
    }

    /// A frame begins: rows the reader has let go no longer count.
    pub(super) fn filter_frame(&mut self) {
        let frames = self.frames;
        self.filter_leaving.retain(|(_, until)| *until > frames);
    }

    /// Row `id` is freed: its layers count until the reader can have let it
    /// go.
    pub(super) fn filter_free(&mut self, id: u32) {
        if let Some(area) = self.filter_areas.remove(&id) {
            self.filter_leaving.push((area, self.frames + RELEASE));
        }
    }

    pub(super) fn filter_close(&mut self) {
        self.direct.isolation = self.direct.isolation.saturating_sub(1);
        self.ops.push(FILTER_END);
        // The reader is back at its matrix from before the filter.
        self.matrix = None;
    }
}

/// The chain's functions as the stream writes them, and how many; `None`
/// when one is not a CSS filter function a `RenderEffect` runs as the
/// painter would. `k`: device pixels per local unit.
fn encode(primitives: &[Primitive], k: (f32, f32)) -> Option<(u32, Vec<u32>)> {
    // HWUI blurs nothing up to half a device pixel, where the CPU chain does.
    let faint = |sx: f32, sy: f32| sx > 0.0 && sx * k.0 <= 0.5 || sy > 0.0 && sy * k.1 <= 0.5;
    let mut words = Vec::new();
    for (i, p) in primitives.iter().enumerate() {
        let input = match i {
            0 => Input::SourceGraphic,
            _ => Input::Result(i as u16 - 1),
        };
        if p.linear || p.inputs[0] != input {
            return None;
        }
        match &p.op {
            Op::Blur(sx, sy) if !faint(*sx, *sy) => words.extend([0, sx.to_bits(), sy.to_bits()]),
            Op::ColorMatrix(m) => {
                words.push(1);
                words.extend(android_matrix(*m).map(f32::to_bits));
            }
            Op::ComponentTransfer(channels) => {
                let mut m = [0.0; 20];
                for (k, f) in channels.iter().enumerate() {
                    let (slope, intercept) = match f {
                        Transfer::Identity => (1.0, 0.0),
                        Transfer::Linear(s, i) => (*s, *i),
                        // Two table entries interpolate as a line.
                        Transfer::Table(v) if v.len() == 2 => (v[1] - v[0], v[0]),
                        _ => return None,
                    };
                    m[k * 5 + k] = slope;
                    m[k * 5 + 4] = intercept;
                }
                words.push(1);
                words.extend(android_matrix(m).map(f32::to_bits));
            }
            Op::DropShadow(sx, sy, dx, dy, color) if sx == sy && !faint(*sx, *sy) => {
                let c = color.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                words.extend([
                    2,
                    sx.to_bits(),
                    dx.to_bits(),
                    dy.to_bits(),
                    Recorder::color(c),
                ]);
            }
            _ => return None,
        }
    }
    Some((primitives.len() as u32, words))
}

/// A 4×5 matrix with offsets in 0–1, as Android's `ColorMatrix` takes it.
fn android_matrix(mut m: [f32; 20]) -> [f32; 20] {
    for row in 0..4 {
        m[row * 5 + 4] *= 255.0;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::super::{
        stream, FINISHED, IMAGE_DEF, ROW_BEGIN, ROW_DRAW, ROW_END, ROW_PAD, UNCLIPPED,
    };
    use super::*;
    use crate::paint::Backend;
    use crate::presenter::{PainterChoice, Presenter};

    #[test]
    fn the_reader_runs_filters_the_window_bounds_and_blurs_it_can_see() {
        let blur = |s: f32| {
            [Primitive {
                op: Op::Blur(s, s),
                inputs: [Input::SourceGraphic, Input::None],
                subregion: (0.0, 0.0, 0.0, 0.0),
                linear: false,
            }]
        };
        let ts = Transform::identity();
        let mut r = Recorder::new();
        // A window of 200 × 200 device pixels.
        r.begin(100.0, 100.0, 2.0);
        // HWUI does not blur half a device pixel or less; the CPU chain does.
        assert!(!r.filter_open(&blur(0.25), (0.0, 0.0, 10.0, 10.0), ts));
        assert!(r.filter_open(&blur(0.3), (0.0, 0.0, 10.0, 10.0), ts));
        r.filter_close();
        // Over two windows.
        assert!(!r.filter_open(&blur(4.0), (0.0, 0.0, 100.0, 201.0), ts));
        // 1.5 windows a row: the third would hold more than four.
        for id in 1..=3 {
            r.row_begin(id, (0.0, 0.0), None);
            assert_eq!(
                r.filter_open(&blur(4.0), (0.0, 0.0, 100.0, 150.0), ts),
                id < 3,
                "row {id}"
            );
            r.filter_close();
            r.row_end((0.0, 0.0, 100.0, 150.0));
        }
        r.row_free(1);
        // The reader lets row 1 go as it takes the next stream.
        r.row_begin(4, (0.0, 0.0), None);
        assert!(
            !r.filter_open(&blur(4.0), (0.0, 0.0, 100.0, 150.0), ts),
            "row 1 still held"
        );
        r.row_end((0.0, 0.0, 100.0, 150.0));
        r.begin(100.0, 100.0, 2.0);
        r.begin(100.0, 100.0, 2.0);
        r.row_begin(5, (0.0, 0.0), None);
        assert!(
            r.filter_open(&blur(4.0), (0.0, 0.0, 100.0, 150.0), ts),
            "row 1 let go"
        );
    }

    #[test]
    fn a_replaced_rows_layers_count_until_the_reader_lets_it_go() {
        let blur = |s: f32| {
            [Primitive {
                op: Op::Blur(s, s),
                inputs: [Input::SourceGraphic, Input::None],
                subregion: (0.0, 0.0, 0.0, 0.0),
                linear: false,
            }]
        };
        let (ts, region) = (Transform::identity(), (0.0, 0.0, 100.0, 150.0));
        let mut r = Recorder::new();
        // A window of 200 × 200 device pixels; two rows of 1.5 windows.
        r.begin(100.0, 100.0, 2.0);
        for id in 1..=2 {
            r.row_begin(id, (0.0, 0.0), None);
            assert!(r.filter_open(&blur(4.0), region, ts));
            r.filter_close();
            r.row_end(region);
        }
        // Row 1 recorded again with another chain: the reader draws it until
        // the new recording is made, so both count, 4.5 windows.
        r.row_begin(3, (0.0, 0.0), Some(1));
        assert!(
            !r.filter_open(&blur(5.0), region, ts),
            "row 1 is still held"
        );
        r.row_end(region);
        r.row_free(1);
        r.begin(100.0, 100.0, 2.0);
        r.row_begin(4, (0.0, 0.0), Some(3));
        assert!(
            !r.filter_open(&blur(5.0), region, ts),
            "the reader takes the stream that frees row 1 now"
        );
        r.row_end(region);
        r.row_free(3);
        r.begin(100.0, 100.0, 2.0);
        r.row_begin(5, (0.0, 0.0), Some(4));
        assert!(r.filter_open(&blur(5.0), region, ts), "row 1 let go");
    }

    /// Chains the reader runs, each over a box and what it holds: the
    /// reader's JVM tests replay this stream through HWUI and hold it to the
    /// CPU chain's pixels (`FilterTest.kt`). The first box's blur reaches
    /// past the page's corner; the rest put a chain's order, a drop shadow
    /// under what it shadows, opacity, an ancestor's clip and the box's own
    /// `clip-path` after the filter.
    const PARITY: &str = r##"component App
  view
    box width=320 height=240 background-color="#ffffff"
      box width=60 height=40 background-color="#ef4444" filter="blur(6px)"
        box width=20 height=20 margin=10 background-color="#2563eb"
      row margin-top=24 margin-left=24 gap=40
        box width=40 height=40 background-color="#0ea5e9" filter="invert(1) sepia(1) contrast(3) brightness(0.5)"
        box width=40 height=40 background-color="#22c55e" filter="drop-shadow(6px 8px 3px #0f172a) hue-rotate(90deg) saturate(1.5)"
          box width=16 height=16 margin=12 background-color="#fde047"
        box width=40 height=40 background-color="#a855f7" opacity=0.5 filter="drop-shadow(10px 10px 0px #0f172a) blur(2px)"
      row margin-top=40 margin-left=24 gap=40
        box width=60 height=60 overflow="hidden" border-radius=12 background-color="#e2e8f0"
          box width=40 height=40 margin=10 background-color="#f97316" filter="drop-shadow(14px 14px 2px #000000)"
        box width=40 height=40 background-color="#14b8a6" clip-path="inset(0px 0px 0px 0px)" filter="drop-shadow(12px 0px 0px #000000)"
        box width=40 height=40 background-color="#64748b" filter="grayscale(1) brightness(1.4) opacity(0.6)"
"##;

    #[test]
    fn the_reader_is_held_to_the_cpu_chain_on_this_stream() {
        let boot = |choice| {
            let (p, error) = Presenter::boot_with_fixture_fonts(
                &crate::test_plan(PARITY),
                (),
                (320., 240.),
                1.,
                std::path::PathBuf::new(),
                choice,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p
        };
        let mut p = boot(PainterChoice::Cpu);
        p.brush.replace_backend(Box::new(Recorder::new()));
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).expect("a frame");
        let mut filters = 0;
        let mut at = 0;
        while at < ops.len() {
            assert_ne!(ops[at], IMAGE_DEF, "every chain is the reader's");
            filters += usize::from(ops[at] == FILTER);
            at = stream::end(&ops, at).expect("a valid stream");
        }
        assert_eq!(filters, 7);
        // When the stream changes, these two are written again from `ops`
        // and the CPU frame (`Pixmap::encode_png`).
        let bytes: Vec<u8> = ops.iter().flat_map(|n| n.to_le_bytes()).collect();
        assert_eq!(
            bytes,
            include_bytes!("../../../android/canvas/src/test/resources/box-filter.ops")
        );
        let reference = tiny_skia::Pixmap::decode_png(include_bytes!(
            "../../../android/canvas/src/test/resources/box-filter.png"
        ))
        .unwrap();
        assert_eq!(boot(PainterChoice::Cpu).frame().data(), reference.data());
    }

    #[test]
    fn the_cpu_chain_pictures_a_rows_filter_past_its_origin() {
        use super::super::{IMAGE, IMAGE_RRECT};
        // Too faint for HWUI's blur, the shadow keeps the CPU chain; it falls
        // 60 points left of and above its row's origin, where the row is
        // walked from.
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(
                r##"component App
  view
    scroll width=300 height=300
      column
        box height=100 width=200 background-color="#2468ac" filter="drop-shadow(-60px -60px 0.4px #000000)"
"##,
            ),
            (),
            (300., 300.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.brush.replace_backend(Box::new(Recorder::new()));
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).expect("a frame");
        let mut placed = None;
        let mut at = 0;
        while at < ops.len() {
            if ops[at] == IMAGE || ops[at] == IMAGE_RRECT {
                placed = Some([2, 3, 4, 5].map(|k| f32::from_bits(ops[at + k])));
            }
            at = match ops[at] {
                ROW_BEGIN => at + 6,
                _ => stream::end(&ops, at).expect("a valid stream"),
            };
        }
        let [x, y, w, h] = placed.expect("the box is pictured");
        assert!(
            x <= -60.0 && y <= -60.0 && w >= 260.0 && h >= 160.0,
            "the picture holds the shadow past the row's origin: {:?}",
            (x, y, w, h)
        );
    }

    #[test]
    fn a_filtered_row_records_its_chain_and_stays_kept() {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(
                r##"component App
  view
    scroll width=300 height=300
      column
        box height=100 width=300 background-color="#2468ac" filter="blur(4px) brightness(0.5)"
        box height=100 width=300 background-color="#2468ac"
        box height=100 width=300 background-color="#2468ac" filter="blur(20px)"
"##,
            ),
            (),
            (400., 400.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.brush.replace_backend(Box::new(Recorder::new()));
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).expect("a frame");
        let (mut row, mut filter, mut recorded, mut heads) = (false, None, Vec::new(), Vec::new());
        let mut at = 0;
        while at < ops.len() {
            match ops[at] {
                ROW_BEGIN => {
                    row = true;
                    recorded.push(ops[at + 1]);
                    heads.push((ops[at + 2] & UNCLIPPED != 0, f32::from_bits(ops[at + 4])));
                    // Into the row's ops, past its header.
                    at += 6;
                    continue;
                }
                ROW_END => row = false,
                FILTER => {
                    assert!(row, "the filter is drawn in its row");
                    filter.get_or_insert(at);
                }
                IMAGE_DEF => panic!("no picture: the reader filters the box"),
                _ => {}
            }
            at = stream::end(&ops, at).expect("a valid stream");
        }
        let at = filter.expect("a FILTER op");
        let f = |k: usize| f32::from_bits(ops[at + k]);
        assert_eq!(ops[at + 6], 2, "two functions");
        assert_eq!((ops[at + 7], f(8), f(9)), (0, 4.0, 4.0), "the blur");
        assert_eq!(ops[at + 10], 1, "a colour matrix");
        let m: Vec<f32> = (11..31).map(f).collect();
        assert_eq!(
            (m[0], m[6], m[12], m[18], m[4]),
            (0.5, 0.5, 0.5, 1.0, 0.0),
            "brightness"
        );
        assert!(ops[at + 31..].contains(&FILTER_END));
        // blur(20px) reaches 3σ = 60 pt past its box, past the row's 48 pt of
        // room: the row's node grows to hold it and is drawn unclipped.
        let unclipped: Vec<bool> = heads.iter().map(|h| h.0).collect();
        assert_eq!(unclipped, [false, false, true]);
        assert!(heads[2].1 >= 100.0 + 60.0 + ROW_PAD, "{:?}", heads[2]);

        p.repaint();
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).expect("a frame");
        let mut at = 0;
        let mut drawn = Vec::new();
        while at < ops.len() {
            assert_ne!(ops[at], ROW_BEGIN, "an unchanged filtered row is kept");
            if ops[at] == ROW_DRAW {
                drawn.push(ops[at + 1]);
            }
            at = stream::end(&ops, at).expect("a valid stream");
        }
        assert_eq!(drawn, recorded);
    }
}
