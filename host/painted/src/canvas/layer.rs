//! The recorder's layers (`crate::host::lower`): a node its reader animates
//! is drawn by reference (`LAYER_REF`) from its row, its drawing sent apart
//! (`LAYER_SET`) only when it changes, with the transform, pivot and base
//! values the reader moves it from. A layer no kept row or frame draws any
//! more is freed (`LAYER_FREE`); its keyframes (`TRACKS`) come from the host
//! after the paint.
use super::{Recorder, RESTORE};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use tiny_skia::Transform;

/// `[29, id]`: draw layer `id` here (its reader's node, recorded from the
/// last `LAYER_SET`).
pub(super) const LAYER_REF: u32 = 29;
/// `[30, id, len, m0..m5, px, py, tx, ty, scale, rotate, opacity, r, dash,
/// at, ops…]`: a layer's matrix (points → the stream's pixels), its pivot and
/// base values in points before the matrix (`dash`: path units per dash
/// offset unit), where its drawing's dash phase is (a word index into the
/// ops, or `u32::MAX`), then its drawing.
pub(super) const LAYER_SET: u32 = 30;
/// `[31, id, len, words…]`: a layer's keyframes (`Host::layer_tracks`);
/// none: it shows its base values.
pub(crate) const TRACKS: u32 = 31;
/// `[32, id]`: layer `id` is gone.
pub(super) const LAYER_FREE: u32 = 32;
/// `[33, lo, hi]`: the monotonic clock's nanoseconds at the host's zero, so
/// the reader samples tracks on the engine's clock.
pub(crate) const CLOCK: u32 = 33;

thread_local! {
    /// The layers alive after the last finished recording: id, node, base.
    pub(crate) static ALIVE: RefCell<Vec<(u32, u64, [f32; 7])>> = const { RefCell::new(Vec::new()) };
}

/// The recorder's layer state.
#[derive(Default)]
pub(super) struct Layers {
    ids: HashMap<u64, u32>,
    info: HashMap<u32, (u64, [f32; 7])>,
    next: u32,
    /// Each layer's header and drawing as last sent.
    sent: HashMap<u32, Vec<u32>>,
    /// Layers recording, innermost last.
    open: Vec<Open>,
    /// `LAYER_SET`s to write after the row (or the outermost layer).
    pub(super) sets: Vec<u32>,
    /// The layers each kept row draws, those the recording row draws, and
    /// those drawn outside rows this frame.
    rows: HashMap<u32, Vec<u32>>,
    row: Vec<u32>,
    frame: Vec<u32>,
}

struct Open {
    id: u32,
    dash: Option<u32>,
    header: Vec<u32>,
    ops: Vec<u32>,
    matrix: Option<[f32; 6]>,
    emitted: Vec<bool>,
}

impl Layers {
    pub(super) fn needs_world(&self) -> bool {
        self.sent
            .values()
            .any(|ops| super::stream::needs_world(&ops[16..]))
    }

    pub(super) fn unlayered(&self) -> bool {
        self.open.is_empty()
    }

    /// A new frame: nothing open, nothing drawn outside rows yet.
    pub(super) fn begin(&mut self) {
        self.open.clear();
        self.sets.clear();
        self.frame.clear();
        self.row.clear();
    }

    /// The recording row ended as kept row `id`.
    pub(super) fn row_end(&mut self, id: u32) {
        self.rows.insert(id, std::mem::take(&mut self.row));
    }

    /// Row `id` is drawn this frame: its layers are alive while the reader
    /// draws it, which it does until the next stream even when the row is
    /// freed in this one (a row the painter could not keep: one with a
    /// hosted field or a caret is recorded again at every paint and freed
    /// at its end). Counted by the row's freeing alone, such a row's layers
    /// were freed in the stream that set them, and got no tracks.
    pub(super) fn row_drawn(&mut self, id: u32) {
        if let Some(layers) = self.rows.get(&id) {
            self.frame.extend(layers);
        }
    }

    /// Kept row `id` is gone.
    pub(super) fn row_free(&mut self, id: u32) {
        self.rows.remove(&id);
    }

    /// The innermost open layer is recorded again by its reader in a colour
    /// transition's colours.
    pub(super) fn note_recolored(&mut self) {
        if let Some(open) = self.open.last_mut() {
            open.dash.get_or_insert(RECOLORED);
        }
    }

