//! Elements a platform draws: `video` (LLP 1042), `iframe` (LLP 1020), a
//! native module's view (LLP 1024) and a GPU canvas's window (LLP 1009). The kernel owns only their box; the walk
//! names each with its box and what the platform needs to show it, and a
//! backend whose reader hosts platform views draws the view's own drawing
//! there, under the same clips and above what came before it, so it scrolls,
//! clips and stacks as the box does. Other backends draw nothing for them.
use super::*;

pub(super) fn is_audio(node: &NodeRef<'_>) -> bool {
    node.node_type == NodeType::Video && node.props.str(PropId::SemanticTag) == Some("audio")
}

pub(super) struct PictureBackend(Box<dyn Backend>, Option<Vec<ViewId>>);

pub(crate) fn object_fit_of(node: &NodeRef<'_>) -> ObjectFit {
    if node.node_type == NodeType::Video {
        exact_kernel::object::video_fit(
            node.style.object_fit,
            node.style.written.has(StyleId::ObjectFit),
        )
    } else {
        node.style.object_fit
    }
}

pub(super) fn new_session() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Which platform element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeKind {
    /// A `video`: a platform player.
    Video = 0,
    /// An `iframe`: a platform web view.
    WebView = 1,
    /// A native module's view, by module name.
    Module = 2,
    /// A GPU canvas's window (LLP 1009): the reader makes one and gives it
    /// to the host, whose module presents into it.
    Surface = 3,
    /// A text field the shell hosts (LLP 1115.001 D8): the platform's own
    /// field, drawn here in the content box of the container this host
    /// paints. What it is and what it holds are not in the stream: the
    /// shell is told of its fields beside it (`presenter/hosted.rs`).
    Field = 4,
    /// A view an app's Kotlin hatch hosts at a node's border box (LLP
    /// 1115.001.000 D4), over the node's own paint: what it is is the
    /// reader's; the props carry how many times the node has been given one.
    Hatch = 5,
}

/// What a media node's props carry beside its own: what the presenter knows
/// of it from the runner (LLP 1042.001 D3); and an iframe's commands.
pub(crate) struct Media {
    /// Each media node has a platform player, which draws its controller
    /// and takes the window's focus: only Android's reader has one.
    pub(crate) players: bool,
    /// The `controls` video exact2's focus is on: its player takes the
    /// window's focus (D4).
    pub(crate) focused: Option<exact_kernel::NodeKey>,
    /// The tree these were found in.
    pub(crate) epoch: Option<u64>,
    /// The media nodes that handle `timeupdate`: only their players run its
    /// 4 Hz clock.
    pub(crate) timed: std::collections::BTreeSet<exact_kernel::NodeKey>,
    /// `fastSeek`, `load` and `requestFullscreen` asked of each node, numbered as the Apple arm
    /// numbers them, so a player made again from its record repeats none.
    pub(crate) commands: std::collections::BTreeMap<exact_kernel::NodeKey, Commands>,
    /// A partitioned iframe's commands (LLP 1020.002 D3), for its reader to run.
    pub(crate) frames: std::collections::BTreeMap<exact_kernel::NodeKey, FrameCommands>,
    /// The nodes asked since the last paint: their rows paint again.
    pub(crate) asked: Vec<exact_kernel::NodeKey>,
    /// The media nodes whose player has shown their source's first frame
    /// (`frameReady`): their poster is gone.
    pub(crate) framed: std::collections::BTreeSet<exact_kernel::NodeKey>,
    /// The `portal` movers alive and shown nowhere (LLP 1013.002 D5).
    pub(crate) held: Vec<ViewId>,
}

impl Default for Media {
    fn default() -> Self {
        Self {
            players: cfg!(target_os = "android"),
            focused: None,
            epoch: None,
            timed: Default::default(),
            commands: Default::default(),
            frames: Default::default(),
            asked: Vec::new(),
            framed: Default::default(),
            held: Vec::new(),
        }
    }
}

