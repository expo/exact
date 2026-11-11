//! The page's canvas on the web (LLP 1115 D2). A browser paints the `html` element's background behind an iPhone's
//! status bar, in the overscroll and past a short page. The native hosts paint their window with the first root's
//! opaque background (`PresenterIOS.paintCanvas`, Linux `Chrome::background`). CSS carries the root element's own
//! background to the canvas (CSS Backgrounds 3 §2.11.1), but the root node is inside `#exact-root`, so the shell's
//! `html` rule reads `--exact-root-background` (`neutral.css`). A static background sets it in a `:root` rule of the
//! page's first sheet, there before the runtime runs; a bound one sets it on the `html` element (rows.rs, said.js `canvasColor`).

use exact_kernel::style::{Color, ColorValue};

/// The `:root` rule that gives the canvas the root's background, from its class `css`, unless the kernel reads that
/// colour as translucent in either scheme; a `var()` it cannot read is taken as written. A bound background is not in
/// the class. The rule also takes the class's `color-scheme` and custom properties, which the colour may read:
/// `light-dark()` and `var()` resolve on the `html` element.
pub(crate) fn rule(css: &str) -> String {
    let Some(value) = crate::modes::declaration(css, "background-color") else {
        return String::new();
    };
    let parsed =
        ColorValue::parse_light_dark(value).or_else(|| Color::parse(value).map(ColorValue::Fixed));
    if parsed.is_some_and(|c| c.resolve(false).a() < 255 || c.resolve(true).a() < 255) {
        return String::new();
    }
    let mut out = format!(":root{{--exact-root-background:{value};");
    for (name, value) in crate::modes::declarations(css) {
        if name.starts_with("--") || name == "color-scheme" {
            out.push_str(&format!("{name}:{value};"));
        }
    }
    out + "}"
}

/// The statement that writes the root's bound background `value` (a function's source) as the canvas, through the
/// runtime's style writer `write` and its opacity test `opaque` (said.js `canvasColor`).
pub(crate) fn bound(write: &str, opaque: &str, value: &str) -> String {
    format!("{write}(document.documentElement,\"--exact-root-background\",\"\",()=>{opaque}(({value})()));")
}

#[cfg(test)]
mod tests {
    /// The root's opaque background reaches the canvas, with the scheme it resolves in; a translucent one, a
    /// route's, and a bound one (the runtime's) do not.
    #[test]
    fn the_root_background_is_the_canvas() {
        let out = |view: &str| {
            let plan =
                contract::compile(&format!("component App\n  state s = true\n  view\n{view}"))
                    .unwrap();
            crate::emit::emit(&plan, false, false).unwrap()
        };
        let css = out(
            "    column background-color=\"#1e1e1f\" color-scheme=\"dark\"\n      text \"a\"\n",
        )
        .css;
        assert!(
            css.contains(":root{--exact-root-background:rgba(30,30,31,1);color-scheme:dark;}"),
            "{css}"
        );
        // A theme's `var()` is taken as written, and the root's custom properties go with it.
        let css = out("    column background-color=\"-exact-secondary-background\" accent-color=\"#245b9e\"\n      text \"a\"\n").css;
        assert!(
            css.contains(":root{--exact-root-background:var(--exact-secondary-background")
                && css.contains("--exact-accent:"),
            "{css}"
        );
        for view in [
            "    column background-color=\"#1e1e1f80\"\n      text \"a\"\n",
            "    column background-color=\"light-dark(#fff, transparent)\"\n      text \"a\"\n",
            "    column\n      column background-color=\"#1e1e1f\"\n        text \"a\"\n",
        ] {
            let css = out(view).css;
            assert!(!css.contains("--exact-root-background"), "{view}: {css}");
        }
        let bound =
            out("    column background-color=(s ? \"#1e1e1f\" : \"#ffffff\")\n      text \"a\"\n");
        assert!(
            !bound.css.contains("--exact-root-background")
                && bound.js.contains("\"--exact-root-background\""),
            "{}",
            bound.js
        );
    }
}
