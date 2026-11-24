use super::{easing_css, Property, StyleId, StyleProps, TimingFunction};
use exact_kernel::StyleMask;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Press rows and the CSS properties that time them (LLP 1061 D7).
pub const PRESS_ROWS: [(StyleId, Property); 6] = [
    (StyleId::PressBackgroundColor, Property::BackgroundColor),
    (StyleId::PressColor, Property::Color),
    (StyleId::PressOpacity, Property::Opacity),
    (StyleId::PressOpacityTo, Property::Opacity),
    (StyleId::PressTranslate, Property::Translate),
    (StyleId::PressScale, Property::Scale),
];

pub(super) fn write_timing(out: &mut String, style: &StyleProps, bound: StyleMask) {
    if !style.mask.union(style.written).has(StyleId::Transition) {
        return;
    }
    let rows = style.mask.union(bound);
    let mut properties = BTreeMap::new();
    let mut has_press = false;
    for (id, property) in PRESS_ROWS {
        if !rows.has(id)
            || (id == StyleId::PressScale && !bound.has(id) && !super::press_composes(style))
        {
            continue;
        }
        has_press = true;
        let Some(item) = style.transition.matching(property) else {
            continue;
        };
        let item = item.governing(property);
        if !item.starts() {
            continue;
        }
        let (duration, easing) = match item.timing {
            TimingFunction::Easing(easing) => (item.duration, easing),
            TimingFunction::Spring(config) => config.easing(),
        };
        properties.insert(
            property.css_name(),
            (duration * 1000.0, item.delay * 1000.0, easing_css(&easing)),
        );
    }
    // An empty map is a written transition that starts no press track.
    if has_press {
        let _ = write!(
            out,
            "--exact-press-timing:{};",
            serde_json::json!(properties)
        );
    }
}
