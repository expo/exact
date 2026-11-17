//! Status items (LLP 1133.025 D4) on a host that places them outside the
//! window: Windows' notification area. The host reads each
//! `-exact-menu="status"` button as a [`StatusItem`], draws its icon from
//! the button's image ([`Presenter::status_icon`]), and on a click presses
//! the button or opens its menu.
//!
//! The menu is the button's `popovertarget`, read as any menu popover is,
//! and painted as the window's MenuFlyout (LLP 1115.004 D10.1), but in a
//! window the host makes for it at the pointer, since the page's window may
//! be hidden: the surface is open as ever (one at a time, the page inert,
//! a row's choice pressing the row), only placed nowhere in the page. The
//! host paints it from [`Presenter::status_menu_pixels`] and hands back its
//! pointer and keys in the menu's own points. When the host asks, the menu
//! ends with a separator and Exit (D7), unless the root's
//! `-exact-menu-remove` names `status-quit`; Exit closes the menu and
//! leaves the rest to the host ([`Presenter::take_status_exit`]).
use super::*;

/// The node an Exit row stands for: none of the tree's.
pub const EXIT: ViewId = ViewId::MAX;

/// The status menu open or opening: its popover, the room its host's window
/// has (the work area, points), and whether it ends with Exit.
#[derive(Clone)]
pub(super) struct Menu {
    root: NodeKey,
    pub(super) room: (f32, f32),
    exit: bool,
}

/// A `-exact-menu="status"` button as its host places it.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusItem {
    /// The button.
    pub node: ViewId,
    /// What keeps the item's place across launches and updates, as the
    /// Mac's `autosaveName`: its `id`, else its `testId`; `None` with
    /// neither.
    pub name: Option<String>,
    /// Its `aria-label`, else its text: the item's tooltip.
    pub label: String,
    /// `disabled`: dimmed, with no press and no menu.
    pub disabled: bool,
    /// It has a `press`.
    pub press: bool,
    /// Its `popovertarget` names a menu-shaped popover.
    pub menu: bool,
    /// The `img` or `svg` its image is drawn from.
    pub image: Option<ViewId>,
}

/// The status menu as its host's window shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusMenu {
    /// Names this opening.
    pub serial: u32,
    /// Moves on whenever its picture changes.
    pub revision: u32,
    /// Its size, points.
    pub size: (f32, f32),
}

impl<D: DataSource> Presenter<D> {
    /// This host places status items: each status button leaves the page.
    pub fn place_status_items(&mut self) {
        if let Err(error) = self.host.place_status_items() {
            self.host.log(error);
        }
        self.dirty = true;
    }

    /// The status items as the tree now stands, in tree order.
    pub fn status_items(&self) -> Vec<StatusItem> {
        let kernel = self.host.kernel();
        let runner = self.host.runner();
        let items = exact_runner::desktop::buttons(runner, "status").into_iter();
        items
            .filter_map(|id| {
                let node = kernel.node(id)?;
                let name = [PropId::Id, PropId::TestId]
                    .into_iter()
                    .find_map(|prop| node.props.str(prop).filter(|s| !s.is_empty()))
                    .map(str::to_owned);
                Some(StatusItem {
                    node: id,
                    name,
                    label: exact_runner::desktop::label(kernel, id),
                    disabled: node.props.bool(PropId::Disabled) == Some(true),
                    press: runner.handlers_of(id).contains(&EventKind::Press),
                    menu: self.status_popover(id).is_some(),
                    image: exact_runner::desktop::image_node(kernel, id),
                })
            })
            .collect()
    }

    /// The menu-shaped popover status button `id`'s `popovertarget` names.
    fn status_popover(&self, id: ViewId) -> Option<NodeKey> {
        let kernel = self.host.kernel();
        let name = kernel.node(id)?.props.str(PropId::Popovertarget)?;
        let key = self.element(name).filter(|_| !name.is_empty())?;
        let pop = kernel.node_by_key(key)?.id;
        matches!(self.surface_plan(pop, false), Some(Plan::Menu(_))).then_some(key)
    }

    /// Whether the root's `-exact-menu-remove` names `status-quit`.
    fn status_quit_removed(&self) -> bool {
        let kernel = self.host.kernel();
        let root = self
            .host
            .runner()
            .roots()
            .first()
            .and_then(|id| kernel.node(*id));
        root.and_then(|n| n.props.str(PropId::MenuRemove))
            .is_some_and(|list| list.split_whitespace().any(|id| id == "status-quit"))
    }

    /// A status menu ends with Exit when its host asked for it.
    pub(super) fn with_exit(&self, pop: ViewId, plan: Option<Plan>) -> Option<Plan> {
        let status = self.top.status.as_ref().filter(|s| s.exit);
        let root = status.and_then(|s| self.host.kernel().node_by_key(s.root));
        match plan {
            Some(Plan::Menu(mut entries)) if root.is_some_and(|n| n.id == pop) => {
                if !entries.is_empty() {
                    entries.push(Entry::Separator);
                }
                entries.push(Entry::Item(Row {
                    node: EXIT,
                    title: "Exit".into(),
                    direction: exact_kernel::Direction::Ltr,
                    symbol: None,
                    destructive: false,
                    enabled: true,
                    cancel: false,
                    checked: false,
                    opens: None,
                }));
                Some(Plan::Menu(entries))
            }
            plan => plan,
        }
    }

