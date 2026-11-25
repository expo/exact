//! A node with `draggable` dragged out of the app on Android (LLP 1094.000
//! D7): a finger held on it for the long-press time asks the reader for the
//! platform's drag, `View.startDragAndDrop`, with what the node's admitted
//! `dataTransfer…` props carry. The reader builds the drag and its shadow;
//! the presenter names the node, its box and the data, each file as the
//! node names it (a shipped `assets/…` name or an `app:/` path), which the
//! reader resolves as it does a `video`'s.
use super::*;
use exact_kernel::NodeKey;

/// What the reader drags: the node's box and the finger's point in the
/// view's pixels, whether an assistive service started it, then its text,
/// url and files.
#[derive(Debug, Clone, PartialEq)]
pub struct DragOut {
    /// The node.
    pub node: ViewId,
    /// Its box (x, y, width, height) in the view's pixels.
    pub rect: [i32; 4],
    /// The finger, in the view's pixels.
    pub at: [i32; 2],
    /// An assistive service started it, with no finger on the view
    /// (`DRAG_FLAG_ACCESSIBILITY_ACTION`).
    pub service: bool,
    /// `text`, empty for none.
    pub text: String,
    /// `url`, empty for none.
    pub url: String,
    /// Each file as the node names it.
    pub files: Vec<String>,
}

impl DragOut {
    /// The words the reader reads: the node, the box, the point, 1 for a
    /// service's drag, the text, the url, then the files' count and each.
    pub(super) fn encode(&self, out: &mut Vec<i32>) {
        out.push(self.node as i32);
        out.extend(self.rect);
        out.extend(self.at);
        out.push(i32::from(self.service));
        requests::string(out, &self.text);
        requests::string(out, &self.url);
        out.push(self.files.len() as i32);
        for file in &self.files {
            requests::string(out, file);
        }
    }
}

impl<D: exact_runner::DataSource> Presenter<D> {
    /// The node a long press at (`x`, `y`) would drag: the innermost one
    /// carrying `draggable=true` from the hit up, unless a disabled node, a
    /// text field or a control that follows a finger (a slider, a switch, a
    /// date or time) comes first, which keeps its contact (LLP 1094.000 D8),
    /// as UIKit's do on iOS. A button, a checkbox or a progress bar is a
    /// press or a picture: a rested finger on it lifts the node around it.
    pub(in crate::presenter) fn drag_source_at(&mut self, x: f32, y: f32) -> Option<ViewId> {
        use exact_kernel::ControlKind as Kind;
        if !self.lifts() || self.page_inert() {
            return None;
        }
        let hit = self.hit(x, y)?;
        let kernel = self.host.kernel();
        let mut at = kernel.node(hit);
        while let Some(node) = at {
            if node.props.bool(PropId::Disabled) == Some(true) {
                return None;
            }
            if node.props.bool(PropId::Draggable) == Some(true) {
                return Some(node.id);
            }
            let follows = matches!(
                Kind::of(node.node_type, node.props),
                Some(Kind::Range | Kind::Switch | Kind::Date | Kind::Time | Kind::DateTimeLocal)
            );
            if node.node_type == NodeType::TextInput || follows {
                return None;
            }
            at = node.parent.and_then(|parent| kernel.node(parent));
        }
        None
    }

    /// The held finger moving on past the platform's touch slop after its
    /// long press opened a node's menu: the menu closes and the node's drag
    /// lifts, as a held app icon's and Chrome's held image's do (LLP
    /// 1094.000 §7 item 6). An alert its `contextmenu` opened stays, and so
    /// does the menu of a node that carries nothing now. The slop is the
    /// reader's: a move past it along either axis. Whether it lifted.
    pub(in crate::presenter) fn lift_moved(&mut self, x: f32, y: f32, now_ms: f64) -> bool {
        let Some((source, x0, y0)) = self.top.lift else {
            return false;
        };
        if f64::from((x - x0).abs().max((y - y0).abs())) <= self.slop {
            return false;
        }
        self.top.lift = None;
        if self.alert.is_some() || !self.lifts() {
            return false;
        }
        let Some(drag) = self.drag_of(source, x, y, false) else {
            return false;
        };
        match self.surface().map(|shown| shown.kind) {
            Some(top::SurfaceKind::Menu) => self.close_surface(now_ms),
            Some(_) => return false,
            None => {}
        }
        self.ask_drag(drag, now_ms);
        true
    }

