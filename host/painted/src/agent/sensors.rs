//! A clock advance across which a canvas has a sensor input (LLP 1133.013
//! D6): the worlds advance a display frame at a time, each frame's samples
//! handed to their surfaces before the frame, as a live frame does. One jump
//! would queue a whole span at once, past the world's 1,024-event bound.

use super::{clock_hatched, Presenter};
use exact_runner::DataSource;

/// One display frame of the agent's clock, in ms.
const FRAME: f64 = 1000.0 / 60.0;

/// [`clock_hatched`] to `to`, stepping the worlds a frame at a time while
/// the substitute feeds a canvas; the caller steps the worlds to the last.
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
        if !p.host().runner().surface_sensor_inputs() {
            return seek(p, to);
        }
        // The frame's samples reach the world before the frame does: a
        // world steps as its host's clock moves, and takes each queued
        // event in the tick its stamp falls in.
        let step = (p.host().now() + FRAME).min(to);
        post_samples(p, step);
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
