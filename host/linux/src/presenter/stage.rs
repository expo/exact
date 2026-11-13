//! The routes a presenter places itself (LLP 1115.001 D10, D12), drawn by
//! the painter's stage (`paint/stage.rs`).
//!
//! **Sheets.** Where a shell shows bars (Android's reader), a `modal`
//! route is Material 3's modal bottom sheet: as wide as the window up to
//! 640, rounded 28 at its top, `surfaceContainerLow` under a route that
//! says no background, a 32 by 4 drag handle 22 below its top edge, over a
//! scrim of 32 % black that dims the page and its bars. It rests at its
//! `navigationDetent` heights (`medium` half the tallest, `large` the
//! window below the status bar, a point height or a percentage of the
//! tallest, each with the bar under its foot added below), opening at
//! `navigationInitialDetent`, else the first. It is laid out once, at its
//! tallest, as Compose lays out a sheet's content: at a lower detent the
//! rest is below the window's edge. It slides up as it opens and down as it
//! leaves, and while the keyboard is up it rests at its tallest. A route
//! pushed while a sheet is on top is shown in it (expo/exact#326), and the
//! sheet keeps the detents of the route that opened it.
//!
//! **Pushes and pops** cross on Material's shared X axis: the route that
//! leaves slides 30 toward where it goes and fades in the first 35 % of
//! 300 ms, the one that arrives slides in 30 from the other side and fades
//! in over the rest. Predictive Back previews a pop or a sheet's dismissal
//! as Material's guidance has it (`stage/touch.rs`). Nothing moves while the
//! person asks for less motion.
//!
//! A shell with no bars (desktop Linux, a headless run) shows a modal
//! route as written, with no motion, and a route pushed inside it in the
//! box the sheet's own route was laid out in.
use super::*;
use crate::navigation::{sheet_of, Presented};
use crate::paint::stage::{Item, Stage};
use crate::paint::tape::Tape;
use crate::paint::{Rect4, Shape};
use exact_kernel::{BlockAxis, HostBlock, NodeKey};
use std::rc::Rc;
use tiny_skia::Transform;

#[cfg(test)]
mod tests;
mod touch;

/// The room a sheet's drag handle takes at its top, points: Material's
/// handle, 4 tall with 22 above and below.
pub const SHEET_HANDLE: f32 = 48.0;
/// Material 3's sheet: its widest, its top corners, and its scrim.
const SHEET_MAX_WIDTH: f32 = 640.0;
const SHEET_CORNER: f32 = 28.0;
const SCRIM: f32 = 0.32;
/// Material's motion: a sheet's entrance (emphasized decelerate), its exit
/// (emphasized accelerate), a settle between detents, and the shared axis.
const ENTER_MS: f64 = 350.0;
const EXIT_MS: f64 = 200.0;
const SETTLE_MS: f64 = 300.0;
const AXIS_MS: f64 = 300.0;
const AXIS_SHIFT: f32 = 30.0;
/// The share of the shared axis the leaving route fades in.
const AXIS_FADE: f32 = 0.35;

/// Material 3's easing curves (`MotionTokens`), as cubic Béziers.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Curve {
    Decelerate,
    Accelerate,
    Standard,
}

impl Curve {
    fn points(self) -> (f32, f32, f32, f32) {
        match self {
            Curve::Decelerate => (0.05, 0.7, 0.1, 1.0),
            Curve::Accelerate => (0.3, 0.0, 0.8, 0.15),
            Curve::Standard => (0.2, 0.0, 0.0, 1.0),
        }
    }

    /// The curve's progress at `t`, both 0 to 1.
    fn ease(self, t: f32) -> f32 {
        let (x1, y1, x2, y2) = self.points();
        let at = |a: f32, b: f32, s: f32| {
            let u = 1.0 - s;
            3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
        };
        // Bisect for the parameter whose x is `t`; x is monotonic here.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if at(x1, x2, mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        at(y1, y2, (lo + hi) / 2.0)
    }
}

/// A value going from one place to another on the presenter's clock.
#[derive(Debug, Clone, Copy)]
struct Glide {
    from: f32,
    to: f32,
    start: f64,
    ms: f64,
    curve: Curve,
}

impl Glide {
    fn still(at: f32) -> Self {
        Self {
            from: at,
            to: at,
            start: 0.0,
            ms: 0.0,
            curve: Curve::Standard,
        }
    }

    fn new(from: f32, to: f32, start: f64, ms: f64, curve: Curve) -> Self {
        Self {
            from,
            to,
            start,
            ms,
            curve,
        }
    }

