//! The audio seam (LLP 1047.001 D11): the voice table (LLP 1096), the
//! transport (LLP 1125 D2–D5) and the MIDI queue (LLP 1125 D6–D9) behind
//! [`Audio`], which only an entry's [`super::ModuleLinks`] names, so a
//! binary whose plans play nothing holds none of their code.
//!
//! The runner's audio methods (`sounds.rs`, `transport.rs`, `midi.rs`) run
//! only through [`LinkedAudio`]. Each provided method here is what a runner
//! answers whose plan declares no sound or sequence, runs no voice-table or
//! MIDI command and reads neither `exactTransport` nor `exactMIDI`: nothing
//! sounds, nothing is due, and a host's MIDI is refused.

use super::provenance::Cause;
use super::*;
use crate::midi::{Midi, MidiOp};
use crate::sound::{SoundOp, Sounds};
use crate::transport::Transport;
use std::any::Any;

/// The voice table, the transport and the MIDI queue.
pub(in crate::runner) struct AudioParts {
    pub(in crate::runner) sounds: Sounds,
    pub(in crate::runner) transport: Transport,
    pub(in crate::runner) midi: Midi,
    /// The cause of the commit that started the sequence playing, which its
    /// `ended=` runs under (LLP 1133.023 D7a), and of the commit whose gate
    /// reading waits to stand.
    pub(in crate::runner) started: Cause,
    pub(in crate::runner) staged: Cause,
}

/// [`AudioParts`] as the runner keeps it: made by the linked seam
/// ([`Audio::state`]) and reached only by audio code, so a binary without
/// the seam holds none of it, its drop included.
pub(in crate::runner) struct AudioState(pub(in crate::runner) Box<dyn Any>);

impl std::ops::Deref for AudioState {
    type Target = AudioParts;

    fn deref(&self) -> &AudioParts {
        self.0
            .downcast_ref()
            .expect("the linked seam made this state")
    }
}

impl std::ops::DerefMut for AudioState {
    fn deref_mut(&mut self) -> &mut AudioParts {
        self.0
            .downcast_mut()
            .expect("the linked seam made this state")
    }
}

/// The runner's audio work. The provided methods are the answers of a
/// runner that plays nothing; [`LinkedAudio`] implements every one.
#[allow(unused_variables)]
pub trait Audio<D: DataSource> {
    /// Whether this is the audio code.
    fn linked(&self) -> bool {
        false
    }
    /// The state for a new runner of `plan`.
    fn state(&self, plan: &exact_plan::Plan, data: &D) -> Box<dyn Any> {
        Box::new(())
    }
    /// [`Runner::sequence_step`].
    fn sequence_step(&self, r: &mut Runner<D>) -> Result<(), RunnerError> {
        Ok(())
    }
    /// [`Runner::transport_stood`].
    fn transport_stood(&self, r: &mut Runner<D>) {}
    /// A refused commit's gate step is let go.
    fn let_go(&self, r: &mut Runner<D>) {}
    /// Whether a sequence stops once the commit stands (the page froze).
    fn suspend(&self, r: &mut Runner<D>, on: bool) {}
    /// [`Runner::apply_sounds`].
    fn apply_sounds(&self, r: &mut Runner<D>, from: usize) {}
    /// When the transport's wake, its end and an answer of `exactTransport`
    /// or `exactMIDI` are due.
    fn due(&self, r: &Runner<D>) -> [f64; 3] {
        [f64::INFINITY; 3]
    }
    /// The answer due at `at`: the transport's readers first, then
    /// `exactMIDI`'s.
    fn answer(&self, r: &mut Runner<D>, at: f64) -> Result<CommitReceipt, RunnerError> {
        Err(RunnerError::Poisoned)
    }
    /// Whether the transport's end runs an `ended=` action.
    fn ends_with_action(&self, r: &Runner<D>) -> bool {
        false
    }
    /// [`Runner::transport_end`].
    fn transport_end(&self, r: &mut Runner<D>) -> Option<(ActionsId, Cause)> {
        None
    }
    /// [`Runner::transport_wake`].
    fn transport_wake(&self, r: &mut Runner<D>, at: f64) {}
    /// `exactTransport()`'s answer.
    fn transport_answer(&self, r: &Runner<D>, i: usize) -> Result<Value, DataError> {
        Err(unlinked(crate::transport::SOURCE))
    }
    /// `exactMIDI()`'s answer.
    fn midi_answer(&self, r: &Runner<D>, i: usize) -> Result<Value, DataError> {
        Err(unlinked(crate::midi::SOURCE))
    }
    /// [`Runner::take_sounds`].
    fn take_sounds(&self, r: &mut Runner<D>) -> Vec<SoundOp> {
        Vec::new()
    }
    /// [`Runner::take_midi`].
    fn take_midi(&self, r: &mut Runner<D>) -> Vec<MidiOp> {
        Vec::new()
    }
    /// [`Runner::end_sounds`].
    fn end_sounds(&self, r: &mut Runner<D>) {}
    /// Whether a sequence plays.
    fn playing(&self, r: &Runner<D>) -> bool {
        false
    }
    /// [`Runner::midi_host`].
    fn midi_host(
        &self,
        r: &mut Runner<D>,
        json: &str,
    ) -> Result<(Vec<CommitReceipt>, Option<RunnerError>), String> {
        Err("midi: this app links no MIDI".into())
    }
    /// [`Runner::midi_agent`].
    fn midi_agent(
        &self,
        r: &mut Runner<D>,
        request: &str,
    ) -> Option<(Vec<CommitReceipt>, Result<String, String>)> {
        None
    }
}

