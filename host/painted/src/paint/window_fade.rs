//! `startViewTransition()` on the painted hosts (LLP 1013.003): the action's
//! commit cross-fades the whole window from how it looked before, as the
//! web's root view transition does with its default animation, over 0.25 s
//! with CSS `ease`.
//!
//! The runner applies a commit to the tree before its commands reach the
//! host, so the window's look before the commit is gone by the time the
//! command is read. A plan that can start one therefore keeps the drawing
//! of each presented frame ([`Painter::keep_window`]), walking it a second
//! time into a tape as the stage keeps a route's (`stage.rs`). A plan that
//! names no `startViewTransition` keeps nothing and pays nothing.
//!
//! The fade draws the kept drawing over everything the new frame paints,
//! its opacity falling from 1 to 0: each pixel is the old look's share
//! plus the new look's, as the web's two images cross with `plus-lighter`.
//! A frame painted while one fades keeps that blend, so a second
//! transition that starts during the first starts from what showed. A
//! fading frame repaints the whole window (`damage.rs`).
use super::tape::Tape;
use super::*;

/// The web's default root view transition: 0.25 s.
const FADE_MS: f64 = 250.0;

/// The kept drawing and the one fading.
#[derive(Default)]
pub(crate) struct WindowFade {
    /// Whether the plan can start a view transition, read once.
    plan: Option<bool>,
    /// The last presented frame's drawing.
    last: Option<Rc<Tape>>,
    /// The drawing fading over the window, when it began, and its opacity
    /// at this paint.
    fading: Option<(Rc<Tape>, f64, f32)>,
}

impl WindowFade {
    /// Read whether the plan can start one, the first time it is asked.
    pub(crate) fn know(&mut self, plan: impl FnOnce() -> bool) {
        self.plan.get_or_insert_with(plan);
    }

    /// Whether each presented frame keeps its drawing.
    fn keeps(&self) -> bool {
        self.plan == Some(true)
    }

    /// The last presented frame starts to fade out at `now`. False when
    /// there is no such frame, as before the first one.
    pub(crate) fn begin(&mut self, now: f64) -> bool {
        let Some(last) = self.last.clone() else {
            return false;
        };
        self.fading = Some((last, now, 1.0));
        true
    }

    /// The fade's opacity at `now`, ending it once 0.25 s have passed.
    pub(crate) fn at(&mut self, now: f64) {
        let Some((_, start, opacity)) = &mut self.fading else {
            return;
        };
        let t = (now - *start) / FADE_MS;
        if t >= 1.0 {
            self.fading = None;
        } else {
            *opacity = (1.0 - exact_motion::Easing::Ease.progress(t)) as f32;
        }
    }

    /// Whether a drawing fades over the window: every frame is owed.
    pub(crate) fn fading(&self) -> bool {
        self.fading.is_some()
    }

    /// When the fade ends.
    pub(crate) fn ends(&self) -> Option<f64> {
        (self.fading.as_ref()).map(|(_, start, _)| start + FADE_MS)
    }
}

impl Painter {
    /// After the walk has drawn the roots, the stage, sheets and alert:
    /// keep a presented frame's drawing, then draw the one fading over it.
    pub(super) fn window_fade(
        &mut self,
        walk: &mut Walk<'_, '_>,
        page: (f64, f64),
        last: Option<ViewId>,
        presented: bool,
    ) {
        if presented && self.window.keeps() {
            self.keep_window(walk, page, last);
        }
        if self.window.fading() {
            self.draw_fading();
            self.damage.unsupported = true;
        }
    }

    /// This frame as a drawing of its own: each root and its stage as the
    /// walk drew them, then the sheets, a dragged ghost and the alert, and
    /// the drawing fading over them.
    fn keep_window(&mut self, walk: &mut Walk<'_, '_>, page: (f64, f64), last: Option<ViewId>) {
        let refer = self.backend.rows_through();
        let roots = walk.scene.roots.to_vec();
        let tape = self.record_with(walk, refer, |painter, walk| {
            for root in roots {
                painter.node(walk, root, Transform::identity(), page, None);
                painter.paint_stage(walk, root, Some(root) == last);
            }
            painter.paint_sheets(walk);
            painter.paint_ghost(walk);
            painter.paint_alert(walk);
            painter.draw_fading();
        });
        self.window.last = Some(Rc::new(tape));
    }

    /// The fading drawing over what is drawn, at the fade's opacity.
    fn draw_fading(&mut self) {
        let Some((tape, _, opacity)) = self.window.fading.clone() else {
            return;
        };
        self.backend.push_opacity(opacity);
        tape.replay(
            self.backend.as_mut(),
            &mut self.text.borrow_mut(),
            Transform::identity(),
        );
        self.backend.pop_opacity();
    }
}
