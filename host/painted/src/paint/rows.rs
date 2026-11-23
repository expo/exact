//! A scroller's rows, recorded once and replayed while unchanged. A backend
//! that keeps recordings (the Canvas recorder: one Android `RenderNode` a
//! row) is asked for a row's drawing only when the row changed; otherwise
//! the walk replays its boxes and paragraphs, moved by however far the row
//! moved, and the backend draws the kept recording there. Scrolling a list
//! of unchanged rows walks none of them.
//!
//! A row is invalid once anything in it changes: a commit touching,
//! creating or destroying a node in it, a presentation value, a press, focus
//! or ring suppression, a picture arriving, a layout publishing something
//! new beside a frame that stayed (a resolved padding, a scroll extent). A
//! layout change that moves a node only makes the row suspect: kept while
//! every node sits where it sat relative to the row.
//!
//! A drawing the stage keeps apart (`stage.rs`) holds the rows the walk drew
//! this frame by reference ([`RowPin`]): a row's id names one recording for
//! its whole life, so the backend is told to free it only once no such
//! drawing holds it. The first attachment also records portable commands,
//! reused until the row changes; a CPU filter can replay those commands.
use super::tape::{Recorder, Tape};
use super::*;
use exact_ink::counters::{self, Counter};
use exact_kernel::id::{IdMap, IdSet};
use exact_kernel::NodeKey;

/// Node changes awaiting row invalidation, and the last unpainted commit's time.
#[derive(Default)]
pub struct Dirty {
    hard: Vec<NodeKey>,
    destroyed: bool,
    moved: Vec<NodeKey>,
    /// Nodes a commit bound to another item (LLP 1078).
    renewed: Vec<NodeKey>,
    /// When the latest commit landed, ms (where a numeric roll starts).
    committed: Option<f64>,
}

impl Dirty {
    #[cfg(test)]
    pub(crate) fn pending_nodes(&self) -> usize {
        self.hard.len() + self.moved.len() + self.renewed.len()
    }

    /// Take node changes while keeping the commit time for the next paint's numeric roll.
    pub(crate) fn take_changes(&mut self) -> Self {
        let changes = std::mem::take(self);
        self.committed = changes.committed;
        changes
    }

    /// A commit's nodes, and when it landed (ms).
    pub fn commit(&mut self, r: &exact_kernel::CommitReceipt, at: f64) {
        self.committed = Some(at);
        self.hard.extend(&r.touched);
        self.hard.extend(&r.created);
        self.hard.extend(&r.destroyed);
        self.destroyed |= !r.destroyed.is_empty();
        self.renewed.extend(&r.renewed);
    }

    /// A layout's moved frames; paragraphs whose exclusions changed, and
    /// nodes it published anew beside a frame that stayed, as changed.
    pub fn layout(&mut self, r: &exact_kernel::LayoutReceipt) {
        let moved: IdSet<NodeKey> = r.changed.iter().copied().collect();
        self.moved.extend(&r.changed);
        self.hard.extend(&r.flow_changed);
        self.hard
            .extend(r.updated.iter().filter(|key| !moved.contains(*key)));
    }

    /// A node whose presentation changed.
    pub fn node(&mut self, key: NodeKey) {
        self.hard.push(key);
    }
}

struct RowFrame {
    node: NodeKey,
    frame: exact_kernel::Frame,
    padding: (f32, f32, f32, f32),
}

struct RowSticky {
    node: NodeKey,
    grid: Option<f32>,
    scroll_grid: Option<f32>,
    shift: PaintOffset,
}

/// One kept row.
struct Row {
    id: u32,
    /// Commands for replay on a backend without this retained row.
    drawing: Option<Rc<Tape>>,
    /// The row's top-left in viewport points when recorded.
    origin: (f32, f32),
    /// Hits and labels for a locally recorded row stay in the recording's space.
    local: bool,
    scale: (f32, f32),
    size: (f32, f32),
    /// A whole-pixel move preserves the snapping of the row's children.
    phase: Option<(f64, f64)>,
    boxes: Vec<PaintedBox>,
    text: Vec<(NodeKey, Rc<Paragraph>)>,
    inline: Vec<(ViewId, Rc<inline_frame::PaintedInline>)>,
    /// Where its fields' labels and values and its buttons' titles were
    /// painted, for an inspector.
    field_texts: Vec<(ViewId, material::Part, material::FieldText)>,
    images: Vec<(ViewId, Option<std::sync::Weak<Bitmap>>)>,
    /// Where each picture kept apart from the row is drawn.
    slots: Vec<Slot>,
    /// Each node's frame relative to the row and its resolved padding.
    frames: Vec<RowFrame>,
    /// The scrollers inside it and their offsets when recorded: one moved
    /// (a swipe) records the row again.
    scrolls: Vec<(ViewId, (f32, f32))>,
    sticky: Vec<RowSticky>,
    /// A fresh geometry walk can translate these hits without consulting descendants.
    can_translate_hits: bool,
    /// Whether a box in it may have hit slop.
    may_have_hit_slop: bool,
    /// Kept through a clipped fractional move. Re-record its glyphs before drawing.
    paint_stale: bool,
    unsupported: bool,
    /// What the row's drawing covers, relative to its origin.
    bounds: Rect4,
    seen: u64,
    /// Something in it changed: recorded again before it draws, and kept
    /// as it was when the new recording is the same.
    stale: bool,
    /// What its subtree lets escape into its list's painting, while nothing
    /// in it changed ([`Painter::refresh_ranks`]).
    order: Option<exact_kernel::paint_order::Potentials>,
}

impl Row {
    fn bounds_at(&self, origin: (f32, f32)) -> Rect4 {
        let (x, y, w, h) = self.bounds;
        (x + origin.0, y + origin.1, w, h)
    }

    /// Whether a replay may move this row's hits to `origin`; `shown`: where
    /// hits can land, the viewport within the row's clip.
    fn can_translate_geometry_to(
        &self,
        kernel: &exact_kernel::Kernel,
        origin: (f32, f32),
        shown: Rect4,
    ) -> bool {
        let (dx, dy) = (origin.0 - self.origin.0, origin.1 - self.origin.1);
        if self.local || (dx.abs() <= MAX_TRANSLATION && dy.abs() <= MAX_TRANSLATION) {
            return true;
        }
        // Further, a hit can round onto the old box's exclusive edge: walk a
        // row that can take hits at its new position, by its drawing or by
        // its hit slop.
        let (x, y, w, h) = self.bounds;
        let drawn = (x + self.origin.0, y + self.origin.1, w, h);
        let (x, y, w, h) = self
            .boxes
            .iter()
            .filter(|_| self.may_have_hit_slop)
            .filter_map(|b| super::fragments::slop_bounds(kernel, b))
            .fold(drawn, |u, r| union(Some(u), r));
        // Recorded far from the viewport, each edge rounded by up to an
        // `f32` step there: allow two.
        let far = [x, y, x + w, y + h]
            .into_iter()
            .fold(0f32, |m, v| m.max(v.abs()));
        let pad = 2. * f32::EPSILON * far;
        let at = (x + dx - pad, y + dy - pad, w + 2. * pad, h + 2. * pad);
        let (_, _, w, h) = intersect(at, shown);
        w <= 0. || h <= 0.
    }
}

