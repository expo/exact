//! Axis-angle rotation shared by style rows and keyframes (LLP 1077 D8).
//! A value holds degrees in x and the axis in y/z/w. An omitted axis is
//! zero, so a keyframe can take its node's axis without losing that intent
//! when the plan serializes it.

use crate::{math, Value};
use exact_num::Shortest;

fn angle(text: &str) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    let (n, factor) = [
        ("deg", 1.0),
        ("grad", 0.9),
        ("rad", 180.0 / std::f64::consts::PI),
        ("turn", 360.0),
    ]
    .into_iter()
    .find_map(|(unit, factor)| lower.strip_suffix(unit).map(|n| (n, factor)))
    .unwrap_or((&lower, 1.0));
    let n = exact_num::parse_f64(n).ok()? * factor;
    n.is_finite().then_some(n)
}

/// CSS rotate, including the unitless degrees accepted by Contract.
pub fn parse(text: &str) -> Option<Value> {
    let words: Vec<_> = text.split_ascii_whitespace().collect();
    match words.as_slice() {
        [none] if none.eq_ignore_ascii_case("none") => Some(Value::scalar(0.0)),
        [a] => Some(Value::scalar(angle(a)?)),
        [axis, a] | [a, axis] if angle(a).is_some() && axis.len() == 1 => {
            let [x, y, z] = match axis.to_ascii_lowercase().as_str() {
                "x" => [1.0, 0.0, 0.0],
                "y" => [0.0, 1.0, 0.0],
                "z" => [0.0, 0.0, 1.0],
                _ => return None,
            };
            Some(Value::four(angle(a)?, x, y, z))
        }
        [x, y, z, a] => {
            let v = Value::four(
                angle(a)?,
                exact_num::parse_f64(x).ok()?,
                exact_num::parse_f64(y).ok()?,
                exact_num::parse_f64(z).ok()?,
            );
            // Native axis rows are f32; zero is the omitted-axis sentinel.
            let axis = [v.y, v.z, v.w].map(|n| n as f32);
            (v.is_finite() && axis.iter().all(|n| n.is_finite()) && axis != [0.0; 3]).then_some(v)
        }
        _ => None,
    }
}

/// A static rotation. The usual z axis keeps the scalar representation.
pub fn value(angle: f64, axis: [f64; 3]) -> Value {
    if axis == [0.0, 0.0, 1.0] {
        Value::scalar(angle)
    } else {
        Value::four(angle, axis[0], axis[1], axis[2])
    }
}

/// The axis, with CSS's z initial value when none was supplied.
pub fn axis(v: Value) -> [f64; 3] {
    if [v.y, v.z, v.w] == [0.0; 3] {
        [0.0, 0.0, 1.0]
    } else {
        [v.y, v.z, v.w]
    }
}

/// Resolve an angle-only keyframe against its node's declared rotation.
pub fn with_axis(v: Value, underlying: Value) -> Value {
    if [v.y, v.z, v.w] == [0.0; 3] {
        Value::four(v.x, underlying.y, underlying.z, underlying.w)
    } else {
        v
    }
}

/// CSS text retaining whether the keyframe declared its axis.
pub fn css(v: Value) -> String {
    let prefix = match [v.y, v.z, v.w] {
        [0.0, 0.0, 0.0] => String::new(),
        [1.0, 0.0, 0.0] => "x ".into(),
        [0.0, 1.0, 0.0] => "y ".into(),
        [0.0, 0.0, 1.0] => "z ".into(),
        [x, y, z] => format!("{} {} {} ", Shortest(x), Shortest(y), Shortest(z)),
    };
    format!("{prefix}{}deg", Shortest(v.x))
}

fn normal(v: Value) -> [f64; 3] {
    let a = axis(v);
    let m = a.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let a = a.map(|v| v / m);
    let n = math::sqrt(a.iter().map(|v| v * v).sum());
    a.map(|v| v / n)
}

// Kernel rows store axes as f32; parsed keyframes retain f64 precision.
fn common_axis(a: [f64; 3], b: [f64; 3]) -> bool {
    a.into_iter()
        .zip(b)
        .all(|(a, b)| (a - b).abs() <= f32::EPSILON as f64)
}

