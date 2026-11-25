//! The session's update (@ref LLP 1123 D2, D9, D7): what the host reports of the session view,
//! which the boxes outside every owner read, as an owner's update is what its boxes read. Every
//! field comes from one native layout pass and is applied whole or refused whole. Its encoding is
//! the hosts' shared wire form, its fields encoded as an owner update's are.

use super::owner_update::{
    configuration_in, floats, four, keyboard_in, keyboard_out, optional, regions_in, regions_out,
    targets_in, targets_out, window_map_in, window_map_out,
};
use super::{Kernel, KeyboardPolicy, KeyboardReport, ReservedRegion, WindowMap};
use crate::error::{KernelError, LayoutError};
use crate::style::targets::NativeTargets;
use crate::wire::codec::{Reader, Writer};

/// What the host reports of the session view.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionUpdate {
    /// The session view's band, top right bottom left in the document's space (D9).
    pub band: [f32; 4],
    /// The chain of the controller that holds the session view and the relational targets beside
    /// it (D9).
    pub targets: NativeTargets,
    /// The keyboard in the document's space (D7), shown or dismissed, or nothing where the host
    /// does not know.
    pub keyboard: KeyboardReport,
    /// The keyboard policy, the first root's `interactive-widget` (LLP 1008 §9), `None` where the
    /// host reports none.
    pub keyboard_policy: Option<KeyboardPolicy>,
    /// The session view's map into its window (D6's `"window"` space), `None` where the host
    /// reports none.
    pub window_map: Option<WindowMap>,
    /// The session view's configuration's identity, as an owner update's (D9), `None` where the
    /// host reports none.
    pub configuration: Option<u64>,
    /// The session view's reserved regions (D16), in the document's space, as an owner update's.
    pub regions: Option<Vec<ReservedRegion>>,
}

impl Kernel {
    /// Apply the session's update (LLP 1123 D2): every field, or none where
    /// [`Kernel::check_session_update`] refuses one. Returns whether a layout is owed.
    pub fn update_session(&mut self, update: SessionUpdate) -> Result<bool, KernelError> {
        self.check_session_update(&update)?;
        let mut owed = self.set_session_band(update.band)?;
        owed |= self.set_session_targets(update.targets)?;
        owed |= self.set_session_keyboard_report(update.keyboard)?;
        owed |= self.set_keyboard_policy(update.keyboard_policy);
        self.set_window_map(None, update.window_map);
        owed |= self.track_band(None, update.configuration);
        self.set_regions(None, update.regions);
        Ok(owed)
    }

    /// Why [`Kernel::update_session`] would refuse `update`, as each field's setter refuses it,
    /// changing nothing.
    pub fn check_session_update(&self, update: &SessionUpdate) -> Result<(), KernelError> {
        if !update.band.iter().all(|v| v.is_finite() && *v >= 0.0) {
            return Err(LayoutError::InvalidEnv.into());
        }
        let seen = self.arena.observations.targets.session_seen();
        if let Some(why) = update.targets.refusal(seen) {
            return Err(LayoutError::NativeTarget(why).into());
        }
        if let KeyboardReport::Shown(k) = &update.keyboard {
            super::keyboard::check(k)?;
        }
        if let Some(why) = WindowMap::refusal(update.window_map.as_ref()) {
            return Err(LayoutError::NativeTarget(why).into());
        }
        if let Some(why) = super::regions::refusal(update.regions.as_deref()) {
            return Err(LayoutError::NativeTarget(why).into());
        }
        Ok(())
    }
}

impl SessionUpdate {
    /// The update `bytes` encode ([`SessionUpdate::encode`]); why they are refused, where they
    /// are.
    pub fn decode(bytes: &[u8]) -> Result<SessionUpdate, &'static str> {
        let mut r = Reader::new(bytes);
        let read = |r: &mut Reader<'_>| {
            Some(SessionUpdate {
                band: four(r)?,
                targets: targets_in(r)?,
                keyboard: keyboard_in(r)?,
                keyboard_policy: match r.u8().ok()? {
                    0 => None,
                    n => Some(*KeyboardPolicy::ALL.get(usize::from(n) - 1)?),
                },
                window_map: window_map_in(r)?,
                configuration: configuration_in(r)?,
                regions: regions_in(r)?,
            })
        };
        match (read(&mut r), r.is_empty()) {
            (Some(update), true) => Ok(update),
            _ => Err("a session update is truncated, malformed or followed by bytes; encode it as `SessionUpdate::encode` does"),
        }
    }

    /// The update as the hosts' wire form: the band (four `f32`s, top, right, bottom, left), the
    /// targets and the keyboard as an owner update encodes them, then the policy, a byte, 0 for
    /// none and else 1 more than its index in [`KeyboardPolicy::ALL`], and the window map, the
    /// configuration and the reserved regions as an owner update encodes them.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        floats(&mut w, &self.band);
        targets_out(&mut w, &self.targets);
        keyboard_out(&mut w, &self.keyboard);
        let policy = self
            .keyboard_policy
            .and_then(|p| KeyboardPolicy::ALL.iter().position(|&q| q == p));
        w.u8(policy.map_or(0, |i| i as u8 + 1));
        window_map_out(&mut w, self.window_map.as_ref());
        optional(&mut w, self.configuration.as_ref(), |w, c| w.u64(*c));
        regions_out(&mut w, self.regions.as_deref());
        w.into_vec()
    }
}