    /// The node a long press at (`x`, `y`) drags now: the one
    /// [`Self::drag_source_at`] finds, when it carries something. The reader
    /// asks it before the press, to learn whether exact2 will take it.
    pub(in crate::presenter) fn lift_source_at(&mut self, x: f32, y: f32) -> Option<ViewId> {
        let source = self.drag_source_at(x, y)?;
        let kernel = self.host.kernel();
        kernel
            .node(source)
            .filter(|node| Self::drags(node))
            .map(|node| node.id)
    }

    /// Whether this host lifts drags: a shell serves the presenter's
    /// requests, and no agent drives it, whose `tree` says what a drag
    /// would carry (LLP 1094.000 D10) and which lifts none, as on the Mac.
    pub(in crate::presenter) fn lifts(&self) -> bool {
        self.serves_requests() && !self.agent && !exact_runner::agent::carried()
    }

    /// Whether a held press on `node` lifts a drag: it says `draggable` and
    /// carries something. An assistive service starts it by the node's
    /// action (LLP 1094.000 D9).
    pub(in crate::presenter) fn drags(node: &exact_kernel::NodeRef<'_>) -> bool {
        node.props.bool(PropId::Draggable) == Some(true)
            && [
                PropId::DataTransferText,
                PropId::DataTransferUrl,
                PropId::DataTransferFiles,
            ]
            .into_iter()
            .any(|prop| node.props.str(prop).is_some_and(|value| !value.is_empty()))
    }

    /// An assistive service's drag of `id`, held at its middle.
    pub(in crate::presenter) fn access_drag(&mut self, id: ViewId, now_ms: f64) -> bool {
        let Some(painted) = self.box_of(id) else {
            return false;
        };
        let (x, y) = painted.center();
        self.drag_out(id, x, y, now_ms, true)
    }

    /// Whether `inner` lies strictly inside `outer` in the authored tree.
    pub(in crate::presenter) fn inside(&self, inner: ViewId, outer: ViewId) -> bool {
        let kernel = self.host.kernel();
        let mut at = kernel.node(inner).and_then(|node| node.parent);
        while let Some(id) = at {
            if id == outer {
                return true;
            }
            at = kernel.node(id).and_then(|node| node.parent);
        }
        false
    }

    /// The reader's drag of `id`, held at (`x`, `y`), by a finger or by an
    /// assistive `service`: asked when the node carries something now.
    /// Whether it was asked.
    pub(in crate::presenter) fn drag_out(
        &mut self,
        id: ViewId,
        x: f32,
        y: f32,
        now_ms: f64,
        service: bool,
    ) -> bool {
        if !self.lifts() {
            return false;
        }
        let Some(drag) = self.drag_of(id, x, y, service) else {
            return false;
        };
        self.ask_drag(drag, now_ms);
        true
    }

    /// The reader's drag, asked: the finger is the drag's now, so nothing it
    /// is over presses or scrolls.
    fn ask_drag(&mut self, drag: DragOut, now_ms: f64) {
        if let Err(e) = self.pointer_cancel(now_ms) {
            self.host.log(e);
        }
        self.request(Request::Drag(drag));
    }

    /// The drag of `id` held at (`x`, `y`): its box as it shows, within its
    /// clip (a card a scroller cuts off is dragged as its shown part).
    fn drag_of(&mut self, id: ViewId, x: f32, y: f32, service: bool) -> Option<DragOut> {
        let rect = self.shown_box(id)?;
        let (text, url, files) = self.carried_values(id);
        if text.is_empty() && url.is_empty() && files.is_empty() {
            return None;
        }
        let scale = self.brush.scale;
        let px = |v: f32| (v * scale).round() as i32;
        Some(DragOut {
            node: id,
            rect: [px(rect.0), px(rect.1), px(rect.2), px(rect.3)],
            at: [px(x), px(y)],
            service,
            text,
            url,
            files,
        })
    }
}

impl<D: exact_runner::DataSource> Presenter<D> {
    /// What `id` carries: its `text`, its `url` and each of its files.
    fn carried_values(&self, id: ViewId) -> (String, String, Vec<String>) {
        let props = |prop| {
            self.host
                .kernel()
                .node(id)
                .and_then(|node| node.props.str(prop))
                .unwrap_or_default()
                .to_string()
        };
        let files = props(PropId::DataTransferFiles)
            .split('\n')
            .filter(|entry| !entry.is_empty())
            .map(str::to_string)
            .collect();
        (
            props(PropId::DataTransferText),
            props(PropId::DataTransferUrl),
            files,
        )
    }

    /// `id`'s box as it shows, in points: within its clip, so a card a
    /// scroller cuts off is dragged as its shown part.
    fn shown_box(&mut self, id: ViewId) -> Option<(f32, f32, f32, f32)> {
        self.box_of(id).as_ref().map(clipped)
    }
}

