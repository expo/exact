//! Common-prefix replay checkpointing for full-redraw 2D canvases.
//!
//! When a canvas redraws from a clean slate (`Op::Reset` or an unclipped
//! full-canvas `Op::ClearRect`) and repeats an unchanged prefix of drawing
//! operations across consecutive draws (for example, background images and
//! finished strokes followed by a growing active stroke), the replayer
//! checkpoints the bitmap and context state at the last paint operation in the
//! shared prefix and replays only the diverging tail.

use super::{is_full_clear_rect, Env, State};
use exact_canvas::list::{Op, Record, HEADER, MAGIC, VERSION};
use std::sync::{Arc, Weak};
use tiny_skia::{PathBuilder, Pixmap, Transform};

/// The last single-list full-redraw recorded for prefix comparison.
pub(super) struct LastDraw {
    pub bytes: Vec<u8>,
}

/// A weak reference distinguishes a replaced handle even if the allocator
/// reuses its address. A missing handle also invalidates when it arrives.
pub(super) type ImageReads = Vec<(String, Option<Weak<super::ImagePixels>>)>;

/// Snapshot of a canvas after replaying `bytes` from `entry_state`.
pub(super) struct Checkpoint {
    pub recs: usize,
    pub bytes: Vec<u8>,
    pub starts_with_reset: bool,
    pub entry_state: State,
    pub objects_epoch: u64,
    pub text_ptr: usize,
    pub images: ImageReads,
    pub pixmap: Arc<Pixmap>,
    /// Pixels painted after this checkpoint in the previous draw.
    pub tail: Option<tiny_skia::IntRect>,
    pub state: State,
    pub stack: Vec<State>,
    pub path: PathBuilder,
    pub cleared: bool,
}

impl Checkpoint {
    pub(super) fn matches(
        &self,
        bytes: &[u8],
        target_recs: usize,
        entry_state: &State,
        objects_epoch: u64,
        text_ptr: usize,
        env: &Env<'_>,
    ) -> bool {
        if self.recs > target_recs
            || bytes.len() < self.bytes.len()
            || bytes[..self.bytes.len()] != self.bytes
            || self.objects_epoch != objects_epoch
            || self.text_ptr != text_ptr
            || (!self.starts_with_reset && !self.entry_state.eq_unclipped(entry_state))
        {
            return false;
        }
        self.images
            .iter()
            .all(|(src, previous)| match (previous, env.images.get(src)) {
                (None, None) => true,
                (Some(previous), Some(current)) => {
                    previous.as_ptr() == Arc::as_ptr(&current.pixels)
                }
                _ => false,
            })
    }
}

/// Where a single-list full redraw can checkpoint.
pub(super) struct PrefixPlan {
    /// Number of leading records to checkpoint (`0` when no paint op falls in
    /// the shared prefix).
    pub checkpoint_recs: usize,
    /// Byte length of `0..checkpoint_recs` including the header.
    pub checkpoint_bytes: usize,
    /// Whether `Op::Reset` ran before any state-reading operation, making
    /// the prefix independent of `entry_state`.
    pub starts_with_reset: bool,
}

/// Number of leading complete records whose encoded bytes are identical in
/// `a` and `b`.
pub(super) fn common_record_count(a: &[u8], b: &[u8]) -> usize {
    if a.len() < HEADER || b.len() < HEADER || a[..HEADER] != b[..HEADER] {
        return 0;
    }
    let word = |buf: &[u8], at: usize| u32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
    if word(a, 0) != MAGIC || word(a, 4) != VERSION {
        return 0;
    }
    let mut at = HEADER;
    let mut count = 0;
    while at + 8 <= a.len() && at + 8 <= b.len() {
        let n = word(a, at + 4) as usize;
        let Some(next) = n
            .checked_mul(8)
            .and_then(|ops| ops.checked_add(8))
            .and_then(|len| at.checked_add(len))
        else {
            break;
        };
        if next > a.len() || next > b.len() || a[at..next] != b[at..next] {
            break;
        }
        at = next;
        count += 1;
    }
    count
}

fn is_paint_op(op: Op) -> bool {
    matches!(
        op,
        Op::Fill
            | Op::FillPath
            | Op::Stroke
            | Op::StrokePath
            | Op::FillRect
            | Op::StrokeRect
            | Op::FillText
            | Op::StrokeText
            | Op::DrawImage
            | Op::PutImageData
    )
}

fn is_object_op(op: Op) -> bool {
    matches!(
        op,
        Op::LinearGradient
            | Op::RadialGradient
            | Op::ConicGradient
            | Op::ColorStop
            | Op::Pattern
            | Op::PatternTransform
            | Op::Image
    )
}

fn reads_path(op: Op) -> bool {
    matches!(
        op,
        Op::MoveTo
            | Op::LineTo
            | Op::QuadTo
            | Op::CubicTo
            | Op::ClosePath
            | Op::Fill
            | Op::Stroke
            | Op::Clip
    )
}