fn quaternion(v: Value, a: [f64; 3]) -> [f64; 4] {
    let half = v.x.to_radians() / 2.0;
    let s = math::sin(half);
    [a[0] * s, a[1] * s, a[2] * s, math::cos(half)]
}

/// Whether a scalar Z track preserves this interval's rotation interpolation.
pub fn planar(from: Value, to: Value) -> bool {
    [from, to].iter().all(|v| v.y == 0.0 && v.z == 0.0)
        && (common_axis(normal(from), normal(to)) || from.x == 0.0 || to.x == 0.0)
}

/// Angular travel in degrees. A common axis keeps authored full turns;
/// differing axes take the shortest quaternion arc.
pub fn distance(from: Value, to: Value) -> f64 {
    let (a, b) = (normal(from), normal(to));
    if common_axis(a, b) || from.x == 0.0 || to.x == 0.0 {
        return to.x - from.x;
    }
    let dot: f64 = quaternion(from, a)
        .iter()
        .zip(quaternion(to, b))
        .map(|(a, b)| a * b)
        .sum();
    2.0 * math::acos(dot.abs().min(1.0)).to_degrees()
}

// The interpolation path's axis in the parent's coordinate frame. A common
// axis uses signed angle travel; slerp uses positive travel around b * inverse(a).
fn direction(from: Value, to: Value) -> [f64; 3] {
    let (a, b) = (normal(from), normal(to));
    if common_axis(a, b) || from.x == 0.0 || to.x == 0.0 {
        return if from.x == 0.0 { b } else { a };
    }
    let (a, b) = (quaternion(from, a), quaternion(to, b));
    let dot: f64 = a.iter().zip(b).map(|(a, b)| a * b).sum();
    let d = [
        a[3] * b[0] - b[3] * a[0] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - b[3] * a[1] + a[2] * b[0] - a[0] * b[2],
        a[3] * b[2] - b[3] * a[2] + a[0] * b[1] - a[1] * b[0],
    ];
    let n = math::sqrt(d.iter().map(|v| v * v).sum());
    if n == 0.0 {
        return normal(to);
    }
    d.map(|v| v / n * if dot < 0.0 { -1.0 } else { 1.0 })
}

/// Carry signed angular velocity through a retarget in the parent's frame,
/// projected onto the new interpolation path's axis.
pub(crate) fn retarget_velocity(from: Value, to: Value, at: Value, next: Value, speed: f64) -> f64 {
    let angular = direction(from, to).map(|v| v * speed);
    angular
        .into_iter()
        .zip(direction(at, next))
        .map(|(v, d)| v * d)
        .sum()
}

/// CSS Transforms 2: interpolate angles on a common axis; otherwise slerp
/// their quaternions. A zero angle adopts the other endpoint's axis.
pub fn interpolate(from: Value, to: Value, p: f64) -> Value {
    presented(interpolate_axis(from, to, p))
}

/// Canonical presentation for hosts whose in-plane renderer reads only the angle.
pub fn presented(v: Value) -> Value {
    // Native 2D renderers draw z with the angle alone, as static rows do.
    if v.y == 0.0 && v.z == 0.0 && v.w != 0.0 {
        Value::four(if v.w < 0.0 { -v.x } else { v.x }, 0.0, 0.0, 1.0)
    } else {
        v
    }
}

