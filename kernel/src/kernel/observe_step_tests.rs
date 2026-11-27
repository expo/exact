//! D9's step on readers that read each other (LLP 1123 D9, "Each reader advances").

use super::*;

#[test]
fn coupled_readers_whose_secants_swing_settle_by_half_steps() {
    // Two readers that read each other, as on the web a box 700 above the
    // foot of an 800-point page under a 47-point band, as tall as its
    // child, reserving 10 plus its own top observation, and its child 100
    // tall less a quarter of its own: the box reads 47 less a quarter of
    // the child's input, the child 57 plus the box's input less a quarter of
    // its own, up to that area's depth. Each round measures both, then steps
    // both, as the solve does. The child's fixed point is 69.33; from a third
    // of the bound half steps settle both within the bound (D9)
    let tolerance = 0.5;
    let (mut a, mut b) = (Samples::default(), Samples::default());
    let (mut x, mut y) = ([Some(0.0f32); COMPONENTS], [Some(0.0f32); COMPONENTS]);
    let mut settled_at = None;
    for round in 0..MAX_ROUNDS {
        let (box_in, child_in) = (x[0].unwrap(), y[0].unwrap());
        let mut box_m = x;
        box_m[0] = Some((47.0 - 0.25 * child_in).clamp(0.0, 47.0));
        let mut child_m = y;
        let depth = 57.0 + box_in;
        child_m[0] = Some((depth - 0.25 * child_in).clamp(0.0, depth));
        if settled(x, box_m, tolerance, 1) && settled(y, child_m, tolerance, 1) {
            settled_at = Some(round);
            break;
        }
        let mut caps = [f32::MAX; COMPONENTS];
        caps[0] = 47.0;
        x = a.step(x, box_m, caps, tolerance, 1, round);
        caps[0] = depth;
        y = b.step(y, child_m, caps, tolerance, 1, round);
    }
    assert!(settled_at.is_some(), "unsettled: {:?} {:?}", x[0], y[0]);
    let child = y[0].unwrap();
    assert!((child - 69.33).abs() <= 1.0, "{child}");
}
