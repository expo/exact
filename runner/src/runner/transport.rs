//! A `sequence` in the runner's commits and clock (LLP 1125 D2–D5): the
//! gate step reads each sequence's gate, key and `play` arguments over the
//! commit's settled state, inside its rollback; the transport takes them
//! once the commit stands, so a refused commit changes nothing that sounds.
//! Its own wakes (a fill, the count-in's end) run no action and make no
//! commit; its end runs `ended=` as a timer runs; and what `exactTransport`
//! answers is published by a commit of its own, as an answer's `then` is.
//! A clock in's deliveries (LLP 1125 D9) come through `midi_input`.

use super::provenance::Cause;
use super::*;
use crate::transport::{Answer, Out, Seen, SOURCE};

impl<D: DataSource> Runner<D> {
    /// The gate step's part (LLP 1092 D8): each sequence's gate, its key's
    /// text while the gate holds, and its six arguments, read now and kept
    /// for [`Runner::transport_stood`]. A trap, or a key that is no key,
    /// refuses the commit.
    pub(super) fn sequence_step(&mut self) -> Result<(), RunnerError> {
        self.audio.transport.pending = None;
        if self.plan.sequences.is_empty() {
            return Ok(());
        }
        let mut seen = Vec::with_capacity(self.plan.sequences.len());
        for row in self.plan.sequences.iter() {
            let on = self.eval(row.gate, &[], &[]).map_err(|e| {
                e.evaluated(|| format!("sequence {:?} gate", self.plan.str(row.name)))
            })? == Value::Bool(true);
            let key = if row.keyed && on {
                let value = self.eval(row.key, &[], &[]).map_err(|e| {
                    e.evaluated(|| format!("sequence {:?} key", self.plan.str(row.name)))
                })?;
                Some(
                    crate::instance::key_text(&value).ok_or_else(|| RunnerError::TaskKey {
                        task: self.plan.str(row.name).to_string(),
                    })?,
                )
            } else {
                None
            };
            let mut args = Vec::new();
            if on {
                for (k, code) in [
                    row.tracks,
                    row.bpm,
                    row.from,
                    row.count_in,
                    row.clock,
                    row.clock_out,
                ]
                .into_iter()
                .enumerate()
                {
                    args.push(self.eval(code, &[], &[]).map_err(|e| {
                        e.evaluated(|| {
                            format!("sequence {:?} argument {}", self.plan.str(row.name), k + 1)
                        })
                    })?);
                }
            }
            seen.push(Seen { on, key, args });
        }
        self.audio.transport.pending = Some(seen);
        // What the gate read starts under the reading commit's cause, which
        // has been put back by the time the commit stands.
        self.audio.staged = self.cause();
        Ok(())
    }

    /// The commit that read the gates stood: arm, restart, edit or stop.
    /// A background stop the commit asked for (`set_page`) is applied here
    /// too, after the gates, so a refused commit leaves it unmade.
    pub(super) fn transport_stood(&mut self) {
        let suspend = std::mem::take(&mut self.audio.transport.suspend);
        let seen = self.audio.transport.pending.take();
        if seen.is_none() && !suspend {
            return;
        }
        let before = self.audio.transport.answer();
        let (now, live) = (self.now_ms, self.clock_live());
        let cause = self.audio.staged;
        let mut lines = Vec::new();
        let a = &mut *self.audio;
        let mut out = Out {
            sounds: &mut a.sounds,
            midi: &mut a.midi,
        };
        if let Some(seen) = seen {
            let starts = a.transport.starts;
            a.transport
                .stood(&self.plan, &mut out, now, live, seen, &mut lines);
            if a.transport.starts != starts {
                a.started = cause;
            }
        }
        if suspend {
            a.transport.suspend(&self.plan, &mut out, now, &mut lines);
        }
        self.transport_said(lines, before);
    }

    /// Journal what the transport said, and answer `exactTransport` again
    /// when what it answers changed.
    fn transport_said(&mut self, mut lines: Vec<String>, before: Answer) {
        self.audio.sounds.settle(&mut lines);
        lines.into_iter().for_each(|l| self.log(l));
        self.audio.transport.publish();
        let now = self.now_ms;
        let t = &mut self.audio.transport;
        if !t.readers.is_empty() && t.answer() != before {
            t.answer_due = t.answer_due.min(now);
        }
    }

