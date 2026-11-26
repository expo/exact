//! LLP 1003.003 D7 on Apple: a box's `clip-path` transition Core Animation
//! plays, as the `path` of the clip's shape layer through paths the engine
//! computes on the curve, and the rule for when it may.
use exact_kernel::clip::ClipPath;
use exact_kernel::{Dimension, NodeRef, PropId};
use exact_motion::{Easing, Engine, PathValue, PlayedCurve, Property};

/// Whether Core Animation can play `key`'s clip transition as the engine
/// would draw it: where the clip is the layer's whole mask, and the run is
/// one [`paths_play`] admits. A corner radius or shape can put an outline
/// over the clip (BoxMaskIOS and BoxMaskMac: an overflow clip with shaped
/// corners, the elliptical outline a radius gives a backdrop on the Mac), as
/// can a `mask-image`, a material and a default surface: the clip is then a
/// mask's own mask, which `CALayer.h` marks unsupported, so a box with any of
/// them is sampled. A layout transition moves the clip to the box's surface.
pub(crate) fn playable(engine: &Engine, key: u64, n: &NodeRef<'_>) -> bool {
    let s = n.style;
    let square = [
        &s.border_radius_top_left,
        &s.border_radius_top_right,
        &s.border_radius_bottom_right,
        &s.border_radius_bottom_left,
    ]
    .into_iter()
    .all(|r| {
        matches!(
            r,
            Dimension::Auto | Dimension::Points(0.0) | Dimension::Percent(0.0)
        )
    }) && s.rare.corner_shape.is_round();
    if !square
        || s.rare.mask_image != Default::default()
        || !s.backdrop_filter.is_none()
        || n.props.str(PropId::BackgroundMaterial).is_some()
        || n.props.bool(PropId::DefaultSurface) == Some(true)
        || !s.rare.layout_transition.0.is_empty()
    {
        return false;
    }
    let (Some((from, to)), Some((t, _))) = (
        engine.clip_ends(key),
        engine.transition(key, Property::ClipPath),
    ) else {
        return false;
    };
    paths_play(&from.path, &to.path, &t.curve)
}

/// Whether Core Animation can play a path's run between `from` and `to` on
/// `curve` (a clip's, and a `d`'s, LLP 1115.003 D4) as D2 draws it: the run
/// has a track to play (a transition of no duration is never handed over:
/// the engine shows its end when the delay ends), and every path Core
/// Animation blends draws the ends' segments. Two ends that draw the same
/// segments with no arc do, since every other command draws segments linear
/// in its numbers; an arc's do not, so a run with one, or with ends whose
/// segments differ, plays only under `steps()`, whose whole paths Core
/// Animation holds without blending.
pub(crate) fn paths_play(from: &PathValue, to: &PathValue, curve: &PlayedCurve) -> bool {
    let arc = |p: &PathValue| p.0.iter().any(|c| c.verb == b'A');
    let segments = |p: &PathValue| exact_kernel::svg::parse_d(&p.to_d()).0;
    let same = || {
        let (a, b) = (segments(from), segments(to));
        a.len() == b.len()
            && (a.iter().zip(&b))
                .all(|(x, y)| std::mem::discriminant(x) == std::mem::discriminant(y))
    };
    path_keys(curve).is_some() && (!blends(curve) || (!arc(from) && !arc(to) && same()))
}

/// The `path` spec of `key`'s played clip transition, as the box's
/// presenter plays it on the clip's shape layer (`BoxAnimations`): one
/// timing function between the ends for a cubic within [0, 1], discrete
/// keys at a `steps()`'s steps, and otherwise linear keys at the curve's
/// frames. The start holds through any delay (backwards fill), and the end
/// is the model's clip.
pub(crate) fn spec(engine: &Engine, key: u64) -> Option<String> {
    let (from, to, start, curve) = engine.played_clip(key)?;
    let keys = path_keys(&curve)?;
    let (times, progress) = keys.track();
    let at = |p: f64| {
        let mut out = String::new();
        let drawn = ClipPath::from_motion(&from.lerp(&to, p));
        super::svg::path_json(drawn.as_ref().map(ClipPath::path).as_ref(), &mut out);
        out
    };
    let num = |v: f64| crate::style::num(v as f32);
    let times: Vec<String> = times.into_iter().map(num).collect();
    let values: Vec<String> = progress.into_iter().map(at).collect();
    let curves = keys.cubic.map_or(String::new(), |c| {
        format!("[{},{},{},{}]", num(c[0]), num(c[1]), num(c[2]), num(c[3]))
    });
    Some(format!(
        "{{\"id\":\"clip#{start}#path\",\"k\":\"path\",\"s\":{start},\"dl\":0,\"d\":{},\"n\":1,\"t\":[{}],\"v\":[{}],\"c\":[{curves}],\"fill\":2,\"m\":{},\"h\":null}}",
        keys.duration,
        times.join(","),
        values.join(","),
        u8::from(keys.discrete)
    ))
}

