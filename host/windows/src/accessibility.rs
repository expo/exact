//! UI Automation over the presenter's accessibility model.
use accesskit::{NodeId, TreeUpdate};
use accesskit_winit::{Adapter, WindowEvent as AccessEvent};
use exact_painted::{presenter::accesskit::WindowTree, presenter::top::StatusMenu, Presenter};
use exact_runner::DataSource;
use std::collections::BTreeMap;
use std::rc::Rc;
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window,
};

pub(super) enum Event {
    DisplayChanged,
    Wake,
    Close,
    /// The app's bare `focus()`.
    Focus,
    /// The app's `hideWindow()`.
    Hide,
    ModalWake,
    Timer(std::sync::Arc<crate::timers::Fired>),
    Access(accesskit_winit::Event),
    TrayAccess(accesskit::ActionRequest),
}
impl From<accesskit_winit::Event> for Event {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event)
    }
}

mod model;
pub(crate) use model::dismiss_popup;
use model::MenuSnapshot;

pub(super) struct Access {
    adapter: Adapter,
    tree: Option<WindowTree>,
    embedded: BTreeMap<u32, (NodeId, u32)>,
}

impl Access {
    pub fn new(events: &ActiveEventLoop, window: &Window, proxy: EventLoopProxy<Event>) -> Self {
        Self {
            adapter: Adapter::with_event_loop_proxy(events, window, proxy),
            tree: None,
            embedded: BTreeMap::new(),
        }
    }

    pub fn process_event(&mut self, window: &Window, event: &WindowEvent) {
        self.adapter.process_event(window, event);
    }

    #[cfg(test)]
    pub(crate) fn root_label(&self) -> Option<&str> {
        self.tree.as_ref()?.title()
    }

    pub fn set_title(&mut self, title: &str) {
        if let Some(update) = self.tree.as_mut().and_then(|tree| tree.set_title(title)) {
            self.adapter.update_if_active(|| update);
        }
    }

    pub fn event<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        window: &Window,
        title: &str,
        event: AccessEvent,
    ) {
        match event {
            AccessEvent::InitialTreeRequested => {
                self.tree = Some(WindowTree::default());
                p.access_reset();
            }
            AccessEvent::AccessibilityDeactivated => {
                self.tree = None;
                p.access_reset();
                return;
            }
            AccessEvent::ActionRequested(request) => {
                self.sync(p, window, title);
                if let Some(tree) = &self.tree {
                    tree.action(p, request, crate::wheel::LINE);
                }
            }
        }
        self.sync(p, window, title);
    }

    pub fn sync<D: DataSource>(&mut self, p: &mut Presenter<D>, window: &Window, title: &str) {
        let Some(tree) = &mut self.tree else { return };
        let scale = window.scale_factor();
        let size = window.inner_size().to_logical::<f64>(scale);
        let detached = p.status_menu().is_some();
        let (update, changed) = tree.update(
            p,
            title,
            (size.width, size.height),
            scale,
            detached,
            |view| {
                self.embedded.get(&view).is_some_and(|(_, generation)| {
                    exact_ink::device::d3d11::fields::identity(view) == Some(*generation)
                })
            },
        );
        if changed {
            self.adapter.update_if_active(|| update);
        }
        self.embed_fields(p, window);
    }
    fn embed_fields<D: DataSource>(&mut self, p: &Presenter<D>, window: &Window) {
        if !exact_ink::device::d3d11::fields::enabled() {
            return;
        }
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = accesskit_windows::HWND(handle.hwnd.get() as *mut _);
        let candidates: BTreeMap<_, _> = p
            .access_told()
            .iter()
            .filter_map(|node| {
                exact_ink::device::d3d11::fields::identity(node.view)
                    .map(|generation| (node.view, (NodeId(node.id as u64), generation)))
            })
            .collect();
        let old = std::mem::take(&mut self.embedded);
        for (view, identity) in old {
            if candidates.get(&view) == Some(&identity) {
                self.embedded.insert(view, identity);
            } else {
                let _ = accesskit_windows::embed_windowless(hwnd, identity.0, None);
            }
        }
        for (view, (id, generation)) in candidates {
            if self.embedded.contains_key(&view) {
                continue;
            }
            let result = (|| -> Result<(), String> {
                let site =
                    accesskit_windows::windowless_site(hwnd, id).map_err(|e| e.to_string())?;
                let provider = exact_ink::device::d3d11::fields::provider(view, &site)
                    .map_err(|e| e.to_string())?;
                accesskit_windows::embed_windowless(hwnd, id, Some(&provider))
                    .map_err(|e| e.to_string())
            })();
            match result {
                Ok(()) => {
                    self.embedded.insert(view, (id, generation));
                    if std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some() {
                        eprintln!("exact-windows: RichEdit UIA view {view} under {}", id.0);
                    }
                    window.request_redraw();
                }
                Err(error) => eprintln!("exact-windows: RichEdit UIA: {error}"),
            }
        }
    }
}

/// A raw popup uses the page's node mapping and AccessKit's HWND adapter.
pub(super) struct MenuAccess {
    adapter: accesskit_windows::SubclassingAdapter,
    snapshot: Rc<MenuSnapshot>,
}

struct MenuActivation(Rc<MenuSnapshot>);
impl accesskit::ActivationHandler for MenuActivation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        Some(self.0.tree())
    }
}
struct MenuActions(EventLoopProxy<Event>);
impl accesskit::ActionHandler for MenuActions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        let _ = self.0.send_event(Event::TrayAccess(request));
    }
}

impl MenuAccess {
    pub fn new(window: accesskit_windows::HWND, proxy: EventLoopProxy<Event>) -> Self {
        let snapshot = Rc::new(MenuSnapshot::default());
        Self {
            adapter: accesskit_windows::SubclassingAdapter::new(
                window,
                MenuActivation(snapshot.clone()),
                MenuActions(proxy),
            ),
            snapshot,
        }
    }

    pub fn update<D: DataSource>(
        &mut self,
        p: &Presenter<D>,
        menu: &StatusMenu,
        scale: f64,
    ) -> Option<accesskit_windows::QueuedEvents> {
        if !self.snapshot.is_open() {
            self.adapter.activate_for_listeners();
        }
        // WM_GETOBJECT can arrive during a window call. Its handler can read
        // the menu's last presented content without borrowing the presenter.
        if !self.snapshot.refresh(p, menu, scale) {
            return None;
        }
        self.adapter.update_if_active(|| self.snapshot.tree())
    }

    pub fn hide(&mut self) {
        self.snapshot.close();
        if let Some(events) = self.adapter.update_if_active(|| self.snapshot.tree()) {
            events.raise();
        }
    }

    pub fn action<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        request: accesskit::ActionRequest,
    ) {
        model::menu_action(p, request);
    }
}
