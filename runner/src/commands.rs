//! The one door a host asks the runner through before it runs a command
//! that shows system UI (`exact_command` on the web and Apple): the
//! runner's ruling, refused, held for the agent, or present it. `command`
//! names which: `share` (LLP 1069.003), `saveFile` (LLP 1069.010 D3), the
//! three pickers (D2) and `requestPermission` (LLP 1069.010.000 D7); and
//! `openURL`'s answer, the element that hears whether the URL opened.

use crate::json::{error, field_str};
use crate::local_network::Reach;
use crate::{DataSource, Runner, Value};
use exact_kernel::ViewId;

/// A host-facing effect an action asked for; executed after commit, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    /// A bare `focus()` carries the issuing work's reach to the window host.
    pub window_reach: Option<Reach>,
    /// The capability name.
    pub name: String,
    /// Its arguments.
    pub args: Vec<Value>,
    /// The node whose input ran the action, when a host event did: where a command that shows system UI anchors it (LLP 1069.003 D3). `None` for a timer, an answer, or anything else no input dispatched.
    pub source: Option<ViewId>,
}

impl Command {
    /// Requests that can show the session's page, open another app window,
    /// or start the app again. Show and restore of the session's window
    /// are both bare `focus()`. An element's focus stays inside the page.
    pub(crate) fn shows_window(&self) -> bool {
        match self.name.as_str() {
            "focus" | "reload" => self.args.is_empty(),
            "panel" | "relaunch" | "openDevTools" => true,
            _ => false,
        }
    }
}

/// `{"command":"share"|"saveFile",…}`: the named command's ruling, as its
/// module's `request` writes it.
pub fn request<D: DataSource>(runner: &mut Runner<D>, json: &str) -> String {
    // Each command's ruling is reached through what the runner links (LLP
    // 1047 D3): a plan that runs one is refused at boot where it isn't.
    let links = runner.device_links();
    match field_str(json, "command").as_deref() {
        Some("share") => match links.share {
            Some((request, _)) => request(runner, json),
            None => error("share is not linked into this artifact"),
        },
        Some("saveFile") => match links.documents {
            Some((save, _)) => save(runner, json),
            None => error("saveFile is not linked into this artifact"),
        },
        Some(
            "showOpenFilePicker"
            | "showDirectoryPicker"
            | "showSaveFilePicker"
            | "requestPermission",
        ) => match links.documents {
            Some((_, pick)) => pick(runner, json),
            None => error("the file pickers are not linked into this artifact"),
        },
        Some("openURL") => open_url(runner, json),
        Some(other) => error(&format!("no ruling for command {other}")),
        None => error("no command"),
    }
}

/// `{"command":"openURL","id":…}`: `openURL(url, id)`'s answer goes to that
/// element's `change` (`opened` or `failed`), `{"view":N}`; an id no live
/// element has is said in the journal, `{"error":…}`, and nothing hears it.
fn open_url<D: DataSource>(runner: &mut Runner<D>, json: &str) -> String {
    let id = field_str(json, "id").unwrap_or_default();
    let kernel = runner.kernel();
    let view = kernel
        .find_by_id(&id)
        .into_iter()
        .find_map(|key| kernel.node_by_key(key).map(|n| n.id));
    match view {
        Some(view) => format!("{{\"view\":{view}}}"),
        None => {
            let why = format!("no element with id \"{id}\" hears whether the URL opened");
            runner.log(format!("openURL: {why}"));
            error(&why)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_plan::Value;

    struct NoData;
    impl DataSource for NoData {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, crate::DataError> {
            Err(crate::DataError::UnknownSource(source.into()))
        }
    }

    #[test]
    fn an_open_is_answered_at_the_element_its_id_names() {
        let plan = contract::compile(
            "component App\n  view\n    column\n      input id=\"opened\" display=\"none\"\n",
        )
        .unwrap();
        let kernel = exact_kernel::Kernel::with_monospace();
        let mut runner = Runner::boot(plan, NoData, kernel, Default::default(), "/").unwrap();
        let found = request(&mut runner, r#"{"command":"openURL","id":"opened"}"#);
        assert!(found.starts_with(r#"{"view":"#), "{found}");
        let absent = request(&mut runner, r#"{"command":"openURL","id":"absent"}"#);
        assert!(
            absent.starts_with(r#"{"error":"no element with id"#),
            "{absent}"
        );
    }
}
