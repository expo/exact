//! Static failures of the transform and height drag bindings in the kernel.
//! Layout, presentation holds and values the compiler cannot resolve stay undecided.

use crate::{swipe_rows::DragRow, LowerError};
use contract_checking::View;
use contract_syntax::{Attr, Expr, Span, StyleDecl};
use exact_kernel::{
    BoxSizing as Sizing, Dimension, Display, NodeType, Overflow, RowValue, StyleId, StyleMask,
    StyleProps, StyleValue,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check(
    view: &View<'_>,
    styles: &[StyleDecl],
    list_rows: &BTreeMap<(Span, u32), DragRow>,
    out: &mut Vec<LowerError>,
) {
    if !view.walk().any(|node| {
        node.element().is_some_and(|(_, _, attrs)| {
            attrs
                .iter()
                .any(|a| matches!(a.name.as_str(), "heightDragFor" | "transformDragFor"))
        })
    }) {
        return;
    }
    let mut seen = BTreeSet::new();
    let mut height_seen = BTreeSet::new();
    let mut path = Vec::new();
    let mut rooted = true;
    let mut uses = Vec::new();
    for (node, (), entering) in view.events((), |_, _| Some(())) {
        if let Some((tag, _, attrs)) = node.element() {
            if entering {
                path.push(Element {
                    tag,
                    rows: rows(tag, attrs, styles),
                    lowered: list_rows.get(&(node.span(), node.instance().unwrap())),
                    span: node.span(),
                });
                binding(&path, rooted, "transformDragFor", &mut seen, out);
                binding(&path, rooted, "heightDragFor", &mut height_seen, out);
            } else {
                path.pop();
            }
        } else if node.component_use().is_some() {
            // An unresolved use does not establish a direct kernel parent.
            if entering {
                uses.push((std::mem::take(&mut path), rooted));
                rooted = false;
            } else {
                (path, rooted) = uses.pop().unwrap();
            }
        }
    }
}

struct Element<'a> {
    tag: &'a str,
    rows: Option<Vec<Attr>>,
    lowered: Option<&'a DragRow>,
    span: Span,
}

impl Element<'_> {
    fn attr(&self, name: &str) -> Option<&Attr> {
        self.rows.as_ref()?.iter().rfind(|a| a.name == name)
    }

    fn grouped(&self) -> bool {
        !matches!(
            crate::grouped::style(self.tag, self.rows.as_deref().unwrap_or_default()),
            Ok(None)
        )
    }
}

fn rows(tag: &str, attrs: &[Attr], styles: &[StyleDecl]) -> Option<Vec<Attr>> {
    let unwritten: Vec<_> = crate::tags::tag(tag)?
        .fixed_styles
        .iter()
        .map(|(id, value)| (*id, Expr::Str((*value).into(), Span::default())))
        .collect();
    let class = crate::class::rows(
        attrs,
        &|name| {
            styles
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.attrs.as_slice())
        },
        &unwritten,
    )
    .ok()?
    .map(|(_, rows)| rows)
    .unwrap_or_default();
    Some(
        class
            .into_iter()
            .chain(attrs.iter().filter(|a| a.name != "class").cloned())
            .collect(),
    )
}

fn literal<'a>(element: &'a Element<'_>, name: &str) -> Option<&'a str> {
    match &element.attr(name)?.value {
        Expr::Str(value, _) => Some(value),
        _ => None,
    }
}

