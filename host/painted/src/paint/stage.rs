//! The routes a touch shell's presenter places itself (`presenter/stage.rs`):
//! a sheet over its scrim, and two routes crossing.
//!
//! A sheet's routes, and on a shell with no bars a route shown in a sheet's
//! box, are placed by the presenter: the walk leaves them to the stage's
//! display list. An open or closing sheet and its scrim are drawn after the
//! whole page, under its top layer, as Compose's `ModalBottomSheet` is a
//! window over the activity's content: what the app wrote after the routes,
//! its own tab bar included, is dimmed beneath it. The rest is drawn after
//! the last route of the stack, so what the app wrote after the routes (a
//! tab bar) stays above a page that moves.
//!
//! A crossing in the page is drawn where the walk paints its routes: each
//! one with its ancestors' transform, opacity and clip, in a split view's
//! pane, and at its place in paint order, with the crossing's opacity and
//! motion as one more layer ([`Cross`]). A route a commit removed is drawn
//! from its kept drawing (`tape.rs`) relative to its nearest ancestor that
//! remains, under that ancestor's current transform, scroll and compositing,
//! by its rank and the nearest sibling before it that remains. A
//! kept drawing takes no touch.
//!
//! What a commit removes with a route goes with it: replay starts at the
//! root of the removed branch. Besides the routes, the walk keeps each
//! ancestor below the navigation root, including its own paint and all its
//! descendants' compositing.
//!
//! A drawing is kept on every frame, by walking the node a second time. A
//! retained row goes in with its commands and a reference (`rows.rs`), as
//! does a kept node inside it. Filters rasterize those commands at replay.
use super::tape::{record, Recorder, Tape};
use super::*;
use exact_kernel::NodeKey;

/// One step of the stage's drawing, in viewport points.
pub(crate) enum Item {
    /// Clip to a shape until [`Item::PopClip`].
    Clip(Shape, Transform),
    PopClip,
    /// Draw at an opacity until [`Item::PopOpacity`].
    Opacity(f32),
    PopOpacity,
    Fill(Shape, [u8; 4], Transform),
    /// A live route, its border box at the origin of `ts`'s space; `tape`
    /// keeps its drawing.
    Route {
        id: ViewId,
        ts: Transform,
        tape: bool,
    },
    /// A route's kept drawing, through `ts`.
    Taped(NodeKey, Transform),
    /// A picture of the presenter's own (a bar), through `ts`.
    Picture(Rc<Tape>, Transform),
    /// A box that takes the touches in a rectangle for a node: a scrim is
    /// its sheet's.
    Hit(ViewId, Rect4),
}

/// A route crossing in the page, drawn where the walk paints it.
#[derive(Clone)]
pub(crate) struct Cross {
    /// The node the crossing moves: a route, or the destination that holds
    /// it; a removed one is drawn from its kept drawing.
    pub(crate) key: NodeKey,
    /// The crossing's motion, in the node's border-box space, or with
    /// `page` in the page's.
    pub(crate) motion: Transform,
    pub(crate) page: bool,
    pub(crate) opacity: f32,
    /// Its corners' radius while Back shrinks it or a container grows.
    pub(crate) radius: f32,
    /// A width-fitted container clips the node to this height in its
    /// border box.
    pub(crate) height: Option<f32>,
    /// Its top bar's picture and place in the page, drawn over it. A
    /// page-space motion leaves it in place.
    pub(crate) bar: Option<(Rc<Tape>, Transform)>,
}

impl Cross {
    /// The crossing's pose of a border box at the origin of `origin`.
    fn pose(&self, origin: Transform) -> Transform {
        match self.page {
            true => self.motion.pre_concat(origin),
            false => origin.pre_concat(self.motion),
        }
    }

    /// The top bar's transform for a node whose border box is at `origin`:
    /// its place in the window, moved as the crossing moves the node on
    /// screen; the node's ancestors neither place nor size it.
    fn bar(&self, origin: Transform, bar: Transform) -> Transform {
        match (self.page, origin.invert()) {
            (false, Some(inverse)) => self.pose(origin).pre_concat(inverse).pre_concat(bar),
            _ => bar,
        }
    }
}