    fn at(&self, now: f64) -> f32 {
        if self.ms <= 0.0 || now >= self.start + self.ms {
            return self.to;
        }
        let t = ((now - self.start) / self.ms).clamp(0.0, 1.0) as f32;
        self.from + (self.to - self.from) * self.curve.ease(t)
    }

    /// How much of its time has gone, 0 to 1.
    fn linear(&self, now: f64) -> f32 {
        match self.ms > 0.0 {
            true => ((now - self.start) / self.ms).clamp(0.0, 1.0) as f32,
            false => 1.0,
        }
    }

    fn ends(&self) -> Option<f64> {
        (self.ms > 0.0).then_some(self.start + self.ms)
    }

    fn moving(&self, now: f64) -> bool {
        self.ends().is_some_and(|end| now < end)
    }
}

/// An open sheet.
struct Sheet {
    /// The route that opened it, and the route it shows: its own, or the
    /// last pushed inside it.
    modal: NodeKey,
    shown: NodeKey,
    /// Its resting heights, shortest first, the bar under its foot
    /// included; points.
    heights: Vec<f32>,
    at: usize,
    /// Its top edge in the page, points.
    top: Glide,
    /// What its heights were computed from: a change places it again.
    said: (Option<String>, (f32, f32), bool),
    /// A full-screen route presented above it covers it: it is drawn
    /// beneath that route, as it was, and takes no touch.
    covered: bool,
}

/// A sheet whose route has gone, sliding out of the window.
struct Closing {
    shown: NodeKey,
    /// Its left edge and width.
    x: f32,
    width: f32,
    top: Glide,
    /// The top edge it started from: the scrim fades from there.
    from: f32,
    /// What a predictive Back had shrunk it to.
    scale: f32,
}

/// A push or a pop crossing.
struct Shift {
    forward: bool,
    /// The route that leaves: still in the tree after a push, only its
    /// kept drawing after a pop.
    outgoing: NodeKey,
    incoming: NodeKey,
    /// The sheet it runs inside, by the route that opened it.
    sheet: Option<NodeKey>,
    progress: Glide,
    /// The leaving route's top bar, as it was last drawn.
    bar: Option<(Rc<Tape>, Rect4)>,
    /// Where a predictive Back had moved the leaving route.
    from: Option<Pose>,
}

/// How a route is drawn, about its own box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Pose {
    dx: f32,
    dy: f32,
    scale: f32,
    /// The point it scales about, in its own box.
    origin: (f32, f32),
    opacity: f32,
    /// Its corners' radius while a predictive Back shrinks it.
    radius: f32,
}

impl Pose {
    const REST: Pose = Pose {
        dx: 0.0,
        dy: 0.0,
        scale: 1.0,
        origin: (0.0, 0.0),
        opacity: 1.0,
        radius: 0.0,
    };

    /// Its box's own space to the page's, the box at `at`.
    fn transform(&self, at: (f32, f32)) -> Transform {
        Transform::from_translate(at.0 + self.dx, at.1 + self.dy)
            .pre_translate(self.origin.0, self.origin.1)
            .pre_scale(self.scale, self.scale)
            .pre_translate(-self.origin.0, -self.origin.1)
    }
}

/// What the presenter keeps of its stage.
#[derive(Default)]
pub(super) struct State {
    sheets: Vec<Sheet>,
    closing: Vec<Closing>,
    shift: Option<Shift>,
    preview: Option<touch::Preview>,
    gesture: Option<touch::Gesture>,
    /// The first root's stack as last seen, by key.
    seen: Vec<NodeKey>,
    /// The top bar's picture as last drawn, and the route it is of.
    bar: Option<(NodeKey, Rc<Tape>, Rect4)>,
    /// The routes given a block of the stage's.
    blocks: Vec<ViewId>,
}

/// The sheet heights `detent` asks for on a page `tallest` high below the
/// status bar, the bar under the foot `foot` high; unreadable words are
/// left out, and none left is the tallest.
fn heights(detent: Option<&str>, tallest: f32, foot: f32) -> Vec<f32> {
    let mut out: Vec<f32> = (detent.unwrap_or("large").split_whitespace())
        .filter_map(|d| match d {
            "large" => Some(tallest),
            "medium" => Some(tallest / 2.0 + foot),
            // Laid out at the tallest, a sheet has no height of its own
            // content to rest at: `fit-content` rests at `medium`.
            "fit-content" => Some(tallest / 2.0 + foot),
            d => match d.strip_suffix('%') {
                Some(p) => p.parse::<f32>().ok().filter(|p| *p > 0.0).map(|p| {
                    if p >= 100.0 {
                        tallest
                    } else {
                        tallest * p / 100.0 + foot
                    }
                }),
                None => d.parse::<f32>().ok().filter(|v| *v > 0.0).map(|v| v + foot),
            },
        })
        .map(|v| v.clamp(1.0, tallest.max(1.0)))
        .collect();
    out.sort_by(f32::total_cmp);
    out.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    if out.is_empty() {
        out.push(tallest.max(1.0));
    }
    out
}

