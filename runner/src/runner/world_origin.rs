//! A world's message carries what caused it (LLP 1133.022 D6, D10 item 6).
//!
//! A `postMessage` to a canvas from a commit that was not a person's or the
//! app's own carries that commit's restricted origins in front of its text;
//! the world keeps the union of the origins posted into it until a fresh
//! start, and marks every message it emits with that set
//! (`game/engine/src/sim/origin.rs`, which reads and writes the same mark).
//! The canvas's `message=` handler then runs under every one of them, joined
//! with what the message would run as unmarked, so a tool that posts into a
//! world cannot start a device through the world's later emissions, and a
//! mark another view writes cannot raise its message. The hosts carry the
//! text unchanged.

use super::provenance::{Cause, Causes, ORIGINS};
use super::{CallOrigin, Command};
use exact_plan::Value;

/// What precedes a marked message's text.
const ORIGIN_MARK: &str = "\u{1}origin:";

/// `text`'s set of origins and the text after its mark; `None` for an
/// unmarked one. A mark can only lower a commit's origin, never raise it:
/// one the runner cannot read (`app`, an unknown or empty word) names every
/// restricted origin, as does one with no end.
pub(super) fn unmark(text: &str) -> Option<(Causes, &str)> {
    let marked = text.strip_prefix(ORIGIN_MARK)?;
    // A mark with no end names nothing readable.
    let (words, text) = marked.split_once('\u{1}').unwrap_or(("", ""));
    let restricted = || ORIGINS.into_iter().filter(|o| *o != CallOrigin::App);
    let named = words.split(',').try_fold(Causes::default(), |set, word| {
        let origin = restricted().find(|o| o.word() == word)?;
        Some(set.union(Causes::of(origin)))
    });
    let every = || restricted().fold(Causes::default(), |set, o| set.union(Causes::of(o)));
    Some((named.unwrap_or_else(every), text))
}

/// Each `postMessage` to a canvas (two arguments: an iframe's carries a
/// third) a commit of `cause` stated: its text carries the cause's
/// restricted origins, unless the cause is the app's alone.
pub(super) fn mark_world_posts(cause: Cause, commands: &mut [Command]) {
    if cause.origin() == CallOrigin::App {
        return;
    }
    let causes = cause.causes();
    let words: Vec<&str> = (ORIGINS.into_iter())
        .filter(|o| *o != CallOrigin::App && causes.has(*o))
        .map(CallOrigin::word)
        .collect();
    let words = words.join(",");
    for command in commands
        .iter_mut()
        .filter(|c| c.name == "postMessage" && c.args.len() == 2)
    {
        if let Some(text) = command.args[0].as_str() {
            let marked = format!("{ORIGIN_MARK}{words}\u{1}{text}");
            command.args[0] = Value::str(&marked);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(origins: &[CallOrigin]) -> Causes {
        (origins.iter()).fold(Causes::default(), |s, o| s.union(Causes::of(*o)))
    }

    #[test]
    fn a_mark_reads_as_its_set_and_never_as_the_apps() {
        use CallOrigin::*;
        assert_eq!(
            unmark("\u{1}origin:tool\u{1}buzz"),
            Some((set(&[Tool]), "buzz"))
        );
        assert_eq!(
            unmark("\u{1}origin:device,action\u{1}"),
            Some((set(&[Device, Action]), ""))
        );
        let every = set(&[Device, Wake, Tool, Action]);
        for unread in ["app", "root", "", "tool,", "tool,app"] {
            let text = format!("\u{1}origin:{unread}\u{1}x");
            assert_eq!(unmark(&text), Some((every, "x")), "{unread:?}");
        }
        assert_eq!(unmark("buzz"), None);
        assert_eq!(unmark("\u{1}origin:tool"), Some((every, "")));
    }
}
