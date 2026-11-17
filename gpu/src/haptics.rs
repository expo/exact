//! A surface's haptics (LLP 1009 D10), by Core Haptics' names: a texture the
//! surface starts, changes as often as every frame and stops, and short
//! patterns of transient and continuous events, with parameter curves, played
//! beside it. A surface hands each to the host through [`crate::Surface::messages`]
//! as [`Haptic::message`]'s string, which the host consumes, as it consumes
//! `"exact:audio"`; the app never sees it.
//!
//! The host plays them on iOS (Core Haptics) and Android (the vibrator), and
//! owns their life: a surface's texture stops when the surface is destroyed,
//! hidden or interrupted, or the app leaves the foreground, and a later
//! texture message starts it again. The person's system haptics setting
//! applies, as it does to `haptic()`. Elsewhere the host consumes the message
//! and plays nothing (`gpu/README.md`, "A module's own haptics").
//!
//! Every value is clamped to its range, and a non-finite one is 0.

/// What prefixes a haptic message.
pub const PREFIX: &str = "exact:haptics ";

/// The longest a pattern may last, seconds: what a pattern says beyond it is
/// cut there. Core Haptics' continuous event lasts at most 30 s.
pub const PATTERN_SECONDS: f32 = 30.0;

/// One request to the host.
#[derive(Debug, Clone, PartialEq)]
pub enum Haptic {
    /// Start the surface's texture, or move it to these values: a
    /// continuous vibration whose `intensity` (0–1, how strong) and
    /// `sharpness` (0–1, from a dull rumble to a crisp buzz) take effect at
    /// once. One texture a surface.
    Texture {
        /// 0–1.
        intensity: f32,
        /// 0–1.
        sharpness: f32,
    },
    /// Stop the surface's texture.
    Stop,
    /// Play a short pattern now, beside the texture.
    Pattern(Pattern),
}

/// A pattern: Core Haptics' `CHHapticPattern`, its events and its dynamic
/// parameter curves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pattern {
    /// Its events, each at its own time.
    pub events: Vec<Event>,
    /// Curves over the whole pattern.
    pub curves: Vec<Curve>,
}

/// A pattern's event, its `time` seconds after the pattern starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A tap: `CHHapticEvent.EventType.hapticTransient`.
    Transient {
        /// Seconds from the pattern's start.
        time: f32,
        /// 0–1.
        intensity: f32,
        /// 0–1.
        sharpness: f32,
    },
    /// A vibration lasting `duration` seconds:
    /// `CHHapticEvent.EventType.hapticContinuous`.
    Continuous {
        /// Seconds from the pattern's start.
        time: f32,
        /// Seconds.
        duration: f32,
        /// 0–1.
        intensity: f32,
        /// 0–1.
        sharpness: f32,
    },
}

/// What a curve moves: Core Haptics' dynamic parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parameter {
    /// `hapticIntensityControl`: multiplies each event's intensity, 0–1.
    Intensity,
    /// `hapticSharpnessControl`: added to each event's sharpness, −1–1.
    Sharpness,
}

/// A `CHHapticParameterCurve`: the parameter at each point's time (seconds
/// from the pattern's start), linearly between points.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// What it moves.
    pub parameter: Parameter,
    /// `(time, value)`, in time order.
    pub points: Vec<(f32, f32)>,
}

impl Haptic {
    /// The texture at these values.
    pub fn texture(intensity: f32, sharpness: f32) -> Haptic {
        Haptic::Texture {
            intensity,
            sharpness,
        }
    }