/// What the stage draws this frame.
#[derive(Default)]
pub(crate) struct Stage {
    /// The routes after the last of which, in paint order, the stage draws.
    pub(crate) after: Vec<ViewId>,
    /// The live routes the walk leaves to the stage.
    pub(crate) skip: Vec<ViewId>,
    /// The routes the walk draws whose drawing is kept as well: a route
    /// Back would pop, or one crossing.
    pub(crate) tape: Vec<ViewId>,
    /// Those kept too, which only a switch to another stack would remove.
    pub(crate) rest: Vec<ViewId>,
    /// Their ancestors below the navigation root, kept as complete branches.
    around: Vec<ViewId>,
    /// The routes drawn on a sheet's own ground: a background their author
    /// did not write is not painted.
    pub(crate) bare: Vec<ViewId>,
    pub(crate) items: Vec<Item>,
    /// The open and closing sheets over their scrims, drawn after the page.
    pub(crate) sheets: Vec<Item>,
    /// Drawn after a route of their own instead, under what the stack
    /// presents above it: a sheet a full-screen route covers.
    pub(crate) beneath: Vec<(ViewId, Vec<Item>)>,
    pub(crate) crossing: Vec<Cross>,
    /// The live nodes of `crossing`, with their index in it.
    live: Vec<(ViewId, usize)>,
    /// The removed ones, where their kept drawings go.
    replays: Vec<Replay>,
    /// The top bars' transforms as drawn this frame.
    #[cfg(test)]
    pub(crate) bars: Vec<Transform>,
}

/// A removed crossing node's kept drawing, at its place in paint order.
struct Replay {
    /// Its crossing, in [`Stage::crossing`].
    cross: usize,
    /// Its nearest ancestor that remains (none: the roots), and the place
    /// of its branch among that ancestor's children.
    parent: Option<ViewId>,
    rank: i64,
    before: Vec<NodeKey>,
    /// The complete branch's drawing.
    key: NodeKey,
    /// The drawing and its pane in the remaining ancestor's child space.
    at: Transform,
    pane: Option<Pane>,
}

/// The pane or column a split view draws a route in (`split.rs`): `rect`
/// in the space of `ts`, which clips the route, filled with `ground`.
#[derive(Clone, Copy)]
pub(crate) struct Pane {
    pub(crate) rect: Rect4,
    pub(crate) ts: Transform,
    pub(crate) ground: Option<[u8; 4]>,
}

/// The drawings kept of routes, by route.
#[derive(Default)]
pub(crate) struct Kept {
    routes: HashMap<NodeKey, KeptRoute>,
    /// The live walk's ancestors and their children's spaces. Recording a
    /// drawing leaves these alone: its placement is the live walk's.
    spaces: Vec<(ViewId, Transform)>,
    /// A drawing is being recorded, into this: it is of the node at rest,
    /// with no crossing and nothing the stage adds.
    recording: Option<Recorder>,
    /// Maps a retained row's local recording space into the page.
    pub(super) to_page: Transform,
    /// How many drawings have been recorded.
    #[cfg(test)]
    pub(crate) recorded: usize,
}

struct KeptRoute {
    tape: Rc<Tape>,
    /// Its border box's space in the page, ancestors' transforms included.
    placement: Transform,
    /// Its border box's size.
    size: (f32, f32),
    /// The page offset when recorded, for a replay among the roots.
    page: (f32, f32),
    /// Its place in paint order and each ancestor's, from the route up.
    path: Vec<Step>,
    /// The split view's pane it was drawn in.
    pane: Option<Pane>,
    /// The paint that recorded it, by the rows' count of paints.
    seen: u64,
}

/// A node's place among its siblings: its rank and the siblings before it
/// in the tree, nearest first, so that its place is found among those that
/// remain when the commit that removed it removed some of them too.
struct Step {
    key: NodeKey,
    rank: i64,
    before: Vec<NodeKey>,
    /// The space in which this ancestor painted its children.
    space: Option<Transform>,
}

impl Kept {
    pub(super) fn push_space(&mut self, id: ViewId, space: Transform) {
        if self.recording.is_none() {
            self.spaces.push((id, self.to_page.pre_concat(space)));
        }
    }

    pub(super) fn pop_space(&mut self) {
        if self.recording.is_none() {
            self.spaces.pop();
        }
    }
}

impl Painter {
    /// How many drawings have been kept.
    #[cfg(test)]
    pub(crate) fn kept_recorded(&self) -> usize {
        self.kept.recorded
    }

