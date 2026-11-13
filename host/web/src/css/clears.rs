//! Explicit CSS clears use the host's mapping for non-CSS rows.
use super::{property, StyleId};

/// False lets the existing special lowering serialize the row's initial
/// value, or report an unsupported row. Those rows are not CSS properties.
pub(super) fn push(out: &mut String, id: StyleId, press: bool) -> bool {
    use StyleId::*;
    match id {
        TintColor | ExitAnimation | DragTimeline | AnimationTimeline | AnimationRange
        | TimelineScope | LayoutTransition | PressScale | PressHaptic | ControlSize
        | ControlCornerStyle | Fit | LineClamp | RotateAxis | TranslateZ | TranslatePercent
        | SymbolRendering | TintAdjustment | SymbolPalette | SymbolValue | SymbolEffect
        | ContentTransition | ScrollEdgeEffect | HoverEffect | SmartInvert | AnimationTrigger => {
            return false
        }
        // The name and the drag code's copy of it (css.rs).
        ScrollTimelineName => {
            out.push_str("scroll-timeline-name:unset;--exact-scroll-timeline:unset;")
        }
        ViewTimelineName => out.push_str("view-timeline-name:unset;--exact-view-timeline:unset;"),
        // `scale` remains the feedback product; an unset own factor is one.
        Scale if press => out.push_str("--exact-scale:1;"),
        AccentColor => out.push_str("accent-color:unset;--exact-accent:unset;"),
        // A clear is unsaid, as `auto` is (selection.rs): no declaration, so the
        // page's sheet decides.
        UserSelect => {}
        // The absent part uses the shell's platform placeholder fallback.
        PlaceholderColor => out.push_str("--exact-placeholder-color:initial;"),
        // No pressed paint is no declaration: the glue and the shell's
        // dim read one's presence (LLP 1061 D7).
        PressOpacity | PressOpacityTo | PressBackgroundColor | PressColor => {}
        // Both spellings, as a value writes them (css.rs, LLP 1019 D12).
        FontSmoothing => {
            out.push_str("-webkit-font-smoothing:unset;-moz-osx-font-smoothing:unset;")
        }
        _ => {
            property(out, id);
            out.push_str(":unset;");
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use exact_kernel::{StyleId, StyleProps};

    /// A cleared row unsets every property its value writes, so neither
    /// spelling of an authored class's font smoothing paints.
    #[test]
    fn a_cleared_font_smoothing_unsets_both_spellings() {
        let mut style = StyleProps::default();
        style.written.set(StyleId::FontSmoothing);
        let css = super::super::css_text(&style, &[]).0;
        assert!(
            css.contains("-webkit-font-smoothing:unset;-moz-osx-font-smoothing:unset;"),
            "{css}"
        );
    }
}