fn unlinked(source: &str) -> DataError {
    DataError::Unavailable(format!("{source}: this app links no audio"))
}

/// The runner without its audio.
pub struct NoAudio;

impl<D: DataSource> Audio<D> for NoAudio {}

/// The audio code: each method runs the runner's own.
pub struct LinkedAudio;

impl<D: DataSource> Audio<D> for LinkedAudio {
    fn linked(&self) -> bool {
        true
    }
    fn state(&self, plan: &exact_plan::Plan, data: &D) -> Box<dyn Any> {
        Box::new(AudioParts {
            sounds: Sounds::default(),
            transport: Transport::new(plan),
            midi: super::midi::queue(plan, data),
            started: Cause::none(),
            staged: Cause::none(),
        })
    }
    fn sequence_step(&self, r: &mut Runner<D>) -> Result<(), RunnerError> {
        r.sequence_step()
    }
    fn transport_stood(&self, r: &mut Runner<D>) {
        r.transport_stood()
    }
    fn let_go(&self, r: &mut Runner<D>) {
        r.audio.transport.pending = None;
    }
    fn suspend(&self, r: &mut Runner<D>, on: bool) {
        r.audio.transport.suspend = on;
    }
    fn apply_sounds(&self, r: &mut Runner<D>, from: usize) {
        r.apply_sounds(from)
    }
    fn due(&self, r: &Runner<D>) -> [f64; 3] {
        let a = &*r.audio;
        [
            a.transport.wake_due(),
            a.transport.ended_due(),
            a.transport.answer_due.min(a.midi.answer_due),
        ]
    }
    fn answer(&self, r: &mut Runner<D>, at: f64) -> Result<CommitReceipt, RunnerError> {
        if r.audio.transport.answer_due <= at {
            r.audio.transport.answer_due = f64::INFINITY;
            let readers = r.audio.transport.readers.clone();
            r.scheduled(at, |r| r.commit_again(readers, "transport"))
        } else {
            r.scheduled(at, |r| r.midi_answered())
        }
    }
    fn ends_with_action(&self, r: &Runner<D>) -> bool {
        r.audio.transport.ended_action(&r.plan).is_some()
    }
    fn transport_end(&self, r: &mut Runner<D>) -> Option<(ActionsId, Cause)> {
        r.transport_end()
    }
    fn transport_wake(&self, r: &mut Runner<D>, at: f64) {
        r.transport_wake(at)
    }
    fn transport_answer(&self, r: &Runner<D>, i: usize) -> Result<Value, DataError> {
        r.transport_answer(i)
    }
    fn midi_answer(&self, r: &Runner<D>, i: usize) -> Result<Value, DataError> {
        r.midi_answer(i)
    }
    fn take_sounds(&self, r: &mut Runner<D>) -> Vec<SoundOp> {
        r.audio.sounds.take()
    }
    fn take_midi(&self, r: &mut Runner<D>) -> Vec<MidiOp> {
        r.midi_open();
        r.audio.midi.take()
    }
    fn end_sounds(&self, r: &mut Runner<D>) {
        r.end_audio()
    }
    fn playing(&self, r: &Runner<D>) -> bool {
        r.audio.transport.playing()
    }
    fn midi_host(
        &self,
        r: &mut Runner<D>,
        json: &str,
    ) -> Result<(Vec<CommitReceipt>, Option<RunnerError>), String> {
        r.midi_from_host(json)
    }
    fn midi_agent(
        &self,
        r: &mut Runner<D>,
        request: &str,
    ) -> Option<(Vec<CommitReceipt>, Result<String, String>)> {
        r.midi_from_agent(request)
    }
}