    /// The stage for the next frame; drawings it neither draws nor keeps
    /// are let go.
    pub(crate) fn set_stage(&mut self, kernel: &Kernel, mut stage: Stage) {
        let routes: Vec<ViewId> = (stage.tape.iter().chain(&stage.rest).copied())
            .chain(
                stage
                    .items
                    .iter()
                    .chain(&stage.sheets)
                    .filter_map(|item| match item {
                        Item::Route { id, tape: true, .. } => Some(*id),
                        _ => None,
                    }),
            )
            .collect();
        // The ancestors a switch could remove with a route.
        for &route in &routes {
            stage
                .rest
                .extend(destination_path(kernel, route, &[]).into_iter().skip(1));
        }
        let tape = stage.tape.clone();
        stage.rest.retain(|id| !tape.contains(id));
        for list in [&mut stage.tape, &mut stage.rest] {
            list.sort_unstable();
            list.dedup();
        }
        // The navigation root, the first that carries a key, no commit removes.
        let below_root = |id: &ViewId| {
            (kernel.node(*id)).is_some_and(|n| n.props.str(PropId::NavigationKey).is_none())
        };
        for route in routes {
            let path = ancestors(kernel, route).into_iter().skip(1);
            stage.around.extend(path.take_while(below_root));
        }
        let (tape, rest) = (&stage.tape, &stage.rest);
        stage.around.sort_unstable();
        stage.around.dedup();
        stage
            .around
            .retain(|id| tape.binary_search(id).is_err() && rest.binary_search(id).is_err());
        for (at, cross) in stage.crossing.iter().enumerate() {
            match kernel.node_by_key(cross.key) {
                Some(node) => stage.live.push((node.id, at)),
                None => stage.replays.extend(self.replay(kernel, at, cross.key)),
            }
        }
        let key = |id: &ViewId| kernel.node(*id).map(|n| n.key);
        let beneath = stage.beneath.iter().flat_map(|(_, items)| items);
        let kept: Vec<NodeKey> = (stage.items.iter().chain(&stage.sheets).chain(beneath))
            .filter_map(|item| match item {
                Item::Taped(key, _) => Some(*key),
                Item::Route { id, tape: true, .. } => key(id),
                _ => None,
            })
            .chain((stage.tape.iter().chain(&stage.rest).chain(&stage.around)).filter_map(key))
            .chain(stage.crossing.iter().map(|cross| cross.key))
            .chain(stage.replays.iter().map(|replay| replay.key))
            .collect();
        self.kept.routes.retain(|key, _| kept.contains(key));
        self.stage = stage;
    }

    /// Replay the leaving branch's complete recorded drawing, from its root
    /// down: every removed node's own paint (background, border, shadow) and
    /// compositing (opacity, clip, transform, mask, filter). Its placement is
    /// relative to the nearest surviving ancestor's child space. Recording
    /// removes the page placement, including row-local and scroll offsets.
    /// Sticky constraints are separate commands: each reads a surviving
    /// scroller's current offset, or keeps its offset if that scroller left.
    /// Replay applies the surviving ancestor's live placement once, inside
    /// its live compositing, at the removed branch's place among its children.
    /// Every retained row also holds commands a different backend can draw.
    fn replay(&self, kernel: &Kernel, cross: usize, key: NodeKey) -> Option<Replay> {
        let path = &self.kept.routes.get(&key)?.path;
        let alive = path
            .iter()
            .position(|s| kernel.node_by_key(s.key).is_some());
        let gone = &path[..alive.unwrap_or(path.len())];
        let branch = gone.last()?;
        let kept = self.kept.routes.get(&branch.key)?;
        let space = match alive {
            Some(at) => path[at].space?,
            None => Transform::from_translate(-kept.page.0, -kept.page.1),
        };
        let inverse = space.invert()?;
        Some(Replay {
            cross,
            parent: alive
                .and_then(|at| kernel.node_by_key(path[at].key))
                .map(|n| n.id),
            rank: branch.rank,
            before: branch.before.clone(),
            key: branch.key,
            at: inverse.pre_concat(kept.placement),
            pane: kept.pane.map(|pane| Pane {
                ts: inverse.pre_concat(pane.ts),
                ..pane
            }),
        })
    }

    /// Whether a drawing of `key` is kept.
    pub(crate) fn kept(&self, key: NodeKey) -> bool {
        self.kept.routes.contains_key(&key)
    }

    /// The rows are gone from the backend: no drawing holding one stands.
    pub(super) fn unkeep_rows(&mut self) {
        self.kept.routes.retain(|_, route| !route.tape.draws_rows());
    }

    /// Whether the walk leaves `id` to the stage.
    pub(super) fn staged(&self, id: ViewId) -> bool {
        self.kept.recording.is_none() && self.stage.skip.contains(&id)
    }

