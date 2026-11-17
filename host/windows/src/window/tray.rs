//! The notification area (`crate::tray`, LLP 1133.025 D4) in the loop's
//! turn: what its windows heard goes to the presenter before the turn's
//! work, and after it the icons and the menu's window follow the tree.
//! A measured run places none.
use super::*;
use crate::tray::placement::Placement;
use crate::tray::{self, Does, Flyout, Heard, Icon, Input, Op, Shell, Shown};
use exact_painted::presenter::top::StatusItem;

/// What an icon shows: its tooltip and its pixels.
type Picture = ([u16; 128], Vec<u8>, u32);

pub(super) struct Tray {
    shell: Option<Shell>,
    flyout: Option<Flyout>,
    /// Each icon the shell shows, as it was last told.
    placed: Vec<(Icon, Picture)>,
    /// Each item as of the last sync, by its icon's id.
    items: Vec<(u16, StatusItem)>,
    /// The kernel epoch and accepted pictures last drawn; `None` draws them again.
    epoch: Option<(u64, Vec<(u32, usize)>)>,
    sizes: Vec<u32>,
    dark: bool,
    /// The app's icon at a side, for an item with no image it can draw.
    app_icon: Option<(u32, Option<Vec<u8>>)>,
    /// The menu's anchor (screen pixels) and scale, and the opening and
    /// revision last painted.
    anchor: (Placement, f32),
    painted: Option<(u32, u32)>,
    /// The app's `id`, which names the icons' GUIDs in a production build.
    guid_owner: Option<String>,
    said_guid: bool,
}

impl Tray {
    fn new(
        name: &str,
        guid_owner: Option<String>,
        proxy: &EventLoopProxy<crate::accessibility::Event>,
    ) -> Tray {
        let shell = Shell::new(name);
        let flyout = Flyout::new(proxy.clone());
        let wake = |proxy: EventLoopProxy<_>| {
            move || drop(proxy.send_event(crate::accessibility::Event::Wake))
        };
        if let Some(shell) = &shell {
            shell.set_wake(wake(proxy.clone()));
        }
        if let Some(flyout) = &flyout {
            flyout.set_wake(wake(proxy.clone()));
        }
        Tray {
            shell,
            flyout,
            placed: Vec::new(),
            items: Vec::new(),
            epoch: None,
            sizes: Vec::new(),
            dark: tray::shell::taskbar_dark(),
            app_icon: None,
            anchor: (Placement::default(), 1.),
            painted: None,
            guid_owner,
            said_guid: false,
        }
    }

    fn app_icon(&mut self, side: u32) -> Option<Vec<u8>> {
        if self.app_icon.as_ref().is_none_or(|(s, _)| *s != side) {
            self.app_icon = Some((side, tray::shell::app_icon(side)));
        }
        self.app_icon.as_ref().and_then(|(_, icon)| icon.clone())
    }
}