/// A picture a backend may keep apart from its row ([`Backend::slot_begin`]):
/// the image node, its content box, the content edge's curve it is clipped
/// to and the transform it was drawn in.
type Slot = (ViewId, Rect4, Shape, Transform);

/// The row state [`Painter::rows_pause`] set aside.
pub(super) struct Paused(Option<Capture>, bool, Option<Refer>);

/// A drawing being kept that holds rows by reference: its recorder, and
/// the scale of the transform that places it in the page.
type Refer = (Recorder, (f32, f32));

/// A kept row a drawing kept apart holds: its free waits until no drawing
/// holds it.
pub(crate) struct RowPin(u32);

impl RowPin {
    pub(crate) fn id(&self) -> u32 {
        self.0
    }
}

/// A row being recorded.
#[derive(Default)]
struct Capture {
    clip: Option<Rect4>,
    refused: bool,
    split_sections: bool,
    may_have_hit_slop: bool,
    /// What it paints past its boxes, in the walk's coordinates, such as
    /// filters (`box_filter.rs`), shadows, overflowing lines and a surface
    /// a layout transition holds larger: every `row_reach` call.
    reach: Option<Rect4>,
    images: Vec<(ViewId, Option<std::sync::Weak<Bitmap>>)>,
    slots: Vec<Slot>,
    text: Vec<(NodeKey, Rc<Paragraph>)>,
    inline: Vec<(ViewId, Rc<inline_frame::PaintedInline>)>,
    field_texts: Vec<(ViewId, material::Part, material::FieldText)>,
    frames: Vec<RowFrame>,
    scrolls: Vec<(ViewId, (f32, f32))>,
    sticky: Vec<RowSticky>,
}

#[derive(Default)]
pub(super) struct Rows {
    kept: IdMap<NodeKey, Row>,
    suspect: IdSet<NodeKey>,
    next: u32,
    pub(super) frame: u64,
    /// [`sticky_parents`] and the walk it was found for.
    sections: Option<(u64, Rc<IdSet<ViewId>>)>,
    sticky_scrollers: Option<IdSet<ViewId>>,
    /// The environment the kept rows resolved `env()` against.
    env: Option<exact_kernel::Env>,
    /// The controls' own states and choices the kept rows drew
    /// ([`Scene::controls`], [`Scene::chosen`]).
    controls: std::collections::BTreeMap<ViewId, bool>,
    chosen: std::collections::BTreeMap<ViewId, (String, String)>,
    active: bool,
    recording: Option<Capture>,
    freed: Vec<u32>,
    /// The rows drawings kept apart hold, by id.
    pins: HashMap<u32, std::rc::Weak<RowPin>>,
    /// Rows no longer kept whose free waits for the drawings holding them.
    held: Vec<u32>,
    /// A drawing is being kept with rows by reference.
    pub(super) refer: Option<Refer>,
    /// Rows drawings took by reference, and rows they walked again.
    #[cfg(test)]
    pub(super) referred: (usize, usize),
    #[cfg(test)]
    geometry_replays: usize,
    /// Viewport width, scale, appearance and whether desktop bars draw.
    world: Option<(u32, u32, bool, bool)>,
    /// The last paint's focused node and whether its ring was suppressed.
    focus: Option<(NodeKey, bool)>,
    /// `EXACT_ROWS=0` turns rows off, for comparison.
    off: Option<bool>,
    /// This walk's scrollers with rows: each one's id and the range of the
    /// walk's boxes its rows pushed.
    groups: Vec<(ViewId, usize, usize)>,
    /// The container whose children are its scroller's rows, while it is
    /// walked (see [`Painter::wraps_rows`]).
    wrapper: Option<NodeKey>,
}

impl Rows {
    /// Whether the last walk kept rows.
    pub(super) fn active(&self) -> bool {
        self.active
    }

    /// The potentials of the kept row at `key` while nothing in it changed.
    pub(super) fn order(&self, key: NodeKey) -> Option<exact_kernel::paint_order::Potentials> {
        let row = self.kept.get(&key)?;
        row.order.filter(|_| !row.stale)
    }
}

/// The ids whose entry differs between `a` and `b`.
fn changed<'a, V: PartialEq>(
    a: &'a std::collections::BTreeMap<ViewId, V>,
    b: &'a std::collections::BTreeMap<ViewId, V>,
) -> impl Iterator<Item = ViewId> + 'a {
    let gone = a.keys().filter(|id| !b.contains_key(id));
    gone.chain(
        b.iter()
            .filter(|(id, v)| a.get(id) != Some(v))
            .map(|(id, _)| id),
    )
    .copied()
}

/// Room around a row's boxes for what paints outside them (shadows).
const MARGIN: f32 = 48.0;

/// How far a kept row's hits may be moved by translation: in `f32`, a
/// coordinate within a few thousand points rounds by under a thousandth of
/// a point.
const MAX_TRANSLATION: f32 = 4096.0;

/// Whether `node` may hold rows split under a sticky child, a section's
/// or a scroller's only child's (`wraps_rows`, which reads only y), where
/// a change to it reaches them. Looser than [`can_split`]: it only marks
/// more rows. A scroller is not one: its children are rows of their own.
fn holds_split_rows(node: &NodeRef<'_>) -> bool {
    node.node_type == NodeType::View
        && node.style.flex_direction == exact_kernel::FlexDirection::Column
        && !scrolls(effective_overflow(node).1)
        && node
            .child_nodes()
            .any(|child| child.layout_position() == exact_kernel::PositionType::Sticky)
}

/// Whether `node` is a column whose children a backend that splits
/// sections keeps as rows under its sticky child
/// ([`Backend::independent_section_rows`]), which the caller knows it has. A
/// list is kept whole: an item's number counts its siblings. A scroll
/// container, which `hidden` makes too, is kept whole: a sticky child
/// sticks to it, not to the scroller.
fn can_split(node: &NodeRef<'_>) -> bool {
    let unscrolled = |o| matches!(o, Overflow::Visible | Overflow::Clip);
    let (x, y) = effective_overflow(node);
    node.node_type == NodeType::View
        && node.style.flex_direction == exact_kernel::FlexDirection::Column
        && node.layout_position() != exact_kernel::PositionType::Sticky
        && unscrolled(x)
        && unscrolled(y)
        && node.list_markers().is_none()
        && node.child_nodes().len() >= 3
}

