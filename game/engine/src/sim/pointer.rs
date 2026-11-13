use super::*;

/// The newest received sample for one pointer, read at presentation time.
/// This is host input, not a tick's input: never saved or hashed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerSample {
    /// Platform contact identifier.
    pub id: u64,
    /// Canvas position in points; no prediction or filtering.
    pub position: Vec2,
    /// Phase of the newest sample. Read contact edges from `Input::pointer_events`.
    pub phase: PointerPhase,
    /// The platform's held button bits.
    pub buttons: u32,
    /// Original host input timestamp in milliseconds, not the world clock.
    pub at_ms: f64,
    /// Whether the tick's contact can anchor this sample. False before a new
    /// Down reaches a tick, after the contact ends and moves again, or after a
    /// tick receives an older sample than this snapshot. A newer delivered
    /// sample from an active contact restores the anchor.
    pub current_contact: bool,
}
impl PointerSample {
    fn event(event: &InputEvent) -> Option<Self> {
        let InputEvent::Pointer {
            id,
            x,
            y,
            phase,
            buttons,
            at_ms,
            ..
        } = *event
        else {
            return None;
        };
        Some(Self {
            id,
            position: Vec2::new(x, y),
            phase,
            buttons,
            at_ms,
            current_contact: true,
        })
    }
}
impl<G: Game> Sim<G> {
    /// Newest available sample for `id`, including input awaiting a tick.
    /// An ended contact remains through its final tick; blur, restart and restore
    /// discard host samples. Paused games return no sample. Calling this does
    /// not consume input or run a tick. Timestamps use the host input clock.
    pub fn frame_pointer(&self, id: u64) -> Option<PointerSample> {
        if G::paused(&self.args) {
            return None;
        }
        let mut latest = self.pointer_samples.iter().find(|p| p.id == id).copied();
        let mut current_contact = latest.is_some_and(|p| p.current_contact);
        let mut down = self.input.pointers().iter().any(|p| p.id == id && p.down);
        for queued in &self.queue {
            if !queued.host_sample {
                continue;
            }
            if matches!(queued.event, InputEvent::Blur { .. }) {
                latest = None;
                current_contact = false;
                down = false;
            } else if let Some(mut sample) =
                PointerSample::event(&queued.event).filter(|p| p.id == id)
            {
                if sample.phase == PointerPhase::Down || !down {
                    current_contact = false;
                }
                down = match sample.phase {
                    PointerPhase::Down => true,
                    PointerPhase::Move => down,
                    PointerPhase::Up => down && sample.buttons != 0,
                    PointerPhase::Cancel => false,
                };
                if latest.is_none_or(|p| p.at_ms <= sample.at_ms) {
                    sample.current_contact = current_contact;
                    latest = Some(sample);
                }
            }
        }
        latest.map(|mut sample| {
            sample.current_contact &= current_contact;
            sample
        })
    }
    pub(super) fn sample_delivered_pointer(&mut self, event: &InputEvent, host_sample: bool) {
        if !host_sample {
            if let InputEvent::Pointer { id, .. } = event {
                self.pointer_samples.retain(|p| p.id != *id);
            } else if matches!(event, InputEvent::Blur { .. }) {
                self.pointer_samples.clear();
            }
            return;
        }
        if matches!(event, InputEvent::Blur { .. }) {
            self.pointer_samples.clear();
        } else if let Some(mut sample) = PointerSample::event(event) {
            sample.current_contact = sample.phase == PointerPhase::Down
                || self
                    .input
                    .pointers()
                    .iter()
                    .any(|p| p.id == sample.id && p.down);
            match self.pointer_samples.iter_mut().find(|p| p.id == sample.id) {
                Some(old) if old.at_ms <= sample.at_ms => *old = sample,
                Some(old) => old.current_contact = false,
                None => self.pointer_samples.push(sample),
            }
        }
    }
    pub(super) fn retain_pointer_samples(&mut self) {
        self.pointer_samples.retain(|p| {
            self.input.pointers().iter().any(|c| c.id == p.id)
                || self.input.pointer().is_some_and(|c| c.id == p.id && c.down)
        });
    }
}
