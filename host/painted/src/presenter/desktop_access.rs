//! Desktop accessibility actions use the same focus and editor as physical input.
use super::*;
use exact_runner::FieldSelection;

pub use crate::text::field::FieldTextRun;

/// The editor layout used by the last paint, with placement separate from shaping
/// so that scrolling can move the text without rebuilding its accessibility runs.
#[derive(Clone)]
pub struct FieldTextLayout {
    geometry: std::rc::Rc<crate::text::field::Geometry>,
    /// Paragraph-to-viewport transform in logical pixels. A perspective projection
    /// retains the text's line breaks but has no affine placement.
    pub transform: Option<[f64; 6]>,
    /// The field's content viewport in paragraph coordinates.
    pub clip: Rect4,
}

impl FieldTextLayout {
    /// Directional runs in source order.
    pub fn runs(&self) -> impl Iterator<Item = FieldTextRun<'_>> {
        self.geometry.access_runs()
    }

    /// Whether the runs retain their positions within the paragraph.
    pub fn same_shape(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.geometry, &other.geometry)
            && self.transform.is_some() == other.transform.is_some()
    }
}

impl<D: DataSource> Presenter<D> {
    /// The live view for an enabled node in the last accessibility statement.
    pub fn access_view(&self, id: i32) -> Option<ViewId> {
        let node = self.access_told().iter().find(|n| n.id == id)?;
        self.enabled_access_view(node)
    }

    fn enabled_access_view(&self, node: &access::Node) -> Option<ViewId> {
        (node.flags & access::flag::DISABLED == 0
            && node.view != 0
            && self.host.kernel().node(node.view).is_some()
            && self.focus_refusal(node.view).is_none())
        .then_some(node.view)
    }

    /// The accessible identity holding keyboard focus, including a menu item
    /// and the row an open select's menu is at.
    pub fn access_focus(&self) -> Option<i32> {
        if let Some(row) = self.access_menu_focus() {
            return Some(row);
        }
        let view = if let Some((focused, _)) = self.desktop_surface_access() {
            focused
        } else {
            self.focus
        }?;
        self.access_told()
            .iter()
            .find(|n| n.view == view)
            .map(|n| n.id)
    }

    /// The top-layer surface's offset inside the desktop window, in logical pixels.
    pub fn access_origin(&self) -> (f32, f32) {
        self.desktop_surface_access()
            .map_or((0., 0.), |(_, origin)| origin)
    }

    /// Focus an accessible control without invoking it.
    pub fn access_focus_node(&mut self, id: i32, now_ms: f64) -> bool {
        let Some(node) = self.access_told().iter().find(|n| n.id == id) else {
            return false;
        };
        if node.flags & access::flag::DISABLED != 0 {
            return false;
        }
        let view = node.view;
        if self.surface_focus(view) {
            return true;
        }
        if self.access_view(id).is_none() || !self.focusable(view) {
            return false;
        }
        if let Some(error) = self.set_focus(Some(view), now_ms) {
            self.host.log(error);
        }
        self.focus == Some(view)
    }

    fn accessible_field_view(&self, source: &access::Node) -> Option<ViewId> {
        let node = self.host.kernel().node(source.view)?;
        if node.node_type != NodeType::TextInput || node.props.str(PropId::Type) == Some("password")
        {
            return None;
        }
        self.enabled_access_view(source)
    }

    /// The current text and UTF-16 selection for a field in the published tree.
    pub fn access_editor(&self, source: &access::Node) -> Option<(String, FieldSelection)> {
        let view = self.accessible_field_view(source)?;
        Some((self.field_text(view), self.field_selection(view)))
    }

    /// The UTF-16 selection of the field addressed by an accessibility action.
    pub fn access_selection(&self, id: i32) -> Option<FieldSelection> {
        let source = self.access_told().iter().find(|n| n.id == id)?;
        Some(self.field_selection(self.accessible_field_view(source)?))
    }