/// The parents of every sticky node: the only columns asked of
/// [`can_split`], found without reading each column's children on every
/// paint.
fn sticky_parents(kernel: &exact_kernel::Kernel) -> IdSet<ViewId> {
    kernel
        .sticky_nodes()
        .into_iter()
        .filter_map(|key| kernel.node_by_key(key)?.parent)
        .collect()
}

pub(super) fn union(a: Option<Rect4>, b: Rect4) -> Rect4 {
    match a {
        None => b,
        Some(a) => {
            let x0 = a.0.min(b.0);
            let y0 = a.1.min(b.1);
            let x1 = (a.0 + a.2).max(b.0 + b.2);
            let y1 = (a.1 + a.3).max(b.1 + b.3);
            (x0, y0, x1 - x0, y1 - y0)
        }
    }
}

fn within(a: Option<Rect4>, outer: Option<Rect4>) -> Option<Rect4> {
    match (a, outer) {
        (Some(a), Some(b)) => Some(intersect(a, b)),
        (a, None) => a,
        (None, b) => b,
    }
}

impl Painter {
    pub(super) fn discard_rows(&mut self) {
        let rows = std::mem::take(&mut self.rows);
        // The reader can still hold these IDs until it receives their deletions.
        self.rows.next = rows.next;
        self.unkeep_rows();
        let held = rows.held.into_iter();
        for id in rows
            .kept
            .into_values()
            .map(|r| r.id)
            .chain(rows.freed)
            .chain(held)
        {
            self.backend.row_free(id);
        }
    }

    /// Invalidate the rows reached by the host's changes.
    pub(crate) fn rows_dirty(&mut self, kernel: &Kernel, dirty: Dirty) {
        if dirty.destroyed {
            self.canvases
                .readbacks
                .retain(|view, _| kernel.node(*view).is_some());
        }
        self.numerals.committed = dirty.committed;
        self.invalidate_rows(kernel, dirty);
    }

    fn invalidate_rows(&mut self, kernel: &Kernel, mut dirty: Dirty) {
        if self.rows.kept.is_empty() {
            return;
        }
        // A source can animate outside the row holding its SVG instance.
        let changed: IdSet<_> = dirty
            .hard
            .iter()
            .filter_map(|k| kernel.node_by_key(*k))
            .filter(|n| n.node_type.is_svg_element())
            .map(|n| n.id)
            .collect();
        if !changed.is_empty() {
            let arena = kernel.arena();
            for slot in arena
                .iter_live()
                .filter(|s| arena.node_type(*s) == NodeType::Svg)
            {
                if exact_kernel::svg::scene_sources(kernel, arena.local_id(slot))
                    .iter()
                    .any(|id| changed.contains(id))
                {
                    dirty.hard.push(arena.key(slot));
                }
            }
        }
        // Each node climbed, with the row it is in (or none): a row's nodes,
        // and those outside rows, climb their shared ancestors once.
        let mut memo: IdMap<NodeKey, Option<NodeKey>> = IdMap::default();
        let mut path = Vec::new();
        let mut row_of = |rows: &Rows, key: NodeKey| {
            let mut at = key;
            let found = loop {
                if let Some(row) = memo.get(&at) {
                    break *row;
                }
                if rows.kept.contains_key(&at) {
                    break Some(at);
                }
                path.push(at);
                let parent = kernel.node_by_key(at).and_then(|n| n.parent);
                match parent.and_then(|p| kernel.node(p)) {
                    Some(parent) => at = parent.key,
                    None => break None,
                }
            };
            for k in path.drain(..) {
                memo.insert(k, found);
            }
            found
        };
        let split = self.backend.independent_section_rows();
        for key in dirty.hard {
            if let Some(row) = row_of(&self.rows, key) {
                self.rows.kept.get_mut(&row).expect("found").stale = true;
            } else if let Some(section) = kernel
                .node_by_key(key)
                .filter(|n| split && holds_split_rows(n))
            {
                // A split section is in none of its rows, though they paint
                // what they take from it, such as a pressed or moving color;
                // nor is a section split within it.
                let mut open = vec![section];
                while let Some(section) = open.pop() {
                    for child in section.child_nodes() {
                        if let Some(row) = self.rows.kept.get_mut(&child.key) {
                            row.stale = true;
                        } else if holds_split_rows(&child) {
                            open.push(child);
                        }
                    }
                }
            }
        }
        // A row bound to another item is a new row (LLP 1078): its kept
        // recording is the other item's, and a backend told the row was that
        // one draws it in the row's place until the new one is made. The
        // Canvas reader did, for as long as no stream followed: a rebound
        // row coming into view showed the item it had been, in a slot that
        // item's height left short (easy at 3,000 dp/s toward the start: up
        // to five frames, three times in two seconds).
        for key in dirty.renewed {
            if let Some(row) = self.rows.kept.remove(&key) {
                self.rows.freed.push(row.id);
            }
        }
        let mut seen = IdSet::default();
        for key in dirty.moved {
            if !seen.insert(key) {
                continue;
            }
            if let Some(row) = row_of(&self.rows, key) {
                self.rows.suspect.insert(row);
            }
        }
    }

    /// The backend has none of the kept rows any more (it is another one, or
    /// its reader was made again): none is drawn from a recording and none
    /// is freed, each is recorded again as a new row.
    pub(crate) fn forget_rows(&mut self) {
        self.rows = Rows::default();
        self.unkeep_rows();
    }

    #[cfg(test)]
    pub(crate) fn retained_row_id(&self, key: NodeKey) -> Option<u32> {
        self.rows.kept.get(&key).map(|row| row.id)
    }

    /// The platform's colours resolve differently (LLP 1115.001 D4):
    /// nothing kept from a paint under the old ones stands. Every kept row
    /// is freed and recorded again, and the next paint is whole.
    pub(crate) fn colors_changed(&mut self) {
        self.rows.world = None;
        self.damage.pixels = None;
        self.region_picture = None;
    }

