//! A world's message carries what caused it (LLP 1133.022 D6, D10 item 6).
//!
//! A `postMessage` to a canvas from a commit that was not a person's or the
//! app's own carries that commit's origin in front of its text; the world
//! keeps the least privileged origin posted into it until a fresh start,
//! and marks every message it emits with it (`game/engine/src/sim/origin.rs`,
//! which reads and writes the same mark). The canvas's `message=` handler
//! then runs as a commit of that origin, so a tool that posts into a world
//! cannot start a device through the world's later emissions. The hosts
//! carry the text unchanged.

use super::provenance::Cause;
use super::{CallOrigin, Command};
use exact_plan::Value;

/// What precedes a marked message's text.
const ORIGIN_MARK: &str = "\u{1}origin:";

/// `text`'s origin and the text after its mark; `None` for an unmarked one.
/// A mark can only lower a commit's origin, never raise it: `app`, or a
/// word the runner does not know, reads as the device's.
pub(super) fn unmark(text: &str) -> Option<(CallOrigin, &str)> {
    let (word, text) = text.strip_prefix(ORIGIN_MARK)?.split_once('\u{1}')?;
    let origin = match word {
        "tool" => CallOrigin::Tool,
        "wake" => CallOrigin::Wake,
        "action" => CallOrigin::Action,
        _ => CallOrigin::Device,
    };
    Some((origin, text))
}

/// The origin a world's posts from a commit of `cause` carry: its own; or,
/// for the app's commit that a background action's or a tool's commit
/// joined (a request it asked that theirs kept), the action's, else the
/// tool's (LLP 1133.023 D7a). A wake or the device that joined is left out:
/// the world keeps the highest-ranked mark posted into it
/// (`game/engine/src/sim/origin.rs`), and either would replace a tool's,
/// losing its command rules.
pub(super) fn mark_origin(cause: Cause) -> CallOrigin {
    match cause.origin {
        CallOrigin::App => ([CallOrigin::Action, CallOrigin::Tool].into_iter())
            .find(|o| cause.causes.has(*o))
            .unwrap_or(CallOrigin::App),
        origin => origin,
    }
}

/// Each `postMessage` to a canvas (two arguments: an iframe's carries a
/// third) a commit of `origin` stated: its text carries the origin, unless
/// the commit is the app's.
pub(super) fn mark_world_posts(origin: CallOrigin, commands: &mut [Command]) {
    if origin == CallOrigin::App {
        return;
    }
    for command in commands
        .iter_mut()
        .filter(|c| c.name == "postMessage" && c.args.len() == 2)
    {
        if let Some(text) = command.args[0].as_str() {
            let marked = format!("{ORIGIN_MARK}{}\u{1}{text}", origin.word());
            command.args[0] = Value::str(&marked);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app's commit an action or a tool joined posts under it; any
    /// other commit under its own origin.
    #[test]
    fn an_apps_commit_an_action_or_a_tool_joined_posts_under_it() {
        let app = Cause::of(CallOrigin::App, 0, false);
        let action = Cause::of(CallOrigin::Action, 3, true);
        let tool = Cause::of(CallOrigin::Tool, 4, true);
        let wake = Cause::of(CallOrigin::Wake, 5, false);
        let device = Cause::none();
        assert_eq!(mark_origin(app), CallOrigin::App);
        assert_eq!(mark_origin(app.joined(action.causes)), CallOrigin::Action);
        let both = app.joined(tool.causes).joined(action.causes);
        assert_eq!(mark_origin(both), CallOrigin::Action);
        assert_eq!(mark_origin(app.joined(tool.causes)), CallOrigin::Tool);
        assert_eq!(mark_origin(app.joined(wake.causes)), CallOrigin::App);
        assert_eq!(mark_origin(app.joined(device.causes)), CallOrigin::App);
        assert_eq!(mark_origin(tool.joined(action.causes)), CallOrigin::Tool);
        assert_eq!(mark_origin(device.joined(tool.causes)), CallOrigin::Device);
    }

    #[test]
    fn a_mark_reads_as_its_origin_and_never_as_the_apps() {
        assert_eq!(
            unmark("\u{1}origin:tool\u{1}buzz"),
            Some((CallOrigin::Tool, "buzz"))
        );
        assert_eq!(
            unmark("\u{1}origin:wake\u{1}"),
            Some((CallOrigin::Wake, ""))
        );
        assert_eq!(
            unmark("\u{1}origin:app\u{1}x"),
            Some((CallOrigin::Device, "x"))
        );
        assert_eq!(
            unmark("\u{1}origin:root\u{1}x"),
            Some((CallOrigin::Device, "x"))
        );
        assert_eq!(unmark("buzz"), None);
        assert_eq!(unmark("\u{1}origin:tool"), None);
    }
}
