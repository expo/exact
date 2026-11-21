//! The first tree a replacement presents over the live AppKit hierarchy.
use super::*;

impl Mirror {
    pub(super) fn update_props(
        &mut self,
        id: ViewId,
        mut props: exact_kernel::SortedMap<&'static str, String>,
        batch: &mut Batch,
    ) {
        // What layout derives (`emit_layout`) is layout's to set and clear.
        // Cleared here as unauthored, a growing list's `flexGrows` went and
        // came back in every batch that touched the list: each row of a
        // scrolling list of tasks reconfigured its checkbox for it.
        for key in super::layout::DERIVED_PROPS {
            if let (Some(value), false) = (self.props.get(key), props.contains_key(key)) {
                props.insert(key, value.clone());
            }
        }
        if props != self.props {
            let set: Vec<(&str, String)> = props
                .iter()
                .filter(|(k, v)| self.props.get(*k) != Some(*v))
                .map(|(k, v)| (*k, v.clone()))
                .collect();
            let clear: Vec<&str> = self
                .props
                .keys()
                .filter(|k| !props.contains_key(*k))
                .copied()
                .collect();
            batch.props(id, &set, &clear);
            self.props = props;
        }
    }
}

impl<D: DataSource> Host<D> {
    pub(super) fn present_first_tree(
        &mut self,
        previous: Option<&Self>,
        order: &[ViewId],
        batch: &mut Batch,
    ) {
        let handlers = self.runner.handlers();
        for &id in order {
            let old = previous.and_then(|h| h.mirror.get(&id)).map(Box::as_ref);
            self.create_over(id, handlers.get(&id).map_or(&[], Vec::as_slice), old, batch);
        }
        self.roots = self.runner.roots();
        let mut roots_changed = previous.is_none_or(|h| h.roots != self.roots);
        if let Some(old) = previous {
            batch.replaces_tree = true;
            // Remove children before their parents. A projected view can
            // disappear while its inline or omitted kernel node survives.
            let mut removed = Batch::new();
            let mut replaced = std::collections::HashSet::new();
            for id in old.preorder().into_iter().rev() {
                let Some(kind) = old.mirror.get(&id).and_then(|m| m.kind) else {
                    continue;
                };
                if self.mirror.get(&id).and_then(|m| m.kind) != Some(kind) {
                    removed.destroy(id);
                    replaced.insert(id);
                }
            }
            if !replaced.is_empty() {
                roots_changed |= self.roots.iter().any(|id| replaced.contains(id));
                for &id in order {
                    let Some(m) = self.mirror.get_mut(&id) else {
                        continue;
                    };
                    // The child list can be equal even when one id now
                    // has a different native object that needs mounting.
                    if m.children
                        .iter()
                        .any(|c| replaced.contains(c) && batch.creates(*c))
                    {
                        m.children = Box::default();
                    }
                }
                batch.prepend(removed);
            }
        }
        for &id in order {
            self.emit_children(id, batch);
        }
        self.emit_paragraphs(batch);
        if roots_changed {
            batch.roots(&self.roots);
        }
    }
}

impl<D: DataSource> Host<D> {
    /// Boot omits motion at identity because a new layer starts there.
    /// A retained layer can still show the previous runner's last sample.
    pub(super) fn finish_replaced_motion(
        &mut self,
        old: &Self,
        order: &[ViewId],
        batch: &mut Batch,
    ) {
        use exact_kernel::motion::layout_presented;
        for &id in order {
            if batch.creates(id) || self.mirror.get(&id).and_then(|m| m.kind).is_none() {
                continue;
            }
            let (Some(before), Some(after)) =
                (old.runner.kernel().node(id), self.runner.kernel().node(id))
            else {
                continue;
            };
            let (before, after) = (motion_node(before.key), motion_node(after.key));
            for property in [
                Property::Translate,
                Property::Rotate,
                Property::Scale,
                Property::Opacity,
            ] {
                let identity = property.identity().expect("box transform or opacity");
                let was = old
                    .engine
                    .sampled_value(before, property)
                    .unwrap_or(identity);
                let now = self
                    .engine
                    .sampled_value(after, property)
                    .unwrap_or(identity);
                if was != identity && now == identity {
                    self.present_value(id, property, after, now, false, batch);
                }
            }
            let layout = |engine: &Engine, key| {
                engine
                    .sampled_value(key, Property::Layout)
                    .map(|sample| layout_presented(engine, key, sample))
                    .unwrap_or_default()
            };
            if layout(&old.engine, before) != [0.; 4] && layout(&self.engine, after) == [0.; 4] {
                batch.present4(id, "layout", [0.; 4]);
            }
        }
        self.svg
            .withdraw_replaced_animations(&old.svg, batch, |id| {
                self.mirror.get(&id).and_then(|m| m.kind).is_some()
            });
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use crate::abi::{Bridge, Hooks};
    use serde_json::Value;

    fn output(bridge: &Bridge<()>, length: u32) -> Value {
        serde_json::from_slice(bridge.output_bytes(length as usize)).unwrap()
    }

    #[test]
    fn commit_updates_a_kept_field_and_replaces_only_its_sibling() {
        let source = r#"component App
  view
    main
      input id="editor" testId="field" value="steady" placeholder="Before" opacity=0.4
      text "Gone" id="old" testId="gone"
"#;
        let first = contract::compile(source).unwrap().encode();
        let next = contract::compile(
            &source
                .replace("testId=\"field\" ", "")
                .replace("Before", "After")
                .replace(" opacity=0.4", "")
                .replace(
                    "text \"Gone\" id=\"old\" testId=\"gone\"",
                    "text \"Added\" id=\"new\" testId=\"added\"",
                ),
        )
        .unwrap()
        .encode();
        let mut bridge = Bridge::new();
        let length = bridge.boot(&first, (), Hooks::none(), 320.0, 240.0);
        let boot = output(&bridge, length);
        assert_ne!(boot["replacesTree"], true);
        let ops = boot["ops"].as_array().unwrap();
        let id = |test_id| {
            ops.iter()
                .find(|op| op["op"] == "create" && op["props"]["testId"] == test_id)
                .unwrap()["id"]
                .as_u64()
                .unwrap()
        };
        let field = id("field");
        let gone = id("gone");
        for commit in [false, true] {
            bridge.input_write(&next);
            bridge.prepare_plan(next.len(), (), Hooks::none(), 320.0, 240.0);
            if !commit {
                bridge.discard_plan();
            }
        }
        let length = bridge.commit_plan();
        let change = output(&bridge, length);
        assert_eq!(change["replacesTree"], true);
        let ops = change["ops"].as_array().unwrap();
        let structural = |kind| ops.iter().filter(|op| op["op"] == kind).collect::<Vec<_>>();
        assert_eq!(
            structural("destroy")
                .iter()
                .map(|op| op["id"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [gone]
        );
        let creates = structural("create");
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0]["props"]["testId"], "added");
        assert!(creates[0]["id"].as_u64().unwrap() > gone);
        assert!(structural("roots").is_empty());
        let children = structural("children");
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0]["ids"],
            serde_json::json!([field, creates[0]["id"]])
        );
        assert!(structural("present").iter().any(|op| op["id"] == field
            && op["property"] == "opacity"
            && op["x"].as_f64() == Some(1.0)));
        let prop = structural("props")
            .into_iter()
            .find(|op| op["id"] == field && op["set"]["placeholder"] == "After")
            .unwrap();
        assert!(prop["set"].get("value").is_none());
        assert!(prop["clear"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("testId")));
    }
}