/// A desktop host that lifts a mouse's drags (LLP 1094.000 D13): which of
/// D1's values it offers, and what it does with a lift, answering whether
/// it took it.
pub struct Lifter {
    /// The values the host offers; a node carrying none of them is not a
    /// source there.
    pub offers: Offers,
    /// Take a lift; false leaves the contact as if nothing had armed.
    pub lift: Box<dyn FnMut(Lift) -> bool>,
}

/// Which of D1's values a host's drag offers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Offers {
    /// `text`.
    pub text: bool,
    /// `url`.
    pub url: bool,
    /// `files`.
    pub files: bool,
}

impl Offers {
    /// The node props that hold what a host offers.
    fn props(self) -> impl Iterator<Item = PropId> {
        [
            (self.text, PropId::DataTransferText),
            (self.url, PropId::DataTransferUrl),
            (self.files, PropId::DataTransferFiles),
        ]
        .into_iter()
        .filter_map(|(offered, prop)| offered.then_some(prop))
    }
}

/// What a desktop host lifts by the mouse (LLP 1094.000 D13): the values it
/// offers from what the node carries, its picture as the frame showed it,
/// and where the press held it in that picture, in the window's logical
/// coordinates.
pub struct Lift {
    /// `text`, empty for none or when not offered.
    pub text: String,
    /// `url`, empty for none or when not offered.
    pub url: String,
    /// Each file as the node names it, none when not offered.
    pub files: Vec<String>,
    /// The node's box, within its clip, cut from the frame last shown.
    pub picture: Option<LiftPicture>,
    /// The press, relative to the picture's top left.
    pub hotspot: (f32, f32),
}

/// A lifted node's pixels: premultiplied RGBA rows, `scale` pixels a point.
pub struct LiftPicture {
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// The frame's pixels a point.
    pub scale: f32,
    /// Its rows, top first.
    pub rgba: Vec<u8>,
}

impl<D: exact_runner::DataSource> Presenter<D> {
    /// Hand the drags a mouse lifts to `lifter`, the desktop host's own
    /// (LLP 1094.000 D13); none lifts nothing by the mouse.
    pub fn set_lifter(&mut self, lifter: Option<Lifter>) {
        self.lifter = lifter;
        self.shown = None;
    }

    /// The node a mouse press on `hit` arms, and a digest of what it offers
    /// (LLP 1094.000 D13): the innermost node that drags a value the host
    /// offers, unless a
    /// disabled or inert node, a text input, a control, a link or a node
    /// that hears `press` comes first, each of which takes a mouse press
    /// itself (LLP 1057.001 rule 3), as AppKit's views do.
    pub(in crate::presenter) fn mouse_lift(&self, hit: NodeKey) -> Option<(NodeKey, u64)> {
        let offers = self.lifter.as_ref()?.offers;
        if self.agent || exact_runner::agent::carried() || self.page_inert() {
            return None;
        }
        let kernel = self.host.kernel();
        let mut at = kernel.node_by_key(hit);
        while let Some(node) = at {
            if node.props.bool(PropId::Disabled) == Some(true) || !self.input_live(node.key) {
                return None;
            }
            let offered = |prop| {
                node.props
                    .str(prop)
                    .is_some_and(|value: &str| !value.is_empty())
            };
            // A source hidden by `visibility` lifts nothing, as the Mac's checks
            if node.props.bool(PropId::Draggable) == Some(true) && offers.props().any(offered) {
                return crate::paint::paints(kernel, node.id, None)
                    .then(|| (node.key, Self::carried(&node, offers)));
            }
            let presses = node.props.link().is_some()
                || exact_kernel::ControlKind::of(node.node_type, node.props).is_some()
                || self
                    .host
                    .runner()
                    .handlers_of(node.id)
                    .contains(&EventKind::Press);
            if node.node_type == NodeType::TextInput || presses {
                return None;
            }
            at = node.parent.and_then(|parent| kernel.node(parent));
        }
        None
    }

    /// A digest of what `node` carries that the host offers, to tell at the
    /// lift whether it changed since the press.
    fn carried(node: &exact_kernel::NodeRef<'_>, offers: Offers) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut digest = std::collections::hash_map::DefaultHasher::new();
        for prop in offers.props() {
            node.props.str(prop).hash(&mut digest);
        }
        digest.finish()
    }

    /// Lift `source`, armed at the press on `hit` carrying `carried`, held
    /// at `origin`, if the press still arms it as the Mac checks: the hit
    /// still mounted, the search from it finding the same source carrying
    /// the same, painted. Whether the host took it.
    pub(in crate::presenter) fn mouse_lifts(
        &mut self,
        hit: Option<NodeKey>,
        source: NodeKey,
        carried: u64,
        origin: (f32, f32),
        now_ms: f64,
    ) -> bool {
        let Some(lift) = self.mouse_lift_of(hit, source, carried, origin) else {
            return false;
        };
        let Some(lifter) = self.lifter.as_mut() else {
            return false;
        };
        if !(lifter.lift)(lift) {
            return false;
        }
        if let Err(e) = self.pointer_cancel(now_ms) {
            self.host.log(e);
        }
        true
    }

