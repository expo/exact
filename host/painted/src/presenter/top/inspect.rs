//! JSON inspection of the open top-layer surface for the agent's `state`.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// The open surface for the agent's `state`: what it is, the authored
    /// node, what it says, and each row's node and box in the window.
    pub fn surface_json(&self) -> serde_json::Value {
        if self.alert.is_some() {
            return self.alert_json();
        }
        let Some(shown) = self.surface() else {
            return serde_json::Value::Null;
        };
        let rows: Vec<_> = (shown.plan.rows().zip(shown.targets))
            .map(|(row, target)| {
                let (x, y, w, h) = target.rect;
                serde_json::json!({"id": row.node, "title": row.title, "enabled": row.enabled,
                    "destructive": row.destructive, "cancel": row.cancel,
                    "x": r2(x), "y": r2(y), "w": r2(w), "h": r2(h)})
            })
            .collect();
        let (title, message) = match shown.plan {
            Plan::Alert { title, message, .. } => (title.clone(), message.clone()),
            Plan::Menu { .. } => (None, None),
        };
        // A menu's section titles, in order.
        let headers: Vec<&str> = match shown.plan {
            Plan::Menu { entries, .. } => (entries.iter())
                .filter_map(|e| match e {
                    shape::Entry::Header(title, _, _) => Some(title.as_str()),
                    _ => None,
                })
                .collect(),
            Plan::Alert { .. } => Vec::new(),
        };
        serde_json::json!({
            "serial": shown.serial,
            "kind": match shown.kind { SurfaceKind::Alert => "alert", SurfaceKind::Menu => "menu" },
            "node": shown.node, "title": title, "message": message,
            "w": r2(shown.size.0), "h": r2(shown.size.1), "rows": rows, "headers": headers,
        })
    }
}