    /// Paint `id` as the walk would, in `pane` if a split view draws it in
    /// one, with its crossing; and keep its drawing if the walk keeps it.
    pub(super) fn route_node(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
        clip: Option<Rect4>,
        pane: Option<Pane>,
    ) {
        let clip = match pane {
            Some(pane) => {
                let clip = self.split_clip(pane.rect, pane.ts, clip);
                if let Some(ground) = pane.ground {
                    self.backend.fill(&Shape::rect(pane.rect), ground, pane.ts);
                }
                clip
            }
            None => clip,
        };
        if !self.nest_kept(walk, id, ts, offset) {
            match self.crossing(id) {
                Some(cross) => self.cross_node(walk, cross, id, ts, offset, clip),
                None => self.node_at(walk, id, ts, offset, clip),
            }
        }
        if pane.is_some() {
            self.backend.pop_clip();
        }
        if self.keeps(walk, id) {
            self.keep_route(walk, id, ts, offset, pane);
        }
    }

    /// While a drawing is recorded, a node inside it whose drawing this
    /// paint kept goes in by reference instead of being walked again.
    fn nest_kept(
        &self,
        walk: &Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
    ) -> bool {
        // A filter may have replaced the recorder with a raster. Its
        // descendants must draw into that raster, inside the filter.
        if self.backend.name() != "tape" {
            return false;
        }
        let (Some(recorder), Some(node)) = (&self.kept.recording, walk.scene.kernel.node(id))
        else {
            return false;
        };
        let Some(kept) = (self.kept.routes.get(&node.key)).filter(|k| k.seen == self.rows.frame)
        else {
            return false;
        };
        let at = ts.pre_translate(node.frame.x - offset.0, node.frame.y - offset.1);
        recorder.nest(kept.tape.clone(), at);
        true
    }

    /// The crossing `id` is drawn with, by its index.
    pub(super) fn crossing(&self, id: ViewId) -> Option<usize> {
        if self.kept.recording.is_some() {
            return None;
        }
        (self.stage.live.iter()).find_map(|(live, at)| (*live == id).then_some(*at))
    }

    /// Paint `id` as the walk would, with its crossing as one more layer:
    /// its boxes are hit where it shows, through its ancestors' clips.
    pub(super) fn cross_node(
        &mut self,
        walk: &mut Walk<'_, '_>,
        cross: usize,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
        clip: Option<Rect4>,
    ) {
        let Some(node) = walk.scene.kernel.node(id) else {
            return;
        };
        let cross = self.stage.crossing[cross].clone();
        let placed = sticky(walk.scene, id, offset);
        let at = (node.frame.x - placed.0, node.frame.y - placed.1);
        let size = (node.frame.width, node.frame.height);
        let hit_space = walk.hit_space;
        self.cross_layer(
            &cross,
            ts.pre_translate(at.0, at.1),
            size,
            |painter, posed| {
                let unplace = Transform::from_translate(-at.0, -at.1);
                walk.hit_space = hit_space.map(|(hit, clip)| {
                    (
                        cross
                            .pose(hit.pre_translate(at.0, at.1))
                            .pre_concat(unplace),
                        clip,
                    )
                });
                painter.node_at(walk, id, posed.pre_concat(unplace), offset, clip);
            },
        );
        walk.hit_space = hit_space;
    }

    /// Draw with a crossing: its opacity, its rounded corners, `draw` given
    /// the pose of a border box at `origin`, and its top bar.
    fn cross_layer(
        &mut self,
        cross: &Cross,
        origin: Transform,
        size: (f32, f32),
        draw: impl FnOnce(&mut Self, Transform),
    ) {
        self.row_refuse();
        self.damage.unsupported = true;
        let fades = cross.opacity < 1.0;
        if fades {
            self.backend.push_opacity(cross.opacity.clamp(0.0, 1.0));
        }
        let posed = cross.pose(origin);
        let rounded = cross.radius > 0.0 || cross.height.is_some();
        if rounded {
            let height = cross.height.unwrap_or(size.1);
            let shape = Shape::new((0.0, 0.0, size.0, height), [cross.radius; 4]);
            self.backend.push_clip(&shape, posed);
        }
        draw(self, posed);
        if let Some((tape, bar)) = &cross.bar {
            let ts = cross.bar(origin, *bar);
            #[cfg(test)]
            self.stage.bars.push(ts);
            tape.replay(self.backend.as_mut(), &mut self.text.borrow_mut(), ts);
        }
        if rounded {
            self.backend.pop_clip();
        }
        if fades {
            self.backend.pop_opacity();
        }
    }

