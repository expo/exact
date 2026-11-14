//! A node's `:hover` and `:focus-visible` looks on the page (LLP 1121 D8,
//! stage 8 B), as one rule a state in the page's looks sheet, composed in
//! that order over the node's inline base, so rows a look doesn't name show
//! the base. They show at once (no transition: a look's colours snap, under
//! Reduce Motion too), and under the shell's pressed paint, which `:active`
//! lowers to (LLP 1061 D7, the input glue's important rule per painting node).

use super::*;
use exact_kernel::{StyleId, StyleProps, LOOK_HOVER};

/// Each look's declarations; a look with none is left out. `css` writes
/// them ([`crate::css::css_text`], or the JS target's relative form).
pub fn declarations(
    looks: &[(u8, StyleProps)],
    css: impl Fn(&StyleProps) -> String,
) -> Vec<(u8, String)> {
    (looks.iter())
        .map(|(state, look)| (*state, css(look)))
        .filter(|(_, css)| !css.is_empty())
        .collect()
}

/// The looks' [`declarations`] for the element `at` selects, as page rules,
/// empty for none: `:hover` (where a pointer hovers and the last one used
/// was no finger, the input glue's `data-exact-input`), then
/// `:focus-visible`, the last winning, each also where it is held; a
/// disabled node takes none.
pub fn rules(at: &str, looks: &[(u8, String)]) -> String {
    let mut out = String::new();
    for (state, css) in looks {
        let (shadow, own): (Vec<_>, Vec<_>) = css
            .split_terminator(';')
            .partition(|row| row.trim_start().starts_with("box-shadow:"));
        let mut rows: String = own.into_iter()
            .map(|row| match row.strip_prefix("scale:") {
                Some(scale) => format!("--exact-scale:{scale}!important;scale:calc(var(--exact-scale,1) * var(--exact-press-factor,1))!important;"),
                None => format!("{row}!important;"),
            })
            .collect();
        // A hover look is the whole hover, as `:active` is the whole press: it
        // replaces the neutral look's unsaid wash on this node, and a bar
        // button's dim (neutral.css, LLP 1129), and no image of the author's.
        if *state == LOOK_HOVER {
            rows.push_str("--exact-hover-wash:none!important;--exact-hover-dim:1!important;");
        }
        holding(&mut out, *state, at, &rows, str::to_string);
        if !shadow.is_empty() {
            // A grouped section owns the state, including hover over its
            // header, but casts on the card. `each` may wrap the section.
            let section =
                ":where(:is([data-liststyle],[data-liststyle]>[data-exact-list-item])>section)";
            let rows = shadow
                .iter()
                .map(|row| format!("{row}!important;"))
                .collect::<String>();
            holding(&mut out, *state, at, &rows, |owner| {
                format!("{owner}:not({section}),{owner}{section}>div")
            });
        }
    }
    out
}

/// How many looks, nearest first, whose colour reaches a boundary `modes.js`
/// names by position (`data-x-lc="1:M 2:N"`); one further out takes the last.
pub const LOOK_DEPTH: usize = 3;