    /// At a walk's start: whether rows are kept this walk.
    pub(super) fn rows_begin(&mut self, walk: &Walk<'_, '_>) {
        let focus = walk
            .scene
            .focus
            .and_then(|id| walk.scene.kernel.node(id))
            .map(|node| (node.key, self.unringed(&node)));
        if self.rows.focus != focus {
            let mut dirty = Dirty::default();
            for (key, _) in self.rows.focus.into_iter().chain(focus) {
                dirty.node(key);
            }
            self.invalidate_rows(walk.scene.kernel, dirty);
            self.rows.focus = focus;
        }
        // The host keeps an unbound control's state and a choice its bound
        // value has not taken, and changes them without a commit.
        let scene = walk.scene;
        let keeps = self.backend.rows() || self.backend.geometry_rows();
        if keeps && (self.rows.controls != *scene.controls || self.rows.chosen != *scene.chosen) {
            let mut dirty = Dirty::default();
            let ids = changed(&self.rows.controls, scene.controls)
                .chain(changed(&self.rows.chosen, scene.chosen));
            for node in ids.filter_map(|id| scene.kernel.node(id)) {
                dirty.node(node.key);
            }
            self.invalidate_rows(scene.kernel, dirty);
            self.rows.controls.clone_from(scene.controls);
            self.rows.chosen.clone_from(scene.chosen);
        }
        let off = *self
            .rows
            .off
            .get_or_insert_with(|| std::env::var("EXACT_ROWS").is_ok_and(|v| v == "0"));
        self.rows.frame += 1;
        self.rows.groups.clear();
        self.rows.sticky_scrollers = None;
        self.rows.active = !off
            && (self.backend.rows() || self.backend.geometry_rows())
            && walk.skip.is_none()
            && walk.replay.is_none()
            && self.placements.is_empty()
            && self.lift.arrange.is_none()
            && self.flatten.is_none();
        let world = (
            self.viewport.0.to_bits(),
            self.scale.to_bits(),
            self.dark,
            self.scrollbars.enabled,
        );
        if self.rows.world != Some(world) {
            self.rows.world = Some(world);
            let all: Vec<u32> = self.rows.kept.drain().map(|(_, r)| r.id).collect();
            self.rows.freed.extend(all);
        }
        // `Kernel::set_env` restyles the boxes that read `env()` in place,
        // and no receipt names them, so any kept row may paint and hit by
        // the old values.
        let env = walk.scene.kernel.arena().env();
        if self.rows.env.as_ref() != Some(env) {
            for row in self.rows.kept.values_mut() {
                row.stale = true;
            }
            self.rows.env = Some(env.clone());
        }
    }

    /// At a walk's end: free what no longer draws.
    pub(super) fn rows_end(&mut self) {
        // A geometry walk's stand-in backend keeps no rows: what is owed to be
        // freed waits for the backend that recorded it.
        if !self.backend.rows() {
            return;
        }
        if self.rows.active {
            let frame = self.rows.frame;
            let stale: Vec<NodeKey> = self
                .rows
                .kept
                .iter()
                .filter(|(_, r)| r.seen != frame)
                .map(|(k, _)| *k)
                .collect();
            for k in stale {
                let gone = self.rows.kept.remove(&k).expect("listed");
                self.rows.freed.push(gone.id);
            }
            self.rows.suspect.clear();
        }
        let held = std::mem::take(&mut self.rows.held);
        for id in std::mem::take(&mut self.rows.freed).into_iter().chain(held) {
            if (self.rows.pins.get(&id)).is_some_and(|pin| pin.strong_count() > 0) {
                self.rows.held.push(id);
            } else {
                self.backend.row_free(id);
            }
        }
        self.rows.pins.retain(|_, pin| pin.strong_count() > 0);
    }

    /// A scroller's rows begin: the backend groups them; their boxes are noted.
    pub(super) fn group_begin(&mut self, walk: &Walk<'_, '_>, id: ViewId) {
        if self.rows.refer.is_some() || self.backend.geometry_rows() {
            return;
        }
        let scroll = walk.scene.scroll.get(&id).copied().unwrap_or((0.0, 0.0));
        self.backend.group_begin(id, scroll);
        self.rows
            .groups
            .push((id, walk.boxes.len(), walk.boxes.len()));
    }

    /// A sticky box moves with its constraint, independently of the group.
    pub(super) fn group_can_move(&mut self, kernel: &Kernel, id: ViewId) -> bool {
        !self.pixel_grid
            || !self
                .rows
                .sticky_scrollers
                .get_or_insert_with(|| {
                    kernel
                        .sticky_nodes()
                        .into_iter()
                        .filter_map(|key| kernel.sticky_constraint(key).map(|c| c.scroller))
                        .collect()
                })
                .contains(&id)
    }

    pub(super) fn group_end(&mut self, walk: &Walk<'_, '_>) {
        if self.rows.refer.is_some() || self.backend.geometry_rows() {
            return;
        }
        self.backend.group_end();
        if let Some(g) = self.rows.groups.last_mut() {
            g.2 = walk.boxes.len();
        }
    }

    /// The last walk's scrollers with rows and the range of its boxes each
    /// one's rows are: the boxes a moved paint moved.
    pub(crate) fn row_groups(&self) -> &[(ViewId, usize, usize)] {
        &self.rows.groups
    }

    /// Whether `node`'s children are rows this walk. A scroller inside a
    /// recording row moves without a commit: the row keeps its offset and
    /// is recorded again once the offset is another.
    pub(super) fn has_rows(&mut self, walk: &Walk<'_, '_>, node: &NodeRef<'_>) -> bool {
        let (x, y) = effective_overflow(node);
        if let Some(c) = &mut self.rows.recording {
            if scrolls(x) || scrolls(y) {
                let at = walk
                    .scene
                    .scroll
                    .get(&node.id)
                    .copied()
                    .unwrap_or((0.0, 0.0));
                c.scrolls.push((node.id, at));
                // Its zoom moves without a commit or an offset.
                c.refused |= zoom::range(node).is_some();
            }
            return false;
        }
        // A zoomable container's child is drawn at the zoom, which a row
        // replayed at another offset would not follow.
        self.rows.active
            && (self.rows.refer.is_some() || self.backend.rows() || self.backend.geometry_rows())
            && (scrolls(y) || self.rows_wrapper(node.key))
            && zoom::range(node).is_none()
    }

    /// Rows neither record nor keep until [`Painter::rows_resume`], while a
    /// drawing is kept apart (`stage.rs`). With `refer`, a row the walk drew
    /// this frame goes into the drawing by reference, if the drawing's
    /// placement in the page has the scale given; else every node is walked.
    pub(super) fn rows_pause(&mut self, refer: Option<Refer>) -> Paused {
        let active = self.rows.active;
        Paused(
            self.rows.recording.take(),
            std::mem::replace(&mut self.rows.active, active && refer.is_some()),
            std::mem::replace(&mut self.rows.refer, refer),
        )
    }

    pub(super) fn rows_resume(&mut self, Paused(recording, active, refer): Paused) {
        self.rows.recording = recording;
        self.rows.active = active;
        self.rows.refer = refer;
    }

