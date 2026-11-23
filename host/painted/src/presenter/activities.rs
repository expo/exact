//! Live activities on Android (LLP 1126 D13): the reader shows each as an
//! ongoing notification, promoted to a Live Update where the platform allows
//! it. The runner's `exact-activity:` sends go to the reader as requests
//! (kind 24); the reader answers each ticket, and lists what it shows, by
//! `Native.activities`, in the shape Apple's arm reports
//! (`host/apple/src/activities.rs`).
use super::*;

/// Whether the bake mapped any activity to Android (`reach.androidActivities`).
pub(super) fn mapped(compat: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(compat).is_ok_and(|c| {
        c["reach"]["androidActivities"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    })
}

impl<D: DataSource> Presenter<D> {
    /// Link the sends once a shell serves requests (Android's reader) and
    /// the bake mapped an activity. Each boot and reload asks again, since
    /// the runner it links is new.
    pub(super) fn link_activities(&mut self) {
        if !self.has_shell() || !mapped(&self.compat) {
            return;
        }
        if let Some(e) = self.host.link_activities() {
            self.host.log(&e);
        }
        // Linking answers the source again, which demands the list: this
        // commit was the host's, so no presenter turn takes the demand.
        if self.host.runner_mut().take_activities_demand() {
            self.request(requests::Request::Activity(r#"{"enumerate":true}"#.into()));
        }
    }

    /// Hand a send to the reader with its ticket; refuse a body that is not
    /// the runner's object.
    pub(super) fn activity_send(&mut self, ticket: u64, body: &[u8]) {
        match serde_json::from_slice(body) {
            Ok(serde_json::Value::Object(mut send)) => {
                send.insert("ticket".into(), ticket.into());
                let send = serde_json::Value::Object(send).to_string();
                self.request(requests::Request::Activity(send));
            }
            _ => {
                let error = serde_json::json!({"ticket": ticket, "status": 400,
                    "error": "an activity send is a JSON object"});
                if let Some(e) = self.activities(&error.to_string()) {
                    self.host.log(&e);
                }
            }
        }
    }

    /// What the reader reported: one send's answer, or the activities it
    /// shows.
    pub fn activities(&mut self, json: &str) -> Option<String> {
        let error = self.host.activities(json);
        self.dirty = true;
        error.or(self.after_commit())
    }
}

#[cfg(test)]
#[path = "activities_tests.rs"]
mod tests;