/// A media node's commands so far.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Commands {
    /// How many `fastSeek`s, and the last one's time.
    pub(crate) seeks: u32,
    pub(crate) seconds: f64,
    /// How many `load`s.
    pub(crate) loads: u32,
    /// How many `requestFullscreen`s.
    pub(crate) fullscreens: u32,
}

/// An iframe's latest commands, `[seq, name, args…]` with `seq` counting
/// from 1, for the partition they were given in. The reader runs those
/// above the last it ran for the frame's allocation, so one it makes again
/// repeats none; a burst of more than [`FrameCommands::KEPT`] between two
/// paints keeps the newest.
#[derive(Clone, Debug, Default)]
pub(crate) struct FrameCommands {
    pub(crate) partition: String,
    pub(crate) next: u64,
    pub(crate) list: std::collections::VecDeque<serde_json::Value>,
}

impl FrameCommands {
    pub(crate) const KEPT: usize = 16;

    /// The frame shows `partition` now: a changed partition is a new web
    /// view, which runs none of the old commands.
    pub(crate) fn held(&mut self, partition: &str) {
        if self.partition != partition {
            self.partition = partition.to_owned();
            self.list.clear();
        }
    }

    /// `name(args)` given to the frame while its partition is `partition`.
    pub(crate) fn push(&mut self, partition: &str, name: &str, args: &[serde_json::Value]) {
        self.held(partition);
        self.next += 1;
        let mut command = vec![self.next.into(), name.into()];
        command.extend_from_slice(args);
        self.list.push_back(command.into());
        if self.list.len() > Self::KEPT {
            self.list.pop_front();
        }
    }
}

/// An auto-width container can be shrink-to-fit and owe its width to this very module.
/// Measure up to its containing width, then let CSS assign the module its final box.
fn module_width_offer(kernel: &Kernel, node: &NodeRef<'_>) -> f32 {
    let mut auto = vec![*node];
    let mut parent = node.parent.and_then(|id| kernel.node(id));
    let mut width = kernel.env().viewport_width;
    while let Some(n) = parent {
        if n.is_root
            || n.style.width.resolve(&kernel.arena().env_of(n.key.index)) != Dimension::Auto
        {
            width = exact_kernel::svg::scene::content_box(kernel, &n).2;
            break;
        }
        auto.push(n);
        parent = n.parent.and_then(|id| kernel.node(id));
    }
    for n in auto.into_iter().rev() {
        let env = kernel.arena().env_of(n.key.index);
        let (pl, _, pr, _) = kernel.resolved_padding(n.key).unwrap_or_default();
        let [_, br, _, bl] = n.style.border_widths_in(&env);
        let margins =
            n.style.margin_left.at_basis(&env, width) + n.style.margin_right.at_basis(&env, width);
        let inset = pl + pr + bl + br;
        let constraint = |size: &Dimension| {
            matches!(
                size.resolve(&env),
                Dimension::Points(_)
                    | Dimension::Percent(_)
                    | Dimension::Calc(..)
                    | Dimension::Compare(_)
            )
            .then(|| {
                let px = size.at_basis(&env, width);
                if n.style.box_sizing == exact_kernel::BoxSizing::BorderBox {
                    px - inset
                } else {
                    px
                }
            })
        };
        let max = constraint(&n.style.max_width).unwrap_or(f32::INFINITY);
        let min = constraint(&n.style.min_width).unwrap_or(0.);
        width = (width - margins - inset).min(max).max(min).max(0.);
    }
    width
}