impl<D: DataSource> Presenter<D> {
    /// Whether this presenter places sheets and moves routes itself: its
    /// shell shows bars (Android's reader).
    pub(super) fn stage_presents(&self) -> bool {
        self.has_bars()
    }

    fn motion_off(&self) -> bool {
        self.host.runner().viewport().preferences.reduced_motion
    }

    /// The page's height, the status bar's room at its top and the
    /// system's bar's at its foot, points.
    fn sheet_room(&self) -> (f32, f32, f32) {
        let h = self.viewport.1;
        match self.window() {
            Some(window) => {
                let frame = self.chrome().frame;
                let status = (window.bars[0].max(window.cutout[0]) - frame[1]).max(0.0);
                let bottom = window.size.1 - window.bars[2].max(window.cutout[2]);
                let foot = (frame[1] + frame[3] - bottom).max(0.0);
                (h, status.min(h), foot.min(h))
            }
            None => (h, 0.0, 0.0),
        }
    }

    /// A sheet's left edge and width.
    fn sheet_span(&self) -> (f32, f32) {
        let w = self.viewport.0;
        let width = w.min(SHEET_MAX_WIDTH);
        ((w - width) / 2.0, width)
    }

    fn sheet_top_now(&self, sheet: &Sheet) -> f32 {
        let now = self.host.now();
        match &self.stage.gesture {
            Some(touch::Gesture::Drag {
                modal, top, moving, ..
            }) if *modal == sheet.modal && *moving => *top,
            _ => sheet.top.at(now),
        }
    }

    /// Follow the first root's stack after a commit or a change of the
    /// window: sheets open and close, a push or a pop starts its crossing,
    /// and the routes in a sheet are laid out in its block.
    pub(super) fn sync_stage(&mut self) {
        let now = self.host.now();
        let presents = self.stage_presents();
        let still = self.motion_off() || !presents;
        let kernel = self.host.kernel();
        let stack: Vec<ViewId> = self
            .host
            .shown_stack()
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let keys: Vec<NodeKey> = (stack.iter())
            .filter_map(|id| kernel.node(*id).map(|n| n.key))
            .collect();
        let kinds: Vec<Presented> = (stack.iter())
            .map(|id| {
                kernel
                    .node(*id)
                    .map_or(Presented::Pushed, |n| Presented::of(&n))
            })
            .collect();
        if keys.len() != stack.len() {
            return;
        }
        // The sheets the stack holds: each one's route and the last route
        // in it.
        let wanted: Vec<(usize, usize)> = (0..stack.len())
            .filter(|i| kinds[*i] == Presented::Sheet)
            .map(|i| {
                let last = (i..stack.len())
                    .take_while(|j| sheet_of(&kinds, *j) == Some(i))
                    .last()
                    .unwrap_or(i);
                (i, last)
            })
            .collect();
        let before = std::mem::take(&mut self.stage.seen);
        self.stage.seen = keys.clone();
        if presents {
            self.sync_shift(&before, &keys, &kinds, still, now);
        }
        // Sheets that went slide out from where they were.
        let (x, width) = self.sheet_span();
        let (h, status, foot) = self.sheet_room();
        let keyboard = self.window().is_some_and(|w| w.keyboard[2] > 0.0);
        let gone: Vec<Sheet> = {
            let (kept, gone): (Vec<Sheet>, Vec<Sheet>) = std::mem::take(&mut self.stage.sheets)
                .into_iter()
                .partition(|s| wanted.iter().any(|(i, _)| keys[*i] == s.modal));
            self.stage.sheets = kept;
            gone
        };
        for sheet in gone {
            let from = self.sheet_top_now(&sheet);
            let scale = self
                .stage
                .preview
                .as_ref()
                .map_or(1.0, |p| p.sheet_scale(sheet.modal));
            if !still && presents && self.brush.kept(sheet.shown) {
                self.stage.closing.push(Closing {
                    shown: sheet.shown,
                    x,
                    width,
                    top: Glide::new(from, h, now, EXIT_MS, Curve::Accelerate),
                    from,
                    scale,
                });
            }
            if matches!(&self.stage.gesture, Some(touch::Gesture::Drag { modal, .. }) if *modal == sheet.modal)
            {
                self.stage.gesture = None;
            }
        }
        self.stage
            .preview
            .take_if(|p| !p.live(&keys) || p.returned(now));
        let tallest = (h - status).max(1.0);
        for (i, last) in wanted {
            let kernel = self.host.kernel();
            let node = kernel.node(stack[i]).expect("a live route");
            let detent = node.props.str(PropId::NavigationDetent).map(str::to_owned);
            let initial = node
                .props
                .str(PropId::NavigationInitialDetent)
                .map(str::to_owned);
            let said = (detent.clone(), (h, tallest), keyboard);
            let mut found = heights(detent.as_deref(), tallest, foot);
            if !presents {
                found = vec![h];
            }
            let rest = |found: &[f32]| -> usize {
                if keyboard {
                    return found.len() - 1;
                }
                let first = heights(initial.as_deref(), tallest, foot);
                (initial.is_some())
                    .then(|| found.iter().position(|v| (v - first[0]).abs() < 0.5))
                    .flatten()
                    .unwrap_or(0)
            };
            let covered = kinds[i..]
                .iter()
                .any(|k| matches!(k, Presented::Over | Presented::Full));
            match self.stage.sheets.iter_mut().find(|s| s.modal == keys[i]) {
                Some(sheet) => {
                    sheet.shown = keys[last];
                    sheet.covered = covered;
                    if sheet.said != said {
                        sheet.at = if keyboard {
                            found.len() - 1
                        } else {
                            sheet.at.min(found.len() - 1)
                        };
                        sheet.top = Glide::still(h - found[sheet.at]);
                        sheet.heights = found;
                        sheet.said = said;
                    }
                }
                None => {
                    let at = rest(&found);
                    let to = h - found[at];
                    let top = match still {
                        true => Glide::still(to),
                        false => Glide::new(h, to, now, ENTER_MS, Curve::Decelerate),
                    };
                    self.stage.sheets.push(Sheet {
                        modal: keys[i],
                        shown: keys[last],
                        heights: found,
                        at,
                        top,
                        said,
                        covered,
                    });
                }
            }
        }
        self.stage.closing.retain(|c| c.top.moving(now));
        if (self.stage.shift.as_ref()).is_some_and(|s| !s.progress.moving(now)) {
            self.stage.shift = None;
        }
        self.place_sheets(&stack, &kinds);
        self.dirty |= self.stage_moving();
    }