    /// The message a surface returns from [`crate::Surface::messages`]:
    /// [`PREFIX`], then the request as JSON.
    pub fn message(&self) -> String {
        let mut s = String::from(PREFIX);
        match self {
            Haptic::Texture {
                intensity,
                sharpness,
            } => {
                s.push_str("{\"texture\":{\"intensity\":");
                number(&mut s, unit(*intensity));
                s.push_str(",\"sharpness\":");
                number(&mut s, unit(*sharpness));
                s.push_str("}}");
            }
            Haptic::Stop => s.push_str("{\"texture\":null}"),
            Haptic::Pattern(p) => {
                s.push_str("{\"pattern\":{\"events\":[");
                for (i, e) in p.events.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    let (kind, time, duration, intensity, sharpness) = match *e {
                        Event::Transient {
                            time,
                            intensity,
                            sharpness,
                        } => ("transient", time, None, intensity, sharpness),
                        Event::Continuous {
                            time,
                            duration,
                            intensity,
                            sharpness,
                        } => ("continuous", time, Some(duration), intensity, sharpness),
                    };
                    let time = seconds(time);
                    s.push_str(&format!("{{\"type\":\"{kind}\",\"time\":"));
                    number(&mut s, time);
                    if let Some(d) = duration {
                        s.push_str(",\"duration\":");
                        number(&mut s, seconds(d).min(PATTERN_SECONDS - time));
                    }
                    s.push_str(",\"intensity\":");
                    number(&mut s, unit(intensity));
                    s.push_str(",\"sharpness\":");
                    number(&mut s, unit(sharpness));
                    s.push('}');
                }
                s.push_str("],\"curves\":[");
                for (i, c) in p.curves.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    let (name, low) = match c.parameter {
                        Parameter::Intensity => ("intensity", 0.0),
                        Parameter::Sharpness => ("sharpness", -1.0),
                    };
                    s.push_str(&format!("{{\"parameter\":\"{name}\",\"points\":["));
                    for (j, (t, v)) in c.points.iter().enumerate() {
                        if j > 0 {
                            s.push(',');
                        }
                        s.push('[');
                        number(&mut s, seconds(*t));
                        s.push(',');
                        number(&mut s, finite(*v).clamp(low, 1.0));
                        s.push(']');
                    }
                    s.push_str("]}");
                }
                s.push_str("]}}");
            }
        }
        s
    }
}

fn finite(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn unit(v: f32) -> f32 {
    finite(v).clamp(0.0, 1.0)
}

fn seconds(v: f32) -> f32 {
    finite(v).clamp(0.0, PATTERN_SECONDS)
}

/// A finite f32 as JSON: its shortest text, which never uses an exponent
/// form JSON refuses.
fn number(s: &mut String, v: f32) {
    use std::fmt::Write;
    let _ = write!(s, "{v}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_is_clamped_and_a_stop_is_null() {
        assert_eq!(
            Haptic::texture(0.5, 0.25).message(),
            r#"exact:haptics {"texture":{"intensity":0.5,"sharpness":0.25}}"#
        );
        assert_eq!(
            Haptic::texture(2.0, f32::NAN).message(),
            r#"exact:haptics {"texture":{"intensity":1,"sharpness":0}}"#
        );
        assert_eq!(Haptic::Stop.message(), r#"exact:haptics {"texture":null}"#);
    }

    #[test]
    fn a_pattern_carries_its_events_and_curves_within_their_ranges() {
        let p = Pattern {
            events: vec![
                Event::Transient {
                    time: 0.0,
                    intensity: 1.0,
                    sharpness: 0.75,
                },
                Event::Continuous {
                    time: 29.5,
                    duration: 2.0,
                    intensity: 0.5,
                    sharpness: -1.0,
                },
            ],
            curves: vec![
                Curve {
                    parameter: Parameter::Intensity,
                    points: vec![(0.0, 1.0), (0.25, 0.0)],
                },
                Curve {
                    parameter: Parameter::Sharpness,
                    points: vec![(0.0, -2.0)],
                },
            ],
        };
        assert_eq!(
            Haptic::Pattern(p).message(),
            concat!(
                r#"exact:haptics {"pattern":{"events":["#,
                r#"{"type":"transient","time":0,"intensity":1,"sharpness":0.75},"#,
                r#"{"type":"continuous","time":29.5,"duration":0.5,"intensity":0.5,"sharpness":0}],"#,
                r#""curves":[{"parameter":"intensity","points":[[0,1],[0.25,0]]},"#,
                r#"{"parameter":"sharpness","points":[[0,-1]]}]}}"#
            )
        );
    }
}