impl Painter {
    /// @ref LLP 1115.001.000 D4 — a view a Kotlin hatch hosts at `node`:
    /// over its paint, in its border box `rect`, where the node is. A node
    /// that is a platform element already is named by its own kind.
    pub(super) fn hosted_view(&mut self, node: &NodeRef<'_>, rect: Rect4, ts: Transform) {
        let element = matches!(
            node.node_type,
            NodeType::Video
                | NodeType::WebView
                | NodeType::NativeView
                | NodeType::TextInput
                | NodeType::Canvas
        );
        if let Some(generation) = crate::hatches::reader::hosted(node.id).filter(|_| !element) {
            // Never a kept recording: the view comes and goes between paints.
            self.row_refuse();
            let props = format!("{{\"gen\":{generation}}}");
            self.backend.native(
                node.id,
                NativeKind::Hatch,
                &Shape::rect(rect),
                ts,
                &props,
                false,
            );
        }
    }

    // @ref LLP 1042.001 D3, D5 — null distinguishes an unbound prop from an assignment.
    pub(crate) fn media_props(&self, node: &NodeRef<'_>) -> serde_json::Value {
        let p = node.props;
        let flag = |id| p.bool(id) == Some(true);
        let number = |id| {
            p.get(id)
                .and_then(|v| v.as_float().or(v.as_int().map(|i| i as f64)))
        };
        let mut props = serde_json::json!({
            "src": p.str(PropId::Src),
            "autoplay": flag(PropId::Autoplay),
            "loop": flag(PropId::Loop),
            "muted": flag(PropId::Muted),
            "paused": p.bool(PropId::Paused),
            "controls": flag(PropId::Controls),
            "poster": p.str(PropId::Poster).is_some_and(|s| !s.is_empty()),
            "preload": p.str(PropId::Preload),
            "controlslist": p.str(PropId::Controlslist),
            "volume": number(PropId::Volume),
            "playbackRate": number(PropId::PlaybackRate),
            "currentTime": number(PropId::CurrentTime),
            "preservesPitch": p.bool(PropId::PreservesPitch),
            "preferredMaximumResolution": p.str(PropId::PreferredMaximumResolution),
            "preferredPeakBitRate": number(PropId::PreferredPeakBitRate),
            "preventsDisplaySleepDuringVideoPlayback": p.bool(PropId::PreventsDisplaySleepDuringVideoPlayback),
            "audio": is_audio(node),
            "label": p.str(PropId::AccessibilityLabel),
            "timeupdate": self.media.timed.contains(&node.key),
            "focused": self.media.focused == Some(node.key),
            "fit": match object_fit_of(node) {
                ObjectFit::Contain => "contain",
                ObjectFit::Cover => "cover",
                ObjectFit::None => "none",
                ObjectFit::ScaleDown => "scale-down",
                ObjectFit::Fill => "fill",
            },
            "generation": node.key.generation,
            "slot": node.key.index,
            "session": self.native_session,
        });
        if let Some(c) = self.media.commands.get(&node.key) {
            if c.seeks > 0 {
                props["seek"] = format!("{} {}", c.seeks, c.seconds).into();
            }
            if c.loads > 0 {
                props["load"] = c.loads.into();
            }
            if c.fullscreens > 0 {
                props["fullscreen"] = c.fullscreens.into();
            }
        }
        props
    }

    /// A subtree that paints nothing (`display: none`, a closed `dialog`):
    /// its audio plays on, boxless, and its module views are named held,
    /// made and given their props but shown nowhere, as iOS keeps a hidden
    /// one and the web its element.
    pub(super) fn unpainted(&mut self, walk: &Walk<'_, '_>, node: &NodeRef<'_>) {
        let kernel = walk.scene.kernel;
        if !(kernel.has_type(NodeType::Video) || kernel.has_type(NodeType::NativeView))
            || (walk.scene.hidden)(node.id)
            || walk.skip == Some(node.key)
            || walk.draws_alert_elsewhere(node)
        {
            return;
        }
        if is_audio(node) {
            self.record_boxless_audio(node);
        } else if node.node_type == NodeType::NativeView {
            self.held_module(kernel, node);
        }
        for child in node.child_nodes() {
            self.unpainted(walk, &child);
        }
    }

