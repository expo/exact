//! The window's app modes (LLP 1121 D5): the glue's `prefer mode`.

use super::Bridge;
use exact_runner::DataSource;

impl<D: DataSource> Bridge<D> {
    /// App modes, `axis=value` pins comma-separated in the input buffer
    /// (LLP 1121 D5), set in one batch; after a `?`, only checked.
    pub fn set_mode(&mut self, len: usize) -> u32 {
        let Ok(text) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return self.refuse("mode: invalid UTF-8");
        };
        // `?` first: only ask whether they would be taken.
        let (check, text) = match text.strip_prefix('?') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let pins: Option<Vec<(String, String)>> = text
            .split(',')
            .filter(|pin| !pin.is_empty())
            .map(|pin| {
                pin.split_once('=')
                    .map(|(a, v)| (a.to_owned(), v.to_owned()))
            })
            .collect();
        let Some(pins) = pins else {
            return self.refuse("mode: write axis=value pins");
        };
        let pins: Vec<(&str, &str)> = pins.iter().map(|(a, v)| (a.as_str(), v.as_str())).collect();
        let out = self.host.as_mut().map_or_else(
            || exact_runner::agent::error("not booted"),
            |h| match check {
                true => h.check_modes(&pins),
                false => h.set_modes(&pins),
            },
        );
        self.emit(out)
    }
}

impl<D: DataSource> Bridge<D> {
    /// The facts a development page holds over the app's own (LLP 1122 T7,
    /// theme2's F2 ruling), in the input buffer: `<flags>:<pins>` holds the
    /// scheme (`s` among the flags), the root size (`r`) and each pinned
    /// axis (`axis=value`, comma-separated); `-` hands them back, in one
    /// batch; `?` asks only what the app asked. Every answer says that.
    pub fn hold_facts(&mut self, len: usize) -> u32 {
        let Ok(text) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return self.refuse("hold: invalid UTF-8");
        };
        let text = text.to_owned();
        let out = self.host.as_mut().map_or_else(
            || exact_runner::agent::error("not booted"),
            |h| h.hold_facts(&text),
        );
        self.emit(out)
    }

    /// A `tamber-batch` or `tamber-snapshot` frame from the dev stream in the
    /// input buffer (LLP 1121 stage 4): applied in place; returns the batch.
    pub fn design_rows(&mut self, len: usize) -> u32 {
        let Ok(text) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return self.refuse("live: invalid UTF-8");
        };
        let text = text.to_owned();
        let out = self.host.as_mut().map_or_else(
            || exact_runner::agent::error("not booted"),
            |h| h.apply_live(&text),
        );
        self.emit(out)
    }
}

/// The app modes' exports, invoked inside [`crate::host!`].
#[macro_export]
macro_rules! mode_exports {
    () => {
        /// App modes, `axis=value` pins in the input buffer (LLP 1121 D5);
        /// returns the batch length.
        #[no_mangle]
        pub extern "C" fn exact_set_mode(len: u32) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().set_mode(len as usize))
        }

        /// The facts a development page holds over the app's own (LLP 1122
        /// T7) in the input buffer; returns the answer's length.
        #[no_mangle]
        pub extern "C" fn exact_hold_facts(len: u32) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().hold_facts(len as usize))
        }

        /// A live design batch or snapshot in the input buffer (LLP 1121
        /// stage 4); returns the batch length. Development builds only.
        #[no_mangle]
        pub extern "C" fn exact_design_rows(len: u32) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().design_rows(len as usize))
        }
    };
}

impl<D: DataSource> Bridge<D> {
    /// Whether the loaded plan opts this surface name into performance reporting.
    pub fn canvas_performance_requested(&self, len: usize) -> u32 {
        let Ok(name) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return 0;
        };
        u32::from(
            self.host
                .as_ref()
                .is_some_and(|h| h.runner().wants_canvas_performance(name)),
        )
    }

    /// Publish or clear a named canvas's performance record.
    pub fn canvas_performance_record(&mut self, len: usize) -> u32 {
        let Ok(text) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return self.refuse("canvas performance: invalid UTF-8");
        };
        let (name, json) = text
            .split_once('\0')
            .map_or((text, None), |(name, json)| (name, Some(json)));
        let out = self.host.as_mut().map_or_else(
            || exact_runner::agent::error("not booted"),
            |h| h.canvas_performance_record(name, json),
        );
        self.emit(out)
    }
}

impl<D: DataSource> Bridge<D> {
    /// Document safe-area insets and row-major segment rectangles, in CSS pixels.
    pub fn viewport_geometry(&mut self, len: usize) -> u32 {
        let Ok((insets, rects)) = serde_json::from_slice::<([f32; 4], Vec<[f32; 4]>)>(
            &self.input[..len.min(self.input.len())],
        ) else {
            return self.refuse("viewport geometry: expected [insets, rectangles]");
        };
        let out = self.host.as_mut().map_or_else(
            || exact_runner::agent::error("viewport geometry: not booted"),
            |host| host.viewport_geometry(insets, rects),
        );
        self.emit(out)
    }
}
