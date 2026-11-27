//! What a frame is painted from ([`Scene`]).
use super::*;

/// What a frame is painted from.
pub struct Scene<'a> {
    /// The kernel: frames, styles, props.
    pub kernel: &'a Kernel,
    /// @ref LLP 1038 D6 — hidden route subtrees paint no pixels or hit boxes.
    pub hidden: &'a dyn Fn(ViewId) -> bool,
    /// The roots, in order.
    pub roots: &'a [ViewId],
    /// A node's presentation values.
    pub presented: &'a dyn Fn(ViewId) -> Presented,
    /// Nodes whose played or held scale differs from identity.
    pub scaled_presented: &'a std::collections::BTreeSet<ViewId>,
    /// A path's `d` while a transition moves it (LLP 1055.000 D15).
    pub paths: &'a dyn Fn(ViewId) -> Option<exact_motion::PathValue>,
    /// A `filter` while a transition moves it (LLP 1003.002 D8).
    pub filters: &'a dyn Fn(ViewId) -> Option<exact_motion::FilterValue>,
    /// A box's `clip-path` while a transition moves it, for its pixels and hits (LLP 1003.003 D7).
    pub clips: &'a dyn Fn(ViewId) -> Option<exact_kernel::clip::ClipPath>,
    /// Scroll offsets of scroll containers (host state, LLP 1010).
    pub scroll: &'a BTreeMap<ViewId, (f32, f32)>,
    /// The page's scroll offset: the window is a viewport over a document.
    pub page: (f32, f32),
    /// Decoded images by node.
    pub images: &'a BTreeMap<ViewId, Arc<Bitmap>>,
    /// The focused node, if any (its ring, or a field's caret, is painted).
    pub focus: Option<ViewId>,
    /// The focused text field's selection (x2apps codeedit #2).
    pub selection: Option<exact_runner::FieldSelection>,
    /// At a wrap, a click or visual-line End can choose the preceding line.
    pub caret_upstream: bool,
    /// Unbound checkboxes' own states, which the host keeps (LLP 1069.001 D4).
    pub controls: &'a BTreeMap<ViewId, bool>,
    /// Values chosen in a date, range or select, or typed into a field, since
    /// its bound value last changed: (choice, bound).
    pub chosen: &'a BTreeMap<ViewId, (String, String)>,
    /// A select's open menu, painted over everything (LLP 1069.001 D7).
    pub menu: Option<control::MenuPaint>,
    /// The pointer, in viewport points, when the host draws one.
    pub pointer: Option<(f32, f32)>,
}