impl<D: DataSource + Default + 'static> App<D> {
    /// Once the presenter is made: the icons' windows, and every status
    /// button out of the page before its first frame.
    pub(super) fn tray_start(&mut self) {
        let Some(p) = self
            .presenter
            .as_mut()
            .filter(|_| self.frame_limit.is_none())
        else {
            return;
        };
        p.place_status_items();
        let guid_owner = tray::guid_owner(&self.config.compat);
        self.tray = Some(Tray::new(&self.name, guid_owner, &self.proxy));
    }

    /// What the icons and the menu heard: a click presses its button or
    /// opens its menu, and the menu's pointer and keys reach it. Exit ends
    /// the app as the person's close of a window that does not hide would.
    pub(super) fn tray_input(&mut self, events: &ActiveEventLoop) {
        let (Some(tray), Some(p)) = (&mut self.tray, &mut self.presenter) else {
            return;
        };
        let heard = tray.shell.as_ref().map(Shell::take).unwrap_or_default();
        let inputs = tray.flyout.as_ref().map(Flyout::take).unwrap_or_default();
        if heard.is_empty() && inputs.is_empty() {
            return;
        }
        p.advance(self.started.elapsed().as_secs_f64() * 1000.);
        let now = p.host().now();
        for heard in heard {
            match heard {
                Heard::TaskbarCreated => {
                    if let Some(shell) = &mut tray.shell {
                        shell.forget();
                    }
                    tray.placed.clear();
                    tray.epoch = None;
                }
                Heard::Dpi => tray.epoch = None,
                Heard::Theme => {
                    tray.dark = tray::shell::taskbar_dark();
                    tray.epoch = None;
                }
                Heard::Callback(heard) => {
                    let Some((_, item)) = tray.items.iter().find(|(id, _)| *id == heard.icon)
                    else {
                        continue;
                    };
                    match tray::does(heard.event, item.press, item.menu, item.disabled) {
                        Does::Press => p.status_press(item.node, now),
                        Does::Menu { keyboard } => {
                            let icon = tray.shell.as_ref().and_then(|shell| shell.rect(heard.icon));
                            let (anchor, scale) = Flyout::anchor(icon, heard.at);
                            let (width, height) = anchor.area.size();
                            let room = (width as f32 / scale, height as f32 / scale);
                            // The menu ends with Exit whenever an icon is
                            // placed, as this one is (D7).
                            if p.open_status_menu(item.node, room, true, keyboard, now) {
                                tray.anchor = (anchor, scale);
                            }
                        }
                        Does::Nothing => {}
                    }
                }
            }
        }
        let scale = tray.anchor.1;
        for input in inputs {
            match input {
                Input::Pointer(phase, x, y) => {
                    p.status_menu_pointer(phase, x / scale, y / scale, now)
                }
                Input::Left => p.status_menu_left(),
                Input::Wheel(delta) => p.status_menu_wheel(delta),
                Input::Key(name) => p.status_menu_key(&name, now),
                Input::Dismissed => p.dismiss_status_menu(now),
            }
        }
        if p.take_status_exit() {
            self.exit_by_person(events);
        }
    }

    pub(super) fn tray_access(
        &mut self,
        events: &ActiveEventLoop,
        request: accesskit::ActionRequest,
    ) {
        if let (Some(tray), Some(p)) = (&mut self.tray, &mut self.presenter) {
            p.advance(self.started.elapsed().as_secs_f64() * 1000.);
            if let Some(flyout) = &mut tray.flyout {
                flyout.access_action(p, request);
            }
        }
        if self
            .presenter
            .as_mut()
            .is_some_and(|p| p.take_status_exit())
        {
            self.exit_by_person(events);
        }
        self.about_to_wait(events);
    }

    /// The icons and the menu's window as the tree now stands.
    pub(super) fn tray_sync(&mut self) {
        let (Some(tray), Some(p)) = (&mut self.tray, &mut self.presenter) else {
            return;
        };
        // A hidden main window never draws, but an image worker still wakes this loop.
        if self.hidden {
            p.poll_images();
        }
        let status = p.status_items();
        let metrics: Vec<_> = status
            .iter()
            .map(|item| {
                let name = item.name.clone().unwrap_or_else(|| item.node.to_string());
                let existing = tray
                    .items
                    .iter()
                    .find(|(_, old)| old.node == item.node)
                    .map(|(id, _)| *id);
                let id = existing.unwrap_or_else(|| tray::id(&name, &[]));
                tray::shell::icon_side(tray.shell.as_ref().and_then(|shell| shell.rect(id)))
            })
            .collect();
        let images = status
            .iter()
            .zip(&metrics)
            .filter_map(|(item, (side, _))| item.image.map(|id| (id, *side)))
            .collect::<Vec<_>>();
        let revision = p.sync_status_images(&images);
        let epoch = (p.host().kernel().epoch(), revision);
        let sizes = metrics.iter().map(|(side, _)| *side).collect::<Vec<_>>();
        if tray.epoch.as_ref() != Some(&epoch) || tray.sizes != sizes {
            tray.epoch = Some(epoch);
            tray.sizes = sizes;

            let mut items: Vec<(u16, StatusItem)> = Vec::new();
            let mut wanted = Vec::new();
            for (item, (side, scale)) in status.into_iter().zip(metrics) {
                let taken: Vec<u16> = items.iter().map(|(id, _)| *id).collect();
                let name = item.name.clone().unwrap_or_else(|| item.node.to_string());
                let id = tray::id(&name, &taken);
                // A GUID needs a name an update keeps, and one no earlier
                // item has.
                let named_before = |name: &String| {
                    (items.iter()).any(|(_, earlier)| earlier.name.as_ref() == Some(name))
                };
                let guid = (tray.guid_owner.as_deref())
                    .zip(item.name.as_ref().filter(|name| !named_before(name)))
                    .map(|(app, name)| tray::guid(app, name));
                let icon = Icon { id, guid };
                let drawn = item
                    .image
                    .and_then(|image| p.status_icon(image, side, scale, tray.dark));
                let drawn = match drawn {
                    Some(Ok(pixmap)) => Some(pixmap.data().to_vec()),
                    Some(Err(error)) => {
                        p.host_mut()
                            .log(format!("status item: its image did not draw: {error}"));
                        None
                    }
                    None => None,
                };
                let pixels = drawn.or_else(|| tray.app_icon(side)).unwrap_or_default();
                wanted.push((
                    icon,
                    (
                        tray::tip(&item.label),
                        tray::icon_bgra(&pixels, item.disabled),
                        side,
                    ),
                ));
                items.push((id, item));
            }
            tray.items = items;
            let Some(shell) = &mut tray.shell else {
                return;
            };
            let mut refused = Vec::new();
            for op in tray::ops(&tray.placed, &wanted) {
                match op {
                    Op::Delete(icon) => shell.remove(icon),
                    Op::Add(icon) | Op::Modify(icon) => {
                        let (_, (tip, pixels, side)) =
                            wanted.iter().find(|(w, _)| *w == icon).unwrap();
                        let shown = tray::shell::icon(pixels, *side)
                            .map_or(Shown::Refused, |hicon| shell.show(icon, *tip, hicon));
                        match shown {
                            Shown::AsAsked => {
                                if matches!(op, Op::Add(_)) {
                                    let _ =
                                        self.proxy.send_event(crate::accessibility::Event::Wake);
                                }
                            }
                            Shown::WithoutGuid => {
                                if !std::mem::replace(&mut tray.said_guid, true) {
                                    p.host_mut().log("status item: the notification area refused the icon's GUID, which Windows binds to the executable's path; it is known by its number, and the person's choice to show it may not carry over (LLP 1133.025 §14)");
                                }
                            }
                            Shown::Refused => {
                                p.host_mut().log(format!(
                                    "status item: the notification area refused icon {}",
                                    icon.id
                                ));
                                refused.push(icon);
                            }
                        }
                    }
                }
            }
            // An icon the shell refused is tried again at the next change.
            tray.placed = (wanted.into_iter())
                .filter(|(icon, _)| !refused.contains(icon))
                .collect();
        }
        let Some(flyout) = &mut tray.flyout else {
            return;
        };
        let Some(menu) = p.status_menu() else {
            flyout.hide();
            flyout.sync_access(p, f64::from(tray.anchor.1));
            tray.painted = None;
            return;
        };
        if tray.painted == Some((menu.serial, menu.revision)) {
            flyout.sync_access(p, f64::from(tray.anchor.1));
            return;
        }
        let (at, scale) = tray.anchor;
        match p.status_menu_pixels(scale) {
            Some(Ok(pixmap)) => {
                flyout.show(
                    menu.serial,
                    at,
                    pixmap.data(),
                    (pixmap.width(), pixmap.height()),
                );
                tray.painted = Some((menu.serial, menu.revision));
            }
            Some(Err(error)) => p.host_mut().log(format!("status menu: {error}")),
            None => {}
        }
        flyout.sync_access(p, f64::from(scale));
    }

    /// The status menu's Exit: `beforeunload` is asked, as for a window's
    /// close, and its question shows in the window; otherwise the app ends.
    fn exit_by_person(&mut self, events: &ActiveEventLoop) {
        let stays = self.presenter.as_mut().is_some_and(|p| {
            p.advance(self.started.elapsed().as_secs_f64() * 1000.);
            let now = p.host().now();
            !p.before_unload(now)
        });
        if stays {
            return self.bring_forward();
        }
        if let Some(p) = &mut self.presenter {
            p.end_hatches();
            p.end_packages();
        }
        events.exit();
    }
}