impl<D: DataSource> Runner<D> {
    /// What the output plays since the last take: a `Play` per new voice,
    /// an `End` per voice whose end changed (LLP 1096 D5; LLP 1125 D3), and a
    /// sequence's timed haptics (LLP 1125 D3).
    pub fn take_sounds(&mut self) -> Vec<SoundOp> {
        let audio = self.seams.audio;
        audio.take_sounds(self)
    }

    /// What MIDI does since the last take: sends and retractions, the
    /// inputs to open, a request (LLP 1125 D7).
    pub fn take_midi(&mut self) -> Vec<MidiOp> {
        let audio = self.seams.audio;
        audio.take_midi(self)
    }

    /// End every live voice before this runner is replaced (a dev reload,
    /// LLP 1096 D5); the host drains `take_sounds` before the restart.
    pub fn end_sounds(&mut self) {
        let audio = self.seams.audio;
        audio.end_sounds(self)
    }

    /// Whether this runner has the audio code: without it, [`Runner::sounds`],
    /// [`Runner::transport`] and [`Runner::midi`] have nothing to show.
    pub fn plays_audio(&self) -> bool {
        self.seams.audio.linked()
    }

    /// Whether a sequence plays now.
    pub fn transport_playing(&self) -> bool {
        self.seams.audio.playing(self)
    }

    /// A host's message (its MIDI arm's, or the agent's on its behalf):
    /// `{"ports":{…}}` ([`crate::midi::Ports::from_json`]) or
    /// `{"messages":[[input, timeStamp, [bytes…]], …]}`, one delivery. The
    /// receipts and the first refusal, or why the message is none.
    pub fn midi_host(
        &mut self,
        json: &str,
    ) -> Result<(Vec<CommitReceipt>, Option<RunnerError>), String> {
        let audio = self.seams.audio;
        audio.midi_host(self, json)
    }

    /// The agent's MIDI (LLP 1125 §4): `{"op":"prefer","midi":{"state"?,
    /// "inputs"?, "outputs"?}}`, the ports a request finds, by name; and
    /// `{"op":"midi","input":…,"data":[…]}`, a message from that input now
    /// (`tap midi`). `None` for any other request; else the receipts and
    /// the reply, or the refusal.
    pub fn midi_agent(
        &mut self,
        request: &str,
    ) -> Option<(Vec<CommitReceipt>, Result<String, String>)> {
        let audio = self.seams.audio;
        audio.midi_agent(self, request)
    }

    /// The audio's due times: [`Audio::due`].
    pub(super) fn audio_due(&self) -> [f64; 3] {
        self.seams.audio.due(self)
    }
}
