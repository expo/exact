//! What a host links of the answers the runner gives itself (LLP 1047 D3).

use super::{canvas2d, router, RunnerError};
use crate::DataError;
use exact_plan::{Plan, ResourcesRow, Value};

/// What a host links of the answers the runner gives itself (LLP 1047 D3):
/// each capability's, or `None` when the artifact doesn't link it, so its
/// code is gone. Native hosts and tests boot with [`RunnerLinks::ALL`]; the
/// web host passes what its entry registered.
#[derive(Clone, Copy)]
pub struct RunnerLinks {
    /// A GPU surface's published record, as its `exactSurface` resource.
    pub surface_answer: SurfaceAnswer,
    /// The plan's router (LLP 1038), from its route table and shapes.
    pub router: RouterLink,
    /// The list engines (LLP 1047.000 §9): [`crate::instance::LISTS`].
    pub lists: Option<&'static crate::instance::ListLinks>,
    /// Canvas 2D (LLP 1056), with surfaces: [`canvas2d::engine`].
    pub canvas: CanvasLink,
    /// `formatDate` and `formatNumber` (LLP 1054.000.003 D8):
    /// [`crate::formatting`].
    pub format: FormatLink,
    /// `frame` and `measure` (LLP 1051.000 D3/D4): the kernel's answers
    /// natively ([`crate::geometry::KERNEL`]), the page's on the web.
    pub geometry: GeometryLink,
    /// The I/O grant grammar (LLP 1047.001): [`crate::grants::LINKED`].
    pub grants: crate::grants::GrantsLink,
    /// CSS animations' `@keyframes`, resolved for a node's `animation` list
    /// (LLP 1055 D5): [`crate::bridge::KeyframesTable::resolve`].
    pub keyframes: crate::bridge::KeyframesLink,
    /// `font-weight`, `font-stretch` and `font-variation-settings`
    /// transitions and keyframes (LLP 1019 §13 D10), which the runner
    /// plays: `None` where a browser plays them.
    pub text_axes: super::text_axes::TextAxesLink,
}

/// How the VM reaches the `format` capability's entries, when linked: the
/// entry, its arguments and the plan (whose tables hold each locale's format
/// data) to its value, `None` when they don't fit.
pub type FormatLink = Option<fn(exact_plan::Stdlib, &[Value], &Plan) -> Option<Value>>;

/// How the runner answers geometry reads, when linked (LLP 1051.000 D2).
pub type GeometryLink = Option<&'static crate::geometry::GeometryLinks>;

/// How a runner makes its Canvas 2D engine, when linked.
pub type CanvasLink = Option<fn() -> Box<dyn canvas2d::CanvasEngine>>;

/// How a host builds a plan's router: [`router::routing`], when linked.
pub type RouterLink = Option<fn(&Plan) -> Result<Option<Box<dyn router::Routing>>, RunnerError>>;

/// The runner's answer for a surface resource, when a host links surfaces:
/// [`crate::surface_record::answer`].
pub type SurfaceAnswer = Option<
    fn(&Plan, &exact_kernel::SortedMap<String, String>, &ResourcesRow) -> Result<Value, DataError>,
>;

impl RunnerLinks {
    /// Every capability.
    pub const ALL: RunnerLinks = RunnerLinks {
        surface_answer: Some(crate::surface_record::answer),
        router: Some(router::routing),
        lists: Some(&crate::instance::LISTS),
        canvas: Some(canvas2d::engine),
        format: Some(crate::format::formatting),
        geometry: Some(&crate::geometry::KERNEL),
        grants: Some(&crate::grants::LINKED),
        keyframes: Some(crate::bridge::KeyframesTable::resolve),
        text_axes: Some(exact_motion::Engine::new),
    };

    /// The core alone.
    pub const CORE: RunnerLinks = RunnerLinks {
        surface_answer: None,
        router: None,
        lists: None,
        canvas: None,
        format: None,
        geometry: None,
        grants: None,
        keyframes: None,
        text_axes: None,
    };
}

impl<D: crate::DataSource> super::Runner<D> {
    /// How this runner reads I/O grants: `None` in an archive without I/O.
    pub(crate) fn grants_link(&self) -> crate::grants::GrantsLink {
        self.links.grants
    }

    /// Whether the app's `capability` (`fs.read`, `fs.write`) lines admit
    /// `path`, read through the I/O link: false in an archive without I/O,
    /// as under grants that name no such line.
    pub fn covers(&self, capability: &str, path: &str) -> bool {
        self.covers_in(self.data.grants(), capability, path)
    }

    /// [`Self::covers`] evaluated against an explicit grant string.
    pub(in crate::runner) fn covers_in(&self, grants: &str, capability: &str, path: &str) -> bool {
        (self.links.grants).is_some_and(|link| (link.covers)(grants, capability, path))
    }
}
