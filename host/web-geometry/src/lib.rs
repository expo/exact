//! `frame` and `measure` on the web (LLP 1051.000 D4): the page answers.
//!
//! The browser lays out, so the kernel's layout there is not the page's: both
//! reads ask the page through one import, `exact_geometry.read`, which
//! `host/web/geometry-glue.js` answers from the live DOM within the call;
//! `elementFromPoint` (LLP 1094 D10) asks `exact_geometry.point` for the
//! view hit, and the runner names it by its nearest `id`. `exactGeometry`
//! (LLP 1123 D6) asks `exact_geometry.fact` for a view's drawn box in the
//! space it names. Until that piece has loaded (after first paint), every
//! read answers `unavailable`.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

use exact_kernel::{Kernel, ViewId};
use exact_runner::geometry::{GeometryAnswer, GeometryLinks, PageBox, PageSpace, Scrolled};

/// The page's answers, for [`exact_runner::RunnerLinks::geometry`].
pub static PAGE: GeometryLinks = GeometryLinks::new(
    page_frame,
    page_measure,
    page_point,
    page_points,
    page_shown,
)
.with_range_frame(page_range_frame)
.with_scroll_metrics(page_scroll_metrics)
.with_page_box(page_box);

/// `fact`'s spaces: the document's, the viewport's, a node's border box's.
const OWNER: u32 = 0;
const WINDOW: u32 = 1;
const NODE: u32 = 2;

/// Bit of `fact`'s reply: the frame was placed in the space asked.
const FRAMED: u32 = 4;

/// A view's drawn box for `exactGeometry`: its size, and its frame in
/// `space` where the reply says it is placed there.
fn page_box(view: ViewId, space: PageSpace) -> Option<PageBox> {
    let (space, node) = match space {
        PageSpace::Owner => (OWNER, 0),
        PageSpace::Window => (WINDOW, 0),
        PageSpace::Node(node) => (NODE, node.map_or(0, |n| n + 1)),
    };
    let mut out = [0.0f64; 4];
    let flags = fact(view, space, node, &mut out);
    page_reply(flags, out)
}

/// The page's reply to `fact` as a box: its size always, its frame where
/// placed; nothing without the answered bit or with a bad number.
fn page_reply(flags: u32, [x, y, width, height]: [f64; 4]) -> Option<PageBox> {
    let finite = [x, y, width, height].iter().all(|v| v.is_finite());
    if flags & ANSWERED == 0 || !finite || width < 0.0 || height < 0.0 {
        return None;
    }
    Some(PageBox {
        size: [width, height],
        frame: (flags & FRAMED != 0).then_some([x, y, width, height]),
    })
}

#[cfg(target_arch = "wasm32")]
fn fact(view: ViewId, space: u32, node: u32, out: &mut [f64; 4]) -> u32 {
    #[link(wasm_import_module = "exact_geometry")]
    extern "C" {
        fn fact(view: u32, space: u32, node: u32, out: *mut f64) -> u32;
    }
    // SAFETY: the page writes at most four f64s into this owned, aligned
    // buffer and calls nothing back into the module.
    unsafe { fact(view, space, node, out.as_mut_ptr()) }
}

/// Off the web there is no page to ask: a reply without the answered bit.
#[cfg(not(target_arch = "wasm32"))]
fn fact(_: ViewId, _: u32, _: u32, _: &mut [f64; 4]) -> u32 {
    0
}

