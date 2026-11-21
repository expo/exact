//! Painted editors scroll their value independently of the surrounding page.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// The focused insertion point in viewport coordinates, clipped to its field.
    pub fn caret_frame(&self) -> Option<Rect4> {
        let id = self.focus?;
        let selection = self.field_selection(id);
        if selection.start != selection.end || self.shape_surface_open() {
            return None;
        }
        #[cfg(windows)]
        if let Some([x, y, w, h]) = exact_ink::device::d3d11::fields::caret(id) {
            let s = self.brush.scale;
            return Some((x / s, y / s, w / s, h / s));
        }
        if self.ime_paint().is_some_and(|c| !c.cursor) {
            return None;
        }
        self.insertion_frame(self.field_cursor(id))
    }

    /// Candidate windows follow the active composition clause, even when
    /// that clause is selected and the ordinary insertion caret is hidden.
    pub fn ime_cursor_area(&self) -> Option<Rect4> {
        self.ime_target()?;
        let id = self.focus?;
        let cursor = if self.ime_owns(id) {
            crate::text::field::Cursor {
                offset: self.field_selection(id).start,
                upstream: false,
            }
        } else {
            self.field_cursor(id)
        };
        self.insertion_frame(cursor)
    }

    fn insertion_frame(&self, cursor: crate::text::field::Cursor) -> Option<Rect4> {
        let id = self.focus?;
        let field = self.brush.field_text.get(&id)?;
        let box_ = self.boxes.iter().find(|b| b.id == id)?;
        if !box_.flat() {
            return None;
        }
        let rect = field.caret_frame(cursor);
        let clip = box_
            .clip
            .unwrap_or((0., 0., self.viewport.0, self.viewport.1));
        let left = rect.0.max(clip.0).max(0.);
        let top = rect.1.max(clip.1).max(0.);
        let right = (rect.0 + rect.2).min(clip.0 + clip.2).min(self.viewport.0);
        let bottom = (rect.1 + rect.3).min(clip.1 + clip.3).min(self.viewport.1);
        (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
    }

    /// Set the next paint's caret visibility without changing selection or focus.
    pub fn set_caret_visible(&mut self, visible: bool) {
        self.brush.caret_hidden = !visible;
    }

    pub(crate) fn reveal_field_cursor(&mut self) -> Option<ViewId> {
        if !std::mem::take(&mut self.field_reveal) {
            return None;
        }
        let id = self.focus?;
        let field = self.brush.field_text.get(&id)?;
        let key = self.host.kernel().node(id)?.key;
        let next = self
            .field_drag
            .as_mut()
            .and_then(|drag| drag.take_scroll(key, &field.value))
            .map(|(x, y)| {
                (
                    x.clamp(0., field.scroll_max.0),
                    y.clamp(0., field.scroll_max.1),
                )
            })
            .unwrap_or_else(|| field.reveal_cursor(self.field_cursor(id)));
        if self.scroll_of(id) == next {
            return None;
        }
        self.scroll.insert(id, next);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_runner::FieldSelection;

    fn fixture(tag: &str, value: &str) -> (Presenter<()>, ViewId) {
        let source = format!("component App\n  view\n    {tag} testId=\"editor\" value={} width=200 height=64 padding=4 appearance=\"none\" outline=\"none\" font-size=16 line-height=\"24px\" caret-color=\"#ff0000\"\n", serde_json::to_string(value).unwrap());
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
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
        p.set_focus(Some(id), 0.);
        (p, id)
    }

    fn assert_caret_in_field(p: &mut Presenter<()>, id: ViewId) {
        let pixels = p.frame();
        let r = p.rect_of(id).unwrap();
        let mut count = 0;
        for y in 0..pixels.height() {
            for x in 0..pixels.width() {
                let pixel = pixels.pixel(x, y).unwrap();
                let coverage = pixel.red().saturating_sub(pixel.green());
                if coverage > 0 {
                    count += u32::from(coverage);
                    assert!(x as f32 >= r.0 && (x as f32) < r.0 + r.2);
                    assert!(y as f32 >= r.1 && (y as f32) < r.1 + r.3);
                }
            }
        }
        assert!(
            count >= 16 * 255,
            "caret coverage={count}, scroll={:?}",
            p.scroll_of(id)
        );
    }

    #[test]
    fn scrolled_selection_keeps_the_visible_parts_of_the_full_range() {
        let value = "אב 😀 a line long enough to wrap in the field\n\n".repeat(16);
        let (mut p, id) = fixture("textarea", &value);
        p.frame();
        let geometry = &p.brush.field_text[&id].geometry;
        let len = value.encode_utf16().count() as u32;
        for (start, end) in [(0, len), (3, len - 5)] {
            let all = geometry.highlight(start, end, None);
            let bottom = all.last().unwrap().1;
            for top in [-100., 0., 57.5, bottom - 24., bottom + 200.] {
                let clip = top..top + 48.;
                let expected: Vec<_> = all
                    .iter()
                    .copied()
                    .filter(|r| r.1 + r.3 >= clip.start && r.1 <= clip.end)
                    .collect();
                assert_eq!(geometry.highlight(start, end, Some(clip)), expected);
            }
        }
    }

    #[test]
    fn multiline_caret_reveals_scrolls_and_hits_the_visible_text() {
        let value = (1..=12)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let (mut p, id) = fixture("textarea", &value);
        assert_caret_in_field(&mut p, id);
        let end = p.scroll_of(id).1;
        assert!(end > 100.);
        p.wheel_at(20., 20., 0., -48.);
        p.frame();
        assert_eq!(
            p.scroll_of(id).1,
            end - 48.,
            "a wheel must not snap back to the caret"
        );
        p.press_field(id, 8., 8.);
        assert!(
            p.field_selection(id).start > 0,
            "hit testing must account for the internal scroll"
        );
        p.hold_modifier("ControlLeft", true);
        p.field_key(id, "Home", 1.);
        p.hold_modifier("ControlLeft", false);
        assert_caret_in_field(&mut p, id);
        assert_eq!(p.scroll_of(id), (0., 0.));
        p.hold_modifier("ControlLeft", true);
        p.field_key(id, "End", 2.);
        p.hold_modifier("ControlLeft", false);
        assert_caret_in_field(&mut p, id);
        p.field_key(id, "!", 3.);
        assert_caret_in_field(&mut p, id);
        assert!(p.field_text(id).ends_with('!'));
        assert_eq!(p.scroll_of(id).1, end);
    }

    #[test]
    fn single_line_caret_scrolls_horizontally_and_returns_home() {
        let (mut p, id) = fixture("input", &"abcdefghij ".repeat(12));
        assert_caret_in_field(&mut p, id);
        assert!(p.scroll_of(id).0 > 500.);
        p.field_key(id, "Home", 1.);
        assert_caret_in_field(&mut p, id);
        assert_eq!(p.scroll_of(id).0, 0.);
    }

    #[test]
    fn rtl_single_line_caret_scrolls_after_focus_typing_and_home() {
        let value = "אבגדהוזחטי ".repeat(12);
        let (mut p, id) = fixture("input direction=\"rtl\"", &value);
        assert_caret_in_field(&mut p, id);
        let end = p.caret_frame().expect("the end caret is visible");
        assert!(p.scroll_of(id).0 > 500.);
        p.field_key(id, "א", 1.);
        assert_caret_in_field(&mut p, id);
        assert_eq!(p.field_text(id), format!("{value}א"));
        let caret = p.caret_frame().unwrap();
        let pixels = p.frame();
        assert!(
            (caret.0 as u32 + 2..caret.0 as u32 + 20).any(|x| {
                (caret.1 as u32..(caret.1 + caret.3) as u32).any(|y| {
                    let pixel = pixels.pixel(x, y).unwrap();
                    pixel.red() < 128 && pixel.green() < 128 && pixel.blue() < 128
                })
            }),
            "the typed text is painted beside the caret"
        );
        p.field_key(id, "Home", 2.);
        assert_caret_in_field(&mut p, id);
        let home = p.caret_frame().expect("the start caret is visible");
        assert_eq!(p.field_selection(id).start, 0);
        assert_eq!(p.scroll_of(id), (0., 0.));
        assert!(home.0 > end.0 + 190., "Home is at the right edge");
        p.field_key(id, "End", 3.);
        assert_caret_in_field(&mut p, id);
        p.press_field(id, end.0, end.1 + end.3 / 2.);
        assert_eq!(
            p.field_selection(id).start,
            value.encode_utf16().count() as u32 + 1,
            "hit testing follows the scrolled text"
        );
    }

    #[test]
    fn rtl_single_line_with_latin_text_reveals_both_logical_edges() {
        let value = format!("{}z", "abcdefghij ".repeat(12));
        let (mut p, id) = fixture("input direction=\"rtl\"", &value);
        assert_caret_in_field(&mut p, id);
        let end = p.caret_frame().unwrap();
        assert_eq!(p.scroll_of(id), (0., 0.));
        p.field_key(id, "Home", 1.);
        assert_caret_in_field(&mut p, id);
        assert!(p.scroll_of(id).0 > 500.);
        let home = p.caret_frame().unwrap();
        assert!(end.0 > home.0 + 190., "Home follows the text's direction");
        p.field_key(id, "End", 2.);
        assert_caret_in_field(&mut p, id);
        assert_eq!(p.scroll_of(id), (0., 0.));
    }

    #[test]
    fn single_line_horizontal_wheel_moves_text_in_physical_direction() {
        horizontal_scroll_moves_text(|p, id, dx| {
            p.wheel(id, dx, 0.).unwrap();
        });
    }

    #[test]
    fn single_line_horizontal_scroll_by_moves_text_in_physical_direction() {
        horizontal_scroll_moves_text(|p, _, dx| {
            p.scroll_element_by(&[
                exact_plan::Value::str("editor"),
                exact_plan::Value::Number(dx as f64),
                exact_plan::Value::Number(0.),
            ]);
        });
    }

    #[test]
    fn single_line_horizontal_reader_wheel_moves_text_in_physical_direction() {
        horizontal_scroll_moves_text(|p, id, dx| {
            p.reader_wheel(id, dx, 0.).unwrap();
        });
    }

    fn horizontal_scroll_moves_text(scroll: impl Fn(&mut Presenter<()>, ViewId, f32)) {
        for (direction, word, sign) in [("ltr", "abcdefghij ", 1.), ("rtl", "אבגדהוזחטי ", -1.)]
        {
            let value = word.repeat(12);
            let (mut p, id) = fixture(
                &format!("input id=\"editor\" direction=\"{direction}\" overflow-x=\"auto\""),
                &value,
            );
            p.mark_field(id, &value, FieldSelection::caret(66), false);
            p.frame();
            let rect = p.rect_of(id).unwrap();
            p.press_field(id, rect.0 + 104., rect.1 + 32.);
            assert_caret_in_field(&mut p, id);
            let selection = p.field_selection(id);
            for dx in [40., -40.] {
                let before = p.caret_frame().unwrap();
                let offset = p.scroll_of(id).0;
                scroll(&mut p, id, dx);
                assert_caret_in_field(&mut p, id);
                assert_eq!(p.scroll_of(id).0, offset + sign * dx, "{direction}");
                let after = p.caret_frame().unwrap();
                assert!((after.0 - before.0 + dx).abs() < 0.01, "{direction}");
                assert_eq!(p.field_selection(id), selection);
            }
        }
    }

    #[test]
    fn rtl_single_line_wheel_chains_at_the_physical_edges() {
        let source = format!(
            r#"component App
  view
    column testId="outer" width=280 height=160 overflow-x="auto"
      box width=800 height=100
        input testId="editor" direction="rtl" overflow-x="auto" value="{}" margin-left=200 width=200 height=64 padding=4 appearance="none" font-size=16
"#,
            "אבגדהוזחטי ".repeat(12)
        );
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(&source),
            (),
            (300., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let k = p.host.kernel();
        let id = k.node_by_key(k.find_by_test_id("editor")[0]).unwrap().id;
        let outer = k.node_by_key(k.find_by_test_id("outer")[0]).unwrap().id;
        let max = p.brush.field_text[&id].scroll_max.0;
        assert!(max > 500.);
        for (start, dx, end, parent) in [
            (0., 40., 0., 140.),
            (0., -40., 40., 100.),
            (max, -40., max, 60.),
            (max, 40., max - 40., 100.),
        ] {
            assert_eq!(p.scroll_to(id, (start, 0.)), Some((start, 0.)));
            assert_eq!(p.scroll_to(outer, (100., 0.)), Some((100., 0.)));
            p.frame();
            let rect = p.rect_of(id).unwrap();
            p.wheel_at(rect.0 + 20., rect.1 + 20., dx, 0.);
            p.frame();
            assert_eq!(p.scroll_of(id), (end, 0.), "start={start}, dx={dx}");
            assert_eq!(p.scroll_of(outer), (parent, 0.), "start={start}, dx={dx}");
        }
    }

    #[test]
    fn blinking_changes_only_caret_pixels_and_preserves_the_editor() {
        let (mut p, id) = fixture("textarea", &"wrapped text ".repeat(30));
        let on = p.frame();
        let rect = p.caret_frame().unwrap();
        let selection = p.field_selection(id);
        let scroll = p.scroll_of(id);
        assert!(scroll.1 > 0.);
        p.set_caret_visible(false);
        let off = p.frame();
        let mut changed = 0;
        for (i, (a, b)) in on.pixels().iter().zip(off.pixels()).enumerate() {
            if a != b {
                let x = i as u32 % on.width();
                let y = i as u32 / on.width();
                assert!((x as f32) >= rect.0.floor() && (x as f32) < (rect.0 + rect.2).ceil());
                assert!((y as f32) >= rect.1.floor() && (y as f32) < (rect.1 + rect.3).ceil());
                changed += 1;
            }
        }
        assert!(changed > 0);
        assert_eq!(p.field_selection(id), selection);
        assert_eq!(p.scroll_of(id), scroll);
        assert_eq!(p.caret_frame(), Some(rect));
        p.set_caret_visible(true);
        assert_eq!(p.frame().data(), on.data());
        p.mark_field(
            id,
            &p.field_text(id),
            FieldSelection {
                start: 0,
                end: 4,
                ..FieldSelection::caret(0)
            },
            false,
        );
        assert_eq!(p.caret_frame(), None);
        p.set_focus(None, 1.);
        assert_eq!(p.caret_frame(), None);
    }

    #[test]
    fn multiline_textarea_paints_scrolling_text_through_vertical_padding() {
        let source = r##"component App
  view
    textarea testId="editor" value="VVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\nVVVVV\n" width=300 height=200 box-sizing="border-box" appearance="none" border-width=0 padding="10px 4px 64px" background-color="#ffffff" color="#000000" font-size=14 line-height="20px"
"##;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (320., 220.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let frame = p.frame();
        // Content box ends at y = 200 - 64 = 136; rows 8..10 paint inside the
        // bottom padding strip (y = 140..195) just as in Chrome.
        let dark_in_bottom_padding = (140..195).any(|y| {
            (4..80).any(|x| {
                let px = frame.pixel(x, y).unwrap();
                px.red() < 200 || px.green() < 200 || px.blue() < 200
            })
        });
        assert!(
            dark_in_bottom_padding,
            "expected textarea ink to paint into vertical padding below y=136"
        );
    }

    #[test]
    fn a_scrolled_textarea_paints_and_hits_its_first_glyph_inside_its_padding() {
        let source = r##"component App
  view
    textarea id="editor" testId="editor" value="M\n0123456789012345678901234567890123456789" width=200 height=90 box-sizing="border-box" appearance="none" border="none" padding="16px 28px" color="#000000" background-color="#ffffff" font-size=16 line-height=1.6 white-space="pre"
"##;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (240., 120.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let first_line = |p: &mut Presenter<()>| {
            let frame = p.frame();
            (0..180)
                .filter(|&x| (16..42).any(|y| frame.pixel(x, y).unwrap().red() < 128))
                .collect::<Vec<_>>()
        };
        let at_start = first_line(&mut p);
        assert!(at_start.first().is_some_and(|&x| x >= 28));
        for dx in [23., -23.] {
            p.scroll_element_by(&[
                exact_plan::Value::str("editor"),
                exact_plan::Value::Number(dx),
                exact_plan::Value::Number(0.),
            ]);
            let expected = if dx > 0. {
                at_start.iter().map(|x| x - 23).collect()
            } else {
                at_start.clone()
            };
            assert_eq!(first_line(&mut p), expected, "scroll by {dx}");
        }
        let k = p.host.kernel();
        let field = k.node_by_key(k.find_by_test_id("editor")[0]).unwrap().id;
        p.pointer_down(28., 28., 10.).unwrap();
        p.pointer_up(28., 28., 20.).unwrap();
        p.frame();
        assert_eq!(p.focus, Some(field));
        assert_eq!(p.field_selection(field), FieldSelection::caret(0));
        for left in [0., 23.] {
            p.scroll_to(field, (left, 0.));
            p.frame();
            p.pointer_down(6., 28., 30.).unwrap();
            p.frame();
            assert_eq!(p.scroll_of(field).0, left, "mouse down");
            assert_eq!(p.field_selection(field), FieldSelection::caret(0));
            p.pointer_up(6., 28., 40.).unwrap();
            p.frame();
            assert_eq!(p.scroll_of(field).0, left, "mouse up");
        }
        // The first line can also end to the left of the visible area.
        p.scroll_to(field, (95., 0.));
        p.frame();
        p.pointer_down(6., 28., 50.).unwrap();
        p.pointer_up(6., 28., 60.).unwrap();
        p.frame();
        assert_eq!(p.field_selection(field), FieldSelection::caret(1));
        assert_eq!(p.scroll_of(field).0, 95.);
    }

    #[test]
    fn first_click_on_horizontally_scrolled_textarea_hits_visible_text_not_old_caret() {
        let source = r##"component App
  state body = "LEFT EDGE | abcdefghijklmnopqrstuvwxyz | ABCDEFGHIJKLMNOPQRSTUVWXYZ | RIGHT EDGE"
  view
    column width=300 height=240 padding=12 gap=10 background-color="#ffffff"
      button "Park" id="park" testId="park" width=80 height=30
      textarea id="field" testId="field" value=body width=180 height=120 padding=8 white-space="pre" overflow-x="auto" overflow-y="auto" font-size=16
"##;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (300., 240.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let (park, field) = {
            let k = p.host.kernel();
            (
                k.node_by_key(k.find_by_test_id("park")[0]).unwrap().id,
                k.node_by_key(k.find_by_test_id("field")[0]).unwrap().id,
            )
        };
        let rect = p.rect_of(field).unwrap();
        let click = (rect.0 + 36., rect.1 + 16.);
        // First measure with caret near 0.
        p.set_focus(Some(park), 0.);
        p.mark_field(field, &p.field_text(field), FieldSelection::caret(0), false);
        p.scroll_to(field, (0., 0.));
        p.frame();
        p.pointer_down(click.0, click.1, 10.).unwrap();
        p.pointer_up(click.0, click.1, 15.).unwrap();
        let near_offset = p.field_selection(field).start;
        // Now place caret far right (offset 70), scroll back to left edge, blur
        // onto Park, and click the exact same visible point.
        p.set_focus(Some(park), 20.);
        p.mark_field(
            field,
            &p.field_text(field),
            FieldSelection::caret(70),
            false,
        );
        p.scroll_to(field, (0., 0.));
        p.frame();
        assert_eq!(p.scroll_of(field).0, 0.);
        p.pointer_down(click.0, click.1, 30.).unwrap();
        p.pointer_up(click.0, click.1, 35.).unwrap();
        let far_offset = p.field_selection(field).start;
        assert_eq!(
            far_offset, near_offset,
            "clicking visible text must not jump to old caret before hit-testing"
        );
        assert!(
            near_offset < 12,
            "expected offset near LEFT EDGE, got {near_offset}"
        );
    }

    #[test]
    fn unchanged_textarea_reuses_retained_field_geometry_across_redraws() {
        let source = r##"component App
  state clicks = 0
  state body = "line one\nline two\nline three\nline four\n"
  action inc
    clicks = clicks + 1
  action edit(v: string)
    body = v
  view
    column width=300 height=200 padding=8 gap=8
      button "Inc" testId="inc" press=inc width=60 height=24
      text `${clicks}`
      textarea testId="editor" value=body input=edit width=260 height=120 font-size=14
"##;
        let coloured = format!("decoration accent color=\"#db1141\"\n{source}").replace(
            "value=body input=edit",
            "value=body input=edit decorations=TextDecorations(value=body,spans=[TextDecorationSpan(name=\"accent\",start=0,end=5)])",
        );
        for source in [source, &coloured] {
            exercise_field_geometry(source);
        }
    }

    fn exercise_field_geometry(source: &str) {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (300., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let (inc, editor) = {
            let k = p.host.kernel();
            (
                k.node_by_key(k.find_by_test_id("inc")[0]).unwrap().id,
                k.node_by_key(k.find_by_test_id("editor")[0]).unwrap().id,
            )
        };
        let first = p.brush.field_text[&editor].geometry.clone();
        p.tap(inc).unwrap();
        p.frame();
        let second = p.brush.field_text[&editor].geometry.clone();
        assert!(
            std::rc::Rc::ptr_eq(&first, &second),
            "unchanged textarea must reuse its cached Geometry across unrelated redraws"
        );
        p.set_focus(Some(editor), 10.);
        p.mark_field(
            editor,
            &p.field_text(editor),
            FieldSelection::caret(0),
            false,
        );
        p.key(Some('x'), false, 11.);
        assert!(p.field_text(editor).starts_with("xline one"));
        p.frame();
        let third = p.brush.field_text[&editor].geometry.clone();
        assert!(
            !std::rc::Rc::ptr_eq(&first, &third),
            "the edited row changes the field geometry"
        );
        assert!(!third.shares_spans(0, &first, 0));
        for row in 1..4 {
            assert!(third.shares_spans(row, &first, row), "unchanged row {row}");
        }
    }
}
