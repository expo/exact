//! What the Android reader's media players report (LLP 1042.001 D7): their
//! events, dispatched as the runner's media events, and their observed state,
//! kept for the agent's `state.media` and for the node's natural size. A host
//! with no player has no reports, and `state.media` says why.

use super::*;
use std::collections::BTreeMap;
use std::time::Instant;

/// The last state each media node's player reported.
#[derive(Default)]
pub(crate) struct Players {
    rows: BTreeMap<ViewId, Row>,
}

struct Row {
    key: exact_kernel::NodeKey,
    state: serde_json::Value,
    at: Instant,
    // Read where a player reports, which is only Android's reader.
    #[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
    size: Option<(f32, f32)>,
}

impl<D: DataSource> Presenter<D> {
    /// A media element's report, only for the session and allocation the
    /// reader's element carries: an event by HTML's name with its payload,
    /// handed to the node when it handles it, or (`event` empty) its state.
    /// Whether anything was dispatched or laid out again.
    #[cfg(any(target_os = "android", test))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reader_media(
        &mut self,
        id: ViewId,
        key: exact_kernel::NodeKey,
        session: u64,
        event: &str,
        payload: &str,
        state: &str,
        now_ms: f64,
    ) -> bool {
        let valid = session == self.brush.native_session
            && self
                .host
                .kernel()
                .node(id)
                .is_some_and(|node| node.node_type == NodeType::Video && node.key == key);
        if !valid {
            return false;
        }
        if event.is_empty() {
            return self.media_state(id, key, state);
        }
        if !self
            .host
            .runner()
            .handlers_of(id)
            .iter()
            .any(|k| k.name() == event)
        {
            return false;
        }
        let Some(media) = Event::media_payload(&format!("{event}\n{payload}")) else {
            eprintln!("exact: video {id}: the player sent {event:?} with {payload:?}, which is not a media event");
            return false;
        };
        if let Some(error) = self.host.dispatch_at(id, media, now_ms) {
            eprintln!("exact: video: {error}");
        }
        if let Some(error) = self.after_commit() {
            eprintln!("exact: video: {error}");
        }
        true
    }

    /// Props of a mounted audio element whose player has no reader.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn background_audio(
        &self,
        id: ViewId,
        key: exact_kernel::NodeKey,
        session: u64,
    ) -> Option<serde_json::Value> {
        let kernel = self.host.kernel();
        let node = kernel.node(id)?;
        if session != self.brush.native_session
            || node.key != key
            || node.node_type != NodeType::Video
            || node.props.str(PropId::SemanticTag) != Some("audio")
        {
            return None;
        }
        let arena = kernel.arena();
        let mut root = node.key.index;
        while let Some(parent) = arena.parent(root) {
            root = parent;
        }
        // A hidden `portal` mover's top is alive and in no root (LLP 1013.002 D5).
        let hidden = || (self.host.runner().hidden_movers()).contains(&arena.local_id(root));
        if !arena.is_root(root) && !hidden() {
            return None;
        }
        let mut props = self.brush.media_props(&node);
        props["focused"] = false.into();
        props["timeupdate"] = self
            .host
            .runner()
            .handlers_of(id)
            .contains(&EventKind::Timeupdate)
            .into();
        Some(props)
    }

    /// Keep a player's state; its video's size, once known, is the node's
    /// natural size (LLP 1042 §2), laid out again when it changes.
    #[cfg(any(target_os = "android", test))]
    fn media_state(&mut self, id: ViewId, key: exact_kernel::NodeKey, state: &str) -> bool {
        let Ok(state) = serde_json::from_str::<serde_json::Value>(state) else {
            return false;
        };
        // Its poster shows until this source's first frame (LLP 1042.001 D5).
        let framed = state["frameReady"].as_bool() == Some(true);
        let poster_changed = framed != self.brush.media.framed.contains(&key);
        if poster_changed {
            if framed {
                self.brush.media.framed.insert(key);
            } else {
                self.brush.media.framed.remove(&key);
            }
            self.brush.media.asked.push(key);
            self.dirty = true;
        }
        let size = match (state["videoWidth"].as_f64(), state["videoHeight"].as_f64()) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Some((w as f32, h as f32)),
            _ => None,
        };
        let before = self
            .media
            .rows
            .get(&id)
            .filter(|r| r.key == key)
            .and_then(|r| r.size);
        self.media.rows.insert(
            id,
            Row {
                key,
                state,
                at: Instant::now(),
                size,
            },
        );
        if size.is_none() || size == before {
            return poster_changed;
        }
        if let Some(error) = self.host.set_intrinsic(id, size) {
            eprintln!("exact: video {id}: {error}");
        }
        self.dirty = true;
        true
    }

    /// @ref LLP 1042.001 D3 — which media nodes handle `timeupdate`, found
    /// again when the tree changed; the rows of those that changed paint
    /// again, so their players hear it.
    pub(crate) fn sync_media(&mut self, dirty: &mut crate::paint::rows::Dirty) {
        for key in self.brush.media.asked.drain(..) {
            dirty.node(key);
        }
        // @ref LLP 1042.001 D4 — the player whose controller exact2's
        // focus is on takes the window's focus, and gives it back after.
        let focused = (self.focus)
            .and_then(|id| self.host.kernel().node(id))
            .filter(|node| self.controller(node))
            .map(|node| node.key);
        if focused != self.brush.media.focused {
            for key in [focused, self.brush.media.focused].into_iter().flatten() {
                dirty.node(key);
            }
            self.brush.media.focused = focused;
        }
        for id in self.images.posters_changed.drain(..) {
            if let Some(node) = self.host.kernel().node(id) {
                dirty.node(node.key);
            }
        }
        self.brush.media.held = self.host.runner().hidden_movers();
        let kernel = self.host.kernel();
        let epoch = kernel.epoch();
        if self.brush.media.epoch == Some(epoch) {
            return;
        }
        self.brush.media.epoch = Some(epoch);
        (self.brush.media.commands).retain(|key, _| kernel.node_by_key(*key).is_some());
        (self.brush.media.framed).retain(|key| kernel.node_by_key(*key).is_some());
        let timed: std::collections::BTreeSet<_> = if kernel.has_type(NodeType::Video) {
            let mut roots = self.host.roots();
            roots.extend_from_slice(&self.brush.media.held);
            (kernel.preorder_where(&roots, |t, _| t == NodeType::Video))
                .into_iter()
                .filter(|id| (self.host.runner().handlers_of(*id)).contains(&EventKind::Timeupdate))
                .filter_map(|id| kernel.node(id).map(|n| n.key))
                .collect()
        } else {
            Default::default()
        };
        for key in timed.symmetric_difference(&self.brush.media.timed) {
            dirty.node(*key);
        }
        self.brush.media.timed = timed;
    }

    /// A media node whose player shows a controller (LLP 1042.001 D4): a
    /// `video` or `audio` with `controls`, on a host with players. It takes
    /// the focus, as a hand-built `PlayerView` does; without `controls` it
    /// takes none, as on the web.
    pub(crate) fn controller(&self, node: &exact_kernel::NodeRef<'_>) -> bool {
        self.brush.media.players
            && node.node_type == NodeType::Video
            && node.props.bool(PropId::Controls) == Some(true)
    }

    /// @ref LLP 1042.001 §5, D9 — `fastSeek(id, seconds)`, `load(id)` or
    /// `requestFullscreen(id)` on the
    /// media node with that HTML `id`, as `focus(id)` names one: counted in
    /// its props, which its player compares with those it last applied.
    pub(crate) fn media_command(&mut self, name: &str, args: &[exact_plan::Value]) {
        let id = args
            .first()
            .and_then(exact_plan::Value::as_str)
            .unwrap_or_default();
        let kernel = self.host.kernel();
        let node = (kernel.find_by_id(id).into_iter()).find_map(|key| kernel.node_by_key(key));
        let key = match node {
            Some(node) if node.node_type == NodeType::Video => node.key,
            found => {
                let why = match found {
                    Some(_) => "not a video or audio",
                    None => "no live node with that id",
                };
                self.host.log(format!("{name} \"{id}\" refused: {why}"));
                return;
            }
        };
        let asked = self.brush.media.commands.entry(key).or_default();
        match name {
            "fastSeek" => {
                asked.seeks += 1;
                asked.seconds = args
                    .get(1)
                    .and_then(exact_plan::Value::as_number)
                    .unwrap_or(f64::NAN);
            }
            "requestFullscreen" => asked.fullscreens += 1,
            _ => asked.loads += 1,
        }
        self.brush.media.asked.push(key);
        self.dirty = true;
    }

    /// `state.media`: each mounted media node's row, as its player last
    /// reported it with the time run on at its rate since, or, where no
    /// player has reported, an element that never plays, saying why.
    pub(crate) fn media_json(&self) -> serde_json::Value {
        let kernel = self.host.kernel();
        let rows: Vec<_> = kernel
            .rows(None)
            .unwrap_or_default()
            .iter()
            .filter(|r| r.node_type == NodeType::Video)
            .map(|r| {
                let reported = kernel
                    .node(r.id)
                    .and_then(|node| self.media.rows.get(&r.id).filter(|row| row.key == node.key));
                let state = match reported {
                    Some(row) => moved(row),
                    None => serde_json::json!({"unavailable": UNAVAILABLE, "paused": true, "currentTime": 0, "duration": null, "readyState": 0}),
                };
                serde_json::json!({"id": r.id, "state": state})
            })
            .collect();
        serde_json::Value::Array(rows)
    }
}

/// Why a media node never plays where no player reports: the Linux and
/// Windows hosts have no decoder or audio output (LLP 1042 §8).
const UNAVAILABLE: &str = "no media decoder or audio output on this host";

/// A reported state with its time advanced by the time since, while playing.
fn moved(row: &Row) -> serde_json::Value {
    let mut state = row.state.clone();
    if state["playing"].as_bool() == Some(true) {
        if let (Some(time), Some(rate)) = (
            state["currentTime"].as_f64(),
            state["playbackRate"].as_f64(),
        ) {
            let mut time = time + row.at.elapsed().as_secs_f64() * rate;
            if let Some(duration) = state["duration"].as_f64() {
                time = time.min(duration);
            }
            state["currentTime"] = serde_json::json!(time);
        }
    }
    state
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