    /// A dash phase is written at `at` (an index into the recording ops):
    /// the innermost open layer's first is the one its reader moves.
    pub(super) fn note_dash(&mut self, at: usize) {
        if let Some(open) = self.open.last_mut() {
            open.dash.get_or_insert(at as u32);
        }
    }
}

/// In a `LAYER_SET`'s dash word: no dash, and the reader keeps the drawing
/// to record it again in a transition's colours (`crate::host::lower`).
pub(super) const RECOLORED: u32 = u32::MAX - 1;

impl Recorder {
    /// Open layer `key` (see [`crate::paint::Backend::layer_begin`]).
    pub(super) fn layer_open(
        &mut self,
        key: u64,
        ts: Transform,
        pivot: (f32, f32),
        base: [f32; 7],
    ) {
        // The clips around the layer stay where they are while the reader
        // moves it: written before it, never in its drawing, where the
        // first drawing that needs them would put them (a clipped spinner
        // turned its container's clip with it). Written whole, not as the
        // drawing's bounds need: the layer moves past them.
        if !self.clips.is_empty() {
            self.materialize(self.clips.len() - 1);
        }
        let l = &mut self.layers;
        let id = *l.ids.entry(key).or_insert_with(|| {
            l.next += 1;
            l.next
        });
        l.info.insert(id, (key, base));
        if self.row.is_some() {
            l.row.push(id);
        } else {
            l.frame.push(id);
        }
        self.ops.extend([LAYER_REF, id]);
        let s = self.scale;
        let m = [
            ts.sx * s,
            ts.ky * s,
            ts.kx * s,
            ts.sy * s,
            (ts.tx - self.origin.0) * s,
            (ts.ty - self.origin.1) * s,
        ];
        let header = m
            .into_iter()
            .chain([pivot.0, pivot.1])
            .chain(base)
            .map(f32::to_bits)
            .collect();
        let emitted = self.clips.iter().map(|c| c.emitted()).collect();
        let ops = std::mem::take(&mut self.ops);
        let matrix = self.matrix.take();
        self.layers.open.push(Open {
            id,
            dash: None,
            header,
            ops,
            matrix,
            emitted,
        });
    }

    /// Close the innermost open layer.
    pub(super) fn layer_close(&mut self) {
        let Some(Open {
            id,
            dash,
            header,
            ops,
            matrix,
            emitted,
        }) = self.layers.open.pop()
        else {
            return;
        };
        // Clips the drawing wrote close inside the layer: outside, pending again.
        for i in (0..self.clips.len()).rev() {
            if self.clips[i].emitted() && !emitted.get(i).copied().unwrap_or(false) {
                self.ops.push(RESTORE);
                self.clips[i].reopen();
            }
        }
        let body = std::mem::replace(&mut self.ops, ops);
        self.matrix = matrix;
        let mut full = header;
        full.push(dash.unwrap_or(u32::MAX));
        full.extend(body);
        let l = &mut self.layers;
        if l.sent.get(&id) != Some(&full) {
            l.sets.extend([LAYER_SET, id, full.len() as u32]);
            l.sets.extend(&full);
            l.sent.insert(id, full);
        }
        // Outside rows and other layers, the drawing follows at once.
        if self.row.is_none() && l.open.is_empty() {
            let sets = std::mem::take(&mut l.sets);
            self.ops.extend(sets);
        }
    }