/// Verify that `recs` starts with a self-contained full-canvas clear (before
/// any paint or clip) and find the last safe paint boundary inside
/// `recs[..common_recs]`.
///
/// Returns `None` when the list does not start with a full-canvas clear (for
/// example, an accumulating canvas).
pub(super) fn scan_prefix(
    recs: &[Record<'_>],
    common_recs: usize,
    pw: u32,
    ph: u32,
    base: Transform,
    entry_author: Transform,
) -> Option<PrefixPlan> {
    let mut saw_clear = false;
    let mut starts_with_reset = false;
    let mut saw_state_before_reset = false;
    let mut path_reset = false;
    let mut scratch_dirty = false;
    let mut clipped = false;
    let mut author = entry_author;
    let mut author_stack: Vec<(Transform, bool)> = Vec::new();
    let mut bytes_at = HEADER;
    let mut checkpoint_recs = 0;
    let mut checkpoint_bytes = HEADER;
    let mut can_checkpoint = true;

    for (idx, r) in recs.iter().enumerate() {
        let rec_bytes = 8 + r.len() * 8;
        let next_bytes = bytes_at + rec_bytes;
        let in_common = idx < common_recs;

        if !saw_clear {
            match r.op {
                Op::Reset => {
                    saw_clear = true;
                    starts_with_reset = !saw_state_before_reset;
                    path_reset = true;
                    scratch_dirty = false;
                    clipped = false;
                    author = Transform::identity();
                    author_stack.clear();
                }
                Op::ClearRect if !clipped => {
                    let ts = base.pre_concat(author);
                    if is_full_clear_rect(ts, r.at(0), r.at(1), r.at(2), r.at(3), pw, ph) {
                        saw_clear = true;
                    } else {
                        return None;
                    }
                }
                Op::Clip | Op::ClipPath | Op::ClearRect => return None,
                op if is_paint_op(op) => return None,
                Op::Save => {
                    saw_state_before_reset = true;
                    author_stack.push((author, clipped));
                }
                Op::Restore => {
                    saw_state_before_reset = true;
                    if let Some((a, c)) = author_stack.pop() {
                        author = a;
                        clipped = c;
                    }
                }
                Op::SetTransform => {
                    saw_state_before_reset = true;
                    author = Transform::from_row(
                        r.at(0) as f32,
                        r.at(1) as f32,
                        r.at(2) as f32,
                        r.at(3) as f32,
                        r.at(4) as f32,
                        r.at(5) as f32,
                    );
                }
                Op::BeginPath => path_reset = true,
                Op::PathMoveTo
                | Op::PathLineTo
                | Op::PathQuadTo
                | Op::PathCubicTo
                | Op::PathClose => scratch_dirty = true,
                op if reads_path(op) && !path_reset => {
                    if in_common {
                        can_checkpoint = false;
                    }
                }
                op if is_object_op(op) => {
                    if in_common {
                        can_checkpoint = false;
                    }
                }
                _ => {
                    saw_state_before_reset = true;
                }
            }
            bytes_at = next_bytes;
            if !in_common {
                // Only needed to see whether the draw starts with a full clear.
                if saw_clear {
                    break;
                }
            }
            continue;
        }

        if !in_common || !can_checkpoint {
            break;
        }

        if is_object_op(r.op) || (reads_path(r.op) && !path_reset) {
            break;
        }
        match r.op {
            Op::Reset => {
                path_reset = true;
                scratch_dirty = false;
                clipped = false;
                author = Transform::identity();
                author_stack.clear();
            }
            Op::Save => author_stack.push((author, clipped)),
            Op::Restore => {
                if let Some((a, c)) = author_stack.pop() {
                    author = a;
                    clipped = c;
                }
            }
            Op::SetTransform => {
                author = Transform::from_row(
                    r.at(0) as f32,
                    r.at(1) as f32,
                    r.at(2) as f32,
                    r.at(3) as f32,
                    r.at(4) as f32,
                    r.at(5) as f32,
                );
            }
            Op::BeginPath => path_reset = true,
            Op::PathMoveTo | Op::PathLineTo | Op::PathQuadTo | Op::PathCubicTo | Op::PathClose => {
                scratch_dirty = true
            }
            Op::FillPath | Op::StrokePath | Op::ClipPath => scratch_dirty = false,
            Op::Clip => clipped = true,
            _ => {}
        }
        bytes_at = next_bytes;
        if is_paint_op(r.op) && !scratch_dirty {
            checkpoint_recs = idx + 1;
            checkpoint_bytes = bytes_at;
        }
    }

    saw_clear.then_some(PrefixPlan {
        checkpoint_recs,
        checkpoint_bytes,
        starts_with_reset,
    })
}