    /// The routes in each sheet laid out in its block: the sheet's width
    /// and its tallest height (on a shell with no bars, the box its own
    /// route was laid out in); the rest given back theirs.
    fn place_sheets(&mut self, stack: &[ViewId], kinds: &[Presented]) {
        let kernel = self.host.kernel();
        let (_, width) = self.sheet_span();
        let mut blocks: Vec<(ViewId, HostBlock)> = Vec::new();
        for (index, route) in stack.iter().enumerate() {
            let Some(sheet_at) = sheet_of(kinds, index) else {
                continue;
            };
            let block = if self.stage_presents() {
                let Some(sheet) = (self.stage.sheets.iter()).find(|s| {
                    kernel
                        .node(stack[sheet_at])
                        .is_some_and(|n| n.key == s.modal)
                }) else {
                    continue;
                };
                let tallest = sheet.heights[sheet.heights.len() - 1];
                HostBlock {
                    width: BlockAxis::Is(width),
                    height: BlockAxis::Is(tallest.round()),
                }
            } else {
                // A route pushed inside a sheet takes the box its sheet was
                // laid out in; the sheet's own keeps the author's.
                if index == sheet_at {
                    continue;
                }
                let Some(frame) = kernel.node(stack[sheet_at]).map(|n| n.frame) else {
                    continue;
                };
                HostBlock {
                    width: BlockAxis::Is(frame.width),
                    height: BlockAxis::Is(frame.height),
                }
            };
            blocks.push((*route, block));
        }
        let mut changed = false;
        let before = std::mem::take(&mut self.stage.blocks);
        for id in before {
            if !blocks.iter().any(|(b, _)| *b == id) && self.host.kernel().host_block(id).is_some()
            {
                changed |= (self.host.runner_mut().kernel_mut())
                    .set_host_block(id, None)
                    .is_ok();
            }
        }
        for (id, block) in &blocks {
            if self.host.kernel().host_block(*id) != Some(*block) {
                match (self.host.runner_mut().kernel_mut()).set_host_block(*id, Some(*block)) {
                    Ok(()) => changed = true,
                    Err(e) => self.host.log(format!("sheet: {e:?}")),
                }
            }
        }
        self.stage.blocks = blocks.into_iter().map(|(id, _)| id).collect();
        if changed {
            if let Err(e) = self.host.layout_unauthored() {
                self.host.log(e);
            }
            self.dirty = true;
        }
    }

