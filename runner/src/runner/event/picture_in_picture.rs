use super::{DataSource, Event, EventKind, NodeKey, NodesId, Runner, ViewId};
use crate::vm::Frame;

pub(in crate::runner) struct FloatingVideo {
    key: NodeKey,
    site: NodesId,
    frames: Vec<Frame>,
}

impl<D: DataSource> Runner<D> {
    /// A native player can leave after the commit that removed its element. The
    /// reader checks the session and allocation before delivering that final event.
    pub fn expects_video_leave(&self, view: ViewId, key: NodeKey) -> bool {
        self.floating_videos
            .get(&view)
            .is_some_and(|v| v.key == key)
    }

    pub(super) fn event_target(
        &mut self,
        view: ViewId,
        event: &Event,
    ) -> Option<(NodesId, Vec<Frame>)> {
        let live = self.find(view);
        match event {
            Event::Media(EventKind::Enterpictureinpicture, _) => {
                if let (Some(node), Some((site, frames))) = (self.kernel.node(view), &live) {
                    if node.node_type == exact_kernel::NodeType::Video
                        && self
                            .plan
                            .node(*site)
                            .handlers
                            .iter()
                            .any(|h| self.plan.handler(h).event == EventKind::Leavepictureinpicture)
                    {
                        self.floating_videos.insert(
                            view,
                            FloatingVideo {
                                key: node.key,
                                site: *site,
                                frames: frames.clone(),
                            },
                        );
                    }
                }
            }
            Event::Media(EventKind::Leavepictureinpicture, _) => {
                if let Some(video) = self.floating_videos.remove(&view) {
                    if self.kernel.node(view).is_none_or(|n| n.key != video.key) {
                        return Some((video.site, video.frames));
                    }
                }
            }
            Event::Focusout { .. } if live.is_none() => return self.take_detached_focus(view),
            _ => {}
        }
        live
    }
}
