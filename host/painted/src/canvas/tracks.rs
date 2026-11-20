//! What the reader was sent of its layers' keyframes (`TRACKS`) and of the
//! clock they run on (`CLOCK`), so each goes once and again only when it
//! changes (`crate::host::lower`).
use super::layer;
use crate::presenter::Presenter;
use exact_runner::DataSource;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Tracks {
    clock_sent: bool,
    /// Each live layer's keyframes as last sent.
    sent: HashMap<u32, (layer::Basis, Vec<u32>)>,
    /// [`crate::host::Host::lowered_epoch`] at the last send.
    pub(super) epoch: u64,
}

impl Tracks {
    /// After a paint: each live layer's keyframes, where they changed, and
    /// the clock they run on (`origin_ns`), once.
    pub(crate) fn send<D: DataSource>(
        &mut self,
        p: &Presenter<D>,
        origin_ns: i64,
        ops: &mut Vec<u32>,
    ) {
        let alive = layer::ALIVE.with(|a| a.borrow().clone());
        if alive.is_empty() && self.sent.is_empty() {
            return;
        }
        if !self.clock_sent {
            self.clock_sent = true;
            ops.extend([layer::CLOCK, origin_ns as u32, (origin_ns >> 32) as u32]);
        }
        let ids: std::collections::HashSet<u32> = alive.iter().map(|a| a.0).collect();
        self.sent.retain(|id, _| ids.contains(id));
        // Plays change only in a sync: until one, only new layers' tracks;
        // after one, the layers of the nodes it changed.
        let epoch = p.host().lowered_epoch();
        let since = self.epoch;
        self.epoch = epoch;
        for (id, key, basis) in alive {
            let words = if let Some(words) = p.own_tracks(key) {
                words
            } else {
                let key = exact_kernel::motion::node_key(key);
                if self.sent.get(&id).is_some_and(|(old, _)| *old == basis)
                    && (epoch == since || !p.host().lowered_changed_after(key, since))
                {
                    continue;
                }
                let base = basis.base;
                let mut at = crate::paint::Presented::IDENTITY;
                at.translate = (base[0], base[1]);
                at.scale = base[2];
                at.rotate = base[3];
                at.opacity = base[4];
                p.host().layer_tracks(key, &at, basis.reference).into()
            };
            match self.sent.get_mut(&id) {
                Some((old, was)) if *was == *words => *old = basis,
                _ => {
                    ops.extend([layer::TRACKS, id, words.len() as u32]);
                    ops.extend(words.iter());
                    self.sent.insert(id, (basis, words.into_owned()));
                }
            }
        }
    }
}