    pub(in crate::presenter) fn mouse_lift_of(
        &mut self,
        hit: Option<NodeKey>,
        source: NodeKey,
        carried: u64,
        origin: (f32, f32),
    ) -> Option<Lift> {
        if self.mouse_lift(hit?)? != (source, carried) {
            return None;
        }
        let id = self.host.kernel().node_by_key(source)?.id;
        // The values are the source's now; the picture is the frame's, when
        // it showed the source
        let (text, url, files) = self.carried_values(id);
        let (picture, hotspot) = self.icon_of(id, origin);
        let offers = self.lifter.as_ref()?.offers;
        Some(Lift {
            text: if offers.text { text } else { String::new() },
            url: if offers.url { url } else { String::new() },
            files: if offers.files { files } else { Vec::new() },
            picture,
            hotspot,
        })
    }

    /// `id`'s picture held at `origin`, and the press relative to it: its
    /// box, within its clip and the frame, cut from the pixels of the frame
    /// last shown, whose boxes and scale they are. None when that frame did
    /// not show the source: no box, a `visibility` or a transparent subtree
    /// that hid it, or no whole pixel of it.
    pub(in crate::presenter) fn icon_of(
        &mut self,
        id: ViewId,
        origin: (f32, f32),
    ) -> (Option<LiftPicture>, (f32, f32)) {
        // The box by the node's identity among the frame's own, whatever the
        // tree has done since
        let painted = self.boxes.iter().find(|b| b.id == id).filter(|b| b.visible);
        let rect = painted.map(clipped);
        let cut = rect
            .zip(self.shown.as_ref())
            .and_then(|(rect, (pixels, scale))| crop(pixels, *scale, rect));
        match cut {
            Some((picture, corner)) => (Some(picture), (origin.0 - corner.0, origin.1 - corner.1)),
            None => (None, (0., 0.)),
        }
    }

    /// Keep the frame `pixels` shown at `scale`, for a lift's picture, while
    /// the plan has a node that drags and a host lifts by the mouse.
    pub(in crate::presenter) fn keep_shown(&mut self, pixels: Option<&Arc<Pixmap>>, scale: f32) {
        let drags = self.lifter.is_some()
            && *self.plan_drags.get_or_insert_with(|| {
                exact_runner::uses(self.host.runner().plan()).has(exact_runner::Capability::DragOut)
            });
        self.shown = pixels
            .filter(|_| drags)
            .map(|pixels| (pixels.clone(), scale));
    }
}

/// `painted`'s box within its clip, in points.
fn clipped(painted: &crate::paint::PaintedBox) -> (f32, f32, f32, f32) {
    let rect = painted.rect;
    let Some(clip) = painted.clip else {
        return rect;
    };
    let (left, top) = (rect.0.max(clip.0), rect.1.max(clip.1));
    let right = (rect.0 + rect.2).min(clip.0 + clip.2);
    let bottom = (rect.1 + rect.3).min(clip.1 + clip.3);
    (left, top, (right - left).max(0.), (bottom - top).max(0.))
}

/// The pixels of `pixmap`, drawn `scale` pixels a point, within `rect` (in
/// points) and the frame, its edges rounded to whole pixels, and that cut's
/// corner in points; none when no whole pixel of it shows.
fn crop(
    pixmap: &Pixmap,
    scale: f32,
    rect: (f32, f32, f32, f32),
) -> Option<(LiftPicture, (f32, f32))> {
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let edge = |v: f32, most: usize| ((v * scale).round().max(0.) as usize).min(most);
    let (left, right) = (edge(rect.0, width), edge(rect.0 + rect.2, width));
    let (top, bottom) = (edge(rect.1, height), edge(rect.1 + rect.3, height));
    if right <= left || bottom <= top {
        return None;
    }
    let (stride, data) = (width * 4, pixmap.data());
    let mut rgba = Vec::with_capacity((right - left) * (bottom - top) * 4);
    for row in top..bottom {
        rgba.extend_from_slice(&data[row * stride + left * 4..row * stride + right * 4]);
    }
    let picture = LiftPicture {
        width: (right - left) as u32,
        height: (bottom - top) as u32,
        scale,
        rgba,
    };
    Some((picture, (left as f32 / scale, top as f32 / scale)))
}
