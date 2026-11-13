//! The two lengths the presenter tells the host that no row of the tree
//! says: the root font size `rem` follows (LLP 1069.000 D3), and the
//! display's scale (LLP 1115.000 D10.3), the pixels a point this host draws
//! at, which a hairline (`-exact-hairline`) is one device pixel of. The
//! presenter says the scale at boot and whenever it changes
//! (`Presenter::resize_scaled`): a desktop's `EXACT_SCALE` (1 when unset),
//! the density an Android reader was told, a Windows window's scale factor.
use super::*;

impl<D: DataSource> Host<D> {
    pub(crate) fn canvas_performance_record(
        &mut self,
        name: &str,
        json: Option<&str>,
    ) -> Option<String> {
        let answer = self.runner.set_canvas_performance(name, json);
        self.told(answer, "canvas performance")
    }

    /// A live design batch or snapshot frame from the dev server (LLP 1121
    /// D13, stage 4), set in place in one commit: `None` when applied or
    /// already this build's design, else why not (`stale`: fetch the
    /// snapshot); whether the wide colour table refused a colour, which
    /// the presenter reports `unsupported` (LLP 1122 D4); and whether the
    /// session's role overrides moved (LLP 1121 stage 5): what was painted
    /// from them re-targets and is painted again.
    pub fn apply_live(&mut self, frame: &str) -> (Option<String>, bool, bool) {
        let Some(edit) = exact_runner::design::Edit::parse(frame) else {
            return (
                Some("live: not a tamber batch or snapshot".into()),
                false,
                false,
            );
        };
        match self.runner.apply_live(&edit) {
            Ok(applied) => {
                // A scrub's frame shows with no transition (LLP 1121 D13).
                let _still = edit.scrub.is_some().then(exact_kernel::motion::still);
                let error = self.commit(
                    &[Timed {
                        at_ms: self.now_ms,
                        receipt: applied.receipt,
                    }],
                    None,
                );
                if applied.roles {
                    self.colors_changed(false);
                }
                (error, applied.full, applied.roles)
            }
            Err(exact_runner::LiveRefusal::Stale) => (Some("stale".into()), false, false),
            Err(exact_runner::LiveRefusal::Structure) => (Some("structure".into()), false, false),
            Err(exact_runner::LiveRefusal::Refused(why)) => {
                (Some(format!("refused: {why}")), false, false)
            }
        }
    }

    /// The root font size `rem` follows (LLP 1069.000 D3): a relayout in one
    /// commit, no resource asked again.
    pub fn set_root_font_size(&mut self, px: f64) -> Option<String> {
        match self.runner.set_root_font_size(px) {
            Ok(Some(receipt)) => {
                self.presence.unauthored = true;
                self.commit(
                    &[Timed {
                        at_ms: self.now_ms,
                        receipt,
                    }],
                    None,
                )
            }
            Ok(None) => None,
            Err(e) => Some(format!("root font size: {e:?}")),
        }
    }

    /// What a window's change brings at once, each part when given: the
    /// viewport, the safe-area insets `env(safe-area-inset-*)` resolves to
    /// and the software keyboard's overlap, answered to `exactViewport()`'s
    /// readers in one commit with one layout (LLP 1039 D9), so no reader sees
    /// the keys without the viewport they shrank, or a rotation's insets
    /// with the size before it. The readers are asked whether or not a style
    /// reads the environment. Whether anything changed; a size given always
    /// lays out, as a resize does. A refused change leaves the kernel's
    /// environment as it was.
    pub fn set_view(
        &mut self,
        size: Option<(f32, f32)>,
        insets: Option<[f32; 4]>,
        keyboard: Option<f32>,
    ) -> Result<bool, String> {
        let before = self.runner.kernel().env();
        let mut restyled = false;
        if let Some([top, right, bottom, left]) = insets {
            let env = before.with_insets(top, right, bottom, left);
            restyled =
                (self.runner.kernel_mut().set_env(env)).map_err(|e| format!("safe area: {e:?}"))?;
        }
        let told = size.map(|(w, h)| (f64::from(w), f64::from(h)));
        let receipt = match self.runner.set_view(told, keyboard.map(f64::from)) {
            Ok(receipt) => receipt,
            Err(e) => {
                if restyled {
                    let _ = self.runner.kernel_mut().set_env(before);
                    let _ = self.layout_unauthored();
                }
                return Err(format!("viewport: {e:?}"));
            }
        };
        if let Some(size) = size {
            self.viewport = size;
            self.presence.snap = true;
        }
        if let Some(receipt) = receipt {
            self.presence.unauthored |= restyled;
            let at_ms = self.now_ms;
            return (self.commit(&[Timed { at_ms, receipt }], None)).map_or(Ok(true), Err);
        }
        if size.is_some() {
            let laid_out = self.layout().err();
            self.observe_layout();
            return laid_out.map_or(Ok(true), Err);
        }
        match restyled {
            true => self.layout_unauthored().map(|_| true),
            false => Ok(false),
        }
    }

    /// Tell the runner the display's scale: a relayout in one commit where a
    /// row is a hairline, nothing where none is.
    pub fn set_display_scale(&mut self, scale: f64) -> Option<String> {
        match self.runner.set_display_scale(scale) {
            Ok(Some(receipt)) => {
                self.presence.unauthored = true;
                self.commit(
                    &[Timed {
                        at_ms: self.now_ms,
                        receipt,
                    }],
                    None,
                )
            }
            Ok(None) => None,
            Err(e) => Some(format!("display scale: {e:?}")),
        }
    }

    /// App modes (LLP 1121 D5, the agent's `prefer mode`): the window's
    /// slice set again in one commit, which paints.
    pub fn set_modes(&mut self, pins: &[(&str, &str)]) -> Option<String> {
        match self.runner.set_modes(pins) {
            Ok(Some(receipt)) => self.commit(
                &[Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => None,
            Err(e) => Some(format!("mode: {e:?}")),
        }
    }
}
