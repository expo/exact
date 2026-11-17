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

/// HWUI draws an effect's input into a texture the size of its node; a box
/// larger than this on a side (device pixels) stays on the CPU, which only
/// filters what the window shows.
const MAX_SIDE: f32 = 8192.0;

impl Recorder {
    /// Open a filter over `region` in `ts`'s space; whether its chain is one
    /// the reader runs.
    pub(super) fn filter_open(
        &mut self,
        primitives: &[Primitive],
        region: Rect4,
        ts: Transform,
    ) -> bool {
        if !*FILTERS || !self.planes.is_empty() {
            return false;
        }
        // A turned or skewed box keeps the CPU chain, which blurs in its space.
        let Some(page) = self.on_page(region, ts) else {
            return false;
        };
        if page.2 * self.scale > MAX_SIDE || page.3 * self.scale > MAX_SIDE {
            return false;
        }
        let Some((count, words)) = encode(primitives) else {
            return false;
        };
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

    pub(super) fn filter_close(&mut self) {
        self.direct.isolation = self.direct.isolation.saturating_sub(1);
        self.ops.push(FILTER_END);
        // The reader is back at its matrix from before the filter.
        self.matrix = None;
    }
}

/// The chain's functions as the stream writes them, and how many; `None`
/// when one is not a CSS filter function a `RenderEffect` runs as the
/// painter would.
fn encode(primitives: &[Primitive]) -> Option<(u32, Vec<u32>)> {
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
            Op::Blur(sx, sy) => words.extend([0, sx.to_bits(), sy.to_bits()]),
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
            Op::DropShadow(sx, sy, dx, dy, color) if sx == sy => {
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
    use super::super::{stream, FINISHED, IMAGE_DEF, ROW_BEGIN, ROW_DRAW, ROW_END};
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

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
        let (mut row, mut filter, mut recorded) = (false, None, Vec::new());
        let mut at = 0;
        while at < ops.len() {
            match ops[at] {
                ROW_BEGIN => {
                    row = true;
                    recorded.push(ops[at + 1]);
                    // Into the row's ops, past its header.
                    at += 6;
                    continue;
                }
                ROW_END => row = false,
                FILTER => {
                    assert!(row, "the filter is drawn in its row");
                    filter = Some(at);
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
