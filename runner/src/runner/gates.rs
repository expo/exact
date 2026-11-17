//! Gated tasks (LLP 1092 D7–D10): a task declared `when cond [key=expr]`
//! has a timer only while its gate holds, as an `if` arm has its nodes;
//! a new key restarts it, as a new `for` key makes a new row. Nothing runs
//! when the gate changes: the change arms or drops a deadline.
//!
//! The step runs inside each commit, right after its settlement succeeds
//! and inside the rollback a settlement failure takes, so a refusal here
//! (a key that is no key, a trap) refuses the whole commit and the
//! checkpoint puts the timers back (D8).
//!
//! What a host's clock waits for lives here too: whether there are timers,
//! frame tasks, and the soonest deadline of any of them.

use super::*;

#[derive(Default)]
pub(super) struct Deadline {
    inputs: Option<DeadlineInputs>,
    timers: Vec<Timer>,
    due: Option<f64>,
    generation: u64,
    #[cfg(test)]
    computations: usize,
}

#[derive(PartialEq)]
struct DeadlineInputs {
    now: f64,
    live: bool,
    presenting: bool,
    native: bool,
    waits: WaitKinds,
    takeover: Option<f64>,
    stopped: bool,
    other: [Option<f64>; 18],
}

impl<D: DataSource> Runner<D> {
    /// Identity of an unchanged collection reply's persistent metadata.
    /// One-shot output and independently running work require a fresh batch.
    /// Call after `collection_timer_due_ms` has compared the current scheduling inputs.
    pub fn collection_reply_key(&self) -> Option<[u64; 8]> {
        let lines_empty = |q: &std::cell::RefCell<(std::collections::VecDeque<String>, u64)>| {
            let q = q.borrow();
            q.0.is_empty() && q.1 == 0
        };
        if self.poisoned
            || !self.requests.is_empty()
            || !self.commands.is_empty()
            || !self.collection_output_quiet()
            || self.canvas_wants_frame()
            || self.canvas_owed(&|_| true)
            || !lines_empty(&self.ids.work.alerts)
            || !lines_empty(&self.ids.work.journal)
            || (!self.ids.work.sites.is_empty() && self.ids.work.reported.get() != self.batch)
            || self.store.has_announced()
            || self.kernel.has_prop(exact_kernel::PropId::Wakelock)
            || self.kernel.has_prop(exact_kernel::PropId::Orientationlock)
        {
            return None;
        }
        let (incarnation, epoch, layout) = self.kernel.layout_stamp();
        Some([
            incarnation,
            epoch,
            layout,
            self.batch,
            self.clock.deadline.borrow().generation,
            self.store.revision(),
            self.root_font_size().to_bits(),
            u64::from(self.must_finish()) | (u64::from(self.data_timer_landing()) << 1),
        ])
    }

    /// D8's table, over the commit's settled state, at the commit's time:
    /// a false gate drops its timer; a gate that turns true arms it from
    /// now; a changed key re-arms it from now; anything else leaves it as
    /// it was (an `every` keeps its phase, a spent `after` stays spent).
    pub(super) fn gate_step(&mut self) -> Result<(), RunnerError> {
        // A sequence's gate, key and arguments (LLP 1125 D2).
        self.sequence_step()?;
        let mut seen = Vec::new();
        for (i, row) in self.plan.timers.iter().enumerate() {
            if !row.gated && !row.keyed {
                continue;
            }
            let on = self
                .eval(row.gate, &[], &[])
                .map_err(|e| e.evaluated(|| format!("task {:?} gate", self.plan.str(row.name))))?
                == Value::Bool(true);
            let key = if row.keyed && on {
                let value = self.eval(row.key, &[], &[]).map_err(|e| {
                    e.evaluated(|| format!("task {:?} key", self.plan.str(row.name)))
                })?;
                match crate::instance::key_text(&value) {
                    Some(k) => Some(k),
                    None => {
                        return Err(RunnerError::TaskKey {
                            task: self.plan.str(row.name).to_string(),
                        })
                    }
                }
            } else {
                None
            };
            seen.push((i, on, key));
        }
        let now = self.now_ms;
        for (i, on, key) in seen {
            let t = &mut self.timers[i];
            if !on {
                t.armed = false;
                t.next_ms = f64::INFINITY;
                t.key = None;
            } else if !t.armed || t.key != key {
                t.armed = true;
                t.generation = t.generation.wrapping_add(1);
                t.completion = Default::default();
                t.key = key;
                let row = &self.plan.timers[i];
                if row.frame {
                    // While the agent or a test seeks, the task's next
                    // virtual frame; a presenting host fires it at its
                    // next frame (D10).
                    t.base = now;
                    t.k = 1;
                    t.next_ms = super::virtual_frame(now, 1);
                } else {
                    t.next_ms = now + row.interval_ms as f64;
                }
            }
        }
        Ok(())
    }

    /// Whether the plan has timers (a host then drives `advance`): a task, a
    /// `then`, or a queue's `next` (LLP 1092 D3).
    pub fn has_timers(&self) -> bool {
        !self.plan.timers.is_empty()
            || !self.plan.sequences.is_empty()
            || self
                .plan
                .mutations
                .iter()
                .any(|m| m.then.is_some() || m.mode == MutationMode::Queue)
    }

