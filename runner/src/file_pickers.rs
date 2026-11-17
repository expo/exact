//! The File System Access API's three pickers, issued as commands (LLP
//! 1069.010 D2): `showOpenFilePicker(id)` (or `(id, multiple)`),
//! `showDirectoryPicker(id)`, `showSaveFilePicker(id, suggestedName)`,
//! each with the API's `startIn` (LLP 1173 D2) as a trailing argument
//! when the call names one.
//! Each opens a document in place: the host mints a `doc:` handle for
//! what the person chose (D1) and reports it as `change` on the element
//! `id` names (several, one per line, under `multiple`), or HTML's `cancel`
//! there. The types are the manifest's `file_handlers`, never the
//! command's, so an app cannot widen them per call.
//!
//! One rule on every host, as for `share` and `saveFile`: the runner
//! refuses an unknown element or bad arguments into the journal (`<name>:
//! refused: <reason>`), holds the request for the agent (capabilities
//! `open-file`, `open-directory`, `save-file`; answered by `type @t
//! <path>…` or `tap @t cancel`, LLP 1069.007 D3), or tells the host to
//! present it. A host without the picker (Linux; a browser without
//! `showOpenFilePicker`) refuses and fires `cancel`.

use crate::agent::{field_bool, field_str, quote};
use crate::{DataSource, Runner};
use exact_kernel::ViewId;
use exact_plan::Value;

/// Which picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `showOpenFilePicker`: files, opened in place.
    Open,
    /// `showDirectoryPicker`: a folder.
    Directory,
    /// `showSaveFilePicker`: a file to write, which may not exist yet.
    Save,
}

impl Kind {
    /// The command's name.
    pub fn command(self) -> &'static str {
        match self {
            Kind::Open => "showOpenFilePicker",
            Kind::Directory => "showDirectoryPicker",
            Kind::Save => "showSaveFilePicker",
        }
    }

    /// The hold's capability in `pending` (LLP 1069.010 §Substitute).
    pub fn capability(self) -> &'static str {
        match self {
            Kind::Open => "open-file",
            Kind::Directory => "open-directory",
            Kind::Save => "save-file",
        }
    }

    /// The picker a command names, if it names one.
    pub fn of(command: &str) -> Option<Kind> {
        [Kind::Open, Kind::Directory, Kind::Save]
            .into_iter()
            .find(|k| k.command() == command)
    }
}

/// A checked picker request.
#[derive(Clone, Debug, PartialEq)]
pub struct Picker {
    /// Which picker.
    pub kind: Kind,
    /// The element whose `change` / `cancel` takes the outcome.
    pub id: String,
    /// `showOpenFilePicker`'s `multiple`.
    pub multiple: bool,
    /// `showSaveFilePicker`'s `suggestedName`.
    pub suggested_name: String,
    /// Where the picker opens (LLP 1173 D2): a [`WELL_KNOWN`] name or a
    /// `doc:/` path, which the presenting host resolves.
    pub start_in: Option<String>,
}

/// The `startIn` names (LLP 1173 D2): the File System Access API's
/// `WellKnownDirectory` values, and `screenshots`, Exact's addition.
pub const WELL_KNOWN: [&str; 7] = [
    "desktop",
    "documents",
    "downloads",
    "music",
    "pictures",
    "videos",
    "screenshots",
];

/// A trailing `startIn` argument: `none` when the call names none.
pub(crate) fn start_in_arg(arg: Option<&Value>) -> Result<Option<String>, ()> {
    match arg {
        None | Some(Value::Option(None)) => Ok(None),
        Some(s @ exact_plan::str_value!()) => Ok(Some(s.text().to_string())),
        Some(Value::Option(Some(v))) if v.is_str() => Ok(Some(v.text().to_string())),
        Some(_) => Err(()),
    }
}