    /// A push or a pop of one route, within the page or within one sheet,
    /// starts a crossing; anything else is a cut.
    fn sync_shift(
        &mut self,
        before: &[NodeKey],
        now_keys: &[NodeKey],
        kinds: &[Presented],
        still: bool,
        now: f64,
    ) {
        let (Some(old), Some(new)) = (before.last(), now_keys.last()) else {
            return;
        };
        if old == new {
            return;
        }
        let pushed = now_keys.len() > before.len() && now_keys.starts_with(before);
        let popped = before.len() > now_keys.len() && before.starts_with(now_keys);
        let top = now_keys.len() - 1;
        let (forward, within) = match (pushed, popped) {
            // The route pushed is a page, or pushed inside a sheet.
            (true, _) if kinds[top] == Presented::Pushed && now_keys.len() == before.len() + 1 => {
                (true, sheet_of(kinds, top))
            }
            // Popped back to a page, or to a route of the same sheet.
            (_, true) => {
                let removed_presented = before.len() == now_keys.len() + 1
                    && !self.stage.sheets.iter().any(|s| s.modal == *old);
                if !removed_presented {
                    return;
                }
                (false, sheet_of(kinds, top))
            }
            _ => return,
        };
        let sheet = within.map(|i| now_keys[i]);
        // A pop out of a sheet's first route closes the sheet instead.
        if !forward && within.is_none() && kinds[top] != Presented::Pushed {
            return;
        }
        let size = match sheet {
            Some(_) => (self.sheet_span().1, self.viewport.1),
            None => self.viewport,
        };
        let from = (self.stage.preview.take()).and_then(|p| p.pose_for(*old, size));
        if still {
            return;
        }
        let bar = (self.stage.bar.as_ref())
            .filter(|(route, ..)| route == old && sheet.is_none())
            .map(|(_, tape, frame)| (tape.clone(), *frame));
        self.stage.shift = Some(Shift {
            forward,
            outgoing: *old,
            incoming: *new,
            sheet,
            progress: Glide::new(0.0, 1.0, now, AXIS_MS, Curve::Standard),
            bar,
            from,
        });
    }

    /// Whether anything on the stage moves on its own: frames are owed.
    pub(crate) fn stage_moving(&self) -> bool {
        let now = self.host.now();
        self.stage.sheets.iter().any(|s| s.top.moving(now))
            || !self.stage.closing.is_empty()
            || self.stage.shift.is_some()
            || self.stage.preview.as_ref().is_some_and(|p| p.moving())
    }

    /// When the last motion on the stage ends, ms.
    pub(crate) fn stage_settles_at(&self) -> Option<f64> {
        let ends = (self.stage.sheets.iter().filter_map(|s| s.top.ends()))
            .chain(self.stage.closing.iter().filter_map(|c| c.top.ends()))
            .chain(self.stage.shift.iter().filter_map(|s| s.progress.ends()))
            .chain(self.stage.preview.iter().filter_map(|p| p.ends()));
        ends.reduce(f64::max).filter(|end| *end > self.host.now())
    }

    /// Whether the bars are drawn in the page this frame, under a sheet's
    /// scrim or with the routes crossing, and not in the shell's views.
    pub(crate) fn stage_draws_bars(&self) -> bool {
        self.stage_presents()
            && (self.front_sheets().next().is_some()
                || !self.stage.closing.is_empty()
                || self.stage.shift.is_some()
                || self.stage.preview.is_some())
    }

    /// The open sheets nothing presented covers, bottom first.
    fn front_sheets(&self) -> impl DoubleEndedIterator<Item = &Sheet> {
        self.stage.sheets.iter().filter(|s| !s.covered)
    }

    /// Whether a sheet is open in front.
    pub(crate) fn sheet_open(&self) -> bool {
        self.stage_presents() && self.front_sheets().next().is_some()
    }

    /// The route the top sheet shows, and its frame (x, top, width, the
    /// height that shows), points.
    pub(crate) fn top_sheet(&self) -> Option<(ViewId, Rect4)> {
        let sheet = self
            .front_sheets()
            .last()
            .filter(|_| self.stage_presents())?;
        let id = self.host.kernel().node_by_key(sheet.shown)?.id;
        let (x, width) = self.sheet_span();
        let top = self.sheet_top_now(sheet);
        Some((id, (x, top, width, (self.viewport.1 - top).max(0.0))))
    }

    /// The live routes the stage draws: they show though the stack hides
    /// them (a route a push covers, while it slides out).
    pub(crate) fn stage_shows(&self) -> Vec<ViewId> {
        let kernel = self.host.kernel();
        let mut out = Vec::new();
        if let Some(shift) = &self.stage.shift {
            out.extend(kernel.node_by_key(shift.incoming).map(|n| n.id));
            out.extend(kernel.node_by_key(shift.outgoing).map(|n| n.id));
        }
        if let Some((outgoing, incoming)) = self.stage.preview.as_ref().and_then(|p| p.pair()) {
            out.extend(kernel.node_by_key(outgoing).map(|n| n.id));
            out.extend(kernel.node_by_key(incoming).map(|n| n.id));
        }
        out
    }