    /// Where among `children`, in paint order, of `parent` (none: the
    /// roots) the kept drawings of removed crossing nodes go: before the
    /// child at each index, by the [`Replay`] at the other.
    pub(super) fn kept_places(
        &self,
        walk: &Walk<'_, '_>,
        parent: Option<&NodeRef<'_>>,
        children: &[ViewId],
    ) -> Vec<(usize, usize)> {
        let id = parent.map(|n| n.id);
        if self.kept.recording.is_some() || !self.stage.replays.iter().any(|r| r.parent == id) {
            return Vec::new();
        }
        let kernel = walk.scene.kernel;
        let tree = parent.map_or_else(|| walk.scene.roots.to_vec(), |n| n.children());
        let index = |id: &ViewId| tree.iter().position(|t| t == id);
        let rank = |id: &ViewId| walk.ranks.get(id).copied().unwrap_or(0);
        (self.stage.replays.iter().enumerate())
            .filter(|(_, replay)| replay.parent == id)
            .map(|(at, replay)| {
                let prev = replay.before.iter().find_map(|key| {
                    tree.iter()
                        .position(|id| kernel.node(*id).is_some_and(|n| n.key == *key))
                });
                let before = children.iter().position(|child| {
                    !self.placements.contains_key(child)
                        && (rank(child) > replay.rank
                            || rank(child) == replay.rank && index(child) > prev)
                });
                (before.unwrap_or(children.len()), at)
            })
            .collect()
    }

    /// Draw the kept drawings `kept_places` put before the child at `at`,
    /// under the parent's current child space.
    pub(super) fn replay_kept(
        &mut self,
        scene: &Scene<'_>,
        kept: &[(usize, usize)],
        at: usize,
        space: Transform,
    ) {
        for &(_, replay) in kept.iter().filter(|(before, _)| *before == at) {
            let replay = &self.stage.replays[replay];
            let Some(kept) = self.kept.routes.get(&replay.key) else {
                continue;
            };
            let tape = kept.tape.clone();
            let origin = space.pre_concat(replay.at);
            let pane = replay.pane.map(|pane| Pane {
                ts: space.pre_concat(pane.ts),
                ..pane
            });
            let cross = self.stage.crossing[replay.cross].clone();
            let size = kept.size;
            // A pane clips where it stands; its ground moves with the route.
            if let Some(pane) = pane {
                self.backend.push_clip(&Shape::rect(pane.rect), pane.ts);
            }
            self.cross_layer(&cross, origin, size, |painter, posed| {
                let ground = pane.and_then(|pane| Some((pane, pane.ground?, origin.invert()?)));
                if let Some((pane, ground, unplace)) = ground {
                    let ts = posed.pre_concat(unplace).pre_concat(pane.ts);
                    painter.backend.fill(&Shape::rect(pane.rect), ground, ts);
                }
                tape.replay_live(
                    painter.backend.as_mut(),
                    &mut painter.text.borrow_mut(),
                    posed,
                    Some(tape::Live {
                        scene,
                        viewport: painter.viewport,
                        scale: painter.scale,
                    }),
                )
            });
            if pane.is_some() {
                self.backend.pop_clip();
            }
        }
    }