    /// Whether the plan has a frame task (LLP 1073 D4): a host keeps its
    /// frame source running and calls [`Runner::frame`] each frame.
    pub fn wants_frames(&self) -> bool {
        if self.poisoned {
            return false;
        }
        // An idle gated frame task keeps no frame source running (LLP 1092 D10).
        self.plan
            .timers
            .iter()
            .zip(&self.timers)
            .any(|(t, s)| t.frame && s.armed)
            // A font axis moving (LLP 1019 §13 D10).
            || self.text_axes.running()
    }

    /// Soonest timer deadline in this runner's clock domain; no host polling.
    /// A frame task's next virtual frame counts only while the host doesn't
    /// present frames: then its frame source wakes it (LLP 1073 D4).
    /// @ref LLP 1043.000 §3 D8 — hosts wake near the authored timer's due time.
    pub fn timer_due_ms(&self) -> Option<f64> {
        if self.poisoned {
            return None;
        }
        self.timer_deadline(self.wake_stops_timers(), self.other_timer_inputs())
    }

    /// A collection report changes no timer without a commit. Reuse its last
    /// deadline while all scheduling inputs, including external ones, agree.
    pub fn collection_timer_due_ms(&self) -> Option<f64> {
        if self.poisoned {
            return None;
        }
        // Compare the scheduling inputs themselves. This also observes deadlines
        // supplied by process work or a backend without a runner commit.
        let inputs = DeadlineInputs {
            now: self.now_ms,
            live: self.clock_live(),
            presenting: self.clock.presenting,
            native: self.clock.native_timers,
            waits: self.clock.waits,
            takeover: self.clock.takeover,
            stopped: self.wake_stops_timers(),
            other: self.other_timer_inputs(),
        };
        let mut cached = self.clock.deadline.borrow_mut();
        if cached.inputs.as_ref() == Some(&inputs) && cached.timers == self.timers {
            return cached.due;
        }
        let due = self.timer_deadline(inputs.stopped, inputs.other);
        cached.inputs = Some(inputs);
        cached.timers.clone_from(&self.timers);
        cached.due = due;
        cached.generation = cached
            .generation
            .checked_add(1)
            .expect("deadline generation");
        #[cfg(test)]
        {
            cached.computations += 1;
        }
        due
    }

    fn timer_deadline(&self, stopped: bool, other: [Option<f64>; 18]) -> Option<f64> {
        self.timers
            .iter()
            .zip(&self.plan.timers)
            .enumerate()
            .filter(|(_, (_, row))| !(row.frame && self.clock.presenting))
            // No timer fires while a wake runs unseen (LLP 1133.017 D6).
            .filter(|_| !stopped)
            .map(|(i, _)| self.timer_execution(i))
            .chain(other.into_iter().flatten())
            .filter(|ms| ms.is_finite())
            .reduce(f64::min)
    }