    /// What the painter's stage draws this frame.
    pub(super) fn stage_paint(&mut self) -> Stage {
        let mut stage = Stage::default();
        let kernel = self.host.kernel();
        let stack: Vec<ViewId> = self
            .host
            .shown_stack()
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let Some(top) = stack.last().copied() else {
            return stage;
        };
        // After the last route of the stack, in the tree's order.
        let parent = kernel.node(top).and_then(|n| n.parent);
        stage.after = (parent.and_then(|p| kernel.node(p)))
            .map(|p| p.children())
            .and_then(|children| {
                (children.into_iter()).rev().find(|c| {
                    kernel
                        .node(*c)
                        .is_some_and(|n| n.props.str(PropId::NavigationKey).is_some())
                })
            });
        if !self.stage_presents() {
            self.desktop_stage(&mut stage, &stack);
            return stage;
        }
        // A page Back would pop is kept, so that it can slide out after the
        // commit that removes it.
        let kinds: Vec<Presented> = (stack.iter())
            .map(|id| {
                kernel
                    .node(*id)
                    .map_or(Presented::Pushed, |n| Presented::of(&n))
            })
            .collect();
        let pops = stack.len() > 1 && kinds[stack.len() - 1] == Presented::Pushed;
        if pops && !self.motion_off() && sheet_of(&kinds, stack.len() - 1).is_none() {
            stage.tape.push(top);
        }
        let now = self.host.now();
        let (bar_top, bar_bottom) = self.bar_pictures();
        let bar_route = self.bar_route();
        let kernel = self.host.kernel();
        if let Some((tape, frame)) = &bar_top {
            if let Some(route) = bar_route.and_then(|r| kernel.node(r)).map(|n| n.key) {
                self.stage.bar = Some((route, tape.clone(), *frame));
            }
        }
        if !self.stage_draws_bars() {
            return stage;
        }
        let page_shift = self.stage.shift.as_ref().filter(|s| s.sheet.is_none());
        // The page's crossing, or its predictive Back, with each route's bar.
        let page_pair = page_shift
            .map(|s| (s.outgoing, s.incoming))
            .or_else(|| (self.stage.preview.as_ref()).and_then(|p| p.page_pair()));
        if let Some((outgoing, incoming)) = page_pair {
            let (out_pose, in_pose) = self.shift_poses(now, None);
            let forward = page_shift.is_some_and(|s| s.forward);
            // A crossing kept the leaving route's bar; under a preview the
            // bar is still the leaving route's, and the one beneath has none.
            let (out_bar, in_bar) = match page_shift {
                Some(s) => (s.bar.clone(), bar_top.clone()),
                None => (bar_top.clone(), None),
            };
            let order = match forward {
                true => [(outgoing, out_pose, out_bar), (incoming, in_pose, in_bar)],
                false => [(incoming, in_pose, in_bar), (outgoing, out_pose, out_bar)],
            };
            for (key, pose, bar) in order {
                self.stage_page_route(&mut stage, key, pose, bar);
            }
        } else if let Some((tape, frame)) = &bar_top {
            stage
                .items
                .push(Item::Picture(tape.clone(), self.window_to_page(*frame)));
        }
        if let Some((tape, frame)) = &bar_bottom {
            stage
                .items
                .push(Item::Picture(tape.clone(), self.window_to_page(*frame)));
        }
        self.stage_sheets(&mut stage, now);
        stage
    }

    /// A bar's frame in the window to the transform that draws its picture
    /// in the page.
    fn window_to_page(&self, frame: Rect4) -> Transform {
        let page = self.chrome().frame;
        Transform::from_translate(frame.0 - page[0], frame.1 - page[1])
    }

