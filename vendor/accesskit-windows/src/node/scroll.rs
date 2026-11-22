// Copyright 2026 The AccessKit Authors and Exact contributors. Apache-2.0 OR MIT.
//! UI Automation ScrollPattern for scrollable containers.
use super::*;
use accesskit::ScrollUnit;

fn axis_has_scroll_range(min: Option<f64>, max: Option<f64>) -> bool {
    matches!(
        (min, max),
        (Some(min), Some(max)) if min.is_finite() && max.is_finite() && max > min
    )
}

/// `UIA_ScrollPatternNoScroll` means "no position on this axis".
fn scroll_percent(current: Option<f64>, min: Option<f64>, max: Option<f64>) -> f64 {
    let (Some(current), Some(min), Some(max)) = (current, min, max) else {
        return UIA_ScrollPatternNoScroll;
    };
    if !axis_has_scroll_range(Some(min), Some(max)) || !current.is_finite() {
        return UIA_ScrollPatternNoScroll;
    }
    let current = current.clamp(min, max);
    let span = max - min;
    let fraction = if span.is_finite() {
        (current - min) / span
    } else {
        (current * 0.5 - min * 0.5) / (max * 0.5 - min * 0.5)
    };
    fraction * 100.0
}

/// View size is the viewport as a percentage of the content.
///
/// AccessKit stores the viewport as the node's own bounds and the hidden
/// remainder as `scroll_*_max - scroll_*_min`. There is no separate viewport
/// field. When that pair is missing, report a fully visible view instead of
/// inventing a content size.
fn scroll_view_size(
    viewport: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    scrollable: bool,
) -> f64 {
    if !scrollable {
        return 100.0;
    }
    let (Some(viewport), Some(min), Some(max)) = (viewport, min, max) else {
        return 100.0;
    };
    if !(viewport.is_finite() && viewport >= 0.0 && axis_has_scroll_range(Some(min), Some(max))) {
        return 100.0;
    }
    let span = max - min;
    let hidden_to_visible = if span.is_finite() {
        span / viewport
    } else {
        (max * 0.5 - min * 0.5) / (viewport * 0.5)
    };
    100.0 / (1.0 + hidden_to_visible)
}

fn offset_for_scroll_percent(
    current: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    percent: f64,
) -> Result<f64> {
    if percent == UIA_ScrollPatternNoScroll {
        return Ok(current.unwrap_or(0.0));
    }
    if !(percent.is_finite() && (0.0..=100.0).contains(&percent)) {
        return Err(invalid_arg());
    }
    if !axis_has_scroll_range(min, max) {
        return Err(not_supported());
    }
    let min = min.unwrap();
    let max = max.unwrap();
    let t = percent / 100.0;
    Ok(min * (1.0 - t) + max * t)
}

/// Map a UIA `ScrollAmount` onto AccessKit's existing directional scroll actions.
/// Increment moves toward larger scroll offsets. `NoAmount` changes nothing.
fn uia_scroll_action(
    amount: ScrollAmount,
    horizontal: bool,
) -> Result<Option<(Action, ScrollUnit)>> {
    let mapped = match (amount, horizontal) {
        (ScrollAmount_NoAmount, _) => None,
        (ScrollAmount_SmallDecrement, true) => Some((Action::ScrollLeft, ScrollUnit::Item)),
        (ScrollAmount_LargeDecrement, true) => Some((Action::ScrollLeft, ScrollUnit::Page)),
        (ScrollAmount_SmallIncrement, true) => Some((Action::ScrollRight, ScrollUnit::Item)),
        (ScrollAmount_LargeIncrement, true) => Some((Action::ScrollRight, ScrollUnit::Page)),
        (ScrollAmount_SmallDecrement, false) => Some((Action::ScrollUp, ScrollUnit::Item)),
        (ScrollAmount_LargeDecrement, false) => Some((Action::ScrollUp, ScrollUnit::Page)),
        (ScrollAmount_SmallIncrement, false) => Some((Action::ScrollDown, ScrollUnit::Item)),
        (ScrollAmount_LargeIncrement, false) => Some((Action::ScrollDown, ScrollUnit::Page)),
        _ => return Err(invalid_arg()),
    };
    Ok(mapped)
}

fn directed_scroll(
    node: &NodeRef<'_>,
    amount: ScrollAmount,
    horizontal: bool,
) -> Result<Option<(Action, ScrollUnit)>> {
    let Some((action, unit)) = uia_scroll_action(amount, horizontal)? else {
        return Ok(None);
    };
    if !node.supports_action(action, &filter) {
        return Err(not_supported());
    }
    Ok(Some((action, unit)))
}