    /// Whether `key` is the container walked for its scroller's rows.
    pub(super) fn rows_wrapper(&self, key: NodeKey) -> bool {
        self.rows.wrapper == Some(key)
    }

    /// Whether a scroller's `children` are one container of several whose
    /// children are the rows (`scroll > column > rows`): each of them then
    /// records, and is kept, apart. `EXACT_WRAPPED_ROWS=0` keeps the
    /// container one row, to compare.
    pub(super) fn wraps_rows(&self, walk: &Walk<'_, '_>, children: &[ViewId]) -> bool {
        static ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            !std::env::var("EXACT_WRAPPED_ROWS").is_ok_and(|v| v == "0")
        });
        let [only] = children else { return false };
        *ON && walk
            .scene
            .kernel
            .node(*only)
            .is_some_and(|n| n.children().len() > 1 && !scrolls(effective_overflow(&n).1))
    }

    /// A column with a sticky heading keeps its body rows apart.
    /// [`sticky_parents`], found once a walk and shared by every row group in
    /// it, a split section's too.
    pub(super) fn row_sections(&mut self, kernel: &exact_kernel::Kernel) -> Rc<IdSet<ViewId>> {
        let frame = self.rows.frame;
        if let Some((_, sections)) = self.rows.sections.as_ref().filter(|(at, _)| *at == frame) {
            return sections.clone();
        }
        let sections = Rc::new(sticky_parents(kernel));
        self.rows.sections = Some((frame, sections.clone()));
        sections
    }

    /// `sections`: [`Painter::row_sections`].
    pub(super) fn is_row_section(
        walk: &Walk<'_, '_>,
        sections: Option<&IdSet<ViewId>>,
        id: ViewId,
    ) -> bool {
        // A sticky node's parent has a sticky child.
        sections.is_some_and(|s| s.contains(&id))
            && walk
                .scene
                .kernel
                .node(id)
                .is_some_and(|node| can_split(&node))
    }

    /// A container walked with its children as the rows.
    pub(super) fn wrapped(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: PaintOffset,
        clip_rect: Option<Rect4>,
    ) {
        let key = walk.scene.kernel.node(id).map(|n| n.key);
        let outer = std::mem::replace(&mut self.rows.wrapper, key);
        self.node(walk, id, ts, offset, clip_rect);
        self.rows.wrapper = outer;
    }

    /// Note a node walked while a row records.
    pub(super) fn row_node(&mut self, walk: &Walk<'_, '_>, node: &NodeRef<'_>, shown: &Presented) {
        let (scene, grid) = (walk.scene, self.grid());
        if let Some(c) = &mut self.rows.recording {
            c.frames.push(RowFrame {
                node: node.key,
                frame: node.frame,
                padding: scene.kernel.resolved_padding(node.key).unwrap_or_default(),
            });
            c.may_have_hit_slop |= c.split_sections && node.style.may_have_hit_slop();
            // The sticky offset can change independently of the row's origin.
            if node.layout_position() == exact_kernel::PositionType::Sticky {
                c.refused |= c.split_sections;
                if self.pixel_grid {
                    let scroll_grid =
                        space::sticky_owner_grid(scene, node, &walk.scroll_grids).flatten();
                    c.sticky.push(RowSticky {
                        node: node.key,
                        grid,
                        scroll_grid,
                        shift: space::sticky_displacement(scene, node, grid, scroll_grid),
                    });
                }
            }
            c.refused |= shown.press != 1.0 || shown.press_translate != [0.; 4];
        }
    }

    /// Note a picture drawn (or missing) while a row records.
    pub(super) fn row_image(&mut self, id: ViewId, image: Option<&Arc<Bitmap>>) {
        if let Some(c) = &mut self.rows.recording {
            // Held weakly: a dropped picture's address is not reused while
            // held, so a new picture never passes for the old.
            c.images.push((id, image.map(Arc::downgrade)));
        }
    }

    /// Note a picture's slot while a row records.
    pub(super) fn row_slot(&mut self, id: ViewId, content: Rect4, edge: Shape, ts: Transform) {
        if let Some(c) = &mut self.rows.recording {
            c.slots.push((id, content, edge, ts));
        }
    }

    /// An image node's picture in its content box, clipped to `edge`, the
    /// content edge's curve, once it has arrived.
    pub(super) fn picture(
        &mut self,
        walk: &Walk<'_, '_>,
        node: &NodeRef<'_>,
        content: Rect4,
        edge: Shape,
        ts: Transform,
    ) {
        let Some(img) = walk.scene.images.get(&node.id) else {
            return;
        };
        // A kept row's slot is painted again outside the node's walk.
        let _accent = super::accent_scope(node);
        if let Some(dst) = object_fit(
            img.natural(),
            object_fit_of(node),
            &node.style.object_position,
            content,
            &walk.scene.kernel.env_of(node.id),
        ) {
            let shown = (walk.scene.presented)(node.id);
            let tint = image_tint(node, &shown, self.dark);
            self.backend.image(
                img,
                img.placed(dst),
                &[edge],
                ts,
                tint,
                node.computed_row(StyleId::ImageRendering, |s| s.image_rendering),
            );
        }
    }

    /// A kept row that only pictures changed in (one arrived, or went):
    /// each is drawn again in its slot, where the backend keeps slots, and
    /// the row stands. A heavy feed's row was otherwise walked whole for
    /// each of its pictures. False: the row is recorded again.
    fn pictures_again(&mut self, walk: &Walk<'_, '_>, key: NodeKey) -> bool {
        let Some(row) = self.rows.kept.get(&key) else {
            return false;
        };
        let rid = row.id;
        let mut changed = Vec::new();
        for (i, (id, kept)) in row.images.iter().enumerate() {
            if same_picture(walk.scene.images.get(id), kept) {
                continue;
            }
            let Some(slot) = row.slots.iter().find(|slot| slot.0 == *id) else {
                return false;
            };
            changed.push((i, *slot));
        }
        for (_, (id, content, edge, ts)) in &changed {
            let Some(node) = walk.scene.kernel.node(*id) else {
                return false;
            };
            if !self.backend.slot_again(rid, *id) {
                return false;
            }
            self.picture(walk, &node, *content, *edge, *ts);
            self.backend.slot_end();
            counters::add(Counter::ImagePatches, 1);
        }
        let row = self.rows.kept.get_mut(&key).expect("kept");
        if !changed.is_empty() {
            row.drawing = None;
        }
        for (i, (id, ..)) in changed {
            row.images[i].1 = walk.scene.images.get(&id).map(Arc::downgrade);
        }
        true
    }

    /// Note a paragraph leased while a row records.
    pub(super) fn row_text(&mut self, key: NodeKey, p: &Rc<Paragraph>) {
        if let Some(c) = &mut self.rows.recording {
            c.text.push((key, p.clone()));
        }
    }

    pub(super) fn row_inline(&mut self, id: ViewId, p: &Rc<inline_frame::PaintedInline>) {
        if let Some(c) = &mut self.rows.recording {
            c.inline.push((id, p.clone()));
        }
    }

    /// What a recording row cannot keep (a caret, a canvas, a 3D island).
    pub(super) fn row_refuse(&mut self) {
        if let Some(c) = &mut self.rows.recording {
            c.refused = true;
        }
    }

    /// Include the recording row's outer clip in a clip in the page's space.
    pub(super) fn row_clip(&self, clip: Option<Rect4>) -> Option<Rect4> {
        within(clip, self.rows.recording.as_ref().and_then(|c| c.clip))
    }

    /// What a box paints past its layout box, which can reach the port while
    /// the box is outside it: its shadows, and its surface while a layout
    /// transition holds it larger (LLP 1063).
    pub(super) fn row_box_reach(
        &mut self,
        shadows: &[shadow::ShadowPaint],
        rect: Rect4,
        layout: Rect4,
        ts: Transform,
    ) {
        if let Some(c) = &mut self.rows.recording {
            if rect != layout {
                c.reach = Some(union(c.reach, bbox(ts, rect)));
            }
            for bounds in shadows
                .iter()
                .filter_map(|shadow| shadow.outer_bounds(rect))
            {
                c.reach = Some(union(c.reach, bbox(ts, bounds)));
            }
        }
    }

    /// Publish where a field's label or value or a button's title was
    /// painted, and keep it with the recording row so a replay publishes it
    /// moved.
    pub(super) fn row_field_text(
        &mut self,
        id: ViewId,
        part: material::Part,
        text: material::FieldText,
    ) {
        self.labels.put(id, part, text);
        if let Some(c) = &mut self.rows.recording {
            c.field_texts.push((id, part, text));
        }
    }

    /// What a recording row paints past its boxes.
    pub(super) fn row_reach(&mut self, r: Rect4) {
        if let Some(c) = &mut self.rows.recording {
            c.reach = Some(union(c.reach, r));
        }
    }

    pub(super) fn row_text_reach(
        &mut self,
        text: &Paragraph,
        at: (f32, f32),
        content: Rect4,
        ts: Transform,
    ) {
        if self.rows.recording.is_none() {
            return;
        }
        let (x, y, w, h) = super::text_shadow::line_bounds(text);
        let lines = (x + at.0, y + at.1, w, h);
        if intersect(lines, content) == lines {
            return;
        }
        let (x, y, w, h) = bbox(ts, lines);
        // Give the lines the same room for glyph overhang as the layout box.
        self.row_reach((x - MARGIN, y - MARGIN, w + 2. * MARGIN, h + 2. * MARGIN));
    }

    /// Whether a kept row draws what it would record now; `pictures`: its
    /// pictures included.
    fn row_valid(
        &self,
        row: &Row,
        node: &NodeRef<'_>,
        walk: &Walk<'_, '_>,
        scale: (f32, f32),
        pictures: bool,
    ) -> bool {
        let kernel = walk.scene.kernel;
        let drawn = self.drawn(node);
        !row.stale
            && row.scale == scale
            && row.size == (drawn.width, drawn.height)
            && row.phase == self.pixel_phase(node.frame)
            && row
                .scrolls
                .iter()
                .all(|(id, at)| walk.scene.scroll.get(id).copied().unwrap_or((0.0, 0.0)) == *at)
            && row.sticky.iter().all(|s| {
                walk.scene.kernel.node_by_key(s.node).is_some_and(|node| {
                    let scroll_grid =
                        space::sticky_owner_grid(walk.scene, &node, &walk.scroll_grids)
                            .unwrap_or(s.scroll_grid);
                    space::sticky_displacement(walk.scene, &node, s.grid, scroll_grid) == s.shift
                })
            })
            && (!pictures
                || row
                    .images
                    .iter()
                    .all(|(id, kept)| same_picture(walk.scene.images.get(id), kept)))
            && (!self.rows.suspect.contains(&node.key)
                || row.frames.iter().all(|saved| {
                    kernel.node_by_key(saved.node).is_some_and(|n| {
                        let (g, f) = (n.frame, saved.frame);
                        let same = |a: f32, b: f32| {
                            if row.phase.is_some() {
                                a == b
                            } else {
                                (a - b).abs() < 0.5
                            }
                        };
                        same(g.x - node.frame.x, f.x)
                            && same(g.y - node.frame.y, f.y)
                            && g.width == f.width
                            && g.height == f.height
                            && kernel.resolved_padding(saved.node).unwrap_or_default()
                                == saved.padding
                    })
                }))
    }

    /// A row: replayed when kept and unchanged, else walked and recorded.
    pub(super) fn row(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: PaintOffset,
        clip_rect: Option<Rect4>,
    ) {
        if self.rows.refer.is_some() {
            return self.row_referred(walk, id, ts, offset, clip_rect);
        }
        let Some(node) = walk.scene.kernel.node(id) else {
            return;
        };
        let split_sections = self.backend.independent_section_rows();
        let geometry = self.backend.geometry_rows();
        if !(self.backend.rows() || geometry)
            || walk.hit_space.is_some()
            || ts.kx != 0.0
            || ts.ky != 0.0
            || (walk.scene.hidden)(id)
            || (split_sections && node.layout_position() == exact_kernel::PositionType::Sticky)
        {
            // A backend that splits sections keeps one row group, the outer
            // scroller's, and none opens inside a layer capture. Another
            // keeps a nested scroller's group, which a move-only frame shows.
            if split_sections || !(self.backend.rows() || geometry) {
                let paused = self.rows_pause(None);
                self.node(walk, id, ts, offset, clip_rect);
                self.rows_resume(paused);
            } else {
                self.node(walk, id, ts, offset, clip_rect);
            }
            return;
        }
        let drawn = self.drawn(&node);
        let r = self.drawn_rect(&node, offset);
        let origin = (ts.sx * r.0 + ts.tx, ts.sy * r.1 + ts.ty);
        let scale = (ts.sx, ts.sy);
        let frame = self.rows.frame;
        let port = (0., 0., self.viewport.0, self.viewport.1);
        let shown = clip_rect.map_or(port, |c| intersect(c, port));
        let kept = self.rows.kept.get(&node.key).filter(|row| {
            !split_sections || row.can_translate_geometry_to(walk.scene.kernel, origin, shown)
        });
        let suspect = self.rows.suspect.contains(&node.key);
        let can_draw = !geometry
            && kept
                .is_some_and(|row| !row.paint_stale && self.backend.row_reusable(row.id, origin));
        let metadata_only = !can_draw
            && !suspect
            && kept.is_some_and(|row| {
                row.can_translate_hits
                    && (geometry
                        || (!self.backend.rows_through()
                            && self
                                .backend
                                .can_keep_clipped_row(row.id, row.bounds_at(origin))))
            });
        let valid = (can_draw || metadata_only)
            && (kept.is_some_and(|row| self.row_valid(row, &node, walk, scale, true))
                || (can_draw
                    && kept.is_some_and(|row| self.row_valid(row, &node, walk, scale, false))
                    && self.pictures_again(walk, node.key)));
        if let Some(row) = self.rows.kept.get(&node.key).filter(|_| valid) {
            counters::add(Counter::RowsReplayed, 1);
            #[cfg(test)]
            if geometry {
                self.rows.geometry_replays += 1;
            }
            let (dx, dy) = if row.local {
                origin
            } else {
                (origin.0 - row.origin.0, origin.1 - row.origin.1)
            };
            for b in &row.boxes {
                let mut b = b.clone();
                b.place_row(dx, dy);
                b.clip = within(b.clip, clip_rect);
                walk.boxes.push(b);
            }
            for (k, p) in &row.text {
                walk.text.insert(*k, p.clone());
            }
            for (id, inline) in &row.inline {
                walk.inline.insert(*id, inline.translated(dx, dy));
            }
            for &(id, part, text) in &row.field_texts {
                self.labels.put(id, part, text.translated(dx, dy));
            }
            self.damage.unsupported |= row.unsupported;
            let bounds = row.bounds_at(origin);
            let rid = row.id;
            if !geometry {
                let row = self.rows.kept.get_mut(&node.key).expect("kept");
                row.seen = frame;
                row.paint_stale |= metadata_only;
                // The pixel replay permits small shifts inside a suspect row;
                // after one, its old hit boxes are no longer safe to translate.
                let shifted = suspect
                    && row.frames.iter().any(|saved| {
                        walk.scene.kernel.node_by_key(saved.node).is_some_and(|n| {
                            n.frame.x - node.frame.x != saved.frame.x
                                || n.frame.y - node.frame.y != saved.frame.y
                        })
                    });
                row.can_translate_hits &= !shifted;
            }
            if can_draw && !self.backend.row_culled(bounds) {
                self.backend.row_draw(rid, origin, bounds);
            }
            return;
        }
        if geometry {
            return self.node(walk, id, ts, offset, clip_rect);
        }
        counters::observe(|s| {
            s.add(Counter::RowsRecorded, 1);
            if let Some(row) = self.rows.kept.get(&node.key) {
                let reason = if row.stale {
                    Some(Counter::RowsStale)
                } else if !self.row_valid(row, &node, walk, scale, false) {
                    Some(if self.rows.suspect.contains(&node.key) {
                        Counter::RowsSuspect
                    } else {
                        Counter::RowsOther
                    })
                } else if row
                    .images
                    .iter()
                    .any(|(id, kept)| !same_picture(walk.scene.images.get(id), kept))
                {
                    Some(Counter::RowsPictures)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    s.add(reason, 1);
                }
            } else {
                s.add(Counter::RowsNew, 1);
            }
        });
        let old = self.rows.kept.remove(&node.key);
        // Potentials found this epoch, else the old recording's when its
        // subtree is unchanged (recorded again for its size or a picture).
        let order = self
            .fresh_order
            .get(&id)
            .copied()
            .or_else(|| old.as_ref().filter(|o| !o.stale).and_then(|o| o.order));
        let previous = old.map(|old| old.id);
        self.rows.next += 1;
        let start = walk.boxes.len();
        let unsupported = std::mem::replace(&mut self.damage.unsupported, false);
        // Keep untransformed rows' hits in recording coordinates so replay
        // preserves fractional edges. Transformed rows walk in viewport
        // coordinates, where software warps clip.
        let local = self.backend.row_local() && ts == Transform::identity();
        let (walk_offset, walk_ts, content) = if local {
            (
                self.drawn_offset((node.frame.x, node.frame.y)),
                Transform::identity(),
                (0.0, 0.0),
            )
        } else {
            (offset, ts, origin)
        };
        self.backend.row_begin(self.rows.next, content, previous);
        self.rows.recording = Some(Capture {
            clip: clip_rect,
            split_sections,
            ..Capture::default()
        });
        let page = self.kept.to_page;
        if local {
            self.kept.to_page = page.pre_translate(origin.0, origin.1);
        }
        self.node(walk, id, walk_ts, walk_offset, None);
        self.kept.to_page = page;
        let capture = self.rows.recording.take().expect("recording");
        let boxes = walk.boxes[start..].to_vec();
        if local {
            for b in &mut walk.boxes[start..] {
                b.place_row(origin.0, origin.1);
                if b.projective.is_none() {
                    if let Some(field) = walk.fields.get_mut(&b.id) {
                        let field = Rc::make_mut(field);
                        field.transform = field.transform.post_translate(origin.0, origin.1);
                    }
                }
            }
            for (id, inline) in &capture.inline {
                if let Some(published) = walk.inline.get_mut(id) {
                    *published = inline.translated(origin.0, origin.1);
                }
            }
            for &(id, part, text) in &capture.field_texts {
                self.labels
                    .put(id, part, text.translated(origin.0, origin.1));
            }
        }
        let row_unsupported = std::mem::replace(&mut self.damage.unsupported, unsupported);
        self.damage.unsupported |= row_unsupported;
        for b in &mut walk.boxes[start..] {
            b.clip = within(b.clip, clip_rect);
            if let Some(hit) = b.hit.as_mut() {
                hit.clip = within(hit.clip, clip_rect);
            }
        }
        // What the row can draw: each box within its clip (a scroller's
        // content box is its whole extent, a horizontal list's thousands of
        // px wide; only its port shows), one clipped away entirely nothing.
        let covered = boxes
            .iter()
            .filter_map(|b| match b.clip {
                None => Some(b.rect),
                Some(c) => Some(intersect(b.rect, c)).filter(|r| r.2 > 0.0 && r.3 > 0.0),
            })
            .fold(None, |u, r| Some(union(u, r)))
            .unwrap_or((content.0, content.1, 0.0, 0.0));
        let placed = sticky(
            walk.scene,
            id,
            offset,
            self.grid().filter(|_| !grid::changes_axes(ts)),
            &walk.scroll_grids,
        );
        let p = (walk.scene.presented)(id);
        let rect = self.node_rect(&node, placed, &p);
        let space =
            ts.pre_concat(p.transform(rect, node.style.transform_origin.resolve(rect.2, rect.3)));
        let covered = self
            .kept_bounds(walk, &node, space, placed)
            .map_or(covered, |bounds| {
                let (x, y, w, h) = bbox(space, bounds);
                union(
                    Some(covered),
                    (x - (origin.0 - content.0), y - (origin.1 - content.1), w, h),
                )
            });
        let bounds = (
            covered.0 - content.0 - MARGIN,
            covered.1 - content.1 - MARGIN,
            covered.2 + 2.0 * MARGIN,
            covered.3 + 2.0 * MARGIN,
        );
        // And what it paints past its boxes, which can pass the margin.
        let bounds = match capture.reach {
            Some(r) => union(Some(bounds), (r.0 - content.0, r.1 - content.1, r.2, r.3)),
            None => bounds,
        };
        // The backend keeps the previous recording when this one is the same.
        let rid = self.backend.row_end(bounds);
        if let Some(old) = previous.filter(|old| *old != rid) {
            self.rows.freed.push(old);
        }
        let page = (bounds.0 + origin.0, bounds.1 + origin.1, bounds.2, bounds.3);
        if !self.backend.row_culled(page) {
            self.backend.row_draw(rid, origin, page);
        }
        let keep = !capture.refused && boxes.iter().all(|b| b.projective.is_none());
        if !keep {
            self.rows.freed.push(rid);
            return;
        }
        let (x0, y0) = (node.frame.x, node.frame.y);
        self.rows.kept.insert(
            node.key,
            Row {
                id: rid,
                drawing: None,
                origin,
                local,
                scale,
                size: (drawn.width, drawn.height),
                phase: self.pixel_phase(node.frame),
                can_translate_hits: boxes.iter().all(|b| b.hit.is_none()),
                may_have_hit_slop: capture.may_have_hit_slop,
                paint_stale: false,
                boxes,
                text: capture.text,
                inline: capture.inline,
                field_texts: capture.field_texts,
                images: capture.images,
                slots: capture.slots,
                frames: capture
                    .frames
                    .into_iter()
                    .map(|mut saved| {
                        saved.frame.x -= x0;
                        saved.frame.y -= y0;
                        saved
                    })
                    .collect(),
                scrolls: capture.scrolls,
                sticky: capture.sticky,
                unsupported: row_unsupported,
                bounds,
                seen: frame,
                stale: false,
                order,
            },
        );
    }
}

