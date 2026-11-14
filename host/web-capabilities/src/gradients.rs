//! `background-image` and `mask-image` on the web: the kernel's gradient grammar,
//! linked when a plan binds the row, so an app that paints no gradient
//! carries none of it.

use exact_web::Linked;

/// Link the gradient grammar into `linked` (LLP 1066, LLP 1077 D2).
pub const fn link(mut linked: Linked) -> Linked {
    linked.gradients = Some(exact_kernel::style::link_gradients);
    linked
}