    /// Removed drawings contribute to every subtree-sized effect through
    /// `space::reach` and retained-row bounds, in the caller's coordinates. Resolve placement and
    /// sticky constraints from the same live state used by replay.
    pub(super) fn kept_bounds(
        &self,
        walk: &Walk<'_, '_>,
        root: &NodeRef<'_>,
        ts: Transform,
        offset: (f32, f32),
    ) -> Option<Rect4> {
        if self.kept.recording.is_some() || self.stage.replays.is_empty() {
            return None;
        }
        let affine =
            |ts| crate::placement::compose([1., 0., 0., 0., 1., 0., 0., 0., 1.], ts, 0., 0.);
        let unplace = affine(ts.invert()?);
        let kernel = walk.scene.kernel;
        let mut area = None;
        for replay in &self.stage.replays {
            let Some(parent) = replay.parent else {
                continue;
            };
            let path = ancestors(kernel, parent);
            let Some(at) = path.iter().position(|id| *id == root.id) else {
                continue;
            };
            let mut space = affine(ts);
            let mut offset = offset;
            for &id in path[..=at].iter().rev() {
                let node = kernel.node(id)?;
                if id != root.id {
                    offset = sticky(walk.scene, id, offset);
                    let rect = paint_rect(node.frame, offset);
                    let p = (walk.scene.presented)(id);
                    space = space::mul3(
                        &space,
                        &space::mapping_at(
                            self.split_parent(kernel, &node),
                            &node,
                            &p,
                            rect,
                            offset,
                        ),
                    );
                }
                let (ox, oy) = effective_overflow(&node);
                if !matches!(ox, Overflow::Visible | Overflow::Clip)
                    || !matches!(oy, Overflow::Visible | Overflow::Clip)
                {
                    let d = walk.scene.scroll.get(&id).copied().unwrap_or_default();
                    offset = (offset.0 + d.0, offset.1 + d.1);
                }
                let zoom = self.zoom_children(kernel, &node, Transform::identity(), offset);
                space = space::mul3(&space, &affine(zoom));
            }
            let node = kernel.node(parent)?;
            let space = space::mul3(
                &space,
                &affine(Transform::from_translate(
                    node.frame.x - offset.0,
                    node.frame.y - offset.1,
                )),
            );
            let origin = space::mul3(&space, &affine(replay.at));
            let cross = &self.stage.crossing[replay.cross];
            let motion = affine(cross.motion);
            let posed = match cross.page {
                true => space::mul3(&motion, &origin),
                false => space::mul3(&origin, &motion),
            };
            let posed = space::mul3(&unplace, &posed);
            let kept = self.kept.routes.get(&replay.key)?;
            let mut bounds = kept.tape.bounds_in(walk.scene, self.scale, posed);
            if let Some(pane) = replay.pane {
                let matrix = space::mul3(&unplace, &space::mul3(&space, &affine(pane.ts)));
                if let Some(clip) = space::mapped_bounds(&matrix, pane.rect) {
                    bounds = bounds.map(|b| intersect(b, clip));
                    if pane.ground.is_some() {
                        bounds = Some(rows::union(bounds, clip));
                    }
                }
            }
            if let Some((bar, at)) = &cross.bar {
                let matrix = match (cross.page, crate::placement::inverse(origin)) {
                    (false, Some(inverse)) => {
                        space::mul3(&posed, &space::mul3(&inverse, &affine(*at)))
                    }
                    _ => space::mul3(&unplace, &affine(*at)),
                };
                if let Some(b) = bar.bounds_in(walk.scene, self.scale, matrix) {
                    bounds = Some(rows::union(bounds, b));
                }
            }
            if let Some(b) = bounds {
                area = Some(rows::union(area, b));
            }
        }
        area
    }

    /// The child of `siblings`, in paint order, after which the stage draws.
    pub(super) fn stage_after(&self, siblings: &[ViewId]) -> Option<ViewId> {
        (siblings.iter().rev())
            .find(|id| self.stage.after.contains(id))
            .copied()
    }

    /// Draw what the stage draws after the route `after`: a covered sheet
    /// beneath it, and with `items` the stage's list.
    pub(super) fn paint_stage(&mut self, walk: &mut Walk<'_, '_>, after: ViewId, items: bool) {
        if self.kept.recording.is_some() {
            return;
        }
        if let Some(at) = self.stage.beneath.iter().position(|(a, _)| *a == after) {
            let items = std::mem::take(&mut self.stage.beneath[at].1);
            self.stage_items(walk, &items);
            self.stage.beneath[at].1 = items;
        }
        if !items || self.stage.items.is_empty() {
            return;
        }
        let items = std::mem::take(&mut self.stage.items);
        self.stage_items(walk, &items);
        self.stage.items = items;
    }

    /// Draw the open and closing sheets over the whole page.
    pub(super) fn paint_sheets(&mut self, walk: &mut Walk<'_, '_>) {
        if self.kept.recording.is_some() || self.stage.sheets.is_empty() {
            return;
        }
        let items = std::mem::take(&mut self.stage.sheets);
        self.stage_items(walk, &items);
        self.stage.sheets = items;
    }

    fn stage_items(&mut self, walk: &mut Walk<'_, '_>, items: &[Item]) {
        if items.is_empty() {
            return;
        }
        self.row_refuse();
        self.damage.unsupported = true;
        for item in items {
            match item {
                Item::Clip(shape, ts) => self.backend.push_clip(shape, *ts),
                Item::PopClip => self.backend.pop_clip(),
                Item::Opacity(a) => self.backend.push_opacity(a.clamp(0.0, 1.0)),
                Item::PopOpacity => self.backend.pop_opacity(),
                Item::Fill(shape, color, ts) => self.backend.fill(shape, *color, *ts),
                Item::Route { id, ts, tape } => self.stage_route(walk, *id, *ts, *tape),
                Item::Taped(key, ts) => {
                    if let Some(route) = self.kept.routes.get(key) {
                        let tape = route.tape.clone();
                        tape.replay_live(
                            self.backend.as_mut(),
                            &mut self.text.borrow_mut(),
                            *ts,
                            Some(tape::Live {
                                scene: walk.scene,
                                viewport: self.viewport,
                                scale: self.scale,
                            }),
                        );
                    }
                }
                Item::Picture(tape, ts) => {
                    tape.replay(self.backend.as_mut(), &mut self.text.borrow_mut(), *ts)
                }
                Item::Hit(id, rect) => walk.boxes.push(PaintedBox {
                    shape_clips: Vec::new(),
                    id: *id,
                    pointer_hit: true,
                    projective: None,
                    affine: Some((Transform::identity(), *rect)),
                    unpress: Transform::identity(),
                    hit: None,
                    rect: *rect,
                    clip: None,
                    scroll: None,
                }),
            }
        }
    }

