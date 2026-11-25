//! An iframe's commands (@ref LLP 1020.002 D3, D7, D11) on a painted host:
//! `navigate(id, url)`, `traverseHistory(id, delta)`, `reload(id, ignoreCache=)`,
//! `stopLoading(id)`, `captureScreenshot(id, path, clip=)` and `openDevTools(id)`.
//! Where the reader keeps partitions (`exactPage().iframePartition`, Android
//! with the WebView's profiles and navigation listener) the four that move a
//! frame go to a partitioned frame's reader in its props
//! (`paint/native.rs`). The rest, and any to an unpartitioned frame or on a
//! host without (LLP 1020.002 §11: Linux `absent`), are refused where the app
//! hears it: the frame's `navigateerror` with `NotSupportedError`, a
//! capture's `capture` with it (LLP 1133 D8).

use super::Presenter;
use exact_kernel::{NodeKey, NodeType, PropId, ViewId};
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

/// `page` with `exactPage().iframePartition` as `kept` answers, asked only
/// by a plan with an `iframe`, as asking may load the platform's web engine.
pub fn with_partitions(
    page: exact_runner::Page,
    plan: &exact_plan::Plan,
    kept: fn() -> Option<bool>,
) -> exact_runner::Page {
    exact_runner::Page {
        iframe_partition: page.iframe_partition || framed(plan) && kept() == Some(true),
        ..page
    }
}

fn framed(plan: &exact_plan::Plan) -> bool {
    (plan.nodes.iter()).any(|node| NodeType::from_wire(node.node_type) == Some(NodeType::WebView))
}

impl<D: DataSource> Presenter<D> {
    /// Asks `kept` for `exactPage().iframePartition` when a live update
    /// brings the first `iframe`, unless the boot's plan had one and the
    /// platform answered (`None`: it could not, and is asked again).
    pub fn serve_frame_partitions(&mut self, kept: fn() -> Option<bool>) {
        self.frame_partitions = Some(kept);
        self.frame_partitions_asked();
    }

    /// `exactPage()` from the first settlement of a runner that replaces
    /// this one: this one's facts, with `iframePartition` asked if the
    /// reader has not been and the new plan has an `iframe`.
    pub(super) fn candidate_page(&self) -> exact_runner::time::PageFacts {
        let (page, kept) = (self.host.runner().page(), self.frame_partitions);
        exact_runner::time::PageFacts::new(move |plan| match kept {
            Some(kept) => with_partitions(page, plan, kept),
            None => page,
        })
    }

    /// A drive's facts (`prefer`): an `iframePartition` it names holds, and
    /// the reader is not asked after.
    pub(crate) fn prefer_page(
        &mut self,
        page: exact_runner::Page,
        partition: bool,
    ) -> Option<String> {
        let error = self.set_page(page);
        if partition && error.is_none() {
            self.frame_partitions = None;
        }
        error
    }

    /// The platform is asked until it answers, once.
    pub(super) fn frame_partitions_asked(&mut self) {
        if framed(self.host.runner().plan())
            && self.frame_partitions.is_some_and(|kept| kept().is_some())
        {
            self.frame_partitions = None;
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
        if let Some(partition) = self.moved_by_reader(name, view, key) {
            let args: Vec<serde_json::Value> = (args.iter().skip(1))
                .map(|a| match (a.as_str(), a.as_number(), a.as_bool()) {
                    (Some(s), ..) => s.into(),
                    (_, Some(n), _) => n.into(),
                    (.., Some(b)) => b.into(),
                    _ => serde_json::Value::Null,
                })
                .collect();
            let frames = self.brush.media.frames.entry(key).or_default();
            frames.push(&partition, name, &args);
            self.brush.media.asked.push(key);
            self.dirty = true;
            return;
        }
        let host = if cfg!(target_os = "android") {
            "Android"
        } else {
            "this host"
        };
        let partitioned = (self.host.kernel().node(view))
            .is_some_and(|node| node.props.str(PropId::Partition).is_some());
        let message = match name {
            "captureScreenshot" | "openDevTools" => format!(
                "{name}: iframe \"{id}\": {host} does not yet {}",
                if name == "openDevTools" {
                    "open a frame's inspector"
                } else {
                    "capture a frame's pixels"
                }
            ),
            // As Apple's: only a partitioned frame is a browsing context of its own to move.
            _ if !partitioned && self.host.runner().page().iframe_partition => {
                format!("{name}: iframe {id} has no partition")
            }
            _ => format!("{name}: iframe \"{id}\": {host} does not yet navigate a frame"),
        };
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

    /// The partition of the frame at `view` whose reader runs `name`: a
    /// command that moves a mounted partitioned frame, where the reader
    /// keeps partitions. The reader refuses it for a partition it cannot keep.
    fn moved_by_reader(&self, name: &str, view: ViewId, key: NodeKey) -> Option<String> {
        let moves = matches!(
            name,
            "navigate" | "traverseHistory" | "reload" | "stopLoading"
        );
        let node = self
            .host
            .kernel()
            .node(view)
            .filter(|node| node.key == key)?;
        let partition = node.props.str(PropId::Partition)?;
        (moves && self.host.runner().page().iframe_partition).then(|| partition.to_owned())
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
