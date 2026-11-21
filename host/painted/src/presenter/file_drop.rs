//! External file drags over the displayed tree (LLP 1069.010 D1).
use super::*;
use exact_kernel::NodeKey;
use exact_plan::EventKind;
use exact_runner::{DropEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct FileDrag {
    entered: Vec<NodeKey>,
    point: (f32, f32),
    held: KeyModifiers,
}

impl<D: DataSource> Presenter<D> {
    /// Update the drag's entered elements and tell the shell whether this point accepts a drop.
    pub fn files_over(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<ViewId> {
        self.file_drag.point = point;
        self.file_drag.held = held;
        let mut entered = Vec::new();
        let mut at = self.file_hit(point);
        while let Some(node) =
            at.and_then(|id| self.pointer_node(id, &[EventKind::Dragenter, EventKind::Dragleave]))
        {
            let kernel = self.host.kernel();
            let Some(key) = kernel.node(node).map(|n| n.key) else {
                break;
            };
            entered.push(key);
            at = self.display.parent(kernel, node);
        }
        self.file_crossings(entered);
        let hit = self.file_hit(point)?;
        self.pointer_node(hit, &[EventKind::Drop])
    }

    /// Cancel the drag, including when the files leave the window.
    pub fn files_left(&mut self) {
        self.file_crossings(Vec::new());
    }

    /// Deliver the chosen files once, in platform order, to the innermost accepting node.
    pub fn files_dropped(
        &mut self,
        point: (f32, f32),
        held: KeyModifiers,
        paths: &[PathBuf],
    ) -> Option<ViewId> {
        let Some(target) = self.files_over(point, held) else {
            self.files_left();
            return None;
        };
        let files: Vec<_> = paths
            .iter()
            .filter(|path| path.is_absolute() && path.exists())
            .filter_map(|path| exact_data::documents::mint(path, 0))
            .collect();
        if files.is_empty() {
            self.files_left();
            return None;
        }
        self.file_drag.entered.clear();
        self.file_event(target, EventKind::Drop, files)
            .then_some(target)
    }

    pub(crate) fn agent_file_drag(
        &mut self,
        id: Option<ViewId>,
        request: &serde_json::Value,
    ) -> Result<String, String> {
        let id = id.ok_or("file drag requires a target")?;
        if request.get("dragleave").is_some() {
            self.files_left();
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
        if paths.is_empty() {
            return Err(format!("{kind} requires at least one file path"));
        }
        let at: Option<[f32; 2]> = serde_json::from_value(request["at"].clone())
            .map_err(|_| "file drag at requires two coordinates")?;
        let point = self.pointer_target(id, at.map(|[x, y]| (x, y)))?;
        let held = match request.get("modifiers") {
            None => self.modifiers(),
            Some(keys) => keys
                .as_str()
                .and_then(KeyModifiers::held)
                .ok_or("invalid file drag modifiers")?,
        };
        let target = if kind == "drop" {
            self.files_dropped(point, held, &paths)
                .ok_or_else(|| format!("view {id} accepted no dropped files"))?
        } else {
            self.files_over(point, held);
            id
        };
        Ok(serde_json::json!({"tapped":target, "at":point, (kind):paths.len(), "delivery":"presenter"}).to_string())
    }

    fn file_hit(&mut self, point: (f32, f32)) -> Option<ViewId> {
        self.hit(point.0, point.1)
            .filter(|id| !self.brush.region_blocks_action(*id))
    }

    fn file_crossings(&mut self, entered: Vec<NodeKey>) {
        let previous = std::mem::take(&mut self.file_drag.entered);
        for (which, other, kind) in [
            (&entered, &previous, EventKind::Dragenter),
            (&previous, &entered, EventKind::Dragleave),
        ] {
            for key in which.iter().filter(|key| !other.contains(key)) {
                if let Some(id) = self.host.kernel().node_by_key(*key).map(|n| n.id) {
                    self.file_event(id, kind, Vec::new());
                }
            }
        }
        self.file_drag.entered = entered;
    }

    fn file_event(&mut self, id: ViewId, kind: EventKind, files: Vec<String>) -> bool {
        let (x, y) = self.file_drag.point;
        let Some(point) = self.pointer_record(id, x, y, -1, 0) else {
            return false;
        };
        let data = DropEvent {
            offset_x: point.offset_x,
            offset_y: point.offset_y,
            held: self.file_drag.held,
            files,
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
        assert!(p.files_over(point, held).is_some());
        assert!(p.files_over(point, held).is_some());
        p.files_left();
        assert!(p.files_over(point, held).is_some());
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let files = [root.join("README.md"), root.join("docs/testing.md")];
        let reply = p
            .agent_file_drag(Some(field), &serde_json::json!({"drop":files}))
            .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["drop"], 2);
        assert_eq!(reply["delivery"], "presenter");
        p.files_left();
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
}