    /// Its own wake at `at` (a fill, the count-in's end): no action, no
    /// commit, no receipt.
    pub(super) fn transport_wake(&mut self, at: f64) {
        let before = self.audio.transport.answer();
        let live = self.clock_live();
        let mut lines = Vec::new();
        let a = &mut *self.audio;
        let mut out = Out {
            sounds: &mut a.sounds,
            midi: &mut a.midi,
        };
        a.transport.wake(&self.plan, &mut out, at, live, &mut lines);
        self.transport_said(lines, before);
    }

    /// One delivery's clock messages from the followed input (LLP 1125 D9),
    /// `(stamp, bytes)` each in arrival order: no action, no commit.
    pub(super) fn transport_clock(&mut self, msgs: &[(f64, Vec<u8>)]) {
        let before = self.audio.transport.answer();
        let (now, live) = (self.now_ms, self.clock_live());
        let mut lines = Vec::new();
        let a = &mut *self.audio;
        let mut out = Out {
            sounds: &mut a.sounds,
            midi: &mut a.midi,
        };
        a.transport
            .follow(&self.plan, &mut out, now, live, msgs, &mut lines);
        self.transport_said(lines, before);
    }

    /// The last finite track ended at the clock's time: the `ended=`
    /// action, if any, to run as a timer's, with the cause of the commit
    /// that started the sequence.
    pub(super) fn transport_end(&mut self) -> Option<(ActionsId, Cause)> {
        let before = self.audio.transport.answer();
        let mut lines = Vec::new();
        let now = self.now_ms;
        let a = &mut *self.audio;
        let action = a.transport.end(&self.plan, &mut a.midi, now, &mut lines);
        self.transport_said(lines, before);
        action.map(|action| (action, self.audio.started))
    }

    /// The transport record, for `state.transport` and a test: it panics on a
    /// runner without the audio code ([`Runner::plays_audio`]).
    pub fn transport(&self) -> &crate::transport::Transport {
        &self.audio.transport
    }

    /// `exactTransport()`'s answer, by the declared fields.
    pub(super) fn transport_answer(&self, i: usize) -> Result<Value, DataError> {
        let row = self.resource_row(i);
        if row.args.len > 0 {
            return Err(DataError::BadArguments(format!(
                "{SOURCE} takes no arguments"
            )));
        }
        let ty = self.plan.type_(row.ty);
        if ty.kind != exact_plan::TypeKind::Record {
            return Err(DataError::Unavailable(format!(
                "{SOURCE} answers a record; this one is declared `{}`",
                self.plan.str(ty.name)
            )));
        }
        let answer = self.audio.transport.answer();
        let mut fields = Vec::with_capacity(ty.fields.len as usize);
        for f in ty.fields.iter() {
            let name = self.plan.str(self.plan.field(f).name);
            fields.push(answer.field(name).ok_or_else(|| {
                DataError::Unavailable(format!("{SOURCE} has no field `{name}`"))
            })?);
        }
        Ok(Value::record(fields))
    }

    /// The voice table (LLP 1096 D5), for `state.sounds` and a test: it
    /// panics on a runner without the audio code ([`Runner::plays_audio`]).
    pub fn sounds(&self) -> &crate::sound::Sounds {
        &self.audio.sounds
    }

    /// [`Runner::end_sounds`]'s work.
    pub(super) fn end_audio(&mut self) {
        // The host's time now, not the last commit's: what its output has
        // handed since is handed here too, so no note-off it needs is taken
        // back.
        self.sample();
        let mut lines = Vec::new();
        if !self.plan.sounds.is_empty() {
            self.audio.sounds.reload(self.now_ms, &mut lines);
        }
        // The MIDI queue as at a stop: every note ends (LLP 1125 D4).
        self.audio.midi.reload(self.now_ms, &mut lines);
        lines.into_iter().for_each(|l| self.log(l));
    }
}
