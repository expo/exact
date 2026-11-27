//! LLP 1077 D12 on the painted hosts: a symbol's `-exact-symbol-effect`,
//! played as the web stands in for SF Symbol effects
//! (`host/web/navigation/symbol.js`). Bounce and pulse, with
//! `symbolEffectValue`, play once each time the value changes, not for the
//! value first shown; without it, they run while set. Scale has no discrete
//! form on Apple's platforms, so the symbol is held scaled up while set. As
//! on Apple, a new effect ends the old one's play and waits for a change of
//! the value. Bounce and scale move the symbol, so they stay still while the
//! person asks for less motion; a pulse still dims. The other effects stay
//! still, as on the web.
//!
//! Timed as the web's, against UIKit's on iOS 26: a pulse dims to 30 % and
//! back in 2 s (a cycle every 1.8 s while set); a bounce grows to 1.2, dips
//! to 0.9 and settles in 0.6 s, and while set repeats every 1.5 s; a scale
//! grows to 1.2 in 0.25 s and holds. Each scales about the symbol's box
//! centre and dims its ink, over what its own style does, as the web's
//! composite `add`. It is painted per frame from the host's clock, as a
//! ripple is (`ripple.rs`), and a symbol keeps what it last showed for as
//! long as its node lives, painted or not, so a value changed while its row
//! was not walked still plays.
use super::*;
use exact_kernel::{NodeKey, SymbolEffect};
use exact_motion::Easing;

/// What one play animates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// Once: grow to 1.2, dip to 0.9 and settle, in 0.6 s.
    Bounce,
    /// Once: dim to 30 % and back, in 2 s.
    Pulse,
    /// While set: the bounce in the first 40 % of every 1.5 s.
    Bouncing,
    /// While set: the pulse, every 1.8 s.
    Pulsing,
    /// While set: grow to 1.2 in 0.25 s, then hold.
    Scale,
}

/// A bounce's scale at progress `p`: the web's `BOUNCE` keyframes.
fn bounce(p: f64) -> f32 {
    keyframes(
        &[(0.0, 1.0), (0.35, 1.2), (0.65, 0.9), (1.0, 1.0)],
        &[Easing::EaseOut, Easing::EaseInOut, Easing::EaseInOut],
        p,
    )
}

/// A pulse's opacity at progress `p`: the web's `PULSE` keyframes.
fn pulse(p: f64) -> f32 {
    keyframes(
        &[(0.0, 1.0), (0.5, 0.3), (1.0, 1.0)],
        &[Easing::EaseInOut, Easing::EaseInOut],
        p,
    )
}

/// The value at progress `p` through `(offset, value)` keyframes, each
/// segment eased by its own easing, as a keyframe's `easing` runs to the
/// next.
fn keyframes(frames: &[(f64, f32)], easings: &[Easing], p: f64) -> f32 {
    let at = (frames.windows(2))
        .position(|pair| p <= pair[1].0)
        .unwrap_or(frames.len() - 2);
    let ((from, a), (to, b)) = (frames[at], frames[at + 1]);
    let local = ((p - from) / (to - from)).clamp(0.0, 1.0);
    a + (b - a) * easings[at].progress(local) as f32
}

impl Form {
    fn ms(self) -> f64 {
        match self {
            Form::Bounce => 600.0,
            Form::Pulse => 2000.0,
            Form::Bouncing => 1500.0,
            Form::Pulsing => 1800.0,
            Form::Scale => 250.0,
        }
    }

    fn repeats(self) -> bool {
        matches!(self, Form::Bouncing | Form::Pulsing)
    }

    /// The scale and opacity `elapsed` ms into a play, or `None` once a
    /// play that ends has ended (a scale holds).
    fn pose(self, elapsed: f64) -> Option<(f32, f32)> {
        let ms = self.ms();
        if !self.repeats() && self != Form::Scale && elapsed >= ms {
            return None;
        }
        let p = if self.repeats() {
            elapsed.rem_euclid(ms) / ms
        } else {
            (elapsed / ms).clamp(0.0, 1.0)
        };
        Some(match self {
            Form::Bounce => (bounce(p), 1.0),
            Form::Bouncing if p < 0.4 => (bounce(p / 0.4), 1.0),
            Form::Bouncing => (1.0, 1.0),
            Form::Pulse | Form::Pulsing => (1.0, pulse(p)),
            Form::Scale => (1.0 + 0.2 * p as f32, 1.0),
        })
    }
}

/// What a symbol last showed.
struct Shown {
    key: NodeKey,
    effect: SymbolEffect,
    /// The form it runs while set, if it runs one.
    running: Option<Form>,
    /// Whether less motion stills it.
    still: bool,
    /// The `symbolEffectValue` last seen.
    value: Option<String>,
    /// The play under way, and when it began.
    playing: Option<(Form, f64)>,
}

/// Each symbol's effect, by view.
#[derive(Default)]
pub(crate) struct SymbolEffects {
    shown: BTreeMap<ViewId, Shown>,
}