/// Why `start` cannot be where a picker opens, if it cannot: a well-known
/// name or a `doc:/<n>/…` path passes here, and the host refuses a handle
/// the session does not hold.
pub fn start_in_refusal(start: &str) -> Option<String> {
    let handle = start
        .strip_prefix("doc:/")
        .and_then(|rest| rest.split('/').next())
        .is_some_and(|n| n.parse::<u64>().is_ok());
    (!WELL_KNOWN.contains(&start) && !handle).then(|| {
        format!(
            "\"{start}\" is neither a well-known folder ({}) nor a doc:/ path",
            WELL_KNOWN.join(", ")
        )
    })
}

/// A request's `startIn` once the runner has ruled: kept, or refused into
/// the journal as `<command>: startIn: refused: <reason>` and dropped, so
/// the picker still opens, where the platform chooses.
pub(crate) fn ruled_start_in<D: DataSource>(
    runner: &mut Runner<D>,
    command: &str,
    start: Option<String>,
) -> Option<String> {
    let start = start?;
    match start_in_refusal(&start) {
        None => Some(start),
        Some(reason) => {
            runner.log(format!("{command}: startIn: refused: {reason}"));
            None
        }
    }
}

/// `,"startIn":…` for a summary, when there is one.
pub(crate) fn push_start_in(start: &Option<String>, s: &mut String) {
    if let Some(start) = start {
        s.push_str(",\"startIn\":");
        quote(start, s);
    }
}

/// What a host does with a picker command.
#[derive(Clone, Debug, PartialEq)]
pub enum Arm {
    /// Refused into the journal; the host fires `cancel` at the view when
    /// there is one.
    Refused(String, Option<ViewId>),
    /// Held for the agent under this ticket.
    Held(u64),
    /// Present the picker; the outcome goes to the view.
    Present(Picker, ViewId),
}

impl Picker {
    /// The command's positional arguments: `(id)`, `(id, multiple)` or
    /// `(id, suggestedName)`, and with `startIn` named `(id, multiple,
    /// startIn)` (`multiple` `none` when absent), `(id, startIn)` or `(id,
    /// suggestedName, startIn)`.
    pub fn from_args(kind: Kind, args: &[Value]) -> Result<Picker, String> {
        let usage = match kind {
            Kind::Open => "showOpenFilePicker takes (id) or (id, multiple)",
            Kind::Directory => "showDirectoryPicker takes (id)",
            Kind::Save => "showSaveFilePicker takes (id, suggestedName)",
        };
        let Some(id) = args.first().and_then(Value::as_str) else {
            return Err(usage.into());
        };
        let mut picker = Picker {
            kind,
            id: id.to_string(),
            multiple: false,
            suggested_name: String::new(),
            start_in: None,
        };
        let start = |i: usize| start_in_arg(args.get(i)).map_err(|()| usage.to_owned());
        match (kind, &args[1..]) {
            (Kind::Open, []) | (Kind::Directory, []) => {}
            (Kind::Directory, [_]) => picker.start_in = start(1)?,
            (Kind::Open, [multiple] | [multiple, _]) => {
                picker.multiple = match multiple {
                    Value::Bool(multiple) => *multiple,
                    Value::Option(None) => false,
                    _ => return Err(usage.into()),
                };
                picker.start_in = start(2)?;
            }
            (
                Kind::Save,
                [name @ exact_plan::str_value!()] | [name @ exact_plan::str_value!(), _],
            ) => {
                picker.suggested_name = name.text().to_string();
                picker.start_in = start(2)?;
            }
            _ => return Err(usage.into()),
        }
        picker.checked()
    }

    fn checked(self) -> Result<Picker, String> {
        let name = &self.suggested_name;
        if self.kind == Kind::Save
            && (name.is_empty()
                || name.len() > 255
                || name == "."
                || name == ".."
                || name
                    .chars()
                    .any(|c| matches!(c, '/' | '\\') || c.is_control()))
        {
            return Err("suggestedName is not a file name".into());
        }
        Ok(self)
    }