    /// Published geometry for this field value. An accessibility client may keep the
    /// snapshot while another frame is being painted.
    pub fn access_text_layout(&self, view: ViewId, value: &str) -> Option<FieldTextLayout> {
        let (field, projected) = self.ranges.published_field(view)?;
        if field.value != value {
            return None;
        }
        let (t, clip) = field.text_placement();
        Some(FieldTextLayout {
            geometry: field.geometry.clone(),
            transform: (!projected).then(|| [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from)),
            clip,
        })
    }

    /// Align a text range's endpoint with the top or bottom of its field.
    pub fn access_scroll_text_line(&mut self, id: i32, offset: u32, top: bool) -> bool {
        let Some(view) = self.access_view(id) else {
            return false;
        };
        let Some(field) = self.brush.field_text.get(&view) else {
            return false;
        };
        let y = field.aligned_line_y(offset, top);
        self.scroll_to(view, (self.scroll_of(view).0, y)).is_some()
    }

    /// Select text requested by an accessibility service, clamped to the current value.
    pub fn access_select(&mut self, id: i32, selection: FieldSelection, now_ms: f64) -> bool {
        if self.access_selection(id).is_none() {
            return false;
        }
        let Some(view) = self.access_view(id) else {
            return false;
        };
        if !self.access_focus_node(id, now_ms) || self.access_view(id) != Some(view) {
            return false;
        }
        let value = self.field_text(view);
        let selection = FieldSelection::clamped(
            selection.start.into(),
            selection.end.into(),
            selection.direction,
            field::utf16_len(&value),
        );
        self.mark_field(view, &value, selection, true);
        self.field_select(view, &value, selection, now_ms);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_runner::SelectionDirection;

    #[test]
    fn accessibility_focus_and_selection_use_the_live_editor() {
        let plan = crate::test_plan(
            r#"component App
  view
    column
      textarea aria-label="Draft" value="a🦀z" width=220 height=60
      input aria-label="Read only" value="fixed" readonly=true
      input aria-label="Secret" type="password" value="secret"
      button aria-label="Disabled" disabled=true
        text "Disabled"
"#,
        );
        let (mut p, error) = Presenter::boot_with(
            &plan,
            (),
            (420., 300.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        p.reader_access(1., true);
        let find = |name| p.access_told().iter().find(|n| n.name == name).unwrap().id;
        let draft = find("Draft");
        let readonly = find("Read only");
        let secret = find("Secret");
        let disabled = find("Disabled");
        assert!(p.access_focus_node(draft, 1.));
        assert_eq!(p.access_focus(), Some(draft));
        let selection = FieldSelection {
            start: 1,
            end: 3,
            direction: SelectionDirection::Backward,
        };
        assert!(p.access_select(draft, selection, 2.));
        assert_eq!(p.access_selection(draft), Some(selection));
        assert_eq!(p.field_text(p.access_view(draft).unwrap()), "a🦀z");
        assert!(!p.access_focus_node(disabled, 3.));
        assert_eq!(p.access_focus(), Some(draft));
        assert_eq!(p.access_selection(secret), None);
        let editor = |p: &Presenter<()>, id| {
            let source = p.access_told().iter().find(|n| n.id == id).unwrap();
            p.access_editor(source)
        };
        assert_eq!(editor(&p, secret), None);
        assert_eq!(editor(&p, disabled), None);
        assert_eq!(editor(&p, draft), Some(("a🦀z".into(), selection)));
        assert!(!p.access_select(secret, selection, 4.));
        assert!(p.access_select(readonly, selection, 5.));
        assert_eq!(p.field_text(p.access_view(readonly).unwrap()), "fixed");
        p.type_text(p.access_view(draft).unwrap(), "new 🦀 draft")
            .unwrap();
        assert_eq!(editor(&p, draft).unwrap().0, "new 🦀 draft");
        p.access_reset();
        assert!(!p.access_focus_node(draft, 6.));
        assert!(!p.access_select(draft, selection, 6.));
    }
}
