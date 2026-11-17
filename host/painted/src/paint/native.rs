//! Elements a platform draws: `video` (LLP 1042), `iframe` (LLP 1020), a
//! native module's view (LLP 1024) and a GPU canvas's window (LLP 1009). The kernel owns only their box; the walk
//! names each with its box and what the platform needs to show it, and a
//! backend whose reader hosts platform views draws the view's own drawing
//! there, under the same clips and above what came before it, so it scrolls,
//! clips and stacks as the box does. Other backends draw nothing for them.
use super::*;

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
/// of it from the runner (LLP 1042.001 D3).
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
    /// The nodes asked since the last paint: their rows paint again.
    pub(crate) asked: Vec<exact_kernel::NodeKey>,
    /// The media nodes whose player has shown their source's first frame
    /// (`frameReady`): their poster is gone.
    pub(crate) framed: std::collections::BTreeSet<exact_kernel::NodeKey>,
}

impl Default for Media {
    fn default() -> Self {
        Self {
            players: cfg!(target_os = "android"),
            focused: None,
            epoch: None,
            timed: Default::default(),
            commands: Default::default(),
            asked: Vec::new(),
            framed: Default::default(),
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

    /// Name `node`'s platform element to the backend, shown in `edge`: its
    /// content box on the content edge's curve.
    pub(super) fn native(&mut self, node: &NodeRef<'_>, edge: &Shape, ts: Transform) {
        let p = node.props;
        let flag = |id| p.bool(id) == Some(true);
        let (kind, props) = match node.node_type {
            // @ref LLP 1042.001 D3, D5 — every prop the player maps; an
            // unset `paused` or `currentTime` is null, so the player knows
            // whether the author binds it.
            NodeType::Video => {
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
                        "audio": p.str(PropId::SemanticTag) == Some("audio"),
                        "label": p.str(PropId::AccessibilityLabel),
                        "timeupdate": self.media.timed.contains(&node.key),
                        "focused": self.media.focused == Some(node.key),
                        "fit": match node.style.object_fit {
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
                (NativeKind::Video, props)
            }
            NodeType::WebView => (
                NativeKind::WebView,
                serde_json::json!({
                    "src": p.str(PropId::Src),
                    "sandbox": p.str(PropId::Sandbox),
                }),
            ),
            NodeType::NativeView => (
                NativeKind::Module,
                serde_json::json!({
                    "module": p.str(PropId::NativeViewModuleName),
                    "generation": node.key.generation,
                    "slot": node.key.index,
                    "session": self.native_session,
                    "disabled": flag(PropId::Disabled),
                    "props": p.str(PropId::NativeViewProps),
                }),
            ),
            NodeType::Canvas => (NativeKind::Surface, serde_json::json!({})),
            _ => return,
        };
        if edge.rect.2 <= 0.0 || edge.rect.3 <= 0.0 {
            return;
        }
        self.backend.native(
            node.id,
            kind,
            edge,
            ts,
            &props.to_string(),
            self.direct_canvases.contains(&node.id),
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