    /// Status button `id`'s own `press`, as a click on its item runs it.
    pub fn status_press(&mut self, id: ViewId, now_ms: f64) {
        self.press_point = None;
        self.dispatch_press(id, now_ms, true);
    }

    /// Open status button `id`'s menu in its host's window, whose screen
    /// has `room` (points) for it, ending with Exit when `exit` (D7).
    /// Opened from the keyboard, its first row takes the focus, as WinUI's
    /// MenuFlyout gives it. Whether it opened.
    pub fn open_status_menu(
        &mut self,
        id: ViewId,
        room: (f32, f32),
        exit: bool,
        keyboard: bool,
        now_ms: f64,
    ) -> bool {
        let Some(root) = self.status_popover(id) else {
            return false;
        };
        let exit = exit && !self.status_quit_removed();
        self.top.status = Some(Menu { root, room, exit });
        let name = self.name_of(root);
        self.present(Some(root), &name, None, None, false, now_ms);
        let opened = self.top.open.as_ref().is_some_and(|open| open.detached);
        if !opened {
            self.top.status = None;
        } else if keyboard {
            self.surface_step(false);
        }
        opened
    }

    /// The status menu, if one is open, read again from the tree as it now
    /// stands (a checkbox row's state, a row gone).
    pub fn status_menu(&mut self) -> Option<StatusMenu> {
        self.top.open.as_ref().filter(|open| open.detached)?;
        self.sync_surface();
        let open = self.top.open.as_ref().filter(|open| open.detached)?;
        Some(StatusMenu {
            serial: open.serial,
            revision: open.revision,
            size: open.layout.size,
        })
    }

    /// The status menu's picture at `scale` device pixels a point.
    pub fn status_menu_pixels(&mut self, scale: f32) -> Option<Result<Pixmap, String>> {
        let open = self.top.open.as_ref().filter(|open| open.detached)?;
        let mut raster = Raster::new();
        raster.page([0; 4]);
        raster.begin(open.layout.size.0, open.layout.size.1, scale);
        let feedback = look::Feedback {
            pressed: open.pressed,
            focused: open.focused,
            hovered: open.hovered,
            ripple: None,
        };
        open.layout
            .paint(&mut raster, &mut self.text.borrow_mut(), feedback);
        Some(raster.finish())
    }

    /// A pointer in the status menu's window (0 down, 1 up, 2 move), at the
    /// menu's own points.
    pub fn status_menu_pointer(&mut self, phase: i32, x: f32, y: f32, now_ms: f64) {
        if self.top.open.as_ref().is_some_and(|open| open.detached) {
            self.menu_pointer(phase, x, y, now_ms);
        }
    }

    /// The pointer left the status menu's window.
    pub fn status_menu_left(&mut self) {
        self.overlay_pointer_cancel();
    }

    /// The wheel over the status menu's window, points down.
    pub fn status_menu_wheel(&mut self, delta: f32) {
        if self.top.open.as_ref().is_some_and(|open| open.detached) {
            self.overlay_scroll(delta);
        }
    }

    /// A key in the status menu's window, by `KeyboardEvent.key`'s name.
    pub fn status_menu_key(&mut self, name: &str, now_ms: f64) {
        if self.top.open.as_ref().is_some_and(|open| open.detached) {
            self.surface_key(name, now_ms);
        }
    }

    /// The status menu's window lost the person (a click outside, another
    /// window activated): the menu closes, running nothing.
    pub fn dismiss_status_menu(&mut self, now_ms: f64) {
        if self.top.open.as_ref().is_some_and(|open| open.detached) {
            self.dismiss_surface(now_ms);
        }
    }

    /// Whether the person chose Exit since the last call.
    pub fn take_status_exit(&mut self) -> bool {
        std::mem::take(&mut self.top.exited)
    }

    /// Status item `image`'s picture (the `img` or `svg` of
    /// [`StatusItem::image`]) for a square of `side` device pixels at `scale`
    /// device pixels a point, on a taskbar `dark` or light. A symbol
    /// (`symbol:`) is drawn in the taskbar's primary text colour, as the
    /// system draws its own glyphs there; an `svg` is drawn as authored, its
    /// `currentColor` and its colours' light and dark forms the taskbar's.
    /// `None` for an image this host cannot draw yet (an `img` of a file).
    pub fn status_icon(
        &mut self,
        image: ViewId,
        side: u32,
        scale: f32,
        dark: bool,
    ) -> Option<Result<Pixmap, String>> {
        let kernel = self.host.kernel();
        let node = kernel.node(image)?;
        let css = side as f32 / scale;
        if node.node_type == NodeType::Svg {
            let scene = crate::paint::resolve_with(
                kernel,
                &node,
                (0.0, 0.0, css, css),
                &|id| self.host.presented(id),
                &|id| self.host.presented_path(id),
                &|id| self.host.presented_filter(id),
                dark,
            );
            return Some(crate::paint::Painter::svg_picture(
                self.text.clone(),
                &scene,
                (css, css),
                scale,
                dark,
            ));
        }
        let source = node.props.str(PropId::ImageSource)?;
        let role = source
            .strip_prefix("symbol:")
            .filter(|_| crate::image::is_symbol(source))?;
        // WinUI's TextFillColorPrimary, dark and light.
        let colour = if dark {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 228]
        };
        let face = node.computed_style(exact_kernel::StyleMask::of(
            exact_kernel::StyleId::FontWeight,
        ));
        Some(crate::paint::Painter::symbol_picture(
            self.text.clone(),
            role,
            &face,
            css,
            scale,
            colour,
        ))
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