    /// `{"id":…,"multiple":…}` / `{"id":…,"suggestedName":…}`: the hold's
    /// inspection summary and what a presenting host reads.
    pub fn summary(&self) -> String {
        let mut s = String::from("{\"id\":");
        quote(&self.id, &mut s);
        match self.kind {
            Kind::Open => s.push_str(&format!(",\"multiple\":{}", self.multiple)),
            Kind::Directory => {}
            Kind::Save => {
                s.push_str(",\"suggestedName\":");
                quote(&self.suggested_name, &mut s);
            }
        }
        push_start_in(&self.start_in, &mut s);
        s.push('}');
        s
    }
}

/// A host's picker command: refuse, hold for the agent, or present.
/// `available` false is a host with no such picker, which refuses outside
/// the agent.
pub fn arm<D: DataSource>(
    runner: &mut Runner<D>,
    kind: Kind,
    request: Result<Picker, String>,
    agent: bool,
    available: bool,
) -> Arm {
    let refuse = |runner: &mut Runner<D>, reason: String, view: Option<ViewId>| {
        runner.log(format!("{}: refused: {reason}", kind.command()));
        Arm::Refused(reason, view)
    };
    let mut request = match request {
        Ok(request) => request,
        Err(reason) => return refuse(runner, reason, None),
    };
    let view = runner
        .kernel()
        .find_by_id(&request.id)
        .into_iter()
        .find_map(|key| runner.kernel().node_by_key(key).map(|n| n.id));
    let Some(view) = view else {
        let reason = format!("no element with id \"{}\"", request.id);
        return refuse(runner, reason, None);
    };
    request.start_in = ruled_start_in(runner, kind.command(), request.start_in.take());
    if agent {
        let summary = request.summary();
        return Arm::Held(runner.hold(kind.capability(), Some(view), &summary, &[], true));
    }
    if !available {
        return refuse(runner, "unavailable".into(), Some(view));
    }
    Arm::Present(request, view)
}

/// [`arm`] for a host whose command arrived as JSON: `{"command":
/// "showOpenFilePicker","id":…,"multiple":false,"agent":true}` (and
/// `suggestedName` for a save, `startIn` when the call names one,
/// `available` false for a host that has no such picker). The reply is `{"refused":"…","view":N?}`, `{"ticket":N}`
/// or `{"present":true,"view":N,"id":…,…}`. `"agent":true,"present":true`
/// (iOS under `--touch platform`) holds it as under the agent and presents
/// it too, `{"ticket":N,"present":true,…}`: the host answers the hold with
/// what the person chose there, as `type @t` or `tap @t cancel` would.
pub fn request<D: DataSource>(runner: &mut Runner<D>, json: &str) -> String {
    let command = field_str(json, "command").unwrap_or_default();
    if command == "requestPermission" {
        return permission(runner, json);
    }
    let Some(kind) = Kind::of(&command) else {
        return crate::agent::error(&format!("{command} is not a picker"));
    };
    let parsed = match field_str(json, "id") {
        Some(id) => Picker {
            kind,
            id,
            multiple: field_bool(json, "multiple"),
            suggested_name: field_str(json, "suggestedName").unwrap_or_default(),
            start_in: field_str(json, "startIn"),
        }
        .checked(),
        None => Err(format!("{command} names an element id")),
    };
    let available = !json.contains("\"available\":false");
    let shown = field_bool(json, "agent") && field_bool(json, "present");
    match arm(
        runner,
        kind,
        parsed,
        field_bool(json, "agent") && !shown,
        available,
    ) {
        Arm::Refused(reason, view) => {
            let mut s = String::from("{\"refused\":");
            quote(&reason, &mut s);
            if let Some(view) = view {
                s.push_str(&format!(",\"view\":{view}"));
            }
            s.push('}');
            s
        }
        Arm::Held(ticket) => format!("{{\"ticket\":{ticket}}}"),
        Arm::Present(request, view) => {
            let summary = request.summary();
            let held = if shown {
                let ticket = runner.hold(kind.capability(), Some(view), &summary, &[], true);
                format!("\"ticket\":{ticket},")
            } else {
                String::new()
            };
            format!("{{{held}\"present\":true,\"view\":{view},{}", &summary[1..])
        }
    }
}

