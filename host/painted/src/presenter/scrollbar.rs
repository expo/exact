use super::*;
use crate::paint::scrollbar::contains;

impl<D: DataSource> Presenter<D> {
    /// Refresh Windows' automatic scrollbar visibility after returning to the app.
    #[cfg(windows)]
    pub fn refresh_scrollbar_preferences(&mut self) {
        if crate::com::ensure_mta().is_err() {
            self.set_scrollbars(true);
            return;
        }
        self.set_scrollbars(
            windows::UI::ViewManagement::UISettings::new()
                .and_then(|settings| settings.AutoHideScrollBars())
                .unwrap_or(true),
        );
    }
    /// Desktop scrollbars, with the person's automatic visibility preference.
    pub fn set_scrollbars(&mut self, auto_hide: bool) {
        self.brush.scrollbars.enabled = true;
        self.brush.scrollbars.auto_hide = auto_hide;
        self.dirty = true;
    }

    pub(super) fn scrollbar_pointer(&mut self, phase: i32, x: f32, y: f32) -> bool {
        if self.shape_surface_open() || !self.brush.scrollbars.enabled {
            return false;
        }
        if let Some((key, vertical, grab)) = self.brush.scrollbars.drag {
            let bar = self
                .brush
                .scrollbars
                .bars
                .iter()
                .find(|b| (b.key, b.vertical) == (key, vertical))
                .copied();
            if phase == 1 {
                self.brush.scrollbars.drag = None;
                self.brush.scrollbars.pressed = false;
            }
            if let Some(bar) = bar.filter(|b| {
                self.host
                    .kernel()
                    .node_by_key(key)
                    .is_some_and(|n| n.id == b.id)
                    && self.alert_allows(b.id)
                    && !self.host.route_visibility(b.id).1
            }) {
                let mut to = self.scroll_of(bar.id);
                let value = bar.scroll_offset(bar.destination(bar.coordinate(x, y), grab));
                if vertical {
                    to.1 = value;
                } else {
                    to.0 = value;
                }
                self.scroll_to(bar.id, to);
            } else {
                self.brush.scrollbars.drag = None;
            }
            self.dirty = true;
            return true;
        }
        if phase == 1 {
            return std::mem::take(&mut self.brush.scrollbars.pressed);
        }
        if phase != 0 {
            return false;
        }
        // A press that lands on an open popover is the popover's: no page
        // scrollbar under it takes it.
        let on_popover = self.alert.is_some()
            && !self.alert_inert()
            && self.hit(x, y).is_some_and(|h| self.alert_holds(h));
        let bar = self
            .brush
            .scrollbars
            .bars
            .iter()
            .rev()
            .find(|b| {
                self.alert_allows(b.id)
                    && (!on_popover || self.alert_holds(b.id))
                    && contains(b.track, x, y)
            })
            .copied();
        let Some(bar) = bar else {
            return false;
        };
        self.brush.scrollbars.pressed = true;
        if contains(bar.thumb, x, y) {
            self.brush.scrollbars.drag = Some((
                bar.key,
                bar.vertical,
                bar.coordinate(x, y) - bar.start(bar.thumb),
            ));
        } else {
            let direction = if bar.coordinate(x, y) < bar.start(bar.thumb) {
                -1.
            } else {
                1.
            };
            let mut to = self.scroll_of(bar.id);
            let value = bar.scroll_offset(bar.offset + direction * bar.length(bar.port));
            if bar.vertical {
                to.1 = value;
            } else {
                to.0 = value;
            }
            self.scroll_to(bar.id, to);
        }
        self.dirty = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(direction: &str) -> (Presenter<()>, ViewId) {
        let value = if direction == "rtl" {
            "אבגדהוזחטי "
        } else {
            "abcdefghij "
        }
        .repeat(12);
        let source = format!(
            "component App\n  view\n    input testId=\"editor\" direction=\"{direction}\" value=\"{value}\" width=200 height=64 padding=4 appearance=\"none\" overflow-x=\"auto\" font-size=16\n"
        );
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (300., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let k = p.host.kernel();
        let id = k.node_by_key(k.find_by_test_id("editor")[0]).unwrap().id;
        p.set_scrollbars(false);
        p.set_focus(Some(id), 0.);
        p.mark_field(id, &value, exact_runner::FieldSelection::caret(66), false);
        p.frame();
        let rect = p.rect_of(id).unwrap();
        p.press_field(id, rect.0 + 104., rect.1 + 32.);
        p.frame();
        (p, id)
    }

    #[test]
    fn single_line_scrollbar_thumb_marks_the_physical_edges() {
        for direction in ["ltr", "rtl"] {
            let (mut p, id) = input(direction);
            let max = p.brush.field_text[&id].scroll_max.0;
            for offset in [0., max] {
                p.scroll_to(id, (offset, 0.));
                p.frame();
                let bar = p.brush.scrollbars.bars[0];
                assert!(!bar.vertical);
                let at_right = (offset == max) != (direction == "rtl");
                let expected = bar.track.0
                    + if at_right {
                        bar.track.2 - bar.thumb.2
                    } else {
                        0.
                    };
                assert!(
                    (bar.thumb.0 - expected).abs() < 0.01,
                    "{direction}, offset={offset}"
                );
            }
        }
    }

    #[test]
    fn single_line_scrollbar_drag_and_track_click_move_text_in_physical_direction() {
        for (direction, sign) in [("ltr", 1.), ("rtl", -1.)] {
            let (mut p, id) = input(direction);
            let selection = p.field_selection(id);
            for dx in [40., -40.] {
                let bar = p.brush.scrollbars.bars[0];
                let before = p.caret_frame().unwrap();
                let offset = p.scroll_of(id).0;
                let x = bar.thumb.0 + bar.thumb.2 / 2.;
                let y = bar.thumb.1 + bar.thumb.3 / 2.;
                let to = x + dx / bar.max * (bar.track.2 - bar.thumb.2);
                p.pointer_down(x, y, 1.).unwrap();
                p.pointer_move(to, y, 2.).unwrap();
                p.pointer_up(to, y, 3.).unwrap();
                p.frame();
                assert!(
                    (p.scroll_of(id).0 - offset - sign * dx).abs() < 0.05,
                    "{direction}"
                );
                let after = p.caret_frame().unwrap();
                assert!((after.0 - before.0 + dx).abs() < 0.05, "{direction}");
            }
            for right in [true, false] {
                let bar = p.brush.scrollbars.bars[0];
                let offset = p.scroll_of(id).0;
                let x = if right {
                    bar.track.0 + bar.track.2 - 1.
                } else {
                    bar.track.0 + 1.
                };
                let y = bar.track.1 + bar.track.3 / 2.;
                p.pointer_down(x, y, 4.).unwrap();
                p.pointer_up(x, y, 5.).unwrap();
                p.frame();
                let delta = if right { bar.port.2 } else { -bar.port.2 };
                assert!(
                    (p.scroll_of(id).0 - offset - sign * delta).abs() < 0.05,
                    "{direction}"
                );
            }
            assert_eq!(p.field_selection(id), selection);
        }
    }

    #[test]
    fn a_wheel_ends_a_scrollbar_drag_without_reverting_its_scroll() {
        for vertical in [false, true] {
            for redraw in [false, true] {
                let source = r#"component App
  state presses = 0
  action press
    presses = presses + 1
  view
    column width=240 height=200 overflow="auto"
      button press=press width=1000 height=1000 flex-shrink=0
        text "A large row"
"#;
                let (mut p, error) = Presenter::boot_with(
                    &crate::test_plan(source),
                    (),
                    (240., 200.),
                    1.,
                    PathBuf::new(),
                    PainterChoice::Cpu,
                )
                .unwrap();
                assert!(error.is_none(), "{error:?}");
                p.set_scrollbars(false);
                p.frame();
                let bar = *p
                    .brush
                    .scrollbars
                    .bars
                    .iter()
                    .find(|b| b.vertical == vertical)
                    .unwrap();
                let (x, y) = (
                    bar.thumb.0 + bar.thumb.2 / 2.,
                    bar.thumb.1 + bar.thumb.3 / 2.,
                );
                p.pointer_down(x, y, 0.).unwrap();
                assert!(p.brush.scrollbars.drag.is_some());
                let offset = if vertical { (0., 40.) } else { (40., 0.) };
                p.wheel_at(x, y, offset.0, offset.1);
                assert_eq!(p.scroll_of(bar.id), offset);
                let dragging = p.brush.scrollbars.drag.is_some();
                let pressed = p.brush.scrollbars.pressed;
                if redraw {
                    p.frame();
                }
                p.pointer_up(x, y, 1.).unwrap();
                assert_eq!(
                    p.scroll_of(bar.id),
                    offset,
                    "vertical={vertical}, redraw={redraw}"
                );
                assert!(!dragging && !pressed);
                assert_eq!(
                    p.host.runner().slot("presses"),
                    Some(&exact_runner::Value::Number(0.))
                );
            }
        }
    }

    #[test]
    fn a_horizontal_track_leaves_the_corner_only_for_a_vertical_bar() {
        for (overflow_y, corner) in [("hidden", 0.), ("clip", 0.), ("auto", 12.)] {
            let source = format!(
                "component App\n  view\n    column width=240 height=200 overflow-x=\"auto\" overflow-y=\"{overflow_y}\"\n      column width=1000 height=1000 flex-shrink=0\n"
            );
            let (mut p, error) = Presenter::boot_with(
                &crate::test_plan(&source),
                (),
                (240., 200.),
                1.,
                PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.set_scrollbars(false);
            p.frame();
            let bars = &p.brush.scrollbars.bars;
            let bar = bars.iter().find(|b| !b.vertical).unwrap();
            assert_eq!(bars.len(), if corner > 0. { 2 } else { 1 }, "{overflow_y}");
            assert_eq!(bar.track.2, bar.port.2 - 4. - corner, "{overflow_y}");
        }
    }

    #[test]
    fn desktop_scrollbars_drag_page_and_cancel_without_pressing_content() {
        let source = r#"component App
  state presses = 0
  action press
    presses = presses + 1
  view
    column width=240 height=200 overflow-y="auto" testId="list"
      button press=press width=240 height=1000 flex-shrink=0
        text "A long row"
"#;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (240., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none());
        p.set_scrollbars(false);
        p.frame();
        let bar = p.brush.scrollbars.bars[0];
        assert!(bar.vertical);
        assert!(bar.thumb.3 < bar.track.3);
        let (x, y) = (
            bar.thumb.0 + bar.thumb.2 / 2.,
            bar.thumb.1 + bar.thumb.3 / 2.,
        );
        p.pointer_down(x, y, 0.).unwrap();
        p.pointer_move(x, bar.track.1 + bar.track.3, 1.).unwrap();
        p.frame();
        p.pointer_up(x, bar.track.1 + bar.track.3, 2.).unwrap();
        assert_eq!(p.scroll_of(bar.id).1, bar.max);
        assert_eq!(
            p.host.runner().slot("presses"),
            Some(&exact_runner::Value::Number(0.))
        );
        p.frame();
        let bar = p.brush.scrollbars.bars[0];
        p.pointer_down(x, bar.track.1 + 1., 3.).unwrap();
        p.pointer_up(x, bar.track.1 + 1., 4.).unwrap();
        assert_eq!(p.scroll_of(bar.id).1, (bar.max - bar.port.3).max(0.));
        assert_eq!(
            p.host.runner().slot("presses"),
            Some(&exact_runner::Value::Number(0.))
        );
        p.frame();
        let bar = p.brush.scrollbars.bars[0];
        p.pointer_down(x, bar.thumb.1 + 2., 5.).unwrap();
        p.pointer_cancel(6.).unwrap();
        assert!(p.brush.scrollbars.drag.is_none());
    }
}