    /// @ref LLP 1013.002 D5 — the media of a hidden mover live on: each
    /// video is named held, so the reader keeps its player out of view, where
    /// the off-screen rule pauses a muted autoplay until it shows again, and
    /// each audio as a boxless one. Named first in the stream, so no layer the
    /// reader takes in before them lets the element go.
    pub(super) fn held_media(&mut self, kernel: &Kernel) {
        let media = |t| t == NodeType::Video || t == NodeType::NativeView;
        if self.media.held.is_empty()
            || !(kernel.has_type(NodeType::Video) || kernel.has_type(NodeType::NativeView))
        {
            return;
        }
        for id in kernel.preorder_where(&self.media.held, |t, _| media(t)) {
            let Some(node) = kernel.node(id) else {
                continue;
            };
            if node.node_type == NodeType::NativeView {
                self.held_module(kernel, &node);
            } else if is_audio(&node) {
                self.record_boxless_audio(&node);
            } else {
                let props = self.media_props(&node);
                self.backend
                    .held(node.id, NativeKind::Video, &props.to_string());
            }
        }
    }

    pub(super) fn record_boxless_audio(&mut self, node: &NodeRef<'_>) {
        if let Some(audio) = &mut self.boxless_audio {
            audio.push(node.id);
            return;
        }
        let props = self.media_props(node);
        self.backend.boxless_audio(node.id, &props.to_string());
    }

    pub(super) fn capture_boxless_audio(&mut self, kernel: &Kernel) -> Option<Vec<ViewId>> {
        let capture = kernel.has_type(NodeType::Video).then(Vec::new);
        std::mem::replace(&mut self.boxless_audio, capture)
    }

    pub(super) fn emit_boxless_audio(&mut self, kernel: &Kernel, audio: Option<Vec<ViewId>>) {
        for id in audio.into_iter().flatten() {
            if let Some(node) = kernel.node(id) {
                self.record_boxless_audio(&node);
            }
        }
    }

    // An island's backend draws its pixels. The enclosing stream still owns its audio players.
    pub(super) fn replace_picture_backend(
        &mut self,
        kernel: &Kernel,
        backend: Box<dyn Backend>,
    ) -> PictureBackend {
        PictureBackend(
            std::mem::replace(&mut self.backend, backend),
            self.capture_boxless_audio(kernel),
        )
    }

    pub(super) fn restore_picture_backend(
        &mut self,
        kernel: &Kernel,
        PictureBackend(backend, parent_audio): PictureBackend,
    ) -> Box<dyn Backend> {
        let picture = std::mem::replace(&mut self.backend, backend);
        let audio = std::mem::replace(&mut self.boxless_audio, parent_audio);
        self.emit_boxless_audio(kernel, audio);
        picture
    }

    /// A module view alive and shown nowhere: in a subtree that paints
    /// nothing or under `visibility: hidden`, as iOS keeps a hidden view.
    pub(super) fn held_module(&mut self, kernel: &Kernel, node: &NodeRef<'_>) {
        let props = self.module_props(kernel, node);
        self.backend
            .held(node.id, NativeKind::Module, &props.to_string());
    }

    fn module_props(&self, kernel: &Kernel, node: &NodeRef<'_>) -> serde_json::Value {
        let p = node.props;
        let mut props = serde_json::json!({
            "module": p.str(PropId::NativeViewModuleName),
            "generation": node.key.generation,
            "slot": node.key.index,
            "session": self.native_session,
            "disabled": p.bool(PropId::Disabled) == Some(true),
            "props": p.str(PropId::NativeViewProps),
        });
        if cfg!(any(target_os = "android", test)) {
            props["hasPreferred"] = kernel.arena().intrinsic(node.key.index).is_some().into();
        }
        props
    }

