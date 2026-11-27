//! @ref LLP 1123 D4, D8, D9, D11 to D14, D16 — the rows the web writes no CSS for yet, as on the web
//! host: a page's ignored regions, insets, margins, reach, scroll insets, tracking and aspect fit
//! wait for the web's owners (D17).

use exact_kernel::StyleId;

/// Whether `id` is one of LLP 1123's rows the web leaves to its owners.
pub(super) fn unwritten(id: StyleId) -> bool {
    matches!(
        id,
        StyleId::ScrollContentInsetTop
            | StyleId::ScrollContentInsetRight
            | StyleId::ScrollContentInsetBottom
            | StyleId::ScrollContentInsetLeft
            | StyleId::ScrollVerticalIndicatorInsetsTop
            | StyleId::ScrollVerticalIndicatorInsetsRight
            | StyleId::ScrollVerticalIndicatorInsetsBottom
            | StyleId::ScrollVerticalIndicatorInsetsLeft
            | StyleId::ScrollHorizontalIndicatorInsetsTop
            | StyleId::ScrollHorizontalIndicatorInsetsRight
            | StyleId::ScrollHorizontalIndicatorInsetsBottom
            | StyleId::ScrollHorizontalIndicatorInsetsLeft
            | StyleId::SafeAreaExtendBoundsTop
            | StyleId::SafeAreaExtendBoundsRight
            | StyleId::SafeAreaExtendBoundsBottom
            | StyleId::SafeAreaExtendBoundsLeft
            | StyleId::ScrollInsetAdjustment
            | StyleId::ScrollIndicatorAdjustment
            | StyleId::SafeAreaTrack
            | StyleId::SafeAreaAspectFit
            | StyleId::ScrollEdgeEffectSoft
            | StyleId::ScrollEdgeEffectHard
            | StyleId::ScrollEdgeEffectHidden
            | StyleId::SafeAreaReach
            | StyleId::SafeAreaReachEdges
            | StyleId::SafeAreaExtend
            | StyleId::SafeAreaIgnore
            | StyleId::SafeAreaIgnoreAlign
            | StyleId::SafeAreaBar
            | StyleId::LayoutMarginTop
            | StyleId::LayoutMarginRight
            | StyleId::LayoutMarginBottom
            | StyleId::LayoutMarginLeft
            | StyleId::LayoutMarginLeading
            | StyleId::LayoutMarginTrailing
            | StyleId::LayoutMarginsPreserved
            | StyleId::LayoutMarginsFromSafeArea
            | StyleId::LayoutMarginsMinimum
            | StyleId::AdditionalSafeAreaTop
            | StyleId::AdditionalSafeAreaRight
            | StyleId::AdditionalSafeAreaBottom
            | StyleId::AdditionalSafeAreaLeft
            | StyleId::SafeAreaPaddingTop
            | StyleId::SafeAreaPaddingRight
            | StyleId::SafeAreaPaddingBottom
            | StyleId::SafeAreaPaddingLeft
    )
}