impl SymbolEffects {
    /// The scale about its box's centre and the opacity `node`'s symbol
    /// paints at `now` (`still`: less motion asked for), what it shows played
    /// on from what it showed before; `None` when it paints as it is.
    pub(super) fn pose(&mut self, node: &NodeRef<'_>, now: f64, still: bool) -> Option<(f32, f32)> {
        let effect = node.style.symbol_effect;
        let (once, run) = match effect {
            SymbolEffect::Bounce => (Some(Form::Bounce), Form::Bouncing),
            SymbolEffect::Pulse => (Some(Form::Pulse), Form::Pulsing),
            SymbolEffect::Scale => (None, Form::Scale),
            _ => {
                self.shown.remove(&node.id);
                return None;
            }
        };
        let value = node.props.str(PropId::SymbolEffectValue);
        let running = match (value, once) {
            (Some(_), Some(_)) => None,
            _ => Some(run),
        };
        let still = still && matches!(effect, SymbolEffect::Bounce | SymbolEffect::Scale);
        let prior = (self.shown.remove(&node.id)).filter(|s| s.key == node.key);
        let mut shown = match prior {
            Some(s) if s.effect == effect && s.running == running && s.still == still => s,
            prior => {
                // The same effect and form keep the value they saw; a new
                // one starts from the value it is first shown with.
                let same =
                    (prior.as_ref()).is_some_and(|s| s.effect == effect && s.running == running);
                Shown {
                    key: node.key,
                    effect,
                    running,
                    still,
                    value: match same {
                        true => prior.and_then(|s| s.value),
                        false => value.map(str::to_owned),
                    },
                    playing: running.filter(|_| !still).map(|form| (form, now)),
                }
            }
        };
        if let Some(once) = once.filter(|_| running.is_none() && !still) {
            if value != shown.value.as_deref() {
                shown.playing = Some((once, now));
            }
        }
        shown.value = value.map(str::to_owned);
        let pose = (shown.playing).and_then(|(form, start)| form.pose((now - start).max(0.0)));
        if pose.is_none() {
            shown.playing = None;
        }
        self.shown.insert(node.id, shown);
        pose.filter(|&(scale, opacity)| scale != 1.0 || opacity != 1.0)
    }

    /// The plays that move at `now`: one that repeats, one that ends and has
    /// not, a scale still growing.
    fn moving_at(&self, now: f64) -> impl Iterator<Item = &Shown> + '_ {
        (self.shown.values()).filter(move |s| {
            s.playing
                .is_some_and(|(form, start)| form.repeats() || now < start + form.ms())
        })
    }

    /// Whether a symbol moves at `now`: a frame is owed.
    pub(super) fn moving(&self, now: f64) -> bool {
        self.moving_at(now).next().is_some()
    }

    /// The nodes whose symbols move at `now`, whose rows paint again.
    pub(crate) fn keys(&self, now: f64) -> impl Iterator<Item = NodeKey> + '_ {
        self.moving_at(now).map(|s| s.key)
    }

    /// When the last play that ends comes to rest, ms; one that repeats
    /// never does.
    pub(super) fn settles_at(&self, now: f64) -> Option<f64> {
        (self.shown.values())
            .filter_map(|s| s.playing)
            .filter(|(form, _)| !form.repeats())
            .map(|(form, start)| start + form.ms())
            .filter(|end| *end > now)
            .reduce(f64::max)
    }

    /// After a paint: the symbols whose node left the tree are forgotten.
    pub(super) fn painted(&mut self, kernel: &Kernel) {
        (self.shown).retain(|id, s| kernel.node(*id).is_some_and(|n| n.key == s.key));
    }

    /// A new tree: none of the old one's symbols plays.
    pub(crate) fn clear(&mut self) {
        self.shown.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether a pose is `(scale, opacity)` to within rounding.
    fn near(pose: Option<(f32, f32)>, (scale, opacity): (f32, f32)) -> bool {
        pose.is_some_and(|(s, o)| (s - scale).abs() < 1e-4 && (o - opacity).abs() < 1e-4)
    }

    #[test]
    fn the_forms_follow_the_webs_keyframes() {
        // A bounce peaks at 1.2 at 35 % and dips to 0.9 at 65 %.
        assert!(near(Form::Bounce.pose(0.0), (1.0, 1.0)));
        assert!(near(Form::Bounce.pose(210.0), (1.2, 1.0)));
        assert!(near(Form::Bounce.pose(390.0), (0.9, 1.0)));
        assert_eq!(Form::Bounce.pose(600.0), None);
        // A pulse dims to 30 % at half time and is over at 2 s.
        assert!(near(Form::Pulse.pose(1000.0), (1.0, 0.3)));
        assert_eq!(Form::Pulse.pose(2000.0), None);
        // While set: the bounce in the first 40 %, every 1.5 s.
        assert!(near(Form::Bouncing.pose(1500.0 + 0.35 * 600.0), (1.2, 1.0)));
        assert!(near(Form::Bouncing.pose(1000.0), (1.0, 1.0)));
        assert!(near(Form::Pulsing.pose(1800.0 + 900.0), (1.0, 0.3)));
        // A scale grows linearly to 1.2 and holds.
        assert!(near(Form::Scale.pose(125.0), (1.1, 1.0)));
        assert!(near(Form::Scale.pose(10_000.0), (1.2, 1.0)));
    }
}