/// `requestPermission(id, path, mode)` (LLP 1069.010.000 D7): `{"command":
/// "requestPermission","id":…,"path":…,"mode":…}`. A path that is not a
/// `doc:` path, a mode that is neither `read` nor `readwrite`, or an
/// unknown element is refused into the journal (`{"refused":"…"}`); else
/// `{"view":N,"path":…,"mode":…}`, and the host asks (the web, under the
/// press's activation) or answers (a native host: `granted` for a document
/// that resolves), as `change` on the view.
fn permission<D: DataSource>(runner: &mut Runner<D>, json: &str) -> String {
    let id = field_str(json, "id").unwrap_or_default();
    let path = field_str(json, "path").unwrap_or_default();
    let mode = field_str(json, "mode").unwrap_or_default();
    let view = runner
        .kernel()
        .find_by_id(&id)
        .into_iter()
        .find_map(|key| runner.kernel().node_by_key(key).map(|n| n.id));
    let refused = if !path.starts_with("doc:/") {
        "path is not a doc:/ path".to_owned()
    } else if mode != "read" && mode != "readwrite" {
        "mode is \"read\" or \"readwrite\"".to_owned()
    } else if let Some(view) = view {
        let mut s = format!("{{\"view\":{view},\"path\":");
        quote(&path, &mut s);
        s.push_str(",\"mode\":");
        quote(&mode, &mut s);
        s.push('}');
        return s;
    } else {
        format!("no element with id \"{id}\"")
    };
    runner.log(format!("requestPermission: refused: {refused}"));
    let mut s = String::from("{\"refused\":");
    quote(&refused, &mut s);
    s.push('}');
    s
}

