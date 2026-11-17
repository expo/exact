//! What the recorder says when the paint walk asks for something the
//! reader's stream has no op for (LLP 1159.000 D1), and the bounds of a
//! path it does record.
use super::*;

/// Say, once a process for each, what the recorder was asked to draw and its
/// stream cannot carry: the author's style is not drawn on Android.
pub(super) fn undrawn(what: &'static str) {
    static SAID: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    if !said.contains(&what) {
        said.push(what);
        eprintln!(
            "exact canvas: {what} is not drawn on Android: the reader's stream has no op for it"
        );
    }
}

/// The bounds of a path's points (control points included).
pub(super) fn path_bounds(ops: &[PathOp]) -> Option<Rect4> {
    let mut b: Option<(f32, f32, f32, f32)> = None;
    let mut add = |x: f32, y: f32| {
        b = Some(match b {
            None => (x, y, x, y),
            Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x), e.max(y)),
        })
    };
    for op in ops {
        match *op {
            PathOp::Move(x, y) | PathOp::Line(x, y) => add(x, y),
            PathOp::Cubic(a, c, d, e, f, g) => {
                add(a, c);
                add(d, e);
                add(f, g);
            }
            PathOp::Close => {}
        }
    }
    b.map(|(x0, y0, x1, y1)| (x0, y0, x1 - x0, y1 - y0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the paint walk rasterises itself reaches the reader as a picture
    /// at its own transform; the default body this replaced dropped it.
    #[test]
    fn an_island_is_recorded_as_a_picture_in_its_transform() {
        let dst = (10., 20., 31., 17.);
        let record = |ts: Transform, mode: u8| {
            let mut r = Recorder::new();
            r.island_image(Arc::new(Pixmap::new(31, 17).unwrap()), dst, ts, mode);
            r.ops
        };
        let placed = |ops: &[u32]| {
            let want = [dst.0, dst.1, dst.2, dst.3].map(f32::to_bits);
            // Alone in its rectangle it is the rounded picture op, its `dst` first.
            ops.windows(6)
                .any(|w| (w[0] == IMAGE || w[0] == IMAGE_RRECT) && w[2..] == want)
        };
        let ops = record(Transform::identity(), 0);
        assert!(placed(&ops), "{ops:?}");
        // The walk asks before it makes a text stroke's band.
        assert!(Recorder::new().supports_island_image());
        assert_ne!(ops, record(Transform::from_translate(12.0, 9.0), 0));
        // A blended island is still placed, unblended.
        assert!(placed(&record(Transform::identity(), 1)));
    }
}
