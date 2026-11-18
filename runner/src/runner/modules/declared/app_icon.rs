//! The app's icon among the alternates it ships (`@exact/app-icon`, LLP
//! 1163.000 D4), substituted. The roster is the agent's word
//! `<extension>.icons`, which a drive sets to the names the binary would
//! ship; the icon shown is the session's, and the primary until a choice.
//!
//! - **`"appIcon": "status"`** on a source answers `{available, icon,
//!   icons}`: available on iOS while the roster is not empty or an
//!   alternate shows, and the roster only while available.
//! - **`"appIcon": "set"`** on a send: the icon already shown answers the
//!   status at once, as iOS is not asked. Another of the roster, or none
//!   for the primary, is held under the send's `choose` ticket, whose tap
//!   stands for iOS's alert: the change happens on `ok`, and on `cancel`
//!   too, since dismissing that alert does not undo it. A name outside the
//!   roster is refused `NotFoundError`, and every call while unavailable
//!   `NotSupportedError`.

use super::super::super::{DataSource, Runner};
use super::super::{Decision, Json, ModulesRow, Waits};
use serde_json::json;

impl<D: DataSource> Runner<D> {
    /// The status an `"appIcon": "status"` source answers.
    pub(in crate::runner) fn app_icon_status(&self, row: &ModulesRow) -> Json {
        let word = format!("{}.icons", self.plan.str(row.extension));
        let icons = (self.extensions.preferred.get(&word))
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let icon = &self.extensions.declared.app_icon;
        let available = self.host_target() == "ios" && (!icons.is_empty() || icon.is_some());
        json!({
            "available": available,
            "icon": icon,
            "icons": if available { icons } else { Vec::new() },
        })
    }

    /// `setAppIcon(icon=)`, substituted: the status now, a refusal, or the
    /// alert's ticket.
    pub(in crate::runner::modules) fn app_icon_decide(
        &mut self,
        row: &ModulesRow,
        args: &Json,
    ) -> Decision {
        let status = self.app_icon_status(row);
        let asked = &args["icon"];
        let refused =
            |code: &str, why: String| Decision::Now(json!({ "error": code, "message": why }));
        let decision = if status["available"] != true {
            refused("NotSupportedError", "no alternate icon here".into())
        } else if *asked == status["icon"] {
            Decision::Now(status)
        } else if asked.is_string()
            && !status["icons"]
                .as_array()
                .is_some_and(|i| i.contains(asked))
        {
            refused("NotFoundError", format!("no alternate icon named {asked}"))
        } else {
            return Decision::Hold(Waits::Choose);
        };
        if let Decision::Now(answer) = &decision {
            let outcome = answer["error"].as_str().unwrap_or("unchanged");
            self.log(format!("{}: {outcome}", self.key(row)));
        }
        decision
    }

    /// The alert's ticket answered: the held call's icon shows, and the call
    /// answers it. A status source is asked again only where the app's
    /// mutation `refreshes` it: `appIcon()` is a fact with no topic (LLP
    /// 1163.000 D3), and the platform announces nothing.
    pub(in crate::runner) fn app_icon_chosen(&mut self, row: &ModulesRow, token: u64) {
        let held = self.extensions.held.iter().find(|h| h.token == token);
        let icon = held
            .and_then(|h| h.args["icon"].as_str())
            .map(str::to_owned);
        self.log(format!(
            "{}: shows {}",
            self.key(row),
            icon.as_deref().unwrap_or("the primary icon")
        ));
        self.extensions.declared.app_icon = icon;
        let answer = self.app_icon_status(row);
        self.release(token, answer);
    }

    /// The status sources that read agent word `word` (`<extension>.icons`),
    /// asked again when it is preferred.
    pub(in crate::runner) fn app_icon_statuses(&self, word: &str) -> Vec<String> {
        let Some(extension) = word.strip_suffix(".icons") else {
            return Vec::new();
        };
        (self.plan.modules.iter())
            .filter(|m| self.plan.str(m.extension) == extension)
            .filter(|m| self.substitute(m)["appIcon"] == "status")
            .map(|m| self.key(m))
            .collect()
    }
}
