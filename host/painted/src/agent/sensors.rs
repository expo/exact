//! Clock advances stop at world ticks so their published records reach
//! Contract before later timers (LLP 1012, LLP 1046.003). Sensor inputs also
//! stop each display frame, posting its samples before the world advances
//! (LLP 1133.013 D6), within the world's 1,024-event bound.

use super::{clock_hatched, Presenter};
use exact_runner::DataSource;

/// One display frame of the agent's clock, in ms.
const FRAME: f64 = 1000.0 / 60.0;

/// [`clock_hatched`] to `to`, stopping at world ticks and sensor frames;
/// the caller steps the worlds to the last.
pub(super) fn clock_framed<D: DataSource>(
    p: &mut Presenter<D>,
    to: f64,
    deadline: std::time::Instant,
) -> (f64, Option<String>) {
    framed(p, to, |p, step| clock_hatched(p, step, deadline))
}

/// [`Presenter::clock`] to `to`, as a contact's hold seeks it, framed as
/// [`clock_framed`] is: a hold may not leave a world's samples unposted.
pub(super) fn hold_framed<D: DataSource>(p: &mut Presenter<D>, to: f64) -> (f64, Option<String>) {
    framed(p, to, |p, step| p.clock(step))
}

fn framed<D: DataSource>(
    p: &mut Presenter<D>,
    to: f64,
    mut seek: impl FnMut(&mut Presenter<D>, f64) -> (f64, Option<String>),
) -> (f64, Option<String>) {
    loop {
        // Deliver world records before later timers (LLP 1012, LLP 1046.003).
        p.sync_surfaces();
        let worlds = p.worlds(serde_json::json!({"op":"clock","settle":false}));
        p.sync_surfaces();
        let now = p.host().now();
        let next = worlds
            .iter()
            .filter_map(|w| w["nextTickAt"].as_f64())
            .filter(|at| at.is_finite() && *at > now)
            .reduce(f64::min);
        let sensing = p.host().runner().surface_sensor_inputs();
        if worlds.is_empty() && !sensing {
            return seek(p, to);
        }
        // The frame's samples reach the world before the frame does: a
        // world steps as its host's clock moves, and takes each queued
        // event in the tick its stamp falls in.
        let mut step = next.unwrap_or(to).min(to);
        if sensing {
            step = step.min(now + FRAME);
        }
        if let Some(due) = p.host().timer_due_ms().filter(|due| *due > now) {
            step = step.min(due);
        }
        if sensing {
            post_samples(p, step);
        }
        let (landed, e) = seek(p, step);
        if e.is_some() || landed < step || landed >= to {
            return (landed, e);
        }
        p.sync_surfaces();
        p.worlds(serde_json::json!({"op":"clock","settle":false}));
    }
}

/// Every sample the substitute's canvas inputs owe their surfaces up to `at`.
fn post_samples<D: DataSource>(p: &mut Presenter<D>, at: f64) {
    let events = p.host_mut().runner_mut().surface_sensor_events(at);
    for (surface, event) in events {
        let Ok(event) = serde_json::from_str(&event) else {
            continue;
        };
        if !p.surfaces.post(&surface, event) {
            let line = format!("sensor input {surface}: dropped, the surface's posts are full");
            p.host_mut().log(&line);
        }
    }
}