impl Painter {
    /// A row in a drawing being kept apart: the kept row the walk drew this
    /// frame, with its portable commands and a reference, if it was drawn
    /// at the scale the drawing is placed at; any other is walked.
    fn row_referred(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: PaintOffset,
        clip_rect: Option<Rect4>,
    ) {
        let Some(node) = walk.scene.kernel.node(id) else {
            return;
        };
        let Some((recorder, place)) = self.rows.refer.clone() else {
            return;
        };
        let near = |a: f32, b: f32| (a - b).abs() <= 1e-4 * b.abs().max(1.0);
        let drawn = (self.rows.kept.get(&node.key)).filter(|row| {
            row.seen == self.rows.frame
                && near(row.scale.0, place.0 * ts.sx)
                && near(row.scale.1, place.1 * ts.sy)
        });
        let plain = self.backend.name() == "tape"
            && ts.kx == 0.0
            && ts.ky == 0.0
            && walk.hit_space.is_none();
        let Some((rid, bounds)) = drawn
            .filter(|_| plain && !(walk.scene.hidden)(id))
            .map(|row| (row.id, row.bounds))
        else {
            #[cfg(test)]
            {
                self.rows.referred.1 += 1;
            }
            return self.node(walk, id, ts, offset, clip_rect);
        };
        #[cfg(test)]
        {
            self.rows.referred.0 += 1;
        }
        let pin = match self.rows.pins.get(&rid).and_then(std::rc::Weak::upgrade) {
            Some(pin) => pin,
            None => {
                let pin = Rc::new(RowPin(rid));
                self.rows.pins.insert(rid, Rc::downgrade(&pin));
                pin
            }
        };
        let r = self.drawn_rect(&node, offset);
        let origin = (ts.sx * r.0 + ts.tx, ts.sy * r.1 + ts.ty);
        let drawing = match self
            .rows
            .kept
            .get(&node.key)
            .and_then(|row| row.drawing.clone())
        {
            Some(drawing) => drawing,
            None => {
                let commands = self.record_node(walk, id, ts, offset, false);
                let normalized = Recorder::default();
                normalized.nest(
                    Rc::new(commands),
                    Transform::from_translate(-origin.0, -origin.1),
                );
                let drawing = Rc::new(normalized.tape());
                self.rows.kept.get_mut(&node.key).expect("kept").drawing = Some(drawing.clone());
                drawing
            }
        };
        recorder.row(pin, origin, bounds, place, drawing);
    }

