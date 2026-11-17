//! Full-screen invoker popovers retain the region their selected option used (LLP 1021 D2).
use crate::host::{Host, Mode};
use exact_kernel::position_try::area::{self, History};
use exact_kernel::style::cells::{COLUMN, ROW};
use exact_kernel::{Dimension, NodeKey, Offer, StyleMask, ViewId};
use exact_runner::DataSource;
use std::collections::HashMap;

type Rect = (f32, f32, f32, f32);
#[derive(Default)]
pub(crate) struct State {
    open: HashMap<ViewId, Open>,
    pub(crate) origins: HashMap<ViewId, (f32, f32)>,
}
struct Open {
    invoker: NodeKey,
    anchor: Option<(Rect, f32)>,
    history: History,
}
impl State {
    pub(crate) fn remember(&mut self, layer: ViewId, invoker: NodeKey) {
        self.open.insert(
            layer,
            Open {
                invoker,
                anchor: None,
                history: History::default(),
            },
        );
    }
}
impl<D: DataSource> Host<D> {
    fn layout_popovers(&mut self) {
        let closed: Vec<_> = self
            .popovers
            .open
            .keys()
            .copied()
            .filter(|id| !self.layers.iter().any(|(layer, ..)| layer == id))
            .collect();
        let mut released = false;
        for id in closed {
            self.popovers.open.remove(&id);
            self.popovers.origins.remove(&id);
            if self.kernel().host_block(id).is_some() {
                released |= self.runner.kernel_mut().set_host_block(id, None).is_ok();
            }
        }
        if released {
            self.layout_popover_blocks();
        }
        if self.mode != Mode::Fullscreen {
            return;
        }
        // Bottom to top: an invoker in an earlier layer uses its placed frame.
        let open: Vec<_> = self.layers.iter().map(|(id, ..)| *id).collect();
        for id in open {
            let Some(state) = self.popovers.open.get(&id) else {
                continue;
            };
            let k = self.kernel();
            let anchor = k
                .node_by_key(state.invoker)
                .and_then(|n| self.popover_anchor(n.id))
                .or(state.anchor);
            let (Some((anchor, scroll)), Some(node)) = (anchor, k.node(id)) else {
                continue;
            };
            let style = node.computed_style(StyleMask::INHERITED);
            let env = k.env_of(id);
            let insets = [
                style.top.clone(),
                style.right.clone(),
                style.bottom.clone(),
                style.left.clone(),
            ];
            if style.position_area.name() == "none"
                && insets.iter().any(|d| d.resolve(&env) != Dimension::Auto)
            {
                if self.popovers.origins.remove(&id).is_some() {
                    let _ = self.runner.kernel_mut().set_host_block(id, None);
                    self.layout_popover_blocks();
                }
                continue;
            }
            if !self.kernel().is_top_layer(id) && self.kernel().host_block(id).is_some() {
                let _ = self.runner.kernel_mut().set_host_block(id, None);
                self.layout_popover_blocks();
            }
            let viewport = (self.cols as f32 * COLUMN, self.rows as f32 * ROW);
            let block_backed = self.kernel().host_block(id).is_some();
            let frame = self.kernel().node(id).unwrap().frame;
            let state = self.popovers.open.get_mut(&id).unwrap();
            state.anchor = Some((anchor, scroll));
            let placed = if block_backed {
                area::place(
                    &style,
                    &env,
                    anchor,
                    viewport,
                    (0., scroll),
                    &mut state.history,
                    |block| self.runner.kernel_mut().measure_block(id, block),
                )
            } else {
                Ok(area::Placement {
                    origin: area::origin(
                        &style,
                        &env,
                        anchor,
                        (frame.width, frame.height),
                        viewport,
                        &mut state.history,
                    ),
                    block: None,
                })
            };
            let Ok(placed) = placed else { continue };
            let k = self.kernel();
            if k.host_block(id) != placed.block
                && (placed.block.is_some() || !k.host_block_implied(id))
            {
                if self
                    .runner
                    .kernel_mut()
                    .set_host_block(id, placed.block)
                    .is_err()
                {
                    continue;
                }
                self.layout_popover_blocks();
            }
            self.popovers.origins.insert(id, placed.origin);
        }
    }

    fn popover_anchor(&self, id: ViewId) -> Option<(Rect, f32)> {
        let k = self.kernel();
        let node = k.node(id)?;
        if node.style.display == exact_kernel::Display::None {
            return None;
        }
        let f = node.frame;
        let (mut x, mut y) = (f.x, f.y);
        let mut scroll = 0.;
        let mut at = Some(id);
        while let Some(id) = at {
            let n = k.node(id)?;
            if self.layers.iter().any(|(layer, ..)| *layer == id) {
                let (dx, dy) = crate::paint::placement(&self.scene(), id, self.cols, self.rows, 0.);
                x -= dx;
                y -= dy;
                break;
            }
            at = n.parent;
            if let Some(parent) = at {
                let offset = self.scroll.get(&parent).copied().unwrap_or_default();
                y -= offset;
                scroll += offset;
            }
        }
        Some(((x, y, f.width, f.height), scroll))
    }

    fn layout_popover_blocks(&mut self) {
        let offer = Offer::definite(self.cols as f32 * COLUMN, self.rows as f32 * ROW);
        for root in self.runner.roots() {
            let _ = self.runner.kernel_mut().compute_layout(root, offer);
        }
    }

    pub(crate) fn sync_top_layer(&mut self) {
        self.layout_popovers();
        let open: Vec<ViewId> = self.layers.iter().map(|(id, ..)| *id).collect();
        let frames = {
            let scene = self.scene();
            let kernel = scene.kernel;
            let shown = |&id: &ViewId| {
                let used = kernel.host_block_size(id)?;
                let (dx, dy) = crate::paint::placement(&scene, id, self.cols, self.rows, 0.0);
                Some((
                    id,
                    [-dx as f64, -dy as f64, used.0 as f64, used.1 as f64],
                    None,
                ))
            };
            let placed: Vec<_> = open.iter().filter_map(shown).collect();
            let at_frame = |&id: &ViewId| {
                let f = kernel.node(id)?.frame;
                let rect = [f.x as f64, f.y as f64, f.width as f64, f.height as f64];
                Some(
                    placed
                        .iter()
                        .find(|p| p.0 == id)
                        .copied()
                        .unwrap_or((id, rect, None)),
                )
            };
            (!placed.is_empty()).then(|| open.iter().filter_map(at_frame).collect::<Vec<_>>())
        };
        let Some(frames) = frames else {
            self.runner.top_layer(&open);
            return;
        };
        self.runner.top_layer_frames(&frames, None);
    }
}
