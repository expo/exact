//! Where a warning stands among a build's (LLP 1116 D6), source order kept
//! within a rank: first what changes what the app shows, or how a person
//! reaches it, on its native targets (a header item the bar leaves out, a
//! button VoiceOver cannot name, a scroller that grows, a flex property on a
//! block); then the rest; then naming advice that changes nothing shown;
//! last a symbol blank on the web alone. The euxy port's
//! `lower-header-unplaced` was one of 43, after 13 aliases. Lowering keeps
//! its first [`crate::MAX_WARNINGS`] by this order, and `contract` orders a
//! build's warnings by it, so a warning that changes the app is never the
//! one the cap drops.

use crate::LowerError;

/// The warning `id`, saying `message`, by its rank: 0 first.
pub fn rank(id: &str, message: &str) -> u8 {
    if web_only(id, message) {
        return 3;
    }
    match id {
        "lower-header-unplaced"
        | "lower-header-nested"
        | "lower-header-routeless"
        | "lower-header-no-bar"
        | "lower-navigation-key"
        | "lower-screen-by-state"
        | "lower-button-unlabelled"
        | "lower-nested-button"
        | "lower-round-button-label"
        | "lower-dialog-role"
        | "lower-level-without-heading"
        | "lower-scroll-grows"
        | "lower-flex-on-block"
        | "lower-width-padding"
        | "lower-swipe-wiring"
        | "lower-transform-drag-ineligible"
        | "lower-row-delete-button"
        | "lower-delete-row"
        | "lower-sound-undeclared"
        | "lower-symbol-tint" => 0,
        "lower-symbol-alias" => 2,
        _ => 1,
    }
}

/// A warning only the web's picture shows: an SF Symbol drawn blank where
/// the web is the one target besides Apple's (with Linux too, it is blank on
/// a native host). A build's text says these in one line.
pub fn web_only(id: &str, message: &str) -> bool {
    // Its first use names the targets, `ships web (…) and linux (…), where
    // it is blank`; a later one, `blank on web too`.
    let shipped = message
        .split_once(" ships ")
        .and_then(|(_, rest)| rest.split_once(", where it is blank"))
        .map(|(targets, _)| targets);
    id == "lower-symbol-platform"
        && match shipped {
            Some(targets) => targets.starts_with("web (") && !targets.contains(") and "),
            None => message.contains(" blank on web too"),
        }
}

/// A warning that is information, not a problem: `tamber-unreachable` names
/// a control whose host draws the platform's own look, and `style-undrawn` a
/// style a host the app ships does not draw (LLP 1159.000 D4), each of which
/// the author should know of and cannot change (LLP 1121 D9, amended
/// 2026-10-30). A build counts it apart, and verify and `test` do not count
/// it (`scripts/app-cli.mjs` `information`).
pub fn information(id: &str) -> bool {
    id == "tamber-unreachable" || id == "style-undrawn"
}

/// The first [`crate::MAX_WARNINGS`] of `out` by rank, source order kept
/// within one.
pub(crate) fn keep_first(out: &mut Vec<LowerError>) {
    out.sort_by_key(|w| rank(w.id, &w.message));
    out.truncate(crate::MAX_WARNINGS);
}

/// A `lower-colour-one-scheme` line for one site: a literal colour or a
/// theme token, not the closing line.
pub fn one_scheme_site(id: &str, message: &str) -> bool {
    id == "lower-colour-one-scheme" && !message.starts_with("these ")
}

/// The closing line of the colours that stay one in dark mode.
pub fn one_scheme_close(id: &str, message: &str) -> bool {
    id == "lower-colour-one-scheme" && message.starts_with("these ")
}

/// The one line a build's text says for its colours that stay one in dark
/// mode, past one, each as said (`file:line:col warning [id] …`): their
/// closing line with the first three sites' `file:line`, theme tokens
/// included, at the first's place (LLP 1168.000 D7.2: a chat app said 31 in
/// every build). `contract build --json` lists each, with a token's own
/// repair; scripts/app-cli.mjs `oneScheme` is the same.
pub fn one_scheme_line(close: &str, sites: &[String]) -> String {
    let place = |said: &str| {
        said.split_once(" warning [")
            .map_or("", |(at, _)| at)
            .to_string()
    };
    let line = |said: &str| {
        let at = place(said);
        at.rsplit_once(':')
            .map_or(at.clone(), |(line, _)| line.to_string())
    };
    let mut shown: Vec<String> = sites.iter().take(3).map(|s| line(s)).collect();
    if sites.len() > 3 {
        shown.push("…".into());
    }
    let message = close.split_once("] ").map_or(close, |(_, message)| message);
    // After `these N colours`, `these N literal colours` or `these N tokens`.
    let named = [" colours ", " tokens "]
        .iter()
        .filter_map(|word| message.find(word).map(|at| at + word.len() - 1))
        .min()
        .unwrap_or(message.len());
    let mut message = format!(
        "{} ({}){}",
        &message[..named],
        shown.join(", "),
        &message[named..]
    )
    .replace(
        "each line above says how to repair it",
        "`contract build --json` says how to repair each",
    )
    .replace(
        " (each token's line says its own repair)",
        " (`contract build --json` says each token's repair)",
    );
    if !message.contains("`contract build --json`") {
        message.push_str(" `contract build --json` names each.");
    }
    format!(
        "{} warning [lower-colour-one-scheme] {message}",
        place(&sites[0])
    )
}

#[cfg(test)]
mod tests {
    use super::one_scheme_line;

    const SITE: &str =
        "app.contract:4:9 warning [lower-colour-one-scheme] `#111111` on `color` stays the same";

    fn sites(lines: &[u32]) -> Vec<String> {
        lines
            .iter()
            .map(|l| SITE.replace(":4:9", &format!(":{l}:9")))
            .collect()
    }

    #[test]
    fn the_one_line_names_three_sites_and_a_tokens_repair_is_jsons() {
        // As scripts/one-scheme.test.mjs says them: a mixed set's line, and tokens alone.
        let mixed = "app.contract:2:3 warning [lower-colour-one-scheme] these 5 colours stay the same in dark mode: name a role for each literal (each token's line says its own repair) or leave it unsaid.";
        assert_eq!(
            one_scheme_line(mixed, &sites(&[2, 4, 5, 7, 9])),
            "app.contract:2:9 warning [lower-colour-one-scheme] these 5 colours (app.contract:2, app.contract:4, app.contract:5, …) stay the same in dark mode: name a role for each literal (`contract build --json` says each token's repair) or leave it unsaid."
        );
        let tokens = "app.contract:2:3 warning [lower-colour-one-scheme] these 2 tokens are the same colour in light and dark: each line above says how to repair it";
        assert_eq!(
            one_scheme_line(tokens, &sites(&[2, 3])),
            "app.contract:2:9 warning [lower-colour-one-scheme] these 2 tokens (app.contract:2, app.contract:3) are the same colour in light and dark: `contract build --json` says how to repair each"
        );
        let literal = "app.contract:4:9 warning [lower-colour-one-scheme] these 2 literal colours stay the same in dark mode.";
        assert!(one_scheme_line(literal, &sites(&[4, 5])).ends_with(
            "these 2 literal colours (app.contract:4, app.contract:5) stay the same in dark mode. `contract build --json` names each."
        ));
    }
}