    #[cfg(test)]
    pub(crate) fn geometry_rows_replayed(&self) -> usize {
        self.rows.geometry_replays
    }

    /// Rows drawings took by reference, and rows they walked again.
    #[cfg(test)]
    pub(crate) fn rows_referred(&self) -> (usize, usize) {
        self.rows.referred
    }
}

impl PaintedBox {
    fn place_row(&mut self, dx: f32, dy: f32) {
        let translate = |(x, y, w, h): Rect4| (x + dx, y + dy, w, h);
        let mapping = |(ts, rect): (Transform, Rect4)| (ts.post_translate(dx, dy), rect);
        let unplace = [1., 0., -dx, 0., 1., -dy, 0., 0., 1.];
        self.translate_clips(dx, dy);
        self.rect = translate(self.rect);
        self.clip = self.clip.map(translate);
        if let Some((inv, ..)) = &mut self.projective {
            *inv = space::mul3(inv, &unplace);
        } else {
            self.affine = self.affine.map(mapping);
        }
        if let Some(hit) = &mut self.hit {
            for clip in &mut hit.shape_clips {
                clip.translate(dx, dy);
            }
            hit.rect = translate(hit.rect);
            hit.clip = hit.clip.map(translate);
            if let Some((inv, ..)) = &mut hit.projective {
                *inv = space::mul3(inv, &unplace);
            } else {
                hit.affine = hit.affine.map(mapping);
            }
        }
    }
}

/// Whether a node's picture is the one a row was recorded with.
fn same_picture(now: Option<&Arc<Bitmap>>, kept: &Option<std::sync::Weak<Bitmap>>) -> bool {
    match (now, kept) {
        (None, None) => true,
        (Some(now), Some(then)) => then.upgrade().is_some_and(|t| Arc::ptr_eq(&t, now)),
        _ => false,
    }
}

/// Whether an overflow scrolls: `scroll`, or `auto` (a scroller's default
/// since the kernel follows CSS there).
fn scrolls(o: Overflow) -> bool {
    matches!(o, Overflow::Scroll | Overflow::Auto)
}
