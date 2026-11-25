//! Drags from outside the app over the displayed tree: files, plain text and
//! a link (LLP 1069.010 D1, LLP 1094.001 D1).
use super::*;
use exact_kernel::NodeKey;
use exact_plan::EventKind;
use exact_runner::{DropEvent, KeyModifiers};
use std::path::PathBuf;

/// What a drag drops: files by path, minted as `doc:` handles when they are
/// delivered, its plain text and its first link that is not a file, each
/// empty when it carries none.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Dropped {
    /// The dropped files, in the order the platform gave them.
    pub paths: Vec<PathBuf>,
    /// The drag's plain text.
    pub text: String,
    /// The drag's first link that is not a file.
    pub url: String,
}

#[derive(Default)]
pub(super) struct DragIn {
    /// The nodes under the drag that hear `dragenter` or `dragleave`,
    /// innermost first.
    chain: Vec<Member>,
    /// Whether a drag is over the window at `point`, even where no node
    /// hears drags.
    over: bool,
    point: (f32, f32),
    held: KeyModifiers,
    /// The drops released whose items have yet to arrive (`drop_released`).
    pending: Vec<Pending>,
    /// The enters and leaves decided and not yet sent, in order. Each is
    /// recorded before any handler runs, and sent only while its node is
    /// still the one it was decided for (`drag_renewed`) and, for an enter,
    /// still hears drags.
    outbox: std::collections::VecDeque<Sent>,
    /// The last pending drop's number, kept across a new plan so that no
    /// copy that arrives late lands on a drop of the new one.
    last: u32,
}

/// A node under the drag, and whether it heard the drag's `dragenter`: one
/// a pending drop is over has not, until that drop lands or this drag drops
/// (LLP 1094.001 D2).
#[derive(Clone, Copy, PartialEq)]
struct Member {
    key: NodeKey,
    entered: bool,
}

/// An enter or a leave to send, with its record's point and modifiers, for
/// the drag under way (`drop` none) or for pending drop `drop`.
struct Sent {
    key: NodeKey,
    kind: EventKind,
    point: (f32, f32),
    held: KeyModifiers,
    drop: Option<u32>,
}

impl DragIn {
    /// A new plan's tracker: its views are new, so no drop or drag is over
    /// them yet.
    pub(super) fn reset(&mut self) {
        *self = DragIn {
            last: self.last,
            ..Default::default()
        };
    }

    fn send(
        &mut self,
        key: NodeKey,
        kind: EventKind,
        at: ((f32, f32), KeyModifiers),
        drop: Option<u32>,
    ) {
        let (point, held) = at;
        self.outbox.push_back(Sent {
            key,
            kind,
            point,
            held,
            drop,
        });
    }

    /// `key` heard no enter of the drag under way, or of pending drop
    /// `drop`, after all, so it hears no leave of that one either; another's
    /// is still owed.
    fn unentered(&mut self, key: NodeKey, drop: Option<u32>) {
        let chain = match drop {
            None => Some(&mut self.chain),
            Some(id) => self
                .pending
                .iter_mut()
                .find(|p| p.id == id)
                .map(|p| &mut p.chain),
        };
        for m in chain.into_iter().flatten().filter(|m| m.key == key) {
            m.entered = false;
        }
        self.outbox
            .retain(|s| !(s.key == key && s.drop == drop && s.kind == EventKind::Dragleave));
    }
}

/// A drop released before its items arrived: its target (none once the
/// target is a new mount, LLP 1078), its record as at the release, and the
/// chain it was over, which hears a leave if the drop is not delivered.
struct Pending {
    id: u32,
    target: Option<NodeKey>,
    offsets: (f64, f64),
    point: (f32, f32),
    held: KeyModifiers,
    chain: Vec<Member>,
}

