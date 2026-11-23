//! The reader's live activity reports (LLP 1126 D13): a send's answer by
//! its ticket, or the activities it shows, as Apple's arm reports them
//! (`host/apple/src/activities.rs`).
use super::*;
use exact_runner::Response;

impl<D: DataSource> Host<D> {
    /// Link the activity sends into this runner.
    pub fn link_activities(&mut self) -> Option<String> {
        match self.runner.link_activities() {
            Ok(Some(receipt)) => self.commit(
                &[Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => None,
            Err(e) => Some(format!("activities: {e:?}")),
        }
    }

    /// A snapshot `{activities, frequentPushesEnabled}`, the completion of
    /// one send `{ticket, status, body|error}`, or a failure `{error}`.
    pub fn activities(&mut self, json: &str) -> Option<String> {
        let value: serde_json::Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return Some(format!("activities: {e}")),
        };
        if let Some(ticket) = value["ticket"].as_u64() {
            let status = value["status"].as_u64().unwrap_or(500) as u16;
            let body = if status == 200 {
                serde_json::to_vec(&value["body"]).unwrap_or_default()
            } else {
                (value["error"].as_str())
                    .unwrap_or("activity failed")
                    .as_bytes()
                    .to_vec()
            };
            let outcome = Outcome::Response(Response {
                status,
                headers: vec![],
                body,
            });
            return self.fulfill_all(vec![(ticket, outcome, None)], self.now_ms);
        }
        if let Some(error) = value["error"].as_str() {
            self.runner.activities_failed();
            self.log(&format!("activities: {error}"));
            return Some(format!("activities: {error}"));
        }
        match self.runner.set_activities(json) {
            Ok(Some(receipt)) => self.commit(
                &[Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => None,
            Err(e) => Some(format!("activities: {e:?}")),
        }
    }
}