    /// At the frame's end: free layers nothing draws, and publish the rest.
    pub(super) fn layers_finish(&mut self) {
        let l = &mut self.layers;
        let alive: HashSet<u32> = l.rows.values().flatten().chain(&l.frame).copied().collect();
        let dead: Vec<u32> = l
            .info
            .keys()
            .filter(|id| !alive.contains(id))
            .copied()
            .collect();
        for id in dead {
            if let Some((key, _)) = l.info.remove(&id) {
                l.ids.remove(&key);
            }
            l.sent.remove(&id);
            self.ops.extend([LAYER_FREE, id]);
        }
        let list = l
            .info
            .iter()
            .map(|(id, (key, base))| (*id, *key, *base))
            .collect();
        ALIVE.with(|a| *a.borrow_mut() = list);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{stream::end, Recorder, Shape, FINISHED};
    use super::*;
    use crate::paint::Backend;

    /// One frame: a row drawing a layer (kept, or freed at the frame's end
    /// as a row with a hosted field is); the stream's layer ops and the
    /// layers alive after it.
    fn frame(r: &mut Recorder, keep: bool) -> (Vec<u32>, Vec<u32>) {
        r.begin(400.0, 400.0, 1.0);
        r.row_begin(7, (0.0, 0.0), None);
        assert!(r.layer_begin(
            42,
            Transform::identity(),
            (10.0, 10.0),
            [0., 0., 1., 0., 1., 0., 0.]
        ));
        r.fill(
            &Shape::rect((0.0, 0.0, 20.0, 20.0)),
            [255, 0, 0, 255],
            Transform::identity(),
        );
        r.layer_end();
        let row = r.row_end((0.0, 0.0, 20.0, 20.0));
        r.row_draw(row, (0.0, 0.0), (0.0, 0.0, 20.0, 20.0));
        if !keep {
            r.row_free(row);
        }
        let _ = r.finish();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut layers = Vec::new();
        let mut at = 0;
        while at < ops.len() {
            if matches!(ops[at], LAYER_SET | LAYER_FREE) {
                layers.push(ops[at]);
            }
            at = end(&ops, at).unwrap();
        }
        let alive = ALIVE.with(|a| a.borrow().iter().map(|l| l.0).collect());
        (layers, alive)
    }

    /// A layer in a row the painter cannot keep stays alive while the
    /// reader draws that row: it gets its tracks, and it is not freed in the
    /// stream that sets it (issue #83's spinner beside hosted fields).
    #[test]
    fn a_layer_in_a_row_freed_after_its_frame_lives_while_the_row_is_drawn() {
        for keep in [true, false] {
            let mut r = Recorder::new();
            let (ops, alive) = frame(&mut r, keep);
            assert_eq!(ops, [LAYER_SET], "kept: {keep}");
            assert_eq!(alive.len(), 1, "kept: {keep}");
            // Recorded again the next frame: the same layer, not set again.
            let (ops, again) = frame(&mut r, keep);
            assert_eq!((ops, again), (vec![], alive), "kept: {keep}");
        }
    }

    /// The ops a stream draws, rows' bodies in place, and each layer's
    /// drawing apart.
    fn walk(b: &[u32], drawn: &mut Vec<u32>, layers: &mut Vec<Vec<u32>>) {
        use super::super::ROW_BEGIN;
        let mut i = 0;
        while i < b.len() {
            let j = end(b, i).unwrap();
            match b[i] {
                ROW_BEGIN => walk(&b[i + 6..j - 1], drawn, layers),
                LAYER_SET => {
                    let mut body = Vec::new();
                    walk(&b[i + 19..j], &mut body, layers);
                    layers.push(body);
                }
                op => drawn.push(op),
            }
            i = j;
        }
    }

    /// Material's indicator (the Material look, so a process of its own) in
    /// a container that clips half of it: the container's clip is written
    /// around the indicator's layer, not inside the drawing the reader
    /// turns, where the clip turned with it and let the clipped half show.
    #[test]
    fn a_spinner_turns_inside_its_containers_clip() {
        use super::super::{clip::CLIP_UNION, CLIP_PATH, CLIP_RRECT, STROKE};
        use crate::presenter::{PainterChoice, Presenter};
        const CHILD: &str = "EXACT_MATERIAL_LOOK_TEST";
        if std::env::var_os(CHILD).is_none() {
            let name = std::thread::current().name().unwrap().to_owned();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &name, "--nocapture"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        crate::app::show_material_look();
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(
                r##"component App
  view
    column padding=10
      box width=20 height=40 overflow="hidden"
        progress aria-label="Loading" width=40 height=40
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
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let (mut drawn, mut layers) = (Vec::new(), Vec::new());
        walk(&ops, &mut drawn, &mut layers);
        let clip = |op: &u32| matches!(*op, CLIP_RRECT | CLIP_PATH | CLIP_UNION);
        let spinner = layers
            .iter()
            .find(|l| l.contains(&STROKE))
            .expect("its layer");
        assert!(
            !spinner.iter().any(clip),
            "the container's clip turns with it"
        );
        let at = drawn.iter().position(|op| *op == LAYER_REF).unwrap();
        assert!(drawn[..at].iter().any(clip), "no clip around it: {drawn:?}");
    }
}
