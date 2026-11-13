use crate::{Vec2, Vec3};

/// Frame-time translation of a camera and its descendants from one pointer.
/// Write from `Game::present` with the position the latest tick consumed.
/// The renderer adds only the motion between that interpolated position and
/// `Sim::frame_pointer`: no spring, prediction, or simulation mutation.
///
/// Each tick must translate the camera by the pointer's position change with
/// this same world-space mapping. Keep the mapping fixed during a drag, and
/// keep it through the tick that consumes release. For a clamped pan, set
/// bounds to the same world-space camera limits the tick applies. After release,
/// tick-driven inertia uses normal interpolation; set delta to zero.
/// This does not describe a locked mouse or rotation.
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
    /// Minimum and maximum world-space camera position after this frame's
    /// translation. None leaves it unbounded. NaN or reversed bounds are
    /// ignored. Infinite limits leave an axis unbounded. A frame with no
    /// unshown pointer motion is not moved.
    pub bounds: Option<[Vec3; 2]>,
}
impl PointerPan {
    /// Translate a pointer displacement through the same mapping as presentation.
    pub fn translation(self, delta: Vec2) -> Vec3 {
        self.horizontal * delta.x + self.vertical * delta.y
    }
}