impl NodeWrapper<'_> {
    pub(super) fn horizontally_scrollable(&self) -> bool {
        axis_has_scroll_range(self.node.scroll_x_min(), self.node.scroll_x_max())
            || self.node.supports_action(Action::ScrollLeft, &filter)
            || self.node.supports_action(Action::ScrollRight, &filter)
    }

    pub(super) fn vertically_scrollable(&self) -> bool {
        axis_has_scroll_range(self.node.scroll_y_min(), self.node.scroll_y_max())
            || self.node.supports_action(Action::ScrollUp, &filter)
            || self.node.supports_action(Action::ScrollDown, &filter)
    }

    pub(super) fn is_scroll_pattern_supported(&self) -> bool {
        let node = self.node;
        node.role() != Role::ScrollBar
            && (node.supports_action(Action::SetScrollOffset, &filter)
                || [
                    Action::ScrollLeft,
                    Action::ScrollRight,
                    Action::ScrollUp,
                    Action::ScrollDown,
                ]
                .iter()
                .any(|a| node.supports_action(*a, &filter)))
            && (self.horizontally_scrollable() || self.vertically_scrollable())
    }

    pub(super) fn horizontal_scroll_percent(&self) -> f64 {
        scroll_percent(
            self.node.scroll_x(),
            self.node.scroll_x_min(),
            self.node.scroll_x_max(),
        )
    }

    pub(super) fn vertical_scroll_percent(&self) -> f64 {
        scroll_percent(
            self.node.scroll_y(),
            self.node.scroll_y_min(),
            self.node.scroll_y_max(),
        )
    }

    pub(super) fn horizontal_view_size(&self) -> f64 {
        let viewport = self.node.raw_bounds().map(|bounds| bounds.width());
        scroll_view_size(
            viewport,
            self.node.scroll_x_min(),
            self.node.scroll_x_max(),
            self.horizontally_scrollable(),
        )
    }

    pub(super) fn vertical_view_size(&self) -> f64 {
        let viewport = self.node.raw_bounds().map(|bounds| bounds.height());
        scroll_view_size(
            viewport,
            self.node.scroll_y_min(),
            self.node.scroll_y_max(),
            self.vertically_scrollable(),
        )
    }
}

impl PlatformNode {
    pub(super) fn scroll(
        &self,
        horizontal_amount: ScrollAmount,
        vertical_amount: ScrollAmount,
    ) -> Result<()> {
        let context = self.upgrade_context()?;
        if context.is_placeholder.load(Ordering::SeqCst) {
            return Err(element_not_enabled());
        }
        let tree = context.read_tree();
        let (node, target_node, target_tree) = self.node_with_location(&tree)?;
        if node.is_disabled() {
            return Err(element_not_enabled());
        }
        let requests = [
            directed_scroll(&node, horizontal_amount, true)?,
            directed_scroll(&node, vertical_amount, false)?,
        ];
        drop(tree);
        for (action, unit) in requests.into_iter().flatten() {
            context.do_action(ActionRequest {
                action,
                target_tree,
                target_node,
                data: Some(ActionData::ScrollUnit(unit)),
            });
        }
        Ok(())
    }

    pub(super) fn set_scroll_percent(
        &self,
        horizontal_percent: f64,
        vertical_percent: f64,
    ) -> Result<()> {
        self.do_complex_action(|node, target_node, target_tree| {
            if node.is_disabled() {
                return Err(element_not_enabled());
            }
            if horizontal_percent == UIA_ScrollPatternNoScroll
                && vertical_percent == UIA_ScrollPatternNoScroll
            {
                return Ok(None);
            }
            if !node.supports_action(Action::SetScrollOffset, &filter) {
                return Err(not_supported());
            }
            let x = offset_for_scroll_percent(
                node.scroll_x(),
                node.scroll_x_min(),
                node.scroll_x_max(),
                horizontal_percent,
            )?;
            let y = offset_for_scroll_percent(
                node.scroll_y(),
                node.scroll_y_min(),
                node.scroll_y_max(),
                vertical_percent,
            )?;
            Ok(Some(ActionRequest {
                action: Action::SetScrollOffset,
                target_tree,
                target_node,
                data: Some(ActionData::SetScrollOffset(Point::new(x, y))),
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentages_remain_finite_across_the_full_coordinate_range() {
        let (min, max) = (Some(-f64::MAX), Some(f64::MAX));
        assert_eq!(scroll_percent(Some(0.), min, max), 50.);
        assert_eq!(offset_for_scroll_percent(None, min, max, 50.).unwrap(), 0.);
        assert_eq!(
            offset_for_scroll_percent(None, min, max, 100.).unwrap(),
            f64::MAX
        );
        assert!((scroll_view_size(Some(f64::MAX), min, max, true) - 100. / 3.).abs() < 1e-12);
    }
}
