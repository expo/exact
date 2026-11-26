//! How a runner answers geometry reads: the host's links (LLP 1051.000 D2, LLP 1123 D6).

use super::{
    kernel_frame, kernel_measure, kernel_point, kernel_points, kernel_range_frame, kernel_shown,
    measure_ahead, read, GeometryAnswer, GeometryEnv, Measured, Scrolled,
};
use exact_kernel::{Kernel, ViewId};
use exact_plan::{Plan, Stdlib, Value};

/// The space a page places a box in for `exactGeometry`: the document's, outside every owner;
/// the viewport's; or a view's border box's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSpace {
    /// The document's.
    Owner,
    /// The viewport's.
    Window,
    /// The border box of this view, or of none where its node is gone.
    Node(Option<ViewId>),
}

/// A box as the page draws it (LLP 1123 D6): its border box's width and height, and its frame in
/// the space asked, `None` where the page cannot place it there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageBox {
    /// The border box's width and height.
    pub size: [f64; 2],
    /// Its left, top, width and height in the space asked.
    pub frame: Option<[f64; 4]>,
}

/// A page's read of a view's box in a space: `None` where the view has no box.
pub type PageBoxRead = fn(ViewId, PageSpace) -> Option<PageBox>;

/// How a runner answers geometry: `frame` from a borrowed kernel while an
/// action runs, `measure` from the kernel just before it (natively it lays
/// out the engine tree). The web's pair asks the page instead and never
/// reads the kernel, whose layout there is not the page's.
///
/// A host hands its reads to [`GeometryLinks::new`], and what every host
/// shares (naming a view by its id, finding an action's `measure`s, building
/// the record) rides along in the same value, so an artifact that links no
/// geometry carries none of it (LLP 1047 D3).
#[derive(Clone, Copy)]
pub struct GeometryLinks {
    pub(super) scroll_metrics: fn(&Kernel, &Scrolled, ViewId) -> Option<[f64; 6]>,
    pub(super) range_frame: fn(&Kernel, &Scrolled, ViewId, u32, u32) -> GeometryAnswer,
    pub(super) frame: fn(&Kernel, &Scrolled, ViewId) -> GeometryAnswer,
    pub(super) measure: fn(&mut Kernel, &Scrolled, ViewId) -> GeometryAnswer,
    pub(super) point: fn(&Kernel, &Scrolled, f64, f64) -> Option<ViewId>,
    pub(super) points: fn(&Kernel, &Scrolled, f64, f64) -> Vec<ViewId>,
    pub(super) shown: fn(&Kernel, &Scrolled, ViewId, bool) -> bool,
    ahead: fn(&Plan, &[u8], &mut Kernel, &Scrolled, &GeometryLinks) -> Measured,
    pub(super) read: fn(&GeometryEnv<'_>, &Plan, Stdlib, &[Value]) -> Option<Value>,
    /// The page's box for `exactGeometry` where the page lays out and the kernel does not (the
    /// web's wasm target): `None` answers the facts from the kernel's layout.
    pub(crate) page_box: Option<PageBoxRead>,
}

impl GeometryLinks {
    /// A host's reads: `frame`, where layout put a view as last laid out,
    /// `measure`, the view's box as if its `height` were `auto`, and
    /// `point`, the front-most view whose box holds a viewport point, and
    /// `points`, every such view in paint order. `shown` receives `true` on
    /// the first candidate of each structural read, so a host can snapshot
    /// its presentation once for the read.
    pub const fn new(
        frame: fn(&Kernel, &Scrolled, ViewId) -> GeometryAnswer,
        measure: fn(&mut Kernel, &Scrolled, ViewId) -> GeometryAnswer,
        point: fn(&Kernel, &Scrolled, f64, f64) -> Option<ViewId>,
        points: fn(&Kernel, &Scrolled, f64, f64) -> Vec<ViewId>,
        shown: fn(&Kernel, &Scrolled, ViewId, bool) -> bool,
    ) -> GeometryLinks {
        GeometryLinks {
            scroll_metrics: |kernel, scrolled, view| {
                let key = kernel.node(view)?.key;
                kernel.laid_out_frame(key)?;
                scrolled
                    .inline
                    .as_ref()?
                    .scroll_metrics(kernel, view, scrolled.offset(key))
            },
            range_frame: kernel_range_frame,
            frame,
            measure,
            point,
            points,
            shown,
            ahead: measure_ahead,
            read,
            page_box: None,
        }
    }

    /// A host's `rangeFrame` reader in place of [`InlineGeometry::range_frame`]
    /// (the web's), given offsets already checked as `rangeFrame` checks them.
    pub const fn with_range_frame(
        mut self,
        range_frame: fn(&Kernel, &Scrolled, ViewId, u32, u32) -> GeometryAnswer,
    ) -> Self {
        self.range_frame = range_frame;
        self
    }

    /// A page that lays out answers `exactGeometry` from its own boxes (LLP 1123 D6): `read`
    /// gives a view's drawn border box, sized and placed in the space asked.
    pub const fn with_page_box(mut self, read: PageBoxRead) -> Self {
        self.page_box = Some(read);
        self
    }

    /// A page that owns layout can read the textarea directly.
    pub const fn with_scroll_metrics(
        mut self,
        read: fn(&Kernel, &Scrolled, ViewId) -> Option<[f64; 6]>,
    ) -> Self {
        self.scroll_metrics = read;
        self
    }

    /// Every `measure` in an action's `code`, answered before it runs, by
    /// its literal id.
    pub fn ahead(
        &self,
        plan: &Plan,
        code: &[u8],
        kernel: &mut Kernel,
        scrolled: &Scrolled,
    ) -> Measured {
        (self.ahead)(plan, code, kernel, scrolled, self)
    }
}

/// The kernel's answers: a native host's kernel is its layout, and its
/// presenters' scroll offsets are noted in [`Scrolled`].
pub static KERNEL: GeometryLinks = GeometryLinks::new(
    kernel_frame,
    kernel_measure,
    kernel_point,
    kernel_points,
    kernel_shown,
);
