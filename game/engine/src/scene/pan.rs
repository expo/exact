use crate::{Vec2, Vec3};

/// Frame-time translation of a camera and its descendants from one pointer.
/// Write from `Game::present` with the position the latest tick consumed.
/// The renderer adds only the motion between that interpolated position and
/// `Sim::frame_pointer`: no spring, prediction, or simulation mutation.
///
/// Each tick must translate the camera by the pointer's position change with
/// this same world-space mapping. Keep the mapping fixed during a drag, and
/// keep it through the tick that consumes release. This does not
/// describe a locked mouse, clamped pan, rotation, or a gesture with inertia.
#[derive(Clone, Copy, Debug, Default, PartialEq, crate::Presentation)]
pub struct PointerPan {
    /// Pointer whose contact the tick is following.
    pub pointer: u64,
    /// Canvas position in points already represented by the tick's camera pose.
    pub position: Vec2,
    /// Pointer displacement applied by the latest tick, before this mapping.
    /// Zero on a tick with no motion, including after release.
    pub delta: Vec2,
    /// World-space camera translation per horizontal pointer point.
    pub horizontal: Vec3,
    /// World-space camera translation per vertical pointer point.
    pub vertical: Vec3,
}
impl PointerPan {
    /// Translate a pointer displacement through the same mapping as presentation.
    pub fn translation(self, delta: Vec2) -> Vec3 {
        self.horizontal * delta.x + self.vertical * delta.y
    }
}