pub(crate) fn interpolate_axis(from: Value, to: Value, p: f64) -> Value {
    if p == 0.0 {
        return from;
    }
    if p == 1.0 {
        return to;
    }
    let (a, b) = (normal(from), normal(to));
    if common_axis(a, b) || from.x == 0.0 || to.x == 0.0 {
        let v = if from.x == 0.0 { to } else { from };
        return Value::four(from.x + (to.x - from.x) * p, v.y, v.z, v.w);
    }
    let (a, mut b) = (quaternion(from, a), quaternion(to, b));
    let mut dot: f64 = a.iter().zip(b).map(|(a, b)| a * b).sum();
    if dot < 0.0 {
        b = b.map(|v| -v);
        dot = -dot;
    }
    let (u, v) = if dot > 0.9995 {
        (1.0 - p, p)
    } else {
        let t = math::acos(dot.clamp(-1.0, 1.0));
        (
            math::sin((1.0 - p) * t) / math::sin(t),
            math::sin(p * t) / math::sin(t),
        )
    };
    let q = std::array::from_fn::<_, 4, _>(|i| a[i] * u + b[i] * v);
    let n = math::sqrt(q.iter().map(|v| v * v).sum());
    let q = q.map(|v| v / n);
    let s = math::sqrt(q[..3].iter().map(|v| v * v).sum());
    if s < 1e-12 {
        return Value::scalar(0.0);
    }
    Value::four(
        2.0 * math::atan2(s, q[3]).to_degrees(),
        q[0] / s,
        q[1] / s,
        q[2] / s,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Animation, Animations, Keyframes, Property};

    fn animation(css: &str) -> Animation {
        let mut a = Animations::parse("fold 1s linear both")
            .unwrap()
            .0
            .remove(0);
        a.keyframes = Keyframes::parse(css).unwrap();
        a
    }

    #[test]
    fn axes_must_remain_finite_and_nonzero_in_native_storage() {
        assert_eq!(parse("1e100 0 0 90deg"), None);
        assert!(Keyframes::parse("100%{rotate:1e100 0 0 90deg;}").is_err());
        assert_eq!(parse("1e-50 0 0 90deg"), None);
        assert!(Keyframes::parse("100%{rotate:1e-50 0 0 90deg;}").is_err());
        let v = parse("1e-20 0 0 90deg").unwrap();
        assert_eq!(parse(&css(v)), Some(v));
    }

    #[test]
    fn retargeting_a_spring_preserves_signed_angular_motion() {
        use crate::{Change, Engine, Transitions};
        for (from, to, next) in [
            ("z 60deg", "0 0 -1 60deg", "z 120deg"),
            ("0 0 -1 60deg", "z 60deg", "0 0 -1 120deg"),
            ("0 0 -1 60deg", "0 0 -1 120deg", "z 120deg"),
        ] {
            let mut e = Engine::new();
            e.set_transitions(
                1,
                Transitions::parse("rotate -exact-spring(100, 10, 1)").unwrap(),
            )
            .unwrap();
            let change = |text| Change {
                node: 1,
                property: Property::Rotate,
                value: parse(text).unwrap(),
                velocity: None,
            };
            e.observe(change(from)).unwrap();
            e.observe(change(to)).unwrap();
            e.advance(0.04999).unwrap();
            let before = e.value(1, Property::Rotate).unwrap().x;
            e.advance(0.05).unwrap();
            let at = e.value(1, Property::Rotate).unwrap().x;
            e.observe(change(next)).unwrap();
            assert!((e.value(1, Property::Rotate).unwrap().x - at).abs() < 1e-8);
            e.advance(0.05001).unwrap();
            let after = e.value(1, Property::Rotate).unwrap().x;
            let (incoming, outgoing) = (at - before, after - at);
            assert!(
                incoming * outgoing > 0.,
                "{from} -> {to} -> {next}: {incoming} -> {outgoing}"
            );
            assert!((outgoing / incoming - 1.).abs() < 0.01);
        }
    }

    #[test]
    fn opposite_z_axes_keep_equivalent_orientations_still() {
        let a = animation("0%{rotate:0 0 -1 10deg;}100%{rotate:z 350deg;}");
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let v = a.sample(t, Property::Rotate, Value::ZERO).unwrap();
            assert!(
                (v.x.to_radians().sin() - (-10_f64).to_radians().sin()).abs() < 1e-8,
                "{t}: {v:?}"
            );
            assert_eq!(axis(v), [0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn transitions_and_springs_interpolate_rotations() {
        use crate::{Change, Engine, Transitions};
        for (from, to, angle, expected_axis) in [
            ("x 0deg", "z 90deg", 45.0, [0.0, 0.0, 1.0]),
            ("x 90deg", "y 90deg", 70.5287793655, [1.0, 1.0, 0.0]),
            ("x 90deg", "z 45deg", 52.4833604988, [0.0; 3]),
        ] {
            for timing in ["1s linear", "-exact-spring(100, 10, 1)"] {
                let mut e = Engine::new();
                let (from, to) = (parse(from).unwrap(), parse(to).unwrap());
                e.observe(Change {
                    node: 1,
                    property: Property::Rotate,
                    value: from,
                    velocity: None,
                })
                .unwrap();
                e.set_transitions(1, Transitions::parse(&format!("rotate {timing}")).unwrap())
                    .unwrap();
                e.observe(Change {
                    node: 1,
                    property: Property::Rotate,
                    value: to,
                    velocity: None,
                })
                .unwrap();
                let progress = if timing == "1s linear" {
                    0.5
                } else {
                    1.0 + crate::SpringConfig::default()
                        .sample(-1.0, 0.0, 0.5)
                        .displacement
                };
                e.advance(0.5).unwrap();
                let actual = e.value(1, Property::Rotate).unwrap();
                let expected = interpolate(from, to, progress);
                assert!(
                    (actual.x - expected.x).abs() < 1e-8,
                    "{timing}: {actual:?} != {expected:?}"
                );
                for (a, b) in axis(actual).into_iter().zip(axis(expected)) {
                    assert!((a - b).abs() < 1e-12);
                }
                if timing != "1s linear" {
                    let (track, _) = e.transition(1, Property::Rotate).unwrap();
                    let crate::PlayedCurve::Frames { values, .. } = track.curve else {
                        panic!("spring frames")
                    };
                    let sample = values[(0.5 * crate::spring::SAMPLE_RATE) as usize];
                    assert_eq!(sample, actual);
                }
                if timing == "1s linear" && expected_axis != [0.0; 3] {
                    assert!((actual.x - angle).abs() < 1e-8);
                    for (a, b) in normal(actual)
                        .into_iter()
                        .zip(normal(value(angle, expected_axis)))
                    {
                        assert!((a - b).abs() < 1e-12);
                    }
                }
            }
        }
    }

    #[test]
    fn a_fold_keeps_its_axis_through_parse_roundtrip_and_sampling() {
        for text in ["-90deg x", "x -0.25turn", "1 0 0 -100grad"] {
            let a = animation(&format!("100%{{rotate:{text};}}"));
            assert_eq!(Keyframes::parse(&a.keyframes.css()).unwrap(), a.keyframes);
            let v = a.sample(0.5, Property::Rotate, Value::ZERO).unwrap();
            assert_eq!(v.x, -45.0);
            assert_eq!(axis(v), [1.0, 0.0, 0.0]);
        }
        let a = animation("100%{rotate:-90deg;}");
        for a0 in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            let v = a.sample(0.5, Property::Rotate, value(0.0, a0)).unwrap();
            assert_eq!(v.x, -45.0);
            assert_eq!(axis(v), a0);
        }
        let a = animation("0%{rotate:0deg z;}100%{rotate:90deg z;}");
        let v = a
            .sample(0.5, Property::Rotate, value(0.0, [1.0, 0.0, 0.0]))
            .unwrap();
        assert_eq!(axis(v), [0.0, 0.0, 1.0]);
        assert_eq!(v.x, 45.0);
    }

    #[test]
    fn retargeted_negative_z_turns_keep_the_authored_axis() {
        use crate::{Change, Engine, Transitions};
        let mut e = Engine::new();
        e.set_transitions(1, Transitions::parse("rotate 1s linear").unwrap())
            .unwrap();
        for (time, angle) in [(0., 10.), (0., 370.), (0.5, 730.)] {
            e.advance(time).unwrap();
            e.observe(Change {
                node: 1,
                property: Property::Rotate,
                value: value(angle, [0., 0., -1.]),
                velocity: None,
            })
            .unwrap();
        }
        e.advance(1.).unwrap();
        assert_eq!(e.value(1, Property::Rotate).unwrap().x, -460.);
        assert_eq!(axis(e.target(1, Property::Rotate).unwrap()), [0., 0., -1.]);
    }

    #[test]
    fn differing_axes_slerp_and_a_common_axis_keeps_full_turns() {
        let v = interpolate(parse("x 90deg").unwrap(), parse("y 90deg").unwrap(), 0.5);
        assert!((v.x - 70.5287793655).abs() < 1e-8);
        let n = normal(v);
        assert!((n[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert!((n[1] - n[0]).abs() < 1e-12);
        assert_eq!(n[2], 0.0);
        assert_eq!(
            interpolate(parse("x 0deg").unwrap(), parse("x 720deg").unwrap(), 0.5).x,
            360.0
        );
    }
}