    /// A page route crossing: its bar, then the route, as `pose` moves both.
    fn stage_page_route(
        &self,
        stage: &mut Stage,
        key: NodeKey,
        pose: Pose,
        bar: Option<(Rc<Tape>, Rect4)>,
    ) {
        let kernel = self.host.kernel();
        let live = kernel.node_by_key(key);
        let at = match &live {
            Some(n) => (n.frame.x - self.page.0, n.frame.y - self.page.1),
            None => (0.0, 0.0),
        };
        let size = live
            .as_ref()
            .map_or(self.viewport, |n| (n.frame.width, n.frame.height));
        let ts = pose.transform(at);
        let fades = pose.opacity < 1.0;
        if fades {
            stage.items.push(Item::Opacity(pose.opacity));
        }
        let rounded = pose.radius > 0.0;
        if rounded {
            let shape = Shape::new((0.0, 0.0, size.0, size.1), [pose.radius; 4]);
            stage.items.push(Item::Clip(shape, ts));
        }
        match live {
            Some(n) => {
                stage.skip.push(n.id);
                let tape = !self.motion_off();
                stage.items.push(Item::Route { id: n.id, ts, tape });
            }
            None => stage.items.push(Item::Taped(key, ts)),
        }
        if let Some((tape, frame)) = bar {
            let page = self.chrome().frame;
            let bar_at = (frame.0 - page[0] - at.0, frame.1 - page[1] - at.1);
            let bar_ts = ts.pre_translate(bar_at.0, bar_at.1);
            stage.items.push(Item::Picture(tape, bar_ts));
        }
        if rounded {
            stage.items.push(Item::PopClip);
        }
        if fades {
            stage.items.push(Item::PopOpacity);
        }
    }