fn binding(
    path: &[Element<'_>],
    rooted: bool,
    attribute: &str,
    seen: &mut BTreeSet<(Span, String, String)>,
    out: &mut Vec<LowerError>,
) {
    let (handle, ancestors) = path.split_last().unwrap();
    let Some(name) = literal(handle, attribute).filter(|n| !n.is_empty()) else {
        return;
    };
    let mut targets = Vec::new();
    for (i, ancestor) in ancestors.iter().enumerate() {
        if ancestor.rows.is_none() {
            return;
        }
        if ancestor.attr("id").is_some() && literal(ancestor, "id").is_none() {
            return;
        }
        if literal(ancestor, "id") == Some(name) {
            targets.push(i);
        }
    }
    let [target] = targets.as_slice() else { return };
    let clip = target.checked_sub(1);
    let height = attribute == "heightDragFor";
    let mut defaults_known = rooted;
    for (i, element) in path.iter().enumerate() {
        let node_type = crate::tags::tag(element.tag).map(|t| t.node_type);
        let plain = node_type == Some(NodeType::View);
        // Transform checks stay conservative below grouped sheets, which
        // also style row parts. Height checks below track only box sizing.
        if element.grouped() {
            defaults_known = false;
        }
        let sheeted = !defaults_known || element.attr("delete").is_some();
        let label = if i == *target {
            format!("target `{name}`")
        } else if !height && Some(i) == clip {
            literal(element, "id").map_or_else(
                || format!("clip (the target's direct `{}` parent)", element.tag),
                |id| format!("clip `{id}`"),
            )
        } else {
            literal(element, "id").map_or_else(
                || format!("`{}` on the handle's path", element.tag),
                |id| format!("path element `{id}`"),
            )
        };
        let unknown = if height {
            // The sheets write box sizing on rows, not height or their
            // descendants' non-inherited box sizing.
            if !rooted {
                StyleMask::ALL
            } else if element.lowered.is_some_and(|row| row.box_sizing) {
                StyleMask::of(StyleId::BoxSizing)
            } else {
                StyleMask::EMPTY
            }
        } else if sheeted {
            StyleMask::ALL
        } else {
            StyleMask::EMPTY
        };
        let style = StaticStyle::new(element.tag, element.rows.as_deref(), unknown);
        let mut repairs = if height {
            let mut repairs = style.visibility();
            if i == *target {
                repairs.extend(style.height_rules());
            }
            repairs
        } else {
            style.path(i == *target)
        };
        if !height && (i == *target || Some(i) == clip) {
            if plain {
                repairs.extend(style.box_rules(i == *target));
            } else if node_type.is_some() {
                repairs.push(if i == *target {
                    "must be a plain View; move the target id and transform to a `column` or `box` around this element".into()
                } else {
                    "must be a plain View; insert a `column` or `box` with `overflow=\"hidden\" padding=0 border-width=0` between this parent and the target".into()
                });
            }
        }
        if !height && i == *target && clip.is_none() && rooted {
            repairs.push("has no clipping parent; wrap it in a `column` or `box` with `overflow=\"hidden\" padding=0 border-width=0`".into());
        }
        for prop in ["inert", "disabled"] {
            let attr = if prop == "disabled" && element.attr("delete").is_some() {
                element
                    .lowered
                    .map_or_else(|| element.attr(prop), |row| row.disabled.as_ref())
            } else {
                element.attr(prop)
            };
            if matches!(attr.map(|a| &a.value), Some(Expr::Bool(true, _))) {
                repairs.push(format!(
                    "is `{prop}=true`; write `{prop}=false` before enabling the handle"
                ));
            }
        }
        for repair in repairs {
            if seen.insert((element.span, name.to_string(), repair.clone())) {
                out.push(LowerError {
                    id: if height {
                        "lower-height-drag-ineligible"
                    } else {
                        "lower-transform-drag-ineligible"
                    },
                    message: format!("`{attribute}=\"{name}\"` cannot bind: {label} {repair}"),
                    span: element.span,
                });
            }
        }
    }
}

struct StaticStyle {
    props: StyleProps,
    unknown: StyleMask,
}

impl StaticStyle {
    fn new(tag: &str, rows: Option<&[Attr]>, unknown: StyleMask) -> Self {
        let mut this = Self {
            props: StyleProps::default(),
            unknown: StyleMask::EMPTY,
        };
        let Some(rows) = rows else {
            this.unknown = StyleMask::ALL;
            return this;
        };
        if let Some(tag) = crate::tags::tag(tag) {
            for (id, value) in tag.fixed_styles {
                this.set(*id, &Expr::Str((*value).into(), Span::default()));
            }
        }
        // A sheet the lowering adds (a grouped list's, a swipe's) sits under authored rows.
        this.unknown = unknown;
        let boxed = crate::box_transform::lower(tag, false, rows).ok().flatten();
        let attrs = boxed.as_deref().unwrap_or(rows);
        for attr in attrs {
            if contract_syntax::state::state_of(&attr.name).is_some() {
                continue;
            }
            match crate::shorthands::style_values(&attr.name, &attr.value) {
                Ok(values) => {
                    for (id, value) in values {
                        this.set(id, &value);
                    }
                }
                Err(_) => {
                    for id in crate::tags::rows(attr) {
                        this.unknown.set(id);
                    }
                }
            }
        }
        // A state part may replace a base row at runtime, regardless of source order.
        for attr in attrs {
            if let Some((_, name)) = contract_syntax::state::state_of(&attr.name) {
                let row = Attr {
                    name: name.into(),
                    ..attr.clone()
                };
                for id in crate::tags::rows(&row) {
                    this.unknown.set(id);
                }
            }
        }
        this
    }

    fn set(&mut self, id: StyleId, expr: &Expr) {
        let value = match expr {
            Expr::Str(s, _)
                if s.trim().eq_ignore_ascii_case("auto")
                    && id.codec() != exact_kernel::StyleCodec::Enum =>
            {
                Some(StyleValue::Auto)
            }
            Expr::Str(s, _) => Some(StyleValue::Text(s.as_str().into())),
            Expr::None(_) => Some(StyleValue::Text("unset".into())),
            _ => crate::values::numeric_literal(expr).map(StyleValue::Number),
        };
        self.unknown.set(id);
        if let Some(value) = value {
            if matches!(&value, StyleValue::Text(s) if exact_kernel::style::theme::names(s.trim()))
                || exact_kernel::style::metrics::of(id, &value).is_some()
            {
                return;
            }
            if value.unsets(id) {
                self.props.clear(StyleMask::of(id));
                self.unknown.clear(id);
            } else if self.props.set_dynamic(id, &value).is_ok()
                && self.props.relative.get(id).is_none()
            {
                self.unknown.clear(id);
            }
        }
    }

    fn known(&self, id: StyleId) -> bool {
        !self.unknown.has(id)
    }

    /// The row's stored value, which the kernel compares as it is: a `calc()`,
    /// `env()` or viewport length is never 100% or zero, whatever it resolves to.
    fn dimension(&self, id: StyleId) -> Option<Dimension> {
        match self.props.get(id) {
            RowValue::Dimension(d) if self.known(id) => Some(d),
            _ => None,
        }
    }

    fn visibility(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.known(StyleId::Display) && self.props.display == Display::None {
            out.push("is `display=\"none\"`; show it before enabling the handle".into());
        }
        out
    }

    fn path(&self, target: bool) -> Vec<String> {
        let s = &self.props;
        let mut out = self.visibility();
        if self.known(StyleId::Rotate) && s.rotate != 0.0 {
            out.push("has rotation; write `rotate=0` and put the rotation on a child outside the handle's path".into());
        }
        if self.known(StyleId::Scale)
            && ((s.scale.x <= 0.0 || s.scale.x != s.scale.y)
                || !target && (s.scale.x != 1.0 || s.scale.y != 1.0))
        {
            out.push(if target {
                "needs a positive uniform scale; write `scale=1`".into()
            } else {
                "must have an identity transform; write `scale=1`".into()
            });
        }
        let percent = self.known(StyleId::TranslatePercent)
            && (s.translate_percent.x != 0.0 || s.translate_percent.y != 0.0);
        let moved = !target
            && self.known(StyleId::Translate)
            && (s.translate.x != 0.0 || s.translate.y != 0.0);
        if percent || moved {
            out.push(if target {
                "needs pixel translation; replace percentage `translate` values with px".into()
            } else {
                "must have an identity transform; write `translate=\"0px 0px\"`".into()
            });
        }
        out
    }

    fn border_box(&self) -> Option<String> {
        (self.known(StyleId::BoxSizing) && self.props.box_sizing != Sizing::BorderBox)
            .then(|| "needs a border box; write `box-sizing=\"border-box\"`".into())
    }

    fn height_rules(&self) -> Vec<String> {
        let mut out: Vec<_> = self.border_box().into_iter().collect();
        if self
            .dimension(StyleId::Height)
            .is_some_and(|d| !matches!(d, Dimension::Points(px) if px.is_finite() && px >= 0.0))
        {
            out.push("needs a finite nonnegative point height; write a numeric `height`, such as `height=200`".into());
        }
        out
    }

    fn box_rules(&self, target: bool) -> Vec<String> {
        use StyleId::*;
        let s = &self.props;
        let mut out = Vec::new();
        if target {
            for (id, name) in [(Width, "width"), (Height, "height")] {
                if self
                    .dimension(id)
                    .is_some_and(|d| d != Dimension::Percent(100.0))
                {
                    out.push(format!(
                        "needs `{name}` exactly 100%; write `{name}=\"100%\"`"
                    ));
                }
            }
            out.extend(self.border_box());
            if self.known(TransformOrigin) && !s.transform_origin.centred() {
                out.push(
                    "needs a centred transform origin; write `transform-origin=\"50% 50%\"`".into(),
                );
            }
            for (ids, auto, name) in [
                (
                    [MarginTop, MarginRight, MarginBottom, MarginLeft],
                    false,
                    "margin",
                ),
                ([Top, Right, Bottom, Left], true, "inset"),
            ] {
                if ids.into_iter().any(|id| {
                    self.dimension(id).is_some_and(|d| {
                        !matches!(d, Dimension::Points(0.0) | Dimension::Percent(0.0))
                            && !(auto && d == Dimension::Auto)
                    })
                }) {
                    out.push(format!("needs zero {name}; write `{name}=0`"));
                }
            }
        } else {
            for (id, name, value) in [
                (OverflowX, "overflow-x", s.overflow_x),
                (OverflowY, "overflow-y", s.overflow_y),
            ] {
                if self.known(id) && value != Overflow::Hidden {
                    out.push(format!("must clip both axes; write `{name}=\"hidden\"`"));
                }
            }
        }
        if [PaddingTop, PaddingRight, PaddingBottom, PaddingLeft]
            .into_iter()
            .any(|id| {
                // A negative padding lays out as zero, as `zero_insets` admits.
                self.dimension(id).is_some_and(
                    |d| !matches!(d, Dimension::Points(v) | Dimension::Percent(v) if v <= 0.0),
                )
            })
        {
            out.push("needs zero effective padding; write `padding=0`".into());
        }
        for ((width, style), effective) in [
            (BorderWidthTop, BorderStyleTop),
            (BorderWidthRight, BorderStyleRight),
            (BorderWidthBottom, BorderStyleBottom),
            (BorderWidthLeft, BorderStyleLeft),
        ]
        .into_iter()
        .zip(s.border_widths())
        {
            if self.known(width) && self.known(style) && effective != 0.0 {
                out.push("needs zero effective border; write `border-width=0` or `border-style=\"none\"`".into());
                break;
            }
        }
        out
    }
}
