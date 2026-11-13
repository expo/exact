//! Whether a row read by id carries only finite numbers.

use super::RowValue;

impl RowValue<'_> {
    /// Whether every number the row carries is finite. Rows without floats
    /// (and the CSS values, which parse to finite numbers) are.
    pub(crate) fn is_finite(&self) -> bool {
        match self {
            RowValue::LineHeight(v) => v.is_finite(),
            RowValue::BackdropFilter(v) => v.is_finite(),
            RowValue::Dimension(v) => v.is_finite(),
            RowValue::Number(v) => v.is_finite(),
            RowValue::Vec2(v) => v.x.is_finite() && v.y.is_finite(),
            RowValue::Tracks(v) => v.is_finite(),
            RowValue::Transitions(v) => v.is_finite(),
            RowValue::Animations(v) => v.is_finite(),
            RowValue::DashArray(v) => v.0.iter().all(|n| n.is_finite()),
            RowValue::Transform(v) => v.is_finite(),
            RowValue::TransformOrigin(v) => v.is_finite(),
            RowValue::AnimationRange(v) => v.is_finite(),
            RowValue::ViewTimelineInset(v) => v.is_finite(),
            RowValue::Paint(_)
            | RowValue::PaintOrder(_)
            | RowValue::Marker(_)
            | RowValue::Filter(_)
            | RowValue::ClipPath(_)
            | RowValue::ShapeOutside(_)
            | RowValue::AspectRatio(_)
            | RowValue::PositionTryFallbacks(_)
            | RowValue::DragTimeline(_)
            | RowValue::AnimationTimeline(_)
            | RowValue::TimelineScope(_)
            | RowValue::TimelineName(_)
            | RowValue::BackgroundImage(_)
            | RowValue::TextShadow(_)
            | RowValue::FontVariations(_)
            | RowValue::FontFeatures(_)
            | RowValue::BoxShadow(_)
            | RowValue::MaskImage(_)
            | RowValue::CornerShape(_)
            | RowValue::RotateAxis(_)
            | RowValue::SymbolPalette(_)
            | RowValue::ColorValue(_)
            | RowValue::Enum(_)
            | RowValue::Placement(_) => true,
        }
    }
}