#[cfg(target_arch = "wasm32")]
fn page_scroll_metrics(_: &Kernel, _: &Scrolled, view: ViewId) -> Option<[f64; 6]> {
    #[link(wasm_import_module = "exact_geometry")]
    extern "C" {
        fn scroll_metrics(view: u32, out: *mut f64) -> u32;
    }
    let mut out = [0.; 6];
    // SAFETY: the synchronous page reader writes six f64 values into this buffer.
    (unsafe { scroll_metrics(view, out.as_mut_ptr()) } != 0).then_some(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn page_scroll_metrics(_: &Kernel, _: &Scrolled, _: ViewId) -> Option<[f64; 6]> {
    None
}

#[cfg(target_arch = "wasm32")]
fn page_range_frame(
    _: &Kernel,
    _: &Scrolled,
    view: ViewId,
    start: u32,
    end: u32,
) -> GeometryAnswer {
    #[link(wasm_import_module = "exact_geometry")]
    extern "C" {
        fn range(view: u32, start: u32, end: u32, out: *mut f64) -> u32;
    }
    let mut out = [0.0; 4];
    // SAFETY: the synchronous page reader writes four f64 values to this buffer.
    let flags = unsafe { range(view, start, end, out.as_mut_ptr()) };
    answer(flags, out)
}

#[cfg(not(target_arch = "wasm32"))]
fn page_range_frame(_: &Kernel, _: &Scrolled, _: ViewId, _: u32, _: u32) -> GeometryAnswer {
    GeometryAnswer::UNAVAILABLE
}

#[cfg(target_arch = "wasm32")]
fn page_shown(_: &Kernel, _: &Scrolled, view: ViewId, first: bool) -> bool {
    #[link(wasm_import_module = "exact_geometry")]
    extern "C" {
        fn shown(view: u32, first: u32) -> u32;
    }
    // SAFETY: a view id and a boolean in, one boolean out; the page calls nothing back.
    unsafe { shown(view, u32::from(first)) != 0 }
}

#[cfg(not(target_arch = "wasm32"))]
fn page_shown(_: &Kernel, _: &Scrolled, _: ViewId, _: bool) -> bool {
    false
}

/// The import's `op`: a layout box (0) or its `height: auto` measure (1).
const FRAME: u32 = 0;
const MEASURE: u32 = 1;

/// Bits of the import's reply: answered, and provisional.
const ANSWERED: u32 = 1;
const PROVISIONAL: u32 = 2;

fn page_frame(_: &Kernel, _: &Scrolled, view: ViewId) -> GeometryAnswer {
    read(FRAME, view)
}

fn page_measure(_: &mut Kernel, _: &Scrolled, view: ViewId) -> GeometryAnswer {
    read(MEASURE, view)
}

fn page_point(_: &Kernel, _: &Scrolled, x: f64, y: f64) -> Option<ViewId> {
    point(x, y).checked_sub(1)
}

fn page_points(_: &Kernel, _: &Scrolled, x: f64, y: f64) -> Vec<ViewId> {
    points(x, y)
        .into_iter()
        .filter_map(|id| id.checked_sub(1))
        .collect()
}

/// The page's reply as an answer: four numbers when answered.
fn answer(flags: u32, [x, y, width, height]: [f64; 4]) -> GeometryAnswer {
    let finite = [x, y, width, height].iter().all(|v| v.is_finite());
    if flags & ANSWERED == 0 || !finite || width < 0.0 || height < 0.0 {
        return GeometryAnswer::UNAVAILABLE;
    }
    GeometryAnswer {
        x,
        y,
        width,
        height,
        provisional: flags & PROVISIONAL != 0,
        unavailable: false,
    }
}

#[cfg(target_arch = "wasm32")]
fn read(op: u32, view: ViewId) -> GeometryAnswer {
    #[link(wasm_import_module = "exact_geometry")]
    extern "C" {
        fn read(op: u32, view: u32, out: *mut f64) -> u32;
    }
    let mut out = [0.0f64; 4];
    // SAFETY: the page writes at most four f64s into this owned, aligned
    // buffer and calls nothing back into the module.
    let flags = unsafe { read(op, view, out.as_mut_ptr()) };
    answer(flags, out)
}

/// The view hit at `(x, y)`, plus one; 0 for none.
#[cfg(target_arch = "wasm32")]
fn point(x: f64, y: f64) -> u32 {
    #[link(wasm_import_module = "exact_geometry")]
    extern "C" {
        fn point(x: f64, y: f64) -> u32;
    }
    // SAFETY: two numbers in, one out; the page calls nothing back.
    unsafe { point(x, y) }
}

#[cfg(target_arch = "wasm32")]
fn points(x: f64, y: f64) -> Vec<u32> {
    #[link(wasm_import_module = "exact_geometry")]
    extern "C" {
        fn points(x: f64, y: f64, out: *mut u32, cap: u32) -> u32;
    }
    // SAFETY: a null pointer with zero capacity asks only for the length.
    let len = unsafe { points(x, y, std::ptr::null_mut(), 0) };
    let mut out = vec![0; len as usize];
    // SAFETY: the page writes at most `len` u32s to this owned buffer.
    let written = unsafe { points(x, y, out.as_mut_ptr(), len) }.min(len);
    out.truncate(written as usize);
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn points(_: f64, _: f64) -> Vec<u32> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn point(_: f64, _: f64) -> u32 {
    0
}

/// Off the web there is no page to ask: a reply without the answered bit.
#[cfg(not(target_arch = "wasm32"))]
fn read(_: u32, _: ViewId) -> GeometryAnswer {
    answer(0, [0.0; 4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_without_the_answered_bit_or_with_a_bad_number_is_unavailable() {
        assert!(answer(0, [1.0, 2.0, 3.0, 4.0]).unavailable);
        assert!(answer(ANSWERED, [f64::NAN, 2.0, 3.0, 4.0]).unavailable);
        assert!(answer(ANSWERED, [1.0, 2.0, -3.0, 4.0]).unavailable);
        let a = answer(ANSWERED | PROVISIONAL, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!((a.x, a.y, a.width, a.height), (1.0, 2.0, 3.0, 4.0));
        assert!(a.provisional && !a.unavailable);
    }

    #[test]
    fn a_fact_reply_is_its_size_and_its_frame_only_where_placed() {
        assert_eq!(page_reply(0, [1.0, 2.0, 3.0, 4.0]), None);
        assert_eq!(page_reply(ANSWERED, [1.0, 2.0, -3.0, 4.0]), None);
        assert_eq!(
            page_reply(ANSWERED | FRAMED, [1.0, f64::INFINITY, 3.0, 4.0]),
            None
        );
        let placed = page_reply(ANSWERED | FRAMED, [1.0, 2.0, 3.0, 4.0]).unwrap();
        assert_eq!(placed.size, [3.0, 4.0]);
        assert_eq!(placed.frame, Some([1.0, 2.0, 3.0, 4.0]));
        assert_eq!(
            page_reply(ANSWERED, [0.0, 0.0, 3.0, 4.0]).unwrap().frame,
            None
        );
    }
}