impl<D: DataSource> Presenter<D> {
    /// The drag is at `point`: the nodes under it hear their enters and
    /// leaves, then the innermost node that takes a drop there, if any, is
    /// what a host shows its copy feedback for.
    pub fn drag_over(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<ViewId> {
        self.drag_at(point, held);
        self.drag_flush();
        let hit = self.drag_hit(point);
        self.drop_target(hit)
    }

    /// Cancel the drag, including when it leaves the window: the nodes under
    /// it hear their leave, and the enters it waited for are not sent.
    pub fn drag_left(&mut self) {
        self.drag_in.over = false;
        self.drag_crossings(Vec::new());
        self.drag_flush();
    }

    /// The drag dropped at `point` where no node takes it, or a field took
    /// it: the nodes under it there hear the enters it waited for, then
    /// every one hears its leave (LLP 1094.001 D2).
    pub fn drag_dropped(&mut self, point: (f32, f32), held: KeyModifiers) {
        self.drag_at(point, held);
        self.drag_ended();
        self.drag_flush();
    }

    /// Deliver what a drag drops once, its files in platform order, to the
    /// innermost node that takes a drop, a field's included: Exact's fields
    /// take no drag on this host (LLP 1094.001 D2). None, and a leave, when
    /// nothing takes it or nothing it carried arrived.
    pub fn dropped(
        &mut self,
        point: (f32, f32),
        held: KeyModifiers,
        dropped: &Dropped,
    ) -> Option<ViewId> {
        let id = self.drop_released(point, held)?;
        self.drop_landed(id, dropped)
    }

    /// Release the drag at `point` before its items arrive, as Android's
    /// copies do (LLP 1094.001 D2): the innermost node that takes a drop
    /// there, its record and the chain under the drag become the drop's,
    /// which `drop_landed` delivers, and the enters this drag waited for are
    /// heard. All of it is decided before any handler runs. The drop's
    /// number, or None, and the drag ends, when nothing takes it.
    pub fn drop_released(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<u32> {
        let hit = self.drag_at(point, held);
        let found = self.drop_target(hit).and_then(|t| {
            let key = self.host.kernel().node(t)?.key;
            let record = self.pointer_record(t, point.0, point.1, -1, 0)?;
            Some((key, (record.offset_x, record.offset_y)))
        });
        let Some((target, offsets)) = found else {
            self.drag_ended();
            self.drag_flush();
            return None;
        };
        self.drag_in.over = false;
        let mut chain = std::mem::take(&mut self.drag_in.chain);
        self.drag_in.last += 1;
        let id = self.drag_in.last;
        // The release's own enters are this drop's now, as its chain is
        for sent in self.drag_in.outbox.iter_mut() {
            if sent.drop.is_none() && chain.iter().any(|m| m.key == sent.key) {
                sent.drop = Some(id);
            }
        }
        for m in chain.iter_mut().filter(|m| !m.entered) {
            m.entered = true;
            self.drag_in
                .send(m.key, EventKind::Dragenter, (point, held), Some(id));
        }
        let target = Some(target);
        self.drag_in.pending.push(Pending {
            id,
            target,
            offsets,
            point,
            held,
            chain,
        });
        self.drag_flush();
        Some(id)
    }

    /// The items of pending drop `id` arrived: its files, minted as `doc:`
    /// handles, its text and its link go to its target if that is still the
    /// same node and still takes a drop, which ends the drag with no leave;
    /// otherwise, or when nothing arrived, the chain it was over hears a
    /// leave. A drag still under way then hears the enters it waited for at
    /// the nodes under it now. The target, when delivered.
    pub fn drop_landed(&mut self, id: u32, dropped: &Dropped) -> Option<ViewId> {
        let at = self.drag_in.pending.iter().position(|p| p.id == id)?;
        let pending = self.drag_in.pending.remove(at);
        let target = pending
            .target
            .and_then(|key| self.host.kernel().node_by_key(key).map(|n| n.id))
            .filter(|&t| {
                self.pointer_node(t, &[EventKind::Drop]) == Some(t)
                    && !self.brush.region_blocks_action(t)
            });
        let files: Vec<_> = match target {
            Some(_) => dropped
                .paths
                .iter()
                .filter(|path| path.is_absolute() && path.exists())
                .filter_map(|path| exact_data::documents::mint(path, 0))
                .collect(),
            None => Vec::new(),
        };
        let arrived = !(files.is_empty() && dropped.text.is_empty() && dropped.url.is_empty());
        let delivered = target.filter(|&t| {
            arrived
                && self.deliver_drop(
                    t,
                    &pending,
                    files,
                    dropped.text.clone(),
                    dropped.url.clone(),
                )
        });
        if delivered.is_none() {
            let at = (pending.point, pending.held);
            for m in pending.chain.iter().filter(|m| m.entered) {
                self.drag_in.send(m.key, EventKind::Dragleave, at, Some(id));
            }
        }
        // The failed drop's leaves go first: they can change what the drag
        // still under way is over
        self.drag_flush();
        if self.drag_in.over {
            self.drag_at(self.drag_in.point, self.drag_in.held);
            self.drag_flush();
        }
        delivered
    }

    /// Who takes a drop that Android gives hosted field `id`'s view at
    /// `point` (LLP 1094.001 D5): -1 not the field, which another node is
    /// painted over there or which takes no touch; else as `drop_owner`.
    pub fn field_drop(&mut self, id: ViewId, point: (f32, f32)) -> i32 {
        if self.drag_hit(point) != Some(id) || self.field_inert(id) {
            return -1;
        }
        self.drop_owner(id)
    }

    /// Which node takes a drop over `id`: 0 none, 1 `id` itself, 2 one above
    /// it, so a hosted field knows whether to give a drag away (LLP 1094.001 D5).
    pub fn drop_owner(&self, id: ViewId) -> i32 {
        match self.pointer_node(id, &[EventKind::Drop]) {
            None => 0,
            Some(n) if n == id => 1,
            Some(_) => 2,
        }
    }

    /// Views a commit renewed (LLP 1078) are new mounts: a pending drop's
    /// target among them is not its target, and none of them heard a
    /// drag's enter.
    pub(super) fn drag_renewed(&mut self, renewed: &[ViewId]) {
        let kernel = self.host.kernel();
        let keys: Vec<NodeKey> = renewed
            .iter()
            .filter_map(|id| kernel.node(*id).map(|n| n.key))
            .collect();
        if keys.is_empty() {
            return;
        }
        let drag = &mut self.drag_in;
        drag.chain.retain(|m| !keys.contains(&m.key));
        drag.outbox.retain(|s| !keys.contains(&s.key));
        for pending in &mut drag.pending {
            pending.chain.retain(|m| !keys.contains(&m.key));
            if pending.target.is_some_and(|k| keys.contains(&k)) {
                pending.target = None;
            }
        }
    }

    pub(crate) fn agent_drag(
        &mut self,
        id: Option<ViewId>,
        request: &serde_json::Value,
    ) -> Result<String, String> {
        let id = id.ok_or("a drag requires a target")?;
        if request.get("dragleave").is_some() {
            self.drag_left();
            return Ok(
                serde_json::json!({"tapped":id, "dragleave":true, "delivery":"presenter"})
                    .to_string(),
            );
        }
        let kind = if request.get("drop").is_some() {
            "drop"
        } else {
            "dragenter"
        };
        let paths: Vec<PathBuf> = serde_json::from_value(request[kind].clone())
            .map_err(|_| format!("{kind} requires a list of file paths"))?;
        let [text, url] = ["text", "url"].map(|key| match request.get(key) {
            None => Ok(String::new()),
            Some(value) => value
                .as_str()
                .map(str::to_owned)
                .ok_or(format!("{kind}'s {key} is a string")),
        });
        let dropped = Dropped {
            paths,
            text: text?,
            url: url?,
        };
        if dropped == Dropped::default() {
            return Err(format!(
                "{kind} requires a file path, --text or --url: a drag carries something"
            ));
        }
        let at: Option<[f32; 2]> = serde_json::from_value(request["at"].clone())
            .map_err(|_| "a drag's at requires two coordinates")?;
        let point = self.pointer_target(id, at.map(|[x, y]| (x, y)))?;
        let held = match request.get("modifiers") {
            None => self.modifiers(),
            Some(keys) => keys
                .as_str()
                .and_then(KeyModifiers::held)
                .ok_or("invalid drag modifiers")?,
        };
        if let Some(path) = (kind == "drop" && cfg!(target_os = "android"))
            .then(|| dropped.paths.iter().find(|p| !p.is_file()))
            .flatten()
        {
            return Err(format!("{} is no file on the device: the reader's carrier puts a drop's files there, and the headless one does not", path.display()));
        }
        if kind == "drop" && cfg!(target_os = "android") && self.field_keeps(point, &dropped) {
            return Err(format!("view {id} is under a field, which keeps a drop of text or a link on Android when its own node takes none (LLP 1094.001 D2), and the driver inserts nothing: drop a file with it, or drop on a node that is not a field"));
        }
        let target = if kind == "drop" {
            self.dropped(point, held, &dropped).ok_or_else(|| {
                format!("view {id} took no drop: nothing under it declares `drop`, or none of its files could be opened")
            })?
        } else {
            self.drag_over(point, held);
            id
        };
        Ok(serde_json::json!({"tapped":target, "at":point, (kind):dropped.paths.len(), "delivery":"presenter"}).to_string())
    }

    /// Whether the field at `point` keeps `dropped`, as Android's text view
    /// does a drag of text or a link that its own node takes no drop of; one
    /// that carries a file goes to the `drop` above it (LLP 1094.001 D5).
    fn field_keeps(&mut self, point: (f32, f32), dropped: &Dropped) -> bool {
        dropped.paths.is_empty()
            && self.drag_hit(point).is_some_and(|hit| {
                self.hosts_field(hit) && !self.field_inert(hit) && self.drop_owner(hit) != 1
            })
    }

    fn drag_hit(&mut self, point: (f32, f32)) -> Option<ViewId> {
        self.hit(point.0, point.1)
            .filter(|id| !self.brush.region_blocks_action(*id))
    }

    /// `hit` and its listener ancestors that hear `dragenter` or
    /// `dragleave`, innermost first.
    fn drag_chain(&self, hit: Option<ViewId>) -> Vec<NodeKey> {
        let mut chain = Vec::new();
        let mut at = hit;
        while let Some(node) =
            at.and_then(|id| self.pointer_node(id, &[EventKind::Dragenter, EventKind::Dragleave]))
        {
            let kernel = self.host.kernel();
            let Some(key) = kernel.node(node).map(|n| n.key) else {
                break;
            };
            chain.push(key);
            at = self.listener_parent(kernel, node);
        }
        chain
    }

    /// The drag is at `point`: its chain there is recorded and its enters
    /// and leaves queued, before any is sent. What it hit there.
    fn drag_at(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<ViewId> {
        self.drag_in.over = true;
        self.drag_in.point = point;
        self.drag_in.held = held;
        let hit = self.drag_hit(point);
        let chain = self.drag_chain(hit);
        self.drag_crossings(chain);
        hit
    }

    /// The innermost node that takes a drop over `hit`.
    fn drop_target(&self, hit: Option<ViewId>) -> Option<ViewId> {
        self.pointer_node(hit?, &[EventKind::Drop])
    }

    /// The drag is over `keys`, innermost first: a node it left will hear a
    /// leave if it heard an enter, and one it came over an enter, unless a
    /// pending drop is over that node, whose enter then waits for the drop
    /// to land (LLP 1094.001 D2).
    fn drag_crossings(&mut self, keys: Vec<NodeKey>) {
        let previous = std::mem::take(&mut self.drag_in.chain);
        let pending = &self.drag_in.pending;
        let chain: Vec<Member> = keys
            .into_iter()
            .map(|key| {
                let was = previous.iter().any(|m| m.key == key && m.entered);
                let waits = pending.iter().any(|p| p.chain.iter().any(|m| m.key == key));
                Member {
                    key,
                    entered: was || !waits,
                }
            })
            .collect();
        let (point, held) = (self.drag_in.point, self.drag_in.held);
        for m in previous
            .iter()
            .filter(|m| m.entered && !chain.iter().any(|n| n.key == m.key))
        {
            self.drag_in
                .send(m.key, EventKind::Dragleave, (point, held), None);
        }
        for m in chain
            .iter()
            .filter(|m| m.entered && !previous.iter().any(|p| p.key == m.key && p.entered))
        {
            self.drag_in
                .send(m.key, EventKind::Dragenter, (point, held), None);
        }
        self.drag_in.chain = chain;
    }

    /// The drag dropped where no node took it: the nodes under it that
    /// waited for a pending drop hear their enter, then every one its leave.
    fn drag_ended(&mut self) {
        self.drag_in.over = false;
        let (point, held) = (self.drag_in.point, self.drag_in.held);
        let chain = std::mem::take(&mut self.drag_in.chain);
        for m in chain.iter().filter(|m| !m.entered) {
            self.drag_in
                .send(m.key, EventKind::Dragenter, (point, held), None);
        }
        for m in &chain {
            self.drag_in
                .send(m.key, EventKind::Dragleave, (point, held), None);
        }
    }

    /// Send the queued enters and leaves in order, each to a node still the
    /// one it was decided for: a handler that renews, disables or hides one
    /// runs before the next is sent. An enter goes only to a node that still
    /// hears drags and has a box, and one that does not then hears no leave
    /// either.
    fn drag_flush(&mut self) {
        while let Some(sent) = self.drag_in.outbox.pop_front() {
            let Some(id) = self.host.kernel().node_by_key(sent.key).map(|n| n.id) else {
                continue;
            };
            let kinds = [EventKind::Dragenter, EventKind::Dragleave];
            let enter = sent.kind == EventKind::Dragenter;
            let heard = (!enter || self.pointer_node(id, &kinds) == Some(id))
                && self.drag_event(id, sent.kind, sent.point, sent.held);
            if enter && !heard {
                self.drag_in.unentered(sent.key, sent.drop);
            }
        }
    }

    /// Send an enter or a leave, which carry no data (LLP 1094.001 D1), to
    /// `id`, its record at `point` with `held`; false when it has no box to
    /// take a record from.
    fn drag_event(
        &mut self,
        id: ViewId,
        kind: EventKind,
        point: (f32, f32),
        held: KeyModifiers,
    ) -> bool {
        let Some(record) = self.pointer_record(id, point.0, point.1, -1, 0) else {
            return false;
        };
        let data = DropEvent {
            offset_x: record.offset_x,
            offset_y: record.offset_y,
            held,
            files: Vec::new(),
            text: String::new(),
            url: String::new(),
        };
        let event = match kind {
            EventKind::Dragenter => Event::Dragenter(data),
            _ => Event::Dragleave(data),
        };
        if let Some(error) = self.pointer_dispatch(id, kind, event, self.pointer_now()) {
            self.host.log(error);
        }
        true
    }

    /// Send pending drop `drop` to `target` with what arrived, at the record it had at its release.
    fn deliver_drop(
        &mut self,
        target: ViewId,
        drop: &Pending,
        files: Vec<String>,
        text: String,
        url: String,
    ) -> bool {
        let data = DropEvent {
            offset_x: drop.offsets.0,
            offset_y: drop.offsets.1,
            held: drop.held,
            files,
            text,
            url,
        };
        if let Some(error) = self.pointer_dispatch(
            target,
            EventKind::Drop,
            Event::Drop(data),
            self.pointer_now(),
        ) {
            self.host.log(error);
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_plan::Value;

    #[test]
    fn two_files_over_an_editor_drop_once_on_its_nearest_target_in_order() {
        let source = r#"component App
  state calls = 0
  state names = ""
  state enters = 0
  state leaves = 0
  action received(e: DragEvent)
    names = e.files.join("|")
    calls = calls + 1
  action outer(e: DragEvent)
    calls = calls + 100
  action entered(e: DragEvent)
    enters = enters + 1
  action left(e: DragEvent)
    leaves = leaves + 1
  view
    column width=300 height=200 drop=outer
      column width=220 height=120 drop=received dragenter=entered dragleave=left
        textarea testId="editor" value="original text" width=180 height=80
"#;
        let mut p = booted(source);
        let kernel = p.host.kernel();
        let field = kernel
            .node_by_key(kernel.find_by_test_id("editor")[0])
            .unwrap()
            .id;
        let rect = p.rect_of(field).unwrap();
        let point = (rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
        assert_eq!(p.hit(point.0, point.1), Some(field));
        let held = KeyModifiers::default();
        assert!(p.drag_over(point, held).is_some());
        assert!(p.drag_over(point, held).is_some());
        p.drag_left();
        assert!(p.drag_over(point, held).is_some());
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let files = [root.join("README.md"), root.join("docs/testing.md")];
        let reply = p
            .agent_drag(Some(field), &serde_json::json!({"drop":files}))
            .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["drop"], 2);
        assert_eq!(reply["delivery"], "presenter");
        p.drag_left();
        let r = p.host.runner();
        assert_eq!(r.slot("calls"), Some(&Value::Number(1.)));
        assert_eq!(r.slot("enters"), Some(&Value::Number(2.)));
        assert_eq!(r.slot("leaves"), Some(&Value::Number(1.)));
        let docs: Vec<_> = r
            .slot("names")
            .and_then(Value::as_str)
            .unwrap()
            .split('|')
            .collect();
        assert_eq!(docs.len(), 2);
        for (doc, name) in docs.iter().zip(["/README.md", "/testing.md"]) {
            assert!(doc.starts_with("doc:/") && doc.ends_with(name), "{doc}");
        }
        assert_eq!(p.field_text(field), "original text");
    }

    #[test]
    fn text_and_a_link_over_an_editor_drop_on_its_nearest_target() {
        let source = r#"component App
  state text = ""
  state url = ""
  state files = -1
  action received(e: DragEvent)
    text = e.text
    url = e.url
    files = e.files.length
  view
    column width=300 height=200 drop=received
      textarea testId="editor" value="original text" width=180 height=80
"#;
        let mut p = booted(source);
        let kernel = p.host.kernel();
        let field = kernel
            .node_by_key(kernel.find_by_test_id("editor")[0])
            .unwrap()
            .id;
        let empty = p.agent_drag(Some(field), &serde_json::json!({"drop": []}));
        assert!(empty
            .unwrap_err()
            .contains("requires a file path, --text or --url"));
        let request = serde_json::json!({"drop": [], "text": "Mount Fuji\nJapan", "url": "https://example.com/fuji"});
        p.agent_drag(Some(field), &request).unwrap();
        let r = p.host.runner();
        assert_eq!(r.slot("text"), Some(&Value::str("Mount Fuji\nJapan")));
        assert_eq!(r.slot("url"), Some(&Value::str("https://example.com/fuji")));
        assert_eq!(r.slot("files"), Some(&Value::Number(0.)));
        // Exact's fields take no drag on this host, so the text goes to the drop above
        assert_eq!(p.field_text(field), "original text");
    }

    /// A zone that counts its drops, enters and leaves, keeping the last drop's text.
    fn zone() -> (Presenter<()>, (f32, f32), ViewId) {
        let source = r#"component App
  state drops = 0
  state enters = 0
  state leaves = 0
  state text = ""
  action received(e: DragEvent)
    drops = drops + 1
    text = e.text
  action entered(e: DragEvent)
    enters = enters + 1
  action left(e: DragEvent)
    leaves = leaves + 1
  view
    column width=300 height=200 testId="zone" drop=received dragenter=entered dragleave=left
      textarea testId="editor" value="original text" width=180 height=80
      textarea testId="off" value="off" disabled=true width=180 height=40
"#;
        let p = booted(source);
        let kernel = p.host.kernel();
        let field = kernel
            .node_by_key(kernel.find_by_test_id("editor")[0])
            .unwrap()
            .id;
        (p, (250., 150.), field)
    }

    fn counts(p: &Presenter<()>) -> [f64; 3] {
        let r = p.host.runner();
        ["drops", "enters", "leaves"].map(|k| match r.slot(k) {
            Some(Value::Number(n)) => *n,
            other => panic!("{k}: {other:?}"),
        })
    }

    fn text(text: &str) -> Dropped {
        Dropped {
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_pending_drop_holds_a_newer_drags_enter_until_it_lands() {
        let (mut p, point, _) = zone();
        let held = KeyModifiers::default();
        p.drag_over(point, held);
        let id = p.drop_released(point, held).unwrap();
        assert_eq!(
            counts(&p),
            [0., 1., 0.],
            "released: the zone neither drops nor leaves yet"
        );
        // A newer drag over the zone while the drop's items copy: its enter waits
        p.drag_over(point, held);
        assert_eq!(counts(&p), [0., 1., 0.]);
        assert!(p.drop_landed(id, &text("Fuji")).is_some());
        assert_eq!(
            counts(&p),
            [1., 2., 0.],
            "the drop, then the newer drag's enter"
        );
        assert_eq!(p.host.runner().slot("text"), Some(&Value::str("Fuji")));
        p.drag_left();
        assert_eq!(counts(&p), [1., 2., 1.]);
        assert_eq!(p.drop_landed(id, &text("again")), None, "a drop lands once");
    }

    #[test]
    fn a_newer_drag_that_leaves_while_it_waits_sends_nothing() {
        let (mut p, point, _) = zone();
        let held = KeyModifiers::default();
        p.drag_over(point, held);
        let id = p.drop_released(point, held).unwrap();
        p.drag_over(point, held);
        p.drag_left();
        p.drop_landed(id, &text("Fuji"));
        assert_eq!(
            counts(&p),
            [1., 1., 0.],
            "the waiting enter was cancelled, and its leave with it"
        );
    }

    #[test]
    fn a_newer_drag_that_drops_first_hears_its_waiting_enter_then_drops() {
        let (mut p, point, _) = zone();
        let held = KeyModifiers::default();
        p.drag_over(point, held);
        let first = p.drop_released(point, held).unwrap();
        p.drag_over(point, held);
        let second = p.drop_released(point, held).unwrap();
        assert_eq!(
            counts(&p),
            [0., 2., 0.],
            "the newer drag's enter, as it drops"
        );
        p.drop_landed(second, &text("second"));
        p.drop_landed(first, &text("first"));
        assert_eq!(counts(&p), [2., 2., 0.]);
        assert_eq!(p.host.runner().slot("text"), Some(&Value::str("first")));
    }

    #[test]
    fn a_drop_whose_items_never_arrived_leaves_its_chain() {
        let (mut p, point, _) = zone();
        let held = KeyModifiers::default();
        p.drag_over(point, held);
        let id = p.drop_released(point, held).unwrap();
        assert_eq!(p.drop_landed(id, &Dropped::default()), None);
        assert_eq!(counts(&p), [0., 1., 1.]);
    }

    #[test]
    fn a_field_keeps_text_and_links_and_gives_files_away() {
        /// A shell that hosts fields, as Android's reader does, measuring none.
        struct Shell;
        impl crate::fields::FieldMetrics for Shell {
            fn measure(
                &self,
                face: &crate::fields::FieldFace,
                _: &str,
                width: Option<f32>,
            ) -> [f32; 3] {
                [width.unwrap_or(0.), face.size, face.size]
            }
        }
        let _hosting = crate::fields::host_here(&Shell, 1.0);
        let (mut p, _, field) = zone();
        let rect = p.rect_of(field).unwrap();
        let over = (rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
        assert_eq!(
            p.drop_owner(field),
            2,
            "the zone above the field takes drops"
        );
        assert!(p.field_keeps(over, &text("Fuji")));
        assert!(p.field_keeps(
            over,
            &Dropped {
                url: "https://example.com".into(),
                ..Default::default()
            }
        ));
        assert!(!p.field_keeps(
            over,
            &Dropped {
                paths: vec!["/tmp/a.txt".into()],
                ..Default::default()
            }
        ));
        assert!(
            !p.field_keeps((250., 150.), &text("Fuji")),
            "no field there"
        );
        let kernel = p.host.kernel();
        let off = kernel
            .node_by_key(kernel.find_by_test_id("off")[0])
            .unwrap()
            .id;
        let rect = p.rect_of(off).unwrap();
        let disabled = (rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
        assert!(
            !p.field_keeps(disabled, &text("Fuji")),
            "a disabled field keeps no drag"
        );
        assert_eq!(
            p.field_drop(field, over),
            2,
            "the field, under a zone that takes drops"
        );
        assert_eq!(
            p.field_drop(off, disabled),
            -1,
            "a disabled field takes no drop"
        );
        assert_eq!(
            p.field_drop(field, (250., 150.)),
            -1,
            "the zone is painted there"
        );
    }

    /// The app `source`, its first frame painted.
    fn booted(source: &str) -> Presenter<()> {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (300., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        p
    }

    /// A zone that logs each event it hears: its kind, whether Shift was
    /// held, and a drop's offsets and text.
    fn logging_zone() -> (Presenter<()>, (f32, f32)) {
        let source = r#"component App
  state log = ""
  action received(e: DragEvent)
    log = `${log}drop ${e.shiftKey ? "S" : "-"} ${e.offsetX} ${e.offsetY} ${e.text};`
  action entered(e: DragEvent)
    log = `${log}enter ${e.shiftKey ? "S" : "-"};`
  action left(e: DragEvent)
    log = `${log}leave ${e.shiftKey ? "S" : "-"};`
  view
    column width=300 height=200 testId="zone" drop=received dragenter=entered dragleave=left
"#;
        (booted(source), (100., 50.))
    }

    fn log(p: &Presenter<()>) -> String {
        p.host
            .runner()
            .slot("log")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    const SHIFT: KeyModifiers = KeyModifiers {
        shift: true,
        ctrl: false,
        alt: false,
        meta: false,
    };

    #[test]
    fn a_failed_drop_leaves_with_its_own_record_before_the_newer_drag_enters() {
        let (mut p, point) = logging_zone();
        p.drag_over(point, SHIFT);
        let id = p.drop_released(point, SHIFT).unwrap();
        p.drag_over((150., 60.), KeyModifiers::default());
        assert_eq!(log(&p), "enter S;", "the newer drag's enter waits");
        p.drop_landed(id, &Dropped::default());
        assert_eq!(
            log(&p),
            "enter S;leave S;enter -;",
            "the failed drop's leave, with its Shift, then the newer enter"
        );
    }

    #[test]
    fn a_drop_hears_the_record_it_had_at_its_release() {
        let (mut p, point) = logging_zone();
        p.drag_over(point, SHIFT);
        let id = p.drop_released(point, SHIFT).unwrap();
        p.drag_over((150., 60.), KeyModifiers::default());
        p.drop_landed(id, &text("Fuji"));
        assert_eq!(log(&p), "enter S;drop S 100 50 Fuji;enter -;");
    }

    #[test]
    fn a_newer_drag_a_field_takes_hears_its_waiting_enter_then_its_leave() {
        let (mut p, point) = logging_zone();
        p.drag_over(point, KeyModifiers::default());
        let id = p.drop_released(point, KeyModifiers::default()).unwrap();
        p.drag_dropped(point, SHIFT);
        assert_eq!(log(&p), "enter -;enter S;leave S;");
        p.drop_landed(id, &text("Fuji"));
        assert_eq!(
            log(&p),
            "enter -;enter S;leave S;drop - 100 50 Fuji;",
            "the drag is over: no enter after the drop"
        );
    }

    #[test]
    fn a_drop_whose_target_was_renewed_leaves_instead() {
        let (mut p, point) = logging_zone();
        p.drag_over(point, KeyModifiers::default());
        let id = p.drop_released(point, KeyModifiers::default()).unwrap();
        let kernel = p.host.kernel();
        let zone = kernel
            .node_by_key(kernel.find_by_test_id("zone")[0])
            .unwrap()
            .id;
        p.drag_renewed(&[zone]);
        assert_eq!(p.drop_landed(id, &text("Fuji")), None);
        assert_eq!(
            log(&p),
            "enter -;",
            "a new mount heard no enter, so it hears no leave either"
        );
    }

    #[test]
    fn a_new_plan_numbers_its_drops_after_the_old_ones() {
        let (mut p, point) = logging_zone();
        let old = p.drop_released(point, KeyModifiers::default()).unwrap();
        p.drag_in.reset();
        let new = p.drop_released(point, KeyModifiers::default()).unwrap();
        assert_ne!(old, new);
        assert_eq!(
            p.drop_landed(old, &text("late")),
            None,
            "the old plan's copy lands nowhere"
        );
        assert!(p.drop_landed(new, &text("Fuji")).is_some());
    }

    /// A zone, a button that disables itself in its own `dragenter`, in a
    /// column that also takes drops; each counts its drops.
    fn self_disabling_zone() -> (Presenter<()>, ViewId) {
        let source = r#"component App
  state off = false
  state outer = 0
  state inner = 0
  action toOuter(e: DragEvent)
    outer = outer + 1
  action toInner(e: DragEvent)
    inner = inner + 1
  action entered(e: DragEvent)
    off = true
  view
    column width=300 height=200 testId="outer" drop=toOuter
      button "Zone" testId="inner" width=200 height=100 disabled=off drop=toInner dragenter=entered
"#;
        let p = booted(source);
        let kernel = p.host.kernel();
        let outer = kernel
            .node_by_key(kernel.find_by_test_id("outer")[0])
            .unwrap()
            .id;
        (p, outer)
    }

    #[test]
    fn an_enter_that_disables_the_target_at_the_release_does_not_move_the_drop() {
        let (mut p, _) = self_disabling_zone();
        let id = p
            .drop_released((100., 50.), KeyModifiers::default())
            .unwrap();
        assert_eq!(
            p.host.runner().slot("off"),
            Some(&Value::Bool(true)),
            "the zone's enter ran at the release"
        );
        assert_eq!(
            p.drop_landed(id, &text("Fuji")),
            None,
            "the zone it was released on takes no drop now"
        );
        let r = p.host.runner();
        assert_eq!(
            [r.slot("outer"), r.slot("inner")],
            [Some(&Value::Number(0.)), Some(&Value::Number(0.))]
        );
    }

    #[test]
    fn a_hover_answers_with_the_target_after_its_enters_ran() {
        let (mut p, outer) = self_disabling_zone();
        assert_eq!(
            p.drag_over((100., 50.), KeyModifiers::default()),
            Some(outer),
            "the zone disabled itself as the drag came over it, so the column above takes the drop there"
        );
    }

    #[test]
    fn an_enter_skipped_at_the_release_owes_no_leave_when_the_drop_fails() {
        let mut p = booted(
            r#"component App
  state off = false
  state log = ""
  action innerIn(e: DragEvent)
    off = true
    log = `${log}enter inner;`
  action innerOut(e: DragEvent)
    log = `${log}leave inner;`
  action outerIn(e: DragEvent)
    log = `${log}enter outer;`
  action outerOut(e: DragEvent)
    log = `${log}leave outer;`
  action received(e: DragEvent)
    log = `${log}drop;`
  view
    column width=300 height=200 disabled=off dragenter=outerIn dragleave=outerOut
      button "Zone" width=200 height=100 drop=received dragenter=innerIn dragleave=innerOut
"#,
        );
        let id = p
            .drop_released((100., 50.), KeyModifiers::default())
            .unwrap();
        assert_eq!(
            p.drop_landed(id, &Dropped::default()),
            None,
            "nothing arrived"
        );
        assert_eq!(
            log(&p),
            "enter inner;leave inner;",
            "the column was disabled before its enter, so it hears neither"
        );
    }

    #[test]
    fn a_failed_drop_whose_leave_enables_a_zone_enters_the_drag_over_it() {
        let mut p = booted(
            r#"component App
  state off = true
  state log = ""
  action received(e: DragEvent)
    log = `${log}drop;`
  action left(e: DragEvent)
    off = false
    log = `${log}leave A;`
  action entered(e: DragEvent)
    log = `${log}enter B;`
  view
    column width=300 height=200
      button "A" width=200 height=100 drop=received dragleave=left
      button "B" width=200 height=100 disabled=off dragenter=entered
"#,
        );
        let id = p
            .drop_released((100., 50.), KeyModifiers::default())
            .unwrap();
        p.drag_over((100., 150.), KeyModifiers::default());
        assert_eq!(log(&p), "", "B is disabled, and A waits for its drop");
        assert_eq!(
            p.drop_landed(id, &Dropped::default()),
            None,
            "nothing arrived"
        );
        assert_eq!(
            log(&p),
            "leave A;enter B;",
            "A's leave enabled B, under the drag still over the window"
        );
    }

    #[test]
    fn an_enter_to_a_node_hidden_at_the_release_owes_no_leave_when_the_drop_fails() {
        let mut p = booted(
            r#"component App
  state look = "flex"
  state log = ""
  action innerIn(e: DragEvent)
    look = "none"
    log = `${log}enter inner;`
  action innerOut(e: DragEvent)
    log = `${log}leave inner;`
  action outerIn(e: DragEvent)
    log = `${log}enter outer;`
  action outerOut(e: DragEvent)
    log = `${log}leave outer;`
  action shown(e: DragEvent)
    look = "flex"
  action received(e: DragEvent)
    log = `${log}drop;`
  view
    row width=300 height=200
      button "Show" width=100 height=100 dragenter=shown
      column width=150 height=200 display=look dragenter=outerIn dragleave=outerOut
        button "Zone" width=150 height=100 drop=received dragenter=innerIn dragleave=innerOut
"#,
        );
        let id = p
            .drop_released((150., 50.), KeyModifiers::default())
            .unwrap();
        assert_eq!(
            log(&p),
            "enter inner;",
            "the column was hidden before its enter"
        );
        p.drag_over((50., 50.), KeyModifiers::default());
        assert_eq!(
            p.drop_landed(id, &Dropped::default()),
            None,
            "nothing arrived"
        );
        assert_eq!(
            log(&p),
            "enter inner;leave inner;",
            "the column, shown again, hears no leave of the drop"
        );
    }
}
