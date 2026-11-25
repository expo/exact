//! The coordinates of a still, in display-local CSS pixels and output pixels.
use super::{Failure, Json};

/// The selected source disappeared or the session never held it.
pub(super) fn missing() -> Failure {
    (
        "NotFoundError",
        "capture source is no longer available".into(),
    )
}

/// The caller combined incompatible selectors or supplied an invalid rectangle.
pub(super) fn invalid() -> Failure {
    (
        "TypeError",
        "choose a window or a display with an optional in-bounds rect".into(),
    )
}

/// Check combinations before enumerating or allocating a capture source.
pub(super) fn validate(args: &Json) -> Result<(), Failure> {
    if (!args["rect"].is_null() && args["screen"].is_null())
        || (!args["screen"].is_null() && !args["window"].is_null())
        || (!args["cursor"].is_null()
            && !matches!(args["cursor"].as_str(), Some("always" | "never")))
    {
        return Err(invalid());
    }
    Ok(())
}

/// A display description can survive a change to the taskbar or session viewport.
pub(super) fn matches(screen: &Json, selected: &Json) -> bool {
    ["left", "top", "width", "height", "devicePixelRatio"]
        .iter()
        .all(|key| screen[key].as_f64() == selected[key].as_f64())
        && screen["label"] == selected["label"]
}

/// Read a DOMRect in CSS pixels.
pub(super) fn css_rect(rect: &Json) -> Result<[f64; 4], Failure> {
    let number = |key: &str| rect[key].as_f64().ok_or_else(invalid);
    Ok([
        number("x")?,
        number("y")?,
        number("width")?,
        number("height")?,
    ])
}

/// The first pixel covered by each edge. The display list rounds CSS extents
/// up on Windows, so its far edge is clamped to the actual last pixel.
pub(super) fn crop(
    screen: &Json,
    rect: &Json,
    width: u32,
    height: u32,
) -> Result<[u32; 4], Failure> {
    if rect.is_null() {
        return Ok([0, 0, width, height]);
    }
    let [x, y, w, h] = css_rect(rect)?;
    let sw = screen["width"].as_f64().ok_or_else(invalid)?;
    let sh = screen["height"].as_f64().ok_or_else(invalid)?;
    if !(x >= 0.
        && y >= 0.
        && w > 0.
        && h > 0.
        && x + w <= sw
        && y + h <= sh
        && width > 0
        && height > 0)
    {
        return Err(invalid());
    }
    let scale = screen["devicePixelRatio"].as_f64().ok_or_else(invalid)?;
    let left = ((x * scale).round() as u32).min(width - 1);
    let top = ((y * scale).round() as u32).min(height - 1);
    Ok([
        left,
        top,
        (((x + w) * scale).round() as u32).clamp(left + 1, width) - left,
        (((y + h) * scale).round() as u32).clamp(top + 1, height) - top,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::json;

    #[test]
    fn adjacent_css_regions_share_one_physical_edge() {
        let screen = json!({"width":20, "height":10, "devicePixelRatio":1.5});
        let region = |x| json!({"x":x, "y":0, "width":1, "height":1});
        let a = crop(&screen, &region(1), 30, 15).unwrap();
        let b = crop(&screen, &region(2), 30, 15).unwrap();
        assert_eq!(a[0] + a[2], b[0]);
        assert_eq!((a[2], b[2]), (1, 2));
    }
}
