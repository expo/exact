//! The text editor's context menu, shown by Windows (`crate::edit_menu`)
//! once the frame that asked for it is drawn.
use super::*;

impl<D: DataSource + Default + 'static> App<D> {
    pub(super) fn show_edit_menu(&mut self) {
        let Some(menu) = self.edit_menu.take() else {
            return;
        };
        let (Some(window), Some(p)) = (&self.window, &mut self.presenter) else {
            return;
        };
        if !p.edit_menu_valid(&menu) {
            return;
        }
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let picked = crate::edit_menu::show(handle.hwnd.get(), &menu, self.scale as f32);
        p.advance(self.started.elapsed().as_secs_f64() * 1000.);
        crate::keyboard::sync(p, None);
        match picked {
            Ok(Some(action)) => p.edit_menu_action(&menu, action),
            Ok(None) => {}
            Err(error) => eprintln!("exact-windows: {error}"),
        }
        self.caret.activity(Instant::now());
        window.request_redraw();
    }
}