    /// Name `node`'s platform element to the backend, shown in `edge`: its
    /// content box on the content edge's curve.
    pub(super) fn native(
        &mut self,
        walk: &Walk<'_, '_>,
        node: &NodeRef<'_>,
        edge: &Shape,
        ts: Transform,
    ) {
        let p = node.props;
        // A module view is made whatever its size, as iOS makes its view at a
        // zero frame: an empty box can hold a player or a scanner that draws
        // elsewhere. Every other element waits for a box.
        let empty = edge.rect.2 <= 0.0 || edge.rect.3 <= 0.0;
        if empty && node.node_type != NodeType::NativeView {
            return;
        }
        let (kind, props) = match node.node_type {
            NodeType::Video => {
                let mut props = self.media_props(node);
                let natural = walk.scene.kernel.arena().intrinsic(node.key.index);
                if let Some(natural) = natural {
                    if let Some(rect) = exact_kernel::object::placed(
                        natural,
                        object_fit_of(node),
                        &node.style.object_position,
                        (0., 0., edge.rect.2, edge.rect.3),
                        &walk.scene.kernel.env_of(node.id),
                    ) {
                        props["objectRect"] = serde_json::json!([
                            rect.0 / edge.rect.2,
                            rect.1 / edge.rect.3,
                            rect.2 / edge.rect.2,
                            rect.3 / edge.rect.3
                        ]);
                    }
                }
                (NativeKind::Video, props)
            }
            NodeType::WebView => {
                let mut props = serde_json::json!({
                    "src": p.str(PropId::Src),
                    // What `@exact/browser-automation` finds the frame by (LLP 1020.002 D9).
                    "id": p.str(PropId::Id),
                    "sandbox": p.str(PropId::Sandbox),
                    "partition": p.str(PropId::Partition),
                    "generation": node.key.generation,
                    "slot": node.key.index,
                    "session": self.native_session,
                });
                let partition = p.str(PropId::Partition).unwrap_or_default();
                if let Some(c) = self.media.frames.get_mut(&node.key) {
                    c.held(partition);
                    props["commands"] = c.list.iter().cloned().collect();
                }
                (NativeKind::WebView, props)
            }
            NodeType::NativeView => {
                let kernel = walk.scene.kernel;
                let mut props = self.module_props(kernel, node);
                if cfg!(any(target_os = "android", test)) {
                    let env = kernel.arena().env_of(node.key.index);
                    if node.style.width.resolve(&env) == exact_kernel::Dimension::Auto {
                        props["autoWidth"] = module_width_offer(kernel, node).into();
                    }
                    props["autoHeight"] =
                        (node.style.height.resolve(&env) == exact_kernel::Dimension::Auto).into();
                }
                (NativeKind::Module, props)
            }
            // Only surfaces opened by a GPU module need a window or buffers from the Android reader.
            #[cfg(target_os = "android")]
            NodeType::Canvas if !self.gpu_canvases.contains_key(&node.id) => return,
            NodeType::Canvas => (NativeKind::Surface, serde_json::json!({})),
            _ => return,
        };
        self.backend.native(
            node.id,
            kind,
            edge,
            ts,
            &props.to_string(),
            self.gpu_canvases.get(&node.id) == Some(&true),
        );
    }
}

impl PaintedBox {
    /// The content bounds through the paint walk's placement, as Android's Player maps op 28.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn native_content(&self, kernel: &Kernel, node: &NodeRef<'_>) -> Option<Rect4> {
        let (left, top, width, height) = exact_kernel::svg::scene::content_box(kernel, node);
        let local = self.scroll_rect();
        space::mapped_bounds(
            &self.scroll_mapping()?,
            (local.0 + left, local.1 + top, width, height),
        )
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;

impl super::Walk<'_, '_> {
    pub(super) fn draws_alert_elsewhere(&self, node: &NodeRef<'_>) -> bool {
        self.alert == Some(node.key) && self.dialog != Some(node.id)
    }
}
