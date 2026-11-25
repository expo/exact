//! Bounded contact synthesis through the same presenter as evdev/VNC.
//! Moves timestamp samples without seeking the runner. A seekable drive
//! advances the clock on a hold. A live carrier waits outside the host.
//! This is not OS input injection or evidence of physical frame delivery.
use super::*;
use serde_json::Value;

struct InputError(String);
// A String is a host refusal; malformed input goes through InputError::malformed.
impl From<String> for InputError {
    fn from(message: String) -> Self {
        Self(error(&message))
    }
}
impl From<&str> for InputError {
    fn from(message: &str) -> Self {
        Self(error(message))
    }
}
impl InputError {
    fn malformed(message: impl AsRef<str>) -> Self {
        Self(malformed(message.as_ref()))
    }
}

fn number(request: &Value, name: &str) -> Result<f64, InputError> {
    request[name]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| InputError::malformed(format!("contact {name} must be finite")))
}
fn coordinate(n: f64) -> Result<f32, InputError> {
    if n.abs() > f32::MAX as f64 {
        Err(InputError::malformed(
            "contact coordinate exceeds layout range",
        ))
    } else {
        Ok(n as f32)
    }
}
pub(crate) fn duration(request: &Value) -> Result<f64, &'static str> {
    let ms = if request.get("ms").is_some() {
        request["ms"]
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or("contact ms must be finite")?
    } else {
        0.
    };
    if !(0. ..=10_000.).contains(&ms) {
        return Err("contact duration must be in 0...10000ms");
    }
    Ok(ms)
}

pub(crate) fn answer<D: DataSource>(p: &mut Presenter<D>, request: &Value) -> String {
    perform(p, request).unwrap_or_else(|e| e.0)
}
fn perform<D: DataSource>(p: &mut Presenter<D>, request: &Value) -> Result<String, InputError> {
    let phase = request["phase"]
        .as_str()
        .ok_or_else(|| InputError::malformed("contact needs a phase"))?;
    let allowed: &[&str] = match phase {
        "down" => &["op", "session", "phase", "id", "x", "y", "mouse"],
        "move" => &[
            "op", "session", "phase", "x", "y", "dx", "dy", "ms", "mouse",
        ],
        // `virtual` is the driver's flag that it will not also seek (platformer R7).
        "hold" => &["op", "session", "phase", "ms", "mouse", "virtual"],
        "reorderHold" | "up" | "cancel" => &["op", "session", "phase", "mouse"],
        _ => return Err(InputError::malformed("unknown contact phase")),
    };
    if request
        .as_object()
        .unwrap()
        .keys()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(InputError::malformed("unexpected field for contact phase"));
    }
    // An accepted receipt can retire native contact before the shared driver
    // hears it. Terminal phases acknowledge that state without dispatch/time.
    if matches!(phase, "up" | "cancel") && p.contact_position().is_none() {
        p.pointer_released();
        return Ok(reply(p, phase, None, None));
    }
    let from = p.host().now();
    let live = p.live_time().is_some();
    let mut at;
    let mut wait_ms = None;
    if phase == "down" {
        if p.contact_position().is_some() {
            return Err("a contact is already down".into());
        }
        let id = request["id"]
            .as_u64()
            .filter(|n| *n <= u32::MAX as u64)
            .ok_or_else(|| InputError::malformed("contact down needs a live id"))?
            as u32;
        let explicit = request.get("x").is_some() || request.get("y").is_some();
        let xy = if explicit {
            Some((
                coordinate(number(request, "x")?)?,
                coordinate(number(request, "y")?)?,
            ))
        } else {
            None
        };
        let (x, y, w, h) = p.rect_of(id).ok_or("contact target has no painted box")?;
        let (x, y) = xy.unwrap_or((x + w / 2., y + h / 2.));
        let taken = p.pointer_down(x, y, from);
        if !matches!(taken, Ok(true)) {
            p.pointer_released(); // the driver holds no contact it cannot lift
        }
        if !taken? {
            return Err("contact down did not hit eligible input".into());
        }
        at = (x, y);
    } else {
        let (x, y) = p.contact_position().ok_or("no contact is down")?;
        let event_from = p.contact_event_ms(live.then_some(from)).unwrap();
        at = (x, y);
        match phase {
            "move" => {
                let relative = request.get("dx").is_some() || request.get("dy").is_some();
                let absolute = request.get("x").is_some() || request.get("y").is_some();
                if relative == absolute {
                    return Err(InputError::malformed(
                        "contact move needs exactly x/y or dx/dy",
                    ));
                }
                let (tx, ty) = if relative {
                    (
                        x as f64 + number(request, "dx")?,
                        y as f64 + number(request, "dy")?,
                    )
                } else {
                    (number(request, "x")?, number(request, "y")?)
                };
                let (tx, ty) = (coordinate(tx)?, coordinate(ty)?);
                let ms = duration(request).map_err(InputError::malformed)?;
                // Fixed upper bound; no real-time sleeps or queued work.
                let steps = ((ms / 16.).ceil() as usize).clamp(1, 256);
                for i in 1..=steps {
                    let f = i as f64 / steps as f64;
                    let sample = (
                        (x as f64 + (tx as f64 - x as f64) * f) as f32,
                        (y as f64 + (ty as f64 - y as f64) * f) as f32,
                    );
                    p.pointer_move_at(sample.0, sample.1, from, event_from + ms * f)?;
                    at = sample;
                    if p.contact_position().is_none() {
                        break;
                    }
                }
            }
            "hold" => {
                let ms = duration(request).map_err(InputError::malformed)?;
                if live {
                    wait_ms = Some(ms);
                } else {
                    let (_, failure) = sensors::hold_framed(p, from + ms);
                    if let Some(error) = failure {
                        return Err(error.into());
                    }
                    p.delay_contact_event(ms);
                }
            }
            "reorderHold" => {
                if !p.recognize_arrange_hold(from) {
                    return Err("the contact no longer holds the reorder grip".into());
                }
            }
            "up" => {
                let event_ms = if live { event_from } else { event_from + 8. };
                p.pointer_up_at(x, y, from, event_ms)?;
            }
            "cancel" => p.pointer_lost(from)?,
            _ => unreachable!(),
        }
    }
    if !live {
        p.land_presses();
    }
    Ok(reply(p, phase, Some(at), wait_ms))
}
fn reply<D: DataSource>(
    p: &Presenter<D>,
    phase: &str,
    at: Option<(f32, f32)>,
    wait_ms: Option<f64>,
) -> String {
    let (mode, timing) = if p.live_time().is_some() {
        (
            "platform",
            "moves timestamped; carrier waits for holds; physical delivery unobserved",
        )
    } else {
        (
            "headless",
            "moves timestamped; holds seek; physical delivery unobserved",
        )
    };
    let mut reply = serde_json::json!({
        "phase": phase, "contact": p.contact_position().is_some(), "at": at,
        "clock": p.host().now(),
        "delivery": "presenter", "carrier": std::env::consts::OS, "mode": mode,
        "native": "Presenter.pointer_*", "timing": timing
    });
    if let Some(ms) = wait_ms {
        reply["waitMs"] = ms.into();
    }
    reply.to_string()
}
