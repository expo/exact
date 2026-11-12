//! The runner's outstanding work: time-to-interactive (TTI) is reached when it
//! is clear. It has no clock; the host timestamps the moment it clears.
//! Only what the screen still lacks counts: a request for a resource already
//! showing an answer (kept, baked or settled) is a background refresh, and a
//! mutation the app sent is a write, so neither holds TTI. Device holds don't
//! count because they wait on a person. A resource whose current arguments
//! failed counts as done.

use super::{Runner, Target};

use crate::DataSource;

/// An `after` task armed with at most this delay counts as startup work.
pub const STARTUP_TIMER_WINDOW_MS: f64 = 1000.0;

/// The runner's outstanding work, by kind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outstanding {
    /// Requests in flight for resources showing no answer yet.
    pub requests: Vec<String>,
    /// Streams for resources showing no answer, before their first value commits.
    pub streams: Vec<String>,
    /// Resources showing a placeholder until their source can answer.
    pub awaiting: Vec<String>,
    /// Armed one-shot tasks due within the startup window, by action name.
    pub one_shots: Vec<String>,
    /// Resources whose current arguments failed: settled, but in error.
    pub failed: Vec<String>,
    /// Mounted elements marked `aria-busy`, by test id, else view id.
    pub busy: Vec<String>,
    /// The data source can answer.
    pub data_ready: bool,
    /// A refused commit poisoned the runner: every action now fails.
    pub poisoned: bool,
}

impl Outstanding {
    /// `{"clear":…,"requests":[…],…}`: the agent's `outstanding` reply.
    pub fn json(&self) -> String {
        let mut out = format!(
            "{{\"clear\":{},\"dataReady\":{},\"poisoned\":{}",
            self.is_clear(),
            self.data_ready,
            self.poisoned
        );
        for (name, items) in [
            ("requests", &self.requests),
            ("streams", &self.streams),
            ("awaiting", &self.awaiting),
            ("oneShots", &self.one_shots),
            ("failed", &self.failed),
            ("busy", &self.busy),
        ] {
            out.push_str(&format!(",\"{name}\":["));
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                crate::agent::quote(item, &mut out);
            }
            out.push(']');
        }
        out.push('}');
        out
    }

    /// Nothing the app started is still on its way.
    pub fn is_clear(&self) -> bool {
        self.requests.is_empty()
            && self.streams.is_empty()
            && self.awaiting.is_empty()
            && self.one_shots.is_empty()
            && self.busy.is_empty()
    }
}

impl<D: DataSource> Runner<D> {
    /// What the app is still doing, as of the last commit.
    pub fn outstanding(&self) -> Outstanding {
        let mut out = Outstanding {
            data_ready: self.data.ready(),
            poisoned: self.poisoned,
            ..Outstanding::default()
        };
        let mut listed = vec![false; self.plan.resources.len()];
        for p in &self.pending {
            if !p.in_flight() || self.device_holds.iter().any(|h| h.ticket == p.ticket) {
                continue;
            }
            let Target::Resource(i) = p.target else {
                continue;
            };
            if matches!(&self.resources[i], Some(s) if !s.placeholder) {
                continue;
            }
            if std::mem::replace(&mut listed[i], true) {
                continue;
            }
            let name = self.target_name(p.target);
            if p.stream.is_some() {
                out.streams.push(name);
            } else {
                out.requests.push(name);
            }
        }
        for i in 0..self.plan.resources.len() {
            let name = || self.plan.str(self.plan.resources[i].name).to_string();
            if matches!((&self.failed_args[i], &self.resources[i]), (Some(f), Some(s)) if f == &s.args)
            {
                out.failed.push(name());
            } else if self.awaiting[i] {
                out.awaiting.push(name());
            }
        }
        for (t, row) in self.timers.iter().zip(&self.plan.timers) {
            if row.once
                && t.next_ms.is_finite()
                && row.interval_ms as f64 <= STARTUP_TIMER_WINDOW_MS
            {
                out.one_shots
                    .push(self.plan.str(self.plan.action(row.action).name).to_string());
            }
        }
        let mut stack: Vec<u32> = self.kernel.roots().into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            let Some(node) = self.kernel.node(id) else {
                continue;
            };
            if node.props.bool(exact_kernel::PropId::AccessibilityBusy) == Some(true) {
                let name = node
                    .props
                    .str(exact_kernel::PropId::TestId)
                    .map_or_else(|| id.to_string(), str::to_owned);
                out.busy.push(name);
            }
            stack.extend(node.children().into_iter().rev());
        }
        out
    }
}
