//! A node's `:hover` and `:focus-visible` looks on the page (LLP 1121 D8,
//! stage 8 B), as one rule a state in the page's looks sheet, composed in
//! that order over the node's inline base, so rows a look doesn't name show
//! the base. They show at once (no transition: a look's colours snap, under
//! Reduce Motion too), and under the shell's pressed paint, which `:active`
//! lowers to (LLP 1061 D7, the input glue's important rule per painting node).

use super::*;
use exact_kernel::{StyleProps, LOOK_HOVER};

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
        let mut rows: String = (css.split_terminator(';'))
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
    }
    out
}

/// How many looks, nearest first, whose colour reaches a boundary `modes.js`
/// names by position (`data-x-lc="1:M 2:N"`); one further out takes the last.
pub const LOOK_DEPTH: usize = 3;

/// Each look's `color` as page rules for the boundaries below the element
/// `at` selects that name look set `set`, in [`rules`]' order; empty for
/// none. A mode or `color-scheme` boundary that does not set its own colour
/// (the JS target's `modes.js` marks it) resolves tokens in its own modes,
/// and writes an inherited token colour again on itself, which would stop
/// the look's; written on the boundary, the look's colour reaches its
/// subtree, its token resolved there. As CSS inheritance would have it, the
/// boundary's own look stands over it (the holder is in `:where()`), and the
/// nearest look that holds over one further out (a nearer position repeats
/// its attribute more).
pub fn colour_below(at: &str, looks: &[(u8, String)], set: usize) -> String {
    let below: Vec<String> = (1..=LOOK_DEPTH)
        .map(|k| format!("[data-x-lc~=\"{k}:{set}\"]").repeat(LOOK_DEPTH + 1 - k))
        .collect();
    let list = |holder: &str| {
        (below.iter())
            .map(|b| format!(":where({holder}) {b}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut out = String::new();
    for (state, css) in looks {
        let row = (css.split_terminator(';')).find(|row| row.trim_start().starts_with("color:"));
        if let Some(row) = row {
            holding(&mut out, *state, at, &format!("{row}!important;"), list);
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

impl<D: DataSource> Host<D> {
    /// Send the node's look rules when they changed (`""` clears them).
    pub(in crate::host) fn emit_looks(&mut self, id: ViewId, batch: &mut Batch) {
        let Some(node) = self.runner.kernel().node(id) else {
            return;
        };
        let fonts = &self.font_names;
        let looks = declarations(node.looks(), |l| crate::css::css_text(l, fonts).0);
        let rules = rules(&format!("[data-view=\"{id}\"]"), &looks);
        let m = self.mirror.entry(id).or_default();
        if rules != m.looks {
            batch.looks(id, &rules);
            m.looks = rules;
        }
    }
}

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
