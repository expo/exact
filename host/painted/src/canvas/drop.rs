//! Drags into the reader's window, from another app or from this one
//! (LLP 1094.001 D5): the reader's root and its hosted fields hear Android's
//! drag events and tell them here at the view's pixels, as a touch is told.

use super::CanvasHost;
use crate::presenter::Dropped;
use exact_runner::DataSource;

impl<D: DataSource + Default> CanvasHost<D> {
    /// The drag is at pixels (`x`, `y`): the nodes under it hear their
    /// enters and leaves.
    pub fn drag_over(&mut self, x: f32, y: f32) {
        let held = self.p.modifiers();
        self.p.drag_over((x / self.scale, y / self.scale), held);
    }

    /// The drag left the window or was cancelled.
    pub fn drag_left(&mut self) {
        self.p.drag_left();
    }

    /// A field took the drop at pixels (`x`, `y`).
    pub fn drag_dropped(&mut self, x: f32, y: f32) {
        let held = self.p.modifiers();
        self.p.drag_dropped((x / self.scale, y / self.scale), held);
    }

    /// The drag was released at pixels (`x`, `y`) before its items are
    /// copied: the drop's number, or 0 when nothing takes it there.
    pub fn drop_released(&mut self, x: f32, y: f32) -> u32 {
        let held = self.p.modifiers();
        self.p
            .drop_released((x / self.scale, y / self.scale), held)
            .unwrap_or(0)
    }

    /// Who takes a drop Android gives field `id`'s view at pixels (`x`,
    /// `y`): -1 not the field, else 0 the field, 1 its own node, 2 one above.
    pub fn field_drop(&mut self, id: u32, x: f32, y: f32) -> i32 {
        self.p.field_drop(id, (x / self.scale, y / self.scale))
    }

    /// Drop `id`'s items arrived, as the reader's JSON
    /// `{"paths": […], "text": "…", "url": "…"}`.
    pub fn drop_landed(&mut self, id: u32, items: &str) {
        let dropped = parse(items);
        if dropped.is_none() {
            self.p
                .host_mut()
                .log(format!("drop: the reader's items were not JSON: {items}"));
        }
        self.p.drop_landed(id, &dropped.unwrap_or_default());
    }
}

/// The reader's items: absolute paths, text and a link.
fn parse(items: &str) -> Option<Dropped> {
    let value: serde_json::Value = serde_json::from_str(items).ok()?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let paths = value
        .get("paths")?
        .as_array()?
        .iter()
        .filter_map(|p| p.as_str().map(Into::into))
        .collect();
    Some(Dropped {
        paths,
        text: text("text"),
        url: text("url"),
    })
}
