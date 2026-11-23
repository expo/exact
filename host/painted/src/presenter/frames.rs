//! An iframe's commands (@ref LLP 1020.002 D3, D7, D11) on a painted host:
//! `navigate(id, url)`, `traverseHistory(id, delta)`, `reload(id, ignoreCache=)`,
//! `stopLoading(id)`, `captureScreenshot(id, path, clip=)` and `openDevTools(id)`.
//! None is built here yet (LLP 1020.002 §11: Android `later`, Linux `absent`),
//! so each is refused where the app hears it: the frame's `navigateerror`
//! with `NotSupportedError`, a capture's `capture` with it (LLP 1133 D8).

use super::Presenter;
use exact_kernel::{NodeKey, NodeType, ViewId};
use exact_runner::{DataSource, Event};

/// The frame commands, by name; a bare `reload()` is the development host's.
pub(super) fn is(name: &str, args: &[exact_plan::Value]) -> bool {
    match name {
        "reload" => !args.is_empty(),
        "navigate" | "traverseHistory" | "stopLoading" | "captureScreenshot" | "openDevTools" => {
            true
        }
        _ => false,
    }
}

/// The iframe `id` a frame command names.
pub(super) fn id(args: &[exact_plan::Value]) -> &str {
    args.first()
        .and_then(exact_plan::Value::as_str)
        .unwrap_or_default()
}

impl<D: DataSource> Presenter<D> {
    /// `exactPage().iframePartition`: what `kept` answers, asked only by a
    /// plan with an `iframe`, as asking may load the platform's web engine.
    pub fn serve_frame_partitions(&mut self, kept: impl FnOnce() -> bool) {
        let plan = self.host.runner().plan();
        if !(plan.nodes.iter())
            .any(|node| NodeType::from_wire(node.node_type) == Some(NodeType::WebView))
        {
            return;
        }
        let page = exact_runner::Page {
            iframe_partition: kept(),
            ..self.host.runner().page()
        };
        if let Some(e) = self.set_page(page) {
            self.host.log(format!("page: {e}"));
        }
    }

    /// Refuses a frame command once the commit's commands have run, at the
    /// frame it named when it was issued, if that frame is still mounted.
    pub(super) fn frame_command(
        &mut self,
        name: &str,
        args: &[exact_plan::Value],
        target: Option<(ViewId, NodeKey)>,
    ) {
        let id = id(args);
        let Some((view, key)) = target else {
            return self
                .host
                .log(format!("{name}: no unique live iframe with id \"{id}\""));
        };
        let host = if cfg!(target_os = "android") {
            "Android"
        } else {
            "this host"
        };
        let message = format!(
            "{name}: iframe \"{id}\": {host} does not yet {}",
            match name {
                "captureScreenshot" => "capture a frame's pixels",
                "openDevTools" => "open a frame's inspector",
                _ => "navigate a frame",
            }
        );
        self.host.log(&message);
        if self
            .host
            .kernel()
            .node(view)
            .is_none_or(|node| node.key != key)
        {
            return;
        }
        let payload = match name {
            // D11 has no event: the refusal is the journal's, as on the web.
            "openDevTools" => return,
            "captureScreenshot" => {
                let path = args
                    .get(1)
                    .and_then(exact_plan::Value::as_str)
                    .unwrap_or_default();
                serde_json::json!({"path": path, "width": 0, "height": 0, "error": "NotSupportedError", "message": message})
            }
            _ => serde_json::json!({
                "url": "", "destination": "", "navigationId": 0, "navigationType": "push",
                "title": "", "icons": [], "canGoBack": false, "canGoForward": false, "loading": false,
                "partitioned": self.host.runner().page().iframe_partition, "controller": "person",
                "error": "NotSupportedError", "message": message, "recovering": false,
            }),
        };
        let event = if name == "captureScreenshot" {
            "capture"
        } else {
            "navigateerror"
        };
        self.frame_event(view, event, &payload.to_string());
    }

    /// The one live iframe with this `id`, and its allocation.
    pub(super) fn frame_by_id(&self, id: &str) -> Option<(ViewId, NodeKey)> {
        let kernel = self.host.kernel();
        let mut frames = (kernel.find_by_id(id).into_iter())
            .filter_map(|key| kernel.node_by_key(key))
            .filter(|node| node.node_type == NodeType::WebView);
        match (frames.next(), frames.next()) {
            (Some(node), None) => Some((node.id, node.key)),
            _ => None,
        }
    }

    /// A frame's record (host kind 58: the event's name, a newline, the
    /// record), to the frame's handler of that event if it has one.
    pub(crate) fn frame_event(&mut self, view: ViewId, event: &str, record: &str) {
        let event = match Event::of_host_kind(58, &format!("{event}\n{record}")) {
            Ok(event) => event,
            Err(e) => return self.host.log(format!("iframe: {e}")),
        };
        let (Event::FrameNavigation(kind, _) | Event::FrameRecord(kind, _)) = &event else {
            return;
        };
        if !self.host.runner().handlers_of(view).contains(kind) {
            return;
        }
        let sent = self.host.dispatch_now(view, event);
        if let Some(error) = sent.or(self.after_commit()) {
            self.host.log(error);
        }
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