    /// A live route at the origin of `ts`, and its drawing kept.
    fn stage_route(&mut self, walk: &mut Walk<'_, '_>, id: ViewId, ts: Transform, tape: bool) {
        let Some(node) = walk.scene.kernel.node(id) else {
            return;
        };
        let at = (node.frame.x, node.frame.y);
        self.node_at(walk, id, ts, at, None);
        if tape {
            self.keep_route(walk, id, ts, at, None);
        }
    }

    /// Whether `id` stands on a sheet's ground, with no background unless
    /// its author wrote one.
    pub(super) fn bare(&self, node: &NodeRef<'_>) -> bool {
        self.stage.bare.contains(&node.id) && !node.style.written.has(StyleId::BackgroundColor)
    }

    /// Record a subtree without changing the live walk's published geometry.
    /// Rows may be reused, together with their backend-independent commands.
    pub(super) fn record_node(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
        refer: bool,
    ) -> Tape {
        let recorder = Recorder::portable(&*self.backend);
        let real = std::mem::replace(&mut self.backend, Box::new(recorder.clone()));
        let rows = self.rows_pause(refer.then(|| (recorder.clone(), (1.0, 1.0))));
        let boxes = walk.boxes.len();
        let texts = std::mem::take(&mut walk.text);
        let fields = std::mem::take(&mut walk.fields);
        let inline = std::mem::take(&mut walk.inline);
        let recording = self.kept.recording.replace(recorder.clone());
        self.node_at(walk, id, ts, offset, None);
        self.kept.recording = recording;
        (walk.text, walk.fields, walk.inline) = (texts, fields, inline);
        walk.boxes.truncate(boxes);
        self.rows_resume(rows);
        self.backend = real;
        recorder.tape()
    }

    pub(super) fn recording(&self) -> Option<Recorder> {
        self.kept
            .recording
            .clone()
            .filter(|_| self.backend.name() == "tape")
    }

    pub(super) fn record_layer(&mut self, draw: impl FnOnce(&mut Self)) -> Tape {
        let recorder = Recorder::portable(&*self.backend);
        let real = std::mem::replace(&mut self.backend, Box::new(recorder.clone()));
        let parent = self.kept.recording.replace(recorder.clone());
        let refer = self.rows.refer.take();
        self.rows.refer = refer.as_ref().map(|(_, scale)| (recorder.clone(), *scale));
        draw(self);
        self.rows.refer = refer;
        self.kept.recording = parent;
        self.backend = real;
        recorder.tape()
    }

    /// A sticky subtree keeps its constraint, rather than its old scroll.
    pub(super) fn record_sticky(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        draw: impl FnOnce(&mut Self, &mut Walk<'_, '_>),
    ) {
        let sticky = self
            .kept
            .recording
            .as_ref()
            .filter(|_| self.backend.name() == "tape")
            .and_then(|_| tape::Sticky::capture(walk.scene, id));
        let Some((sticky, inverse)) = sticky.zip(ts.invert()) else {
            draw(self, walk);
            return;
        };
        let drawing = self.record_layer(|painter| draw(painter, walk));
        let normalized = Recorder::default();
        normalized.nest(Rc::new(drawing), sticky.unplace(inverse));
        if let Some(parent) = &self.kept.recording {
            parent.sticky(sticky, Rc::new(normalized.tape()), ts);
        }
    }

    /// Every ancestor may leave with a route, including one that draws
    /// only a background, border or shadow. Keep its complete subtree.
    fn keeps(&self, walk: &Walk<'_, '_>, id: ViewId) -> bool {
        self.kept.recording.is_none()
            && !(walk.scene.hidden)(id)
            && [&self.stage.tape, &self.stage.rest, &self.stage.around]
                .iter()
                .any(|nodes| nodes.binary_search(&id).is_ok())
    }

