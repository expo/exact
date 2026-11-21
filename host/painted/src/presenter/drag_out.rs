//! A node with `draggable` dragged out of the app on Android (LLP 1094.000
//! D7): a finger held on it for the long-press time asks the reader for the
//! platform's drag, `View.startDragAndDrop`, with what the node's admitted
//! `dataTransfer…` props carry. The reader builds the drag and its shadow;
//! the presenter names the node, its box and the data, each file as the
//! node names it (a shipped `assets/…` name or an `app:/` path), which the
//! reader resolves as it does a `video`'s.
use super::*;

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
        let painted = self.box_of(id)?;
        let mut rect = painted.rect;
        if let Some(clip) = painted.clip {
            let (left, top) = (rect.0.max(clip.0), rect.1.max(clip.1));
            let right = (rect.0 + rect.2).min(clip.0 + clip.2);
            let bottom = (rect.1 + rect.3).min(clip.1 + clip.3);
            rect = (left, top, (right - left).max(0.), (bottom - top).max(0.));
        }
        let props = |prop| {
            self.host
                .kernel()
                .node(id)
                .and_then(|node| node.props.str(prop))
                .unwrap_or_default()
                .to_string()
        };
        let (text, url, listed) = (
            props(PropId::DataTransferText),
            props(PropId::DataTransferUrl),
            props(PropId::DataTransferFiles),
        );
        let files: Vec<String> = listed
            .split('\n')
            .filter(|entry| !entry.is_empty())
            .map(str::to_string)
            .collect();
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