/// The paths of a `type @t` answer: one per line, each absolute.
pub fn answer_paths(value: &str) -> Vec<&str> {
    value
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_permission_names_a_document_a_mode_and_an_element() {
        let plan = contract::compile(
            "component App\n  view\n    column\n      input id=\"asked\" display=\"none\"\n",
        )
        .unwrap();
        struct NoData;
        impl DataSource for NoData {
            fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, crate::DataError> {
                Err(crate::DataError::UnknownSource(source.into()))
            }
        }
        let kernel = exact_kernel::Kernel::with_monospace();
        let mut runner = Runner::boot(plan, NoData, kernel, Default::default(), "/").unwrap();
        let ask = |runner: &mut Runner<NoData>, id: &str, path: &str, mode: &str| {
            request(
                runner,
                &format!(
                    r#"{{"command":"requestPermission","id":"{id}","path":"{path}","mode":"{mode}"}}"#
                ),
            )
        };
        let asked = ask(&mut runner, "asked", "doc:/3/notes", "readwrite");
        assert!(asked.starts_with(r#"{"view":"#), "{asked}");
        assert!(
            asked.ends_with(r#","path":"doc:/3/notes","mode":"readwrite"}"#),
            "{asked}"
        );
        assert_eq!(
            ask(&mut runner, "asked", "app:/data/x", "read"),
            r#"{"refused":"path is not a doc:/ path"}"#
        );
        assert_eq!(
            ask(&mut runner, "asked", "doc:/3/notes", "write"),
            r#"{"refused":"mode is \"read\" or \"readwrite\""}"#
        );
        assert_eq!(
            ask(&mut runner, "absent", "doc:/3/notes", "read"),
            r#"{"refused":"no element with id \"absent\""}"#
        );
    }

    #[test]
    fn each_picker_takes_its_own_arguments() {
        let s = Value::str;
        let open = Picker::from_args(Kind::Open, &[s("f")]).unwrap();
        assert_eq!(open.summary(), r#"{"id":"f","multiple":false}"#);
        let many = Picker::from_args(Kind::Open, &[s("f"), Value::Bool(true)]).unwrap();
        assert!(many.multiple);
        assert_eq!(
            Picker::from_args(Kind::Directory, &[s("d")])
                .unwrap()
                .summary(),
            r#"{"id":"d"}"#
        );
        let save = Picker::from_args(Kind::Save, &[s("s"), s("a.md")]).unwrap();
        assert_eq!(save.summary(), r#"{"id":"s","suggestedName":"a.md"}"#);
        assert!(Picker::from_args(Kind::Save, &[s("s")]).is_err());
        assert!(Picker::from_args(Kind::Save, &[s("s"), s("../a")]).is_err());
        assert!(Picker::from_args(Kind::Directory, &[s("d"), Value::Bool(true)]).is_err());
        assert_eq!(Kind::of("showDirectoryPicker"), Some(Kind::Directory));
        assert_eq!(Kind::Save.capability(), "save-file");
        assert_eq!(answer_paths("/a\n /b \n"), ["/a", "/b"]);
    }

    #[test]
    fn start_in_trails_each_pickers_arguments() {
        let (s, none) = (Value::str, Value::Option(None));
        let open = Picker::from_args(Kind::Open, &[s("f"), none.clone(), s("pictures")]).unwrap();
        assert_eq!(
            open.summary(),
            r#"{"id":"f","multiple":false,"startIn":"pictures"}"#
        );
        let many = Picker::from_args(Kind::Open, &[s("f"), Value::Bool(true), none.clone()]);
        assert_eq!(many.unwrap().start_in, None);
        let folder = Picker::from_args(Kind::Directory, &[s("d"), s("doc:/3/notes")]).unwrap();
        assert_eq!(folder.summary(), r#"{"id":"d","startIn":"doc:/3/notes"}"#);
        let save = Picker::from_args(Kind::Save, &[s("s"), s("a.png"), s("screenshots")]);
        assert_eq!(save.unwrap().start_in.as_deref(), Some("screenshots"));
        assert!(Picker::from_args(Kind::Open, &[s("f"), s("pictures")]).is_err());
        assert!(Picker::from_args(Kind::Save, &[s("s"), s("a"), Value::Number(1.0)]).is_err());
        assert!(Picker::from_args(Kind::Directory, &[s("d"), s("a"), s("b")]).is_err());
        // The host resolves a handle; a name outside the set never reaches it.
        assert_eq!(start_in_refusal("videos"), None);
        assert_eq!(start_in_refusal("doc:/12/notes/2026"), None);
        for refused in ["Pictures", "home:/Pictures", "doc:/x/a", "/Users/me", ""] {
            assert!(start_in_refusal(refused).is_some(), "{refused}");
        }
    }

    #[test]
    fn a_start_in_outside_the_set_is_journaled_and_the_picker_still_opens() {
        let plan = contract::compile(
            "component App\n  view\n    column\n      input id=\"f\" display=\"none\"\n",
        )
        .unwrap();
        struct NoData;
        impl DataSource for NoData {
            fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, crate::DataError> {
                Err(crate::DataError::UnknownSource(source.into()))
            }
        }
        let kernel = exact_kernel::Kernel::with_monospace();
        let mut runner = Runner::boot(plan, NoData, kernel, Default::default(), "/").unwrap();
        let ask = |runner: &mut Runner<NoData>, start: &str| {
            request(
                runner,
                &format!(r#"{{"command":"showDirectoryPicker","id":"f","startIn":"{start}"}}"#),
            )
        };
        let named = ask(&mut runner, "downloads");
        assert!(
            named.ends_with(r#","id":"f","startIn":"downloads"}"#),
            "{named}"
        );
        let elsewhere = ask(&mut runner, "/etc");
        assert!(elsewhere.ends_with(r#","id":"f"}"#), "{elsewhere}");
        let journal: Vec<_> = runner.journal().collect();
        assert!(
            journal.iter().any(|line| line
                .ends_with(r#"showDirectoryPicker: startIn: refused: "/etc" is neither a well-known folder (desktop, documents, downloads, music, pictures, videos, screenshots) nor a doc:/ path"#)),
            "{journal:?}"
        );
    }
}
