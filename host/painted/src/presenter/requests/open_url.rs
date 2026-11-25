//! An outgoing command can open a scheme the app declared. Inline links
//! from rendered content keep the narrower link policy (LLP 1133.024).
use super::*;

fn external(url: &str) -> bool {
    crate::text::markup::target(url).is_some_and(|t| !t.starts_with('/') && !t.contains('\0'))
}

fn declares(compat: &str, scheme: &str) -> bool {
    let name = scheme.to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "javascript"
            | "vbscript"
            | "data"
            | "file"
            | "filesystem"
            | "blob"
            | "about"
            | "view-source"
            | "jar"
            | "intent"
            | "content"
            | "doc"
            | "app"
            | "app-settings"
    ) {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(compat)
        .ok()
        .is_some_and(|compat| {
            compat
                .pointer("/inputs/capabilities/openUrlSchemes")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|list| list.iter().any(|s| s.as_str() == Some(name.as_str())))
        })
}

impl<D: DataSource> Presenter<D> {
    pub(in crate::presenter) fn open_url(&mut self, args: &[exact_plan::Value]) {
        let raw = args
            .first()
            .and_then(exact_plan::Value::as_str)
            .unwrap_or_default();
        let url = raw.trim();
        let element = args.get(1).and_then(exact_plan::Value::as_str);
        if external(url) && self.hand_url(url, element) {
            return;
        }
        let settings = raw == "app-settings:";
        let scheme = url.split_once(':').map_or("", |(scheme, _)| scheme);
        let declared = !settings
            && self.has_shell()
            && !scheme.is_empty()
            && !url.contains('\0')
            && declares(&self.compat, scheme);
        if settings && !self.has_shell() {
            self.host
                .log("openURL app-settings: unavailable on this host");
        } else if settings || declared {
            let agent = self.agent || std::env::var("EXACT_AGENT").as_deref() == Ok("1");
            let action = if agent {
                "journaled under the agent"
            } else if settings {
                "opens the app's page in Settings"
            } else {
                "opens the declared scheme"
            };
            self.host.log(format!("openURL {scheme}: {action}"));
            if agent {
                self.answer_open(element, true);
            } else {
                self.queue_open(url, element);
            }
            return;
        } else {
            self.host.log(format!("openURL refused: {url:?} is not an absolute http, https, mailto or tel URL or a declared outgoing scheme"));
        }
        self.answer_open(element, false);
    }

    pub(in crate::presenter) fn leave(&mut self, url: &str) -> bool {
        let url = url.trim();
        external(url) && self.hand_url(url, None)
    }

    fn hand_url(&mut self, url: &str, element: Option<&str>) -> bool {
        if let Some(open) = &self.url_opener {
            let result = open(url);
            if let Err(error) = &result {
                self.host.log(format!("openURL: {error}"));
            }
            self.answer_open(element, result.is_ok());
        } else if self.has_shell() {
            self.queue_open(url, element);
        } else {
            return false;
        }
        true
    }

    fn queue_open(&mut self, url: &str, element: Option<&str>) {
        let id = self.request(Request::Open(url.into()));
        let scheme = url
            .split_once(':')
            .expect("an absolute URL")
            .0
            .to_ascii_lowercase();
        self.requests.waiting.insert(
            id,
            Waiting::Open {
                element: element.map(str::to_owned),
                scheme,
            },
        );
    }

    pub(in crate::presenter) fn opens_replaced(&mut self) {
        self.requests
            .waiting
            .retain(|_, w| !matches!(w, Waiting::Open { .. }));
    }

    pub(in crate::presenter) fn answer_open(&mut self, element: Option<&str>, opened: bool) {
        let Some(id) = element else {
            return;
        };
        let request = serde_json::json!({ "command": "openURL", "id": id });
        let ruling = exact_runner::commands::request(self.host.runner_mut(), &request.to_string());
        let ruling: serde_json::Value = serde_json::from_str(&ruling).unwrap_or_default();
        self.dirty = true;
        let Some(view) = ruling["view"].as_u64() else {
            return;
        };
        let answer = if opened { "opened" } else { "failed" };
        let now = self.host.now();
        let change = Event::Change(ControlValue::Text(answer.into()));
        if let Some(e) = self.host.dispatch_at(view as ViewId, change, now) {
            eprintln!("exact: {e}");
        }
        if let Some(e) = self.after_commit() {
            eprintln!("exact: {e}");
        }
    }
}