/// A selector, for [`colour_below`], of the boundaries under a holder that
/// the JS target marks with look set `set` (`modes.js`, `data-x-lc`). A
/// nearer position repeats its attribute more, so the nearest look that
/// holds wins, as CSS inheritance would have it.
pub fn marked(set: usize) -> impl Fn(&str) -> String {
    let below: Vec<String> = (1..=LOOK_DEPTH)
        .map(|k| format!("[data-x-lc~=\"{k}:{set}\"]").repeat(LOOK_DEPTH + 1 - k))
        .collect();
    move |holder| {
        (below.iter())
            .map(|b| format!(":where({holder}) {b}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Each look's `color` as page rules, in [`rules`]' order, on the
/// boundaries that `below` selects under the holder `at` selects; `below`
/// takes the holder's selector while the state holds. Empty for none. A mode
/// or `color-scheme` boundary that does not set its own colour writes an
/// inherited token colour again on itself, in its own modes (LLP 1121 D7),
/// which would stop the look's; a rule on the boundary gives it the look's
/// colour, which its subtree inherits. The holder is in `:where()`, so the
/// boundary's own look wins.
pub fn colour_below(at: &str, looks: &[(u8, String)], below: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    for (state, css) in looks {
        let row = (css.split_terminator(';')).find(|row| row.trim_start().starts_with("color:"));
        if let Some(row) = row {
            holding(&mut out, *state, at, &format!("{row}!important;"), &below);
        }
    }
    out
}

/// `rows` as look `state`'s rule on what `selector` makes of each selector
/// of the element `at` selects while the state holds (that selector itself,
/// for the element's own rule).
fn holding(out: &mut String, state: u8, at: &str, rows: &str, selector: impl Fn(&str) -> String) {
    // A disabled box says `disabled` (a bool's text; none, or "false", is enabled).
    let on = ":not(:disabled,[disabled]:not([disabled=\"false\"]))";
    match state {
        // A held state (LLP 1122 D8, `data-exact-held`) is the state's own
        // rule, whatever the pointer (the input glue's `hold`).
        LOOK_HOVER => write!(
            out,
            "@media (any-hover:hover){{{}{{{rows}}}}}{}{{{rows}}}",
            selector(&format!(
                ":where(html:not([data-exact-input=touch])) {at}:hover{on}"
            )),
            selector(&format!("{at}[data-exact-held~=hover]{on}")),
        ),
        _ => write!(
            out,
            "{},{}{{{rows}}}",
            selector(&format!("{at}:focus-visible{on}")),
            selector(&format!("{at}[data-exact-held~=focus-visible]{on}")),
        ),
    }
    .expect("a String");
}

/// The element of view `id`.
fn view(id: ViewId) -> String {
    format!("[data-view=\"{id}\"]")
}

/// Whether a node with `style` sets its own colour, which the colour above
/// it does not reach: written, not a clear and not a boundary's implied row.
fn sets_colour(style: &StyleProps) -> bool {
    style.mask.has(StyleId::TextColor) && style.written.has(StyleId::TextColor)
}

/// What a mode or `color-scheme` boundary (LLP 1121 D7) that does not set
/// its own colour takes from above it, as `modes.js` finds it on the JS
/// target.
struct Above {
    /// The views whose colour reaches it, nearest first: those up to the
    /// nearest that sets its colour, that one included. Their looks' colour
    /// and their press's colour reach it (a press's only when
    /// [`Above::from`] is set).
    views: Vec<ViewId>,
    /// The nearest view that sets its colour, when the boundary writes that
    /// colour again (its implied row): a press on that view, or on a view
    /// between it and the boundary, paints the boundary too (the input
    /// glue's `pressBelow`).
    from: Option<ViewId>,
}

impl<D: DataSource> Host<D> {
    /// What view `id` takes from above it as a boundary; `None` for a view
    /// that is no boundary or sets its own colour.
    fn above(&self, id: ViewId) -> Option<Above> {
        let kernel = self.runner.kernel();
        let node = kernel.node(id)?;
        if !self.runner.is_boundary(id) || sets_colour(node.style) {
            return None;
        }
        let (mut views, mut from) = (Vec::new(), None);
        let mut up = node.parent.and_then(|p| kernel.node(p));
        while let Some(at) = up {
            views.push(at.id);
            if sets_colour(at.style) {
                from = node.style.mask.has(StyleId::TextColor).then_some(at.id);
                break;
            }
            up = at.parent.and_then(|p| kernel.node(p));
        }
        Some(Above { views, from })
    }

    /// Boundary `id`'s rules from the views above it, the farthest first so
    /// that the nearest wins: each view's look colour, resolved in the
    /// boundary's tuple. When the boundary writes the nearest view's colour
    /// again, also each pressable view's press colour
    /// (`--exact-press-color-below-<view>`): resolved there when it read a
    /// token, else `initial`, so that the input glue takes the pressed view's
    /// own press colour, not one an outer boundary resolved in its tuple. The
    /// glue paints the boundary with it when that view is pressed.
    fn boundary_rules(&self, id: ViewId, above: &Above) -> String {
        let (at, fonts) = (view(id), &self.font_names);
        let kernel = self.runner.kernel();
        let tuple = self.runner.tuple_at(id);
        let (mut out, mut presses) = (String::new(), String::new());
        for &up in above.views.iter().rev() {
            let looks = self.runner.looks_in(up, tuple);
            let looks = declarations(&looks, |l| crate::css::css_text(l, fonts).0);
            out += &colour_below(&view(up), &looks, |h| format!(":where({h}) {at}"));
            let pressable = kernel
                .node(up)
                .is_some_and(|n| n.style.mask.has(StyleId::PressColor));
            if above.from.is_some() && pressable {
                let press = self.runner.row_in(up, StyleId::PressColor, tuple);
                let colour =
                    press.map_or_else(|| "initial".into(), |p| crate::css::colour(p.press_color));
                write!(presses, "--exact-press-color-below-{up}:{colour};").expect("a String");
            }
        }
        if !presses.is_empty() {
            write!(out, "{at}{{{presses}}}").expect("a String");
        }
        out
    }

    /// Send view `id`'s look rules, and as a boundary its `data-exact-from`,
    /// when they changed (`""` and `None` clear them). A boundary's rules
    /// from above it ([`Host::boundary_rules`]) come before its own.
    pub(in crate::host) fn emit_looks(&mut self, id: ViewId, batch: &mut Batch) {
        let Some(node) = self.runner.kernel().node(id) else {
            return;
        };
        let above = self.above(id);
        let mut out = above
            .as_ref()
            .map_or_else(String::new, |a| self.boundary_rules(id, a));
        let fonts = &self.font_names;
        out += &rules(
            &view(id),
            &declarations(node.looks(), |l| crate::css::css_text(l, fonts).0),
        );
        let from = above.as_ref().and_then(|a| a.from);
        match above {
            Some(_) => self.boundaries.insert(id),
            None => self.boundaries.remove(&id),
        };
        let m = self.mirror.entry(id).or_default();
        if out != m.looks {
            batch.looks(id, &out);
            m.looks = out;
        }
        if from != m.from {
            match from {
                Some(from) => batch.props(id, &[(FROM, from.to_string())], &[]),
                None => batch.props(id, &[], &[FROM]),
            }
            m.from = from;
        }
    }

    /// Each boundary's look rules and `data-exact-from` again, after a
    /// batch that commits, as `modes.js` writes them after each commit: a
    /// look, a colour or a pin above one may change with nothing of its own.
    pub(in crate::host) fn emit_boundaries(&mut self, batch: &mut Batch) {
        for id in self.boundaries.clone() {
            self.emit_looks(id, batch);
        }
    }

    /// Forget destroyed view `id`'s looks: its mirror, and its look rules on
    /// the page.
    pub(in crate::host) fn forget_looks(&mut self, id: ViewId, batch: &mut Batch) {
        self.boundaries.remove(&id);
        if self.mirror.remove(&id).is_some_and(|m| !m.looks.is_empty()) {
            batch.looks(id, "");
        }
    }
}

/// A boundary's attribute naming [`Above::from`].
const FROM: &str = "data-exact-from";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_compose_hover_then_focus_over_the_base() {
        let mut hover = StyleProps {
            opacity: 0.5,
            ..Default::default()
        };
        hover.mask.set(exact_kernel::StyleId::Opacity);
        let looks = [
            (LOOK_HOVER, hover.clone()),
            (exact_kernel::LOOK_FOCUS_VISIBLE, hover),
        ];
        let css = rules(
            "[data-view=\"4\"]",
            &declarations(&looks, |l| crate::css::css_text(l, &[]).0),
        );
        let at = |s: &str| css.find(s).unwrap_or_else(|| panic!("{s} in {css}"));
        assert!(at(":hover") < at("[data-exact-held~=hover]"), "{css}");
        assert!(
            at("[data-exact-held~=hover]") < at(":focus-visible"),
            "{css}"
        );
        assert!(
            css.contains("[data-exact-held~=focus-visible]"),
            "a held state's rule: {css}"
        );
        assert!(css.contains("opacity:0.5!important"), "{css}");
        assert_eq!(
            css.matches("--exact-hover-wash:none!important;--exact-hover-dim:1!important").count(),
            2,
            "the hover replaces the unsaid wash and a bar button's dim, held too, and the focus does not: {css}"
        );
        assert!(
            !css.contains("transition"),
            "the rules add no transition: {css}"
        );
        assert!(rules("x", &[]).is_empty(), "no looks, no rule");
    }
}