    /// Record the complete branch from `id` down: each node's own paint
    /// and compositing, including portable commands for retained rows.
    /// Normalize its border box to the origin and retain the ancestor
    /// spaces that remove page, row-local and scroll placement at replay.
    /// Surviving ancestors supply their live placement and compositing;
    /// sticky commands read surviving scrollers' current offsets.
    fn keep_route(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
        pane: Option<Pane>,
    ) {
        let Some(node) = walk.scene.kernel.node(id) else {
            return;
        };
        let (key, at) = (node.key, (node.frame.x, node.frame.y));
        let size = (node.frame.width, node.frame.height);
        let local = ts.pre_translate(at.0 - offset.0, at.1 - offset.1);
        let placement = self.kept.to_page.pre_concat(local);
        let Some(unplace) = local.invert() else {
            self.kept.routes.remove(&key);
            return;
        };
        let plain = placement.kx == 0.0 && placement.ky == 0.0 && self.backend.rows_through();
        let tape = self.record_node(walk, id, ts, offset, plain);
        let normalized = Recorder::default();
        normalized.nest(Rc::new(tape), unplace);
        let path = steps(walk, id, &self.kept.spaces);
        #[cfg(test)]
        {
            self.kept.recorded += 1;
        }
        self.kept.routes.insert(
            key,
            KeptRoute {
                tape: Rc::new(normalized.tape()),
                placement,
                size,
                page: walk.scene.page,
                path,
                pane: pane.map(|pane| Pane {
                    ts: self.kept.to_page.pre_concat(pane.ts),
                    ..pane
                }),
                seen: self.rows.frame,
            },
        );
    }
}

/// `id`'s place in paint order and each ancestor's, from `id` up.
fn steps(walk: &Walk<'_, '_>, id: ViewId, spaces: &[(ViewId, Transform)]) -> Vec<Step> {
    let kernel = walk.scene.kernel;
    let mut path = Vec::new();
    let mut at = Some(id);
    while let Some(id) = at {
        let Some(node) = kernel.node(id) else { break };
        let siblings = (node.parent.and_then(|p| kernel.node(p)))
            .map_or_else(|| walk.scene.roots.to_vec(), |p| p.children());
        let at_sibling = siblings.iter().position(|s| *s == id).unwrap_or(0);
        let before = (siblings[..at_sibling].iter().rev())
            .filter_map(|s| kernel.node(*s).map(|n| n.key))
            .collect();
        path.push(Step {
            key: node.key,
            rank: walk.ranks.get(&id).copied().unwrap_or(0),
            before,
            space: spaces
                .iter()
                .rev()
                .find_map(|(ancestor, space)| (*ancestor == id).then_some(*space)),
        });
        at = node.parent;
    }
    path
}

fn ancestors(kernel: &Kernel, id: ViewId) -> Vec<ViewId> {
    let mut path = Vec::new();
    let mut at = Some(id);
    while let Some(id) = at {
        let Some(node) = kernel.node(id) else { break };
        path.push(id);
        at = node.parent;
    }
    path
}

/// The route and its ancestors up to the destination a switch to another
/// stack moves: the route itself, or in a tabbed root the branch that holds
/// it below the tabpanels' common ancestor, never the tab bar or the whole
/// page. It ends below the first ancestor whose key is in `shared`, which
/// the other side of the switch holds too.
pub(crate) fn destination_path(kernel: &Kernel, route: ViewId, shared: &[NodeKey]) -> Vec<ViewId> {
    let path = ancestors(kernel, route);
    let tabs = path
        .iter()
        .skip(1)
        .find_map(|id| kernel.root_tabs(*id).map(|tabs| (*id, tabs)));
    let Some((root, tabs)) =
        tabs.filter(|(_, tabs)| tabs.panels.iter().any(|id| path.contains(id)))
    else {
        return vec![route];
    };
    let panels: Vec<_> = tabs
        .panels
        .iter()
        .map(|id| ancestors(kernel, *id))
        .collect();
    let scope = if panels.len() == 1 {
        // Other panels may arrive only when selected, replacing this one.
        Some(root)
    } else {
        path.iter()
            .copied()
            .find(|id| panels.iter().all(|path| path.contains(id)))
    };
    let mut path: Vec<_> = (path.into_iter())
        .take_while(|id| Some(*id) != scope)
        .collect();
    let held = |id: &ViewId| kernel.node(*id).is_some_and(|n| shared.contains(&n.key));
    if let Some(at) = path.iter().skip(1).position(held) {
        path.truncate(at + 1);
    }
    path
}

/// A picture of the presenter's own, recorded once and drawn by the stage.
pub(crate) fn picture(paint: impl FnOnce(&mut dyn Backend)) -> Rc<Tape> {
    Rc::new(record(paint))
}
