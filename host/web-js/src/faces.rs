//! Declared fonts (LLP 1019) and Canvas 2D surface arguments (LLP 1056) in
//! the JS target (LLP 1071 §7).
//!
//! Fonts are the stylesheet's, not the runtime's: each declared face is an
//! `@font-face` rule under the family name the web host's CSS uses
//! (`exact_web::host::fonts`), with `font-display: optional` — the wasm
//! host's own policy (glue.js `prepareFonts`: a face not ready within about
//! 100 ms stays unused for the page, no swap after paint) — and a preload in
//! the served head, so a face is usually there before the text that needs it.
//! The page's first render waits for its faces up to those 100 ms, as the wasm
//! host's does (build.mjs's `main.js`): an `optional` face first needed while
//! still loading would otherwise never be used, as with an uncached font file.

use exact_plan::Plan;

/// The faces' `@font-face` rules, the preload links for the head, and the
/// declared-name → CSS-family map canvas text uses (`exact.fontAliases`,
/// LLP 1056 D8), as a JavaScript object literal; empty when none.
pub struct Fonts {
    pub css: String,
    pub preloads: String,
    pub aliases: String,
}

pub fn fonts(plan: &Plan) -> Result<Fonts, String> {
    let faces = exact_web::host::fonts::font_faces(plan);
    let mut out = Fonts {
        css: String::new(),
        preloads: String::new(),
        aliases: String::new(),
    };
    let mut aliases = Vec::new();
    for f in &faces {
        if f.source
            .chars()
            .any(|c| matches!(c, '"' | '\\' | '<' | '>' | '\n' | '\r'))
        {
            return Err(format!("font source {:?} can't be a CSS string", f.source));
        }
        out.css.push_str(&format!(
            "@font-face{{font-family:\"{}\";src:url(\"{}\");font-weight:{};font-stretch:{};font-style:{};font-display:optional}}",
            f.family,
            f.source,
            f.weight_css(),
            f.stretch_css(),
            if f.italic { "italic" } else { "normal" }
        ));
        // host/web/src/page.rs `fonts`: a preload each, for the formats a
        // browser names.
        let kind = f.source.rsplit('.').next().map(str::to_ascii_lowercase);
        if let Some(kind) = kind.filter(|k| matches!(k.as_str(), "ttf" | "otf" | "woff" | "woff2"))
        {
            out.preloads.push_str(&format!(
                "<link rel=\"preload\" href=\"{}\" as=\"font\" type=\"font/{kind}\" crossorigin>\n",
                f.source
            ));
        }
        let pair = format!(
            "{}:{}",
            serde_json::to_string(&f.declared).unwrap(),
            serde_json::to_string(&f.family).unwrap()
        );
        if !aliases.contains(&pair) {
            aliases.push(pair);
        }
    }
    if !aliases.is_empty() {
        out.aliases = format!("{{{}}}", aliases.join(","));
    }
    Ok(out)
}