/// The keys Core Animation plays a path's progress curve by (a clip's, and a
/// `d`'s, LLP 1115.003 D4), each a time and a progress.
pub(crate) struct PathKeys {
    pub duration: f64,
    pub keys: Vec<(f64, f64)>,
    /// One timing function between the two keys.
    pub cubic: Option<[f64; 4]>,
    /// Each key holds to the next, the last to the end (`kCAAnimationDiscrete`).
    pub discrete: bool,
}

impl PathKeys {
    /// The key times and the progress each key shows, as Core Animation
    /// takes them. Backwards fill holds the first key through any delay,
    /// where the run shows its start (CSS Transitions 1 §4), so a curve that
    /// starts elsewhere (a `steps()` that jumps at its start) begins just
    /// inside the run at its eased start. A discrete run has one key time
    /// more than its values: its last key holds to the end.
    pub(crate) fn track(&self) -> (Vec<f64>, Vec<f64>) {
        let mut keys = self.keys.clone();
        if keys[0].1 != 0.0 {
            keys[0].0 = 1e-4 * keys.get(1).map_or(1.0, |k| k.0);
            keys.insert(0, (0.0, 0.0));
        }
        let times = (keys.iter().map(|k| k.0)).chain(self.discrete.then_some(1.0));
        (times.collect(), keys.iter().map(|k| k.1).collect())
    }
}

/// A cubic within [0, 1] as one timing function between the ends; a
/// `steps()` as a key at each step, holding; any other curve (a cubic that
/// overshoots, `linear()`, a spring's frames) as linear keys at its frames.
pub(crate) fn path_keys(curve: &PlayedCurve) -> Option<PathKeys> {
    let (duration, keys, cubic, discrete) = match curve {
        PlayedCurve::Easing { easing, duration } => match easing {
            Easing::Steps { count, .. } => {
                let count = (*count).max(1);
                let n = f64::from(count);
                let keys = (0..count)
                    .map(|i| (f64::from(i) / n, easing.progress((f64::from(i) + 0.5) / n)))
                    .collect();
                (*duration, keys, None, true)
            }
            _ => match super::svg_lower::bezier(easing)
                .filter(|c| c.iter().all(|v| (0.0..=1.0).contains(v)))
            {
                Some(c) => (*duration, vec![(0.0, 0.0), (1.0, 1.0)], Some(c), false),
                None => {
                    let xs = (0..=32).map(|j| f64::from(j) / 32.0);
                    (
                        *duration,
                        xs.map(|x| (x, easing.progress(x))).collect(),
                        None,
                        false,
                    )
                }
            },
        },
        PlayedCurve::Frames { duration, values } => {
            let last = values.len().saturating_sub(1).max(1) as f64;
            let keys = values
                .iter()
                .enumerate()
                .map(|(i, v)| (i as f64 / last, v.x))
                .collect();
            (*duration, keys, None, false)
        }
    };
    (duration > 0.0 && keys.len() >= if discrete { 1 } else { 2 }).then_some(PathKeys {
        duration,
        keys,
        cubic,
        discrete,
    })
}

/// Whether Core Animation would blend between the keys of `curve`: every
/// curve but a `steps()`, whose keys hold.
pub(crate) fn blends(curve: &PlayedCurve) -> bool {
    !matches!(
        curve,
        PlayedCurve::Easing {
            easing: Easing::Steps { .. },
            ..
        }
    )
}
