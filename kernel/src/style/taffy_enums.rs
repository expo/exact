//! The engine's forms of the flex and alignment keywords.

use crate::generated::{
    AlignContent, AlignItems, AlignSelf, Direction, FlexDirection, FlexWrap, JustifyContent,
    JustifyItems,
};

pub(super) fn flex_direction(v: FlexDirection) -> taffy::style::FlexDirection {
    match v {
        FlexDirection::Row => taffy::style::FlexDirection::Row,
        FlexDirection::Column => taffy::style::FlexDirection::Column,
        FlexDirection::RowReverse => taffy::style::FlexDirection::RowReverse,
        FlexDirection::ColumnReverse => taffy::style::FlexDirection::ColumnReverse,
    }
}

pub(super) fn flex_wrap(v: FlexWrap) -> taffy::style::FlexWrap {
    match v {
        FlexWrap::Nowrap => taffy::style::FlexWrap::NoWrap,
        FlexWrap::Wrap => taffy::style::FlexWrap::Wrap,
        FlexWrap::WrapReverse => taffy::style::FlexWrap::WrapReverse,
    }
}

// CSS `normal` (the initial value of all four rows below) lowers to `None`,
// which is Taffy's `normal`: each algorithm resolves it for its display type
// as CSS Box Alignment §5.1/§6.1 does. Flex: `justify-content` is
// `flex-start`, `align-content` and `align-items` are `stretch`. Grid: auto
// tracks stretch into the free space on both axes, and an item stretches
// unless it has a definite size or an aspect ratio on that axis (then it
// starts). Block: `align-content` is `start` without establishing an
// independent formatting context, which every other value does.

pub(super) fn justify_content(v: JustifyContent) -> Option<taffy::style::JustifyContent> {
    Some(match v {
        JustifyContent::Normal => return None,
        JustifyContent::Start => taffy::style::JustifyContent::START,
        JustifyContent::End => taffy::style::JustifyContent::END,
        JustifyContent::FlexStart => taffy::style::JustifyContent::FLEX_START,
        JustifyContent::FlexEnd => taffy::style::JustifyContent::FLEX_END,
        JustifyContent::Center => taffy::style::JustifyContent::CENTER,
        JustifyContent::SpaceBetween => taffy::style::JustifyContent::SPACE_BETWEEN,
        JustifyContent::SpaceAround => taffy::style::JustifyContent::SPACE_AROUND,
        JustifyContent::SpaceEvenly => taffy::style::JustifyContent::SPACE_EVENLY,
    })
}

pub(super) fn align_items(v: AlignItems) -> Option<taffy::style::AlignItems> {
    Some(match v {
        AlignItems::Normal => return None,
        AlignItems::Start => taffy::style::AlignItems::START,
        AlignItems::End => taffy::style::AlignItems::END,
        AlignItems::SelfStart => taffy::style::AlignItems::SELF_START,
        AlignItems::SelfEnd => taffy::style::AlignItems::SELF_END,
        AlignItems::FlexStart => taffy::style::AlignItems::FLEX_START,
        AlignItems::FlexEnd => taffy::style::AlignItems::FLEX_END,
        AlignItems::Center => taffy::style::AlignItems::CENTER,
        AlignItems::Baseline => taffy::style::AlignItems::BASELINE,
        AlignItems::Stretch => taffy::style::AlignItems::STRETCH,
    })
}

pub(super) fn justify_items(
    v: JustifyItems,
    direction: Direction,
) -> Option<taffy::style::AlignItems> {
    use taffy::style::AlignItems as T;
    Some(match v {
        JustifyItems::Normal => return None,
        JustifyItems::Stretch => T::STRETCH,
        JustifyItems::Baseline => T::BASELINE,
        JustifyItems::Center | JustifyItems::UnsafeCenter => T::CENTER,
        JustifyItems::Start | JustifyItems::UnsafeStart => T::START,
        JustifyItems::End | JustifyItems::UnsafeEnd => T::END,
        JustifyItems::SelfStart | JustifyItems::UnsafeSelfStart => T::SELF_START,
        JustifyItems::SelfEnd | JustifyItems::UnsafeSelfEnd => T::SELF_END,
        JustifyItems::FlexStart | JustifyItems::UnsafeFlexStart => T::FLEX_START,
        JustifyItems::FlexEnd | JustifyItems::UnsafeFlexEnd => T::FLEX_END,
        JustifyItems::Left | JustifyItems::UnsafeLeft => match direction {
            Direction::Ltr => T::START,
            Direction::Rtl => T::END,
        },
        JustifyItems::Right | JustifyItems::UnsafeRight => match direction {
            Direction::Ltr => T::END,
            Direction::Rtl => T::START,
        },
        JustifyItems::SafeCenter => T::SAFE_CENTER,
        JustifyItems::SafeStart => T::SAFE_START,
        JustifyItems::SafeEnd => T::SAFE_END,
        JustifyItems::SafeSelfStart => T::SAFE_SELF_START,
        JustifyItems::SafeSelfEnd => T::SAFE_SELF_END,
        JustifyItems::SafeFlexStart => T::SAFE_FLEX_START,
        JustifyItems::SafeFlexEnd => T::SAFE_FLEX_END,
        JustifyItems::SafeLeft => match direction {
            Direction::Ltr => T::SAFE_START,
            Direction::Rtl => T::SAFE_END,
        },
        JustifyItems::SafeRight => match direction {
            Direction::Ltr => T::SAFE_END,
            Direction::Rtl => T::SAFE_START,
        },
    })
}

pub(super) fn align_self(v: AlignSelf) -> Option<taffy::style::AlignSelf> {
    match v {
        AlignSelf::Auto => None,
        AlignSelf::Start => Some(taffy::style::AlignSelf::START),
        AlignSelf::End => Some(taffy::style::AlignSelf::END),
        AlignSelf::SelfStart => Some(taffy::style::AlignSelf::SELF_START),
        AlignSelf::SelfEnd => Some(taffy::style::AlignSelf::SELF_END),
        AlignSelf::FlexStart => Some(taffy::style::AlignSelf::FLEX_START),
        AlignSelf::FlexEnd => Some(taffy::style::AlignSelf::FLEX_END),
        AlignSelf::Center => Some(taffy::style::AlignSelf::CENTER),
        AlignSelf::Baseline => Some(taffy::style::AlignSelf::BASELINE),
        AlignSelf::Stretch => Some(taffy::style::AlignSelf::STRETCH),
    }
}

pub(super) fn align_content(v: AlignContent) -> Option<taffy::style::AlignContent> {
    Some(match v {
        AlignContent::Normal => return None,
        AlignContent::Start => taffy::style::AlignContent::START,
        AlignContent::End => taffy::style::AlignContent::END,
        AlignContent::FlexStart => taffy::style::AlignContent::FLEX_START,
        AlignContent::FlexEnd => taffy::style::AlignContent::FLEX_END,
        AlignContent::Center => taffy::style::AlignContent::CENTER,
        AlignContent::Stretch => taffy::style::AlignContent::STRETCH,
        AlignContent::SpaceBetween => taffy::style::AlignContent::SPACE_BETWEEN,
        AlignContent::SpaceAround => taffy::style::AlignContent::SPACE_AROUND,
        AlignContent::SpaceEvenly => taffy::style::AlignContent::SPACE_EVENLY,
    })
}