    /// Wake tolerance bounded by every armed timer and other known deadline.
    /// It changes host wakes only; advances retain their nominal due times.
    pub fn timer_tolerance_ms(&self) -> f64 {
        if !self.clock_live() || !self.plan.timers.iter().any(|row| row.tolerance_ms != 0) {
            return 0.0;
        }
        let Some(due) = self.timer_due_ms() else {
            return 0.0;
        };
        let latest = self
            .timers
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                !(self.plan.timers[*i].frame && self.clock.presenting) && !self.wake_stops_timers()
            })
            .map(|(i, _)| self.timer_execution(i) + self.plan.timers[i].tolerance_ms as f64)
            .chain(self.other_timer_due_ms())
            .filter(|at| at.is_finite())
            .reduce(f64::min)
            .unwrap_or(due);
        (latest - due).max(0.0)
    }

    /// A coalesced wake on hosts without a tolerance parameter.
    pub fn timer_wake_ms(&self) -> Option<f64> {
        self.timer_due_ms()
            .map(|due| due + self.timer_tolerance_ms())
    }

    fn other_timer_due_ms(&self) -> Option<f64> {
        self.other_timer_inputs()
            .into_iter()
            .flatten()
            .filter(|ms| ms.is_finite())
            .reduce(f64::min)
    }

    fn other_timer_inputs(&self) -> [Option<f64>; 18] {
        [
            self.wake_due(),
            // An input held behind a change action's receipt (`inputs.rs`).
            self.held_input_due(),
            self.call_due(),
            self.presence_due_ms(),
            // A resource a turn over its budget refused, asked again (`overrun.rs`).
            self.reask_due_ms(),
            // An answer waiting on its own timer (`data_timers.rs`).
            self.data_timer_due_ms()
                .filter(|_| !self.wake_stops_timers()),
            self.then_due
                .iter()
                .copied()
                .filter(|ms| ms.is_finite())
                .reduce(f64::min),
            // A drop's hold waits a second for its move (LLP 1094 D8).
            self.reorder_deadline(),
            // A module send the substitute answers after its span.
            self.span_due(),
            self.live_times.due(),
            (self.text_axes.running() && !self.clock.presenting)
                .then(|| super::virtual_frame(self.now_ms, 1)),
            self.queues
                .next_due
                .iter()
                .copied()
                .filter(|ms| ms.is_finite())
                .reduce(f64::min),
            // A polled module source on the live clock (LLP 1133.009 D10
            // item 7); under the agent a poll never stops a jump.
            self.clock
                .sample
                .is_some()
                .then(|| self.poll_due_ms())
                .flatten(),
            // The host's process work on the agent's clock (LLP 1133.007 §7),
            // and what it owes, due now so a jump waits for it to land.
            self.work_due(),
            self.work_owed().then_some(self.now_ms),
            // A sequence's fill, count-in's end, end and answer (LLP 1125 D3).
            Some(self.transport.wake_due()),
            Some(self.transport.ended_due()),
            Some(self.transport.answer_due.min(self.midi.answer_due)),
        ]
    }

    /// Each task's next due time, or `None` while it is idle or spent (the
    /// agent's `state.tasks`, D10).
    pub fn tasks(&self) -> Vec<(String, Option<f64>)> {
        self.plan
            .timers
            .iter()
            .zip(&self.timers)
            .map(|(row, t)| {
                let name = self.plan.str(row.name).to_string();
                (name, Some(t.next_ms).filter(|ms| ms.is_finite()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CollectionFeedback;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    struct NoData;
    impl DataSource for NoData {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            unreachable!()
        }
    }

    #[test]
    fn a_hundred_offset_reports_reuse_the_deadline_and_each_schedule_change_recomputes() {
        let plan = contract::compile(
            r#"component App
  state armed = false
  state key = 0
  action arm
    armed = true
  action disarm
    armed = false
  action rekey
    key = key + 1
  action noop
    key = key
  task later when armed key=key
    after(1000, noop)
  view
    list virtualized=true height=160 width=240
      for i in range(100) key=i
        text `${i}` height=20
"#,
        )
        .unwrap();
        let mut r = Runner::boot(
            plan,
            NoData,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let report = |r: &mut Runner<NoData>, offset| {
            let c = r.collections().remove(0);
            r.collection_feedback(CollectionFeedback {
                view: c.view,
                revision: c.revision,
                scroll_sequence: c.scroll_sequence + 1,
                offset,
                port_main: 160.,
                port_cross: 240.,
                cross: 240.,
                padding: [0.; 2],
                measurements: vec![],
                focus_view: None,
                interaction_view: None,
            })
            .unwrap()
        };
        report(&mut r, 35.);
        r.set_live_clock(Some(Box::new(|| 0.)));
        assert_eq!(r.collection_timer_due_ms(), None);
        let polls = super::super::modules::POLL_SCANS.with(|n| n.get());
        let before = r.clock.deadline.borrow().computations;
        let walks = r.tree.as_ref().unwrap().collection_walks;
        for n in 0..100 {
            assert!(report(&mut r, 35. + f64::from(n) / 1000.)
                .receipts
                .is_empty());
            assert_eq!(r.collection_timer_due_ms(), None);
        }
        assert_eq!(
            (
                r.clock.deadline.borrow().computations,
                r.tree.as_ref().unwrap().collection_walks,
                super::super::modules::POLL_SCANS.with(|n| n.get())
            ),
            (before, walks, polls),
            "deadline reductions, feedback walks, module poll scans"
        );
        for (action, due) in [
            ("arm", Some(1000.)),
            ("rekey", Some(1000.)),
            ("disarm", None),
        ] {
            let before = r.clock.deadline.borrow().computations;
            r.act(action, vec![]).unwrap();
            assert_eq!(r.collection_timer_due_ms(), due);
            assert_eq!(
                r.clock.deadline.borrow().computations,
                before + 1,
                "{action}"
            );
            assert_eq!(r.collection_timer_due_ms(), due);
            assert_eq!(r.clock.deadline.borrow().computations, before + 1);
            for _ in 0..100 {
                assert!(report(&mut r, 35.1).receipts.is_empty());
                assert_eq!(r.collection_timer_due_ms(), due);
            }
            assert_eq!(r.clock.deadline.borrow().computations, before + 1);
        }

        struct Work(Arc<AtomicU64>);
        impl crate::process::WorkClock for Work {
            fn advance(&self, _: f64) {}
            fn due(&self) -> Option<f64> {
                Some(self.0.load(Ordering::Relaxed) as f64)
            }
            fn owed(&self) -> bool {
                false
            }
        }
        let due = Arc::new(AtomicU64::new(1200));
        r.set_work_clock(Some(Arc::new(Work(due.clone()))));
        assert_eq!(r.collection_timer_due_ms(), Some(1200.));
        let before = r.clock.deadline.borrow().computations;
        due.store(900, Ordering::Relaxed);
        assert_eq!(r.collection_timer_due_ms(), Some(900.));
        assert_eq!(r.clock.deadline.borrow().computations, before + 1);
        r.set_work_clock(None);
        assert_eq!(r.collection_timer_due_ms(), None);
        assert_eq!(r.clock.deadline.borrow().computations, before + 2);
    }
}
