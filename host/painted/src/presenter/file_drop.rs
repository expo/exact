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
    entered: Vec<NodeKey>,
    point: (f32, f32),
    held: KeyModifiers,
}

impl<D: DataSource> Presenter<D> {
    /// Update the drag's entered elements and tell the shell whether this point accepts a drop.
    pub fn drag_over(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<ViewId> {
        self.drag_in.point = point;
        self.drag_in.held = held;
        let mut entered = Vec::new();
        let mut at = self.drag_hit(point);
        while let Some(node) =
            at.and_then(|id| self.pointer_node(id, &[EventKind::Dragenter, EventKind::Dragleave]))
        {
            let kernel = self.host.kernel();
            let Some(key) = kernel.node(node).map(|n| n.key) else {
                break;
            };
            entered.push(key);
            at = self.listener_parent(kernel, node);
        }
        self.drag_crossings(entered);
        let hit = self.drag_hit(point)?;
        self.pointer_node(hit, &[EventKind::Drop])
    }

    /// Cancel the drag, including when it leaves the window.
    pub fn drag_left(&mut self) {
        self.drag_crossings(Vec::new());
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
        let Some(target) = self.drag_over(point, held) else {
            self.drag_left();
            return None;
        };
        let files: Vec<_> = dropped
            .paths
            .iter()
            .filter(|path| path.is_absolute() && path.exists())
            .filter_map(|path| exact_data::documents::mint(path, 0))
            .collect();
        if files.is_empty() && dropped.text.is_empty() && dropped.url.is_empty() {
            self.drag_left();
            return None;
        }
        // A drop the target hears ends the drag with no leave; one it cannot hear is a leave
        let data = (files, dropped.text.clone(), dropped.url.clone());
        if !self.drag_event(target, EventKind::Drop, data) {
            self.drag_left();
            return None;
        }
        self.drag_in.entered.clear();
        Some(target)
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

    fn drag_hit(&mut self, point: (f32, f32)) -> Option<ViewId> {
        self.hit(point.0, point.1)
            .filter(|id| !self.brush.region_blocks_action(*id))
    }

    fn drag_crossings(&mut self, entered: Vec<NodeKey>) {
        let previous = std::mem::take(&mut self.drag_in.entered);
        for (which, other, kind) in [
            (&entered, &previous, EventKind::Dragenter),
            (&previous, &entered, EventKind::Dragleave),
        ] {
            for key in which.iter().filter(|key| !other.contains(key)) {
                if let Some(id) = self.host.kernel().node_by_key(*key).map(|n| n.id) {
                    self.drag_event(id, kind, Default::default());
                }
            }
        }
        self.drag_in.entered = entered;
    }

    /// Send `kind` to `id` with the drop's files, text and link, empty for an
    /// enter or a leave.
    fn drag_event(
        &mut self,
        id: ViewId,
        kind: EventKind,
        (files, text, url): (Vec<String>, String, String),
    ) -> bool {
        let (x, y) = self.drag_in.point;
        let Some(point) = self.pointer_record(id, x, y, -1, 0) else {
            return false;
        };
        let data = DropEvent {
            offset_x: point.offset_x,
            offset_y: point.offset_y,
            held: self.drag_in.held,
            files,
            text,
            url,
        };
        let event = match kind {
            EventKind::Dragenter => Event::Dragenter(data),
            EventKind::Dragleave => Event::Dragleave(data),
            EventKind::Drop => Event::Drop(data),
            _ => return false,
        };
        if let Some(error) = self.pointer_dispatch(id, kind, event, self.pointer_now()) {
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
}