    /// Each open sheet over its scrim, then the sheets sliding out.
    fn stage_sheets(&self, stage: &mut Stage, now: f64) {
        let kernel = self.host.kernel();
        let (w, h) = self.viewport;
        let page = self.chrome().frame;
        let window = self.window().map_or((w, h), |win| win.size);
        // The scrim covers the window, the system's bars included.
        let whole = (-page[0], -page[1], window.0, window.1);
        let container = |route: Option<exact_kernel::NodeRef<'_>>| -> [u8; 4] {
            let written =
                route.is_some_and(|n| n.style.written.has(exact_kernel::StyleId::BackgroundColor));
            match written {
                true => [0; 4],
                false => crate::presenter::top::look::Role::SurfaceContainerLow.color(self.dark()),
            }
        };
        let handle = crate::presenter::top::look::Role::OnSurfaceVariant.color(self.dark());
        let handle = [handle[0], handle[1], handle[2], (0.4 * 255.0) as u8];
        let (x, width) = self.sheet_span();
        for sheet in &self.stage.sheets {
            let Some(shown) = kernel.node_by_key(sheet.shown) else {
                continue;
            };
            // A sheet a full-screen route covers is drawn beneath it where
            // that route lets it show, and takes no touch.
            if sheet.covered && self.host.route_visibility(shown.id).0 {
                continue;
            }
            let mut items = Vec::new();
            let top = self.sheet_top_now(sheet);
            let scrim = ((h - top) / sheet.heights[0]).clamp(0.0, 1.0) * SCRIM;
            items.push(Item::Fill(
                Shape::rect(whole),
                [0, 0, 0, (scrim * 255.0).round() as u8],
                Transform::identity(),
            ));
            if !sheet.covered {
                items.push(Item::Hit(shown.id, whole));
            }
            let scale = self
                .stage
                .preview
                .as_ref()
                .map_or(1.0, |p| p.sheet_scale(sheet.modal));
            let modal = kernel.node_by_key(sheet.modal);
            let ts = Pose {
                scale,
                origin: (width / 2.0, h - top),
                ..Pose::REST
            }
            .transform((x, top));
            let tallest = sheet.heights[sheet.heights.len() - 1];
            let body = (0.0, 0.0, width, (h - top).max(tallest));
            let shape = Shape::new(body, [SHEET_CORNER, SHEET_CORNER, 0.0, 0.0]);
            items.push(Item::Clip(shape, ts));
            items.push(Item::Fill(shape, container(modal), ts));
            // A crossing inside this sheet, else the route it shows.
            let inner = (self.stage.shift.as_ref())
                .filter(|s| s.sheet == Some(sheet.modal))
                .map(|s| (s.outgoing, s.incoming, s.forward))
                .or_else(|| {
                    (self.stage.preview.as_ref())
                        .and_then(|p| p.inner_pair(sheet.modal))
                        .map(|(o, i)| (o, i, false))
                });
            match inner {
                Some((outgoing, incoming, forward)) => {
                    let (out_pose, in_pose) = self.shift_poses(now, Some(sheet.modal));
                    let order = match forward {
                        true => [(outgoing, out_pose), (incoming, in_pose)],
                        false => [(incoming, in_pose), (outgoing, out_pose)],
                    };
                    for (key, pose) in order {
                        let ts = ts.pre_concat(pose.transform((0.0, 0.0)));
                        items.push(Item::Opacity(pose.opacity));
                        match kernel.node_by_key(key) {
                            Some(n) => {
                                stage.skip.push(n.id);
                                stage.bare.push(n.id);
                                items.push(Item::Route {
                                    id: n.id,
                                    ts,
                                    tape: true,
                                });
                            }
                            None => items.push(Item::Taped(key, ts)),
                        }
                        items.push(Item::PopOpacity);
                    }
                }
                None => {
                    stage.skip.push(shown.id);
                    stage.bare.push(shown.id);
                    let tape = !self.motion_off();
                    items.push(Item::Route {
                        id: shown.id,
                        ts,
                        tape,
                    });
                }
            }
            let grip = Shape::new(((width - 32.0) / 2.0, 22.0, 32.0, 4.0), [2.0; 4]);
            items.push(Item::Fill(grip, handle, ts));
            items.push(Item::PopClip);
            match sheet.covered {
                true => stage.beneath.push((shown.id, items)),
                false => stage.items.extend(items),
            }
        }
        for closing in &self.stage.closing {
            let top = closing.top.at(now);
            let left = ((h - top) / (h - closing.from).max(1.0)).clamp(0.0, 1.0) * SCRIM;
            stage.items.push(Item::Fill(
                Shape::rect(whole),
                [0, 0, 0, (left * 255.0).round() as u8],
                Transform::identity(),
            ));
            let ts = Pose {
                scale: closing.scale,
                origin: (closing.width / 2.0, h - closing.from),
                ..Pose::REST
            }
            .transform((closing.x, top));
            let body = (0.0, 0.0, closing.width, (h - closing.from).max(1.0));
            let shape = Shape::new(body, [SHEET_CORNER, SHEET_CORNER, 0.0, 0.0]);
            stage.items.push(Item::Clip(shape, ts));
            stage.items.push(Item::Fill(shape, container(None), ts));
            stage.items.push(Item::Taped(closing.shown, ts));
            let grip = Shape::new(((closing.width - 32.0) / 2.0, 22.0, 32.0, 4.0), [2.0; 4]);
            stage.items.push(Item::Fill(grip, handle, ts));
            stage.items.push(Item::PopClip);
        }
    }

    /// The leaving and arriving routes' poses in a crossing or a
    /// predictive Back, in the page or in the sheet `sheet` opened.
    fn shift_poses(&self, now: f64, sheet: Option<NodeKey>) -> (Pose, Pose) {
        let width = match sheet {
            Some(_) => self.sheet_span().1,
            None => self.viewport.0,
        };
        if let Some(shift) = self.stage.shift.as_ref().filter(|s| s.sheet == sheet) {
            let (t, linear) = (
                shift.progress.at(now).clamp(0.0, 1.0),
                shift.progress.linear(now),
            );
            let way = if shift.forward { 1.0 } else { -1.0 };
            // The fades go by the clock, the moves by the curve.
            let out_fade = (1.0 - linear / AXIS_FADE).clamp(0.0, 1.0);
            let in_fade = ((linear - AXIS_FADE) / (1.0 - AXIS_FADE)).clamp(0.0, 1.0);

            let mut out_pose = Pose {
                dx: -way * AXIS_SHIFT * t,
                opacity: out_fade,
                ..Pose::REST
            };
            if let Some(from) = shift.from {
                // From where Back's preview left it, on toward its edge.
                out_pose = Pose {
                    dx: from.dx + (from.dx.signum() * width * 0.1) * t,
                    opacity: out_fade,
                    ..from
                };
            }
            let in_pose = match shift.from {
                Some(_) => Pose::REST,
                None => Pose {
                    dx: way * AXIS_SHIFT * (1.0 - t),
                    opacity: in_fade,
                    ..Pose::REST
                },
            };
            return (out_pose, in_pose);
        }
        match &self.stage.preview {
            Some(p) => (p.page_pose(now, width, self.viewport.1), Pose::REST),
            None => (Pose::REST, Pose::REST),
        }
    }

    /// On a shell with no bars, a route pushed inside a sheet is drawn in
    /// the box its sheet's route was laid out in.
    fn desktop_stage(&self, stage: &mut Stage, stack: &[ViewId]) {
        let kernel = self.host.kernel();
        let kinds: Vec<Presented> = (stack.iter())
            .map(|id| {
                kernel
                    .node(*id)
                    .map_or(Presented::Pushed, |n| Presented::of(&n))
            })
            .collect();
        let Some(top) = stack.len().checked_sub(1) else {
            return;
        };
        let Some(sheet) = sheet_of(&kinds, top).filter(|s| *s != top) else {
            return;
        };
        let (Some(modal), Some(shown)) = (kernel.node(stack[sheet]), kernel.node(stack[top]))
        else {
            return;
        };
        let at = (modal.frame.x - self.page.0, modal.frame.y - self.page.1);
        stage.skip.push(shown.id);
        stage.items.push(Item::Route {
            id: shown.id,
            ts: Transform::from_translate(at.0, at.1),
            tape: false,
        });
    }
}
