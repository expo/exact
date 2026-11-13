use super::*;

impl<D: DataSource> Presenter<D> {
    /// The open surface's nodes, in its own window's points: an alert's
    /// headline, its supporting text and its buttons; a menu's items, each
    /// section's title a heading before its items.
    pub(super) fn access_surface(&self, b: &mut Build) -> u32 {
        let Some(shown) = self.surface() else {
            return 0;
        };
        let whole = [0.0, 0.0, shown.size.0, shown.size.1];
        let parent = if let Some((offset, max)) = shown.scroll {
            let id = b.unique(identity(self.host.kernel(), shown.node));
            let flags = if offset > 0. { flag::BACKWARD } else { 0 }
                | if offset < max { flag::FORWARD } else { 0 };
            let name = self
                .host
                .kernel()
                .node(shown.node)
                .and_then(|n| self.access_label(&n))
                .unwrap_or_default();
            b.out.push(Node {
                id,
                parent: 0,
                view: shown.node,
                role: role::SCROLLER,
                flags,
                rect: whole,
                range: [offset, max, 0.],
                set: (0, 0),
                name,
                value: String::new(),
                hint: String::new(),
            });
            id
        } else {
            0
        };
        if let Plan::Alert { title, message, .. } = shown.plan {
            let words = [
                (HEADLINE, role::HEADING, title),
                (SUPPORTING, role::TEXT, message),
            ];
            let mut texts = shown.texts.iter().filter(|(node, _)| *node == shown.node);
            for (id, role, text) in words {
                let (Some(text), Some((_, rect))) = (text, texts.next()) else {
                    continue;
                };
                let id = b.unique(id);
                b.out.push(Node {
                    id,
                    parent: 0,
                    view: 0,
                    role,
                    flags: 0,
                    rect: [rect.0, rect.1, rect.2, rect.3],
                    range: [0.0; 3],
                    set: (0, 0),
                    name: text.clone(),
                    value: String::new(),
                    hint: String::new(),
                });
            }
        }
        // A menu's section titles, by the item each heads (its index in the
        // rows).
        let mut headers: Vec<(usize, &String, ViewId)> = Vec::new();
        if let Plan::Menu(entries) = shown.plan {
            let mut items = 0;
            for entry in entries {
                match entry {
                    Entry::Item(_) => items += 1,
                    Entry::Header(title, node) => headers.push((items, title, *node)),
                    Entry::Separator => {}
                }
            }
        }
        let mut headers = headers.into_iter().peekable();
        for (index, (row, target)) in shown.plan.rows().zip(shown.targets).enumerate() {
            while let Some((_, title, node)) = headers.next_if(|h| h.0 == index) {
                let rect = shown
                    .texts
                    .iter()
                    .find(|(n, _)| *n == node)
                    .map(|(_, r)| *r);
                let Some(rect) =
                    rect.and_then(|r| shown.clip.map_or(Some(r), |clip| intersect(r, clip)))
                else {
                    continue;
                };
                self.access_bar_node(
                    b,
                    node,
                    parent,
                    role::HEADING,
                    0,
                    title.clone(),
                    (rect, whole),
                );
            }
            let rect = if let Some(clip) = shown.clip {
                let Some(rect) = intersect(target.rect, clip) else {
                    continue;
                };
                rect
            } else {
                target.rect
            };
            let mut flags = if row.enabled {
                flag::CLICK
            } else {
                flag::DISABLED
            };
            if row.checked {
                flags |= flag::CHECKABLE | flag::CHECKED;
            }
            let place = (rect, whole);
            self.access_bar_node(
                b,
                row.node,
                parent,
                role::BUTTON,
                flags,
                row.title.clone(),
                place,
            );
        }
        shown.serial
    }
}
