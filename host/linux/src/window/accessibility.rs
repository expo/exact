//! AT-SPI for the Wayland window (LLP 1115.004 D11).
use super::Event;
use accesskit::{ActivationHandler, TreeUpdate};
use accesskit_winit::{Adapter, WindowEvent};
use exact_painted::{presenter::accesskit::WindowTree, Presenter};
use exact_runner::DataSource;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use winit::{
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

impl From<accesskit_winit::Event> for Event {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event)
    }
}

struct Activation {
    first: Arc<AtomicBool>,
    proxy: EventLoopProxy<Event>,
    window_id: WindowId,
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // AccessKit can accept an update for this session before winit delivers the wake.
        self.first.store(true, Ordering::Release);
        let _ = self.proxy.send_event(
            accesskit_winit::Event {
                window_id: self.window_id,
                window_event: WindowEvent::InitialTreeRequested,
            }
            .into(),
        );
        None
    }
}

pub(super) struct Access {
    pub(super) adapter: Adapter,
    first: Arc<AtomicBool>,
    tree: Option<WindowTree>,
}

impl Access {
    pub(super) fn new(
        events: &ActiveEventLoop,
        window: &Window,
        proxy: EventLoopProxy<Event>,
    ) -> Self {
        let first = Arc::new(AtomicBool::new(false));
        let activation = Activation {
            first: first.clone(),
            proxy: proxy.clone(),
            window_id: window.id(),
        };
        Self {
            adapter: Adapter::with_mixed_handlers(events, window, activation, proxy),
            first,
            tree: None,
        }
    }

    pub(super) fn event<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        window: &Window,
        title: &str,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::InitialTreeRequested => {
                self.tree = Some(WindowTree::default());
                p.access_reset();
                self.sync(p, window, title);
            }
            WindowEvent::AccessibilityDeactivated => {
                self.tree = None;
                p.access_reset();
            }
            WindowEvent::ActionRequested(request) => {
                self.sync(p, window, title);
                if let Some(tree) = &self.tree {
                    tree.action(p, request, super::input::SCROLL_LINE);
                    self.sync(p, window, title);
                }
            }
        }
    }

    pub(super) fn sync<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        window: &Window,
        title: &str,
    ) {
        let Some(tree) = &mut self.tree else { return };
        let scale = window.scale_factor();
        let size = window.inner_size().to_logical::<f64>(scale);
        let build = |tree: &mut WindowTree, p: &mut Presenter<D>| {
            tree.update(p, title, (size.width, size.height), scale, false, |_| false)
        };
        let (update, changed) = build(tree, p);
        if changed {
            self.adapter.update_if_active(|| {
                // A new reader can connect while the UI thread builds a delta.
                // Under the adapter's session lock, its first update must contain the root.
                if self.first.swap(false, Ordering::AcqRel) && update.tree.is_none() {
                    *tree = WindowTree::default();
                    p.access_reset();
                    build(tree, p).0
                } else {
                    update
                }
            });
        }
    }
}

#[cfg(test)]
#[path = "accessibility_tests.rs"]
mod tests;
