//! Calculation handles live as long as their lowered styles, including retained clones.
use super::intern::{Table, Value};
use super::{Comparison, Env};
use std::cell::RefCell;
use std::sync::{Arc, LazyLock, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Pair {
        percent: u32,
        points: u32,
        floor: bool,
        added: u32,
    },
    Tree {
        tree: usize,
        floor: bool,
        added: u32,
    },
}

type Calculation = Value<Key, Option<Comparison>>;
static HANDLES: LazyLock<Mutex<Table<Key, Option<Comparison>>>> =
    LazyLock::new(|| Mutex::new(Table::new()));

thread_local! {
    static LOWERING: RefCell<Vec<Vec<Arc<Calculation>>>> = const { RefCell::new(Vec::new()) };
}

/// Collect the owners while constructing a style, including grid tracks.
/// Nested scopes keep independent owners; unwinding releases unfinished work.
pub(super) struct Lowering;
impl Lowering {
    pub(super) fn new() -> Self {
        LOWERING.with(|stack| stack.borrow_mut().push(Vec::new()));
        Self
    }
    pub(super) fn finish(self, style: &mut taffy::Style) {
        LOWERING.with(|stack| {
            for owner in stack.borrow_mut().last_mut().unwrap().drain(..) {
                style.rare.keep_calc(owner);
            }
        });
    }
}
impl Drop for Lowering {
    fn drop(&mut self) {
        LOWERING.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn intern(key: Key, tree: Option<Comparison>) -> *const () {
    let value = Table::intern(&HANDLES, key, tree);
    let handle = (value.id << 3) as *const ();
    LOWERING.with(|stack| {
        stack
            .borrow_mut()
            .last_mut()
            .expect("calculation lowering scope")
            .push(value);
    });
    handle
}

pub(super) fn calc_handle(percent: f32, points: f32) -> *const () {
    calc_handle_in(percent, points, false, 0.0)
}
pub(crate) fn calc_handle_in(percent: f32, points: f32, floor: bool, added: f32) -> *const () {
    intern(
        Key::Pair {
            percent: percent.to_bits(),
            points: points.to_bits(),
            floor,
            added: added.to_bits(),
        },
        None,
    )
}
pub(crate) fn tree_handle(tree: Comparison, floor: bool, added: f32) -> *const () {
    intern(
        Key::Tree {
            tree: tree.identity(),
            floor,
            added: added.to_bits(),
        },
        Some(tree),
    )
}

std::thread_local! {
    /// The environment of the layout running on this thread.
    static LAYOUT_ENV: std::cell::RefCell<Env> = std::cell::RefCell::new(Env::default());
}

/// The environment for resolving tree handles during layout or host-block
/// lowering. Dropping the guard restores the previous environment.
pub(crate) struct LayoutEnv(Option<Env>);

impl LayoutEnv {
    pub(crate) fn set(env: &Env) -> LayoutEnv {
        LayoutEnv(Some(LAYOUT_ENV.with(|e| e.replace(env.clone()))))
    }
}

impl Drop for LayoutEnv {
    fn drop(&mut self) {
        if let Some(before) = self.0.take() {
            LAYOUT_ENV.with(|e| *e.borrow_mut() = before);
        }
    }
}

/// Resolve a live handle; a released or unknown handle fails closed.
pub(crate) fn resolve_calc(handle: *const (), basis: f32) -> f32 {
    let Some(calc) = HANDLES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(handle as usize >> 3)
    else {
        return 0.0;
    };
    let basis = f64::from(basis);
    let (value, floor, added) = match calc.key {
        Key::Tree { floor, added, .. } => {
            let value =
                LAYOUT_ENV.with(|e| calc.value.as_ref().unwrap().wide(&e.borrow(), Some(basis)));
            (value.unwrap_or(0.0), floor, added)
        }
        Key::Pair {
            percent,
            points,
            floor,
            added,
        } => (
            basis * f64::from(f32::from_bits(percent)) / 100.0 + f64::from(f32::from_bits(points)),
            floor,
            added,
        ),
    };
    let value = if value.is_nan() { 0.0 } else { value };
    super::calc::narrow(
        (if floor { value.max(0.0) } else { value }) + f64::from(f32::from_bits(added)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dimension;

    fn left_handle(style: &taffy::Style) -> *const () {
        let taffy::style::ExpandedLengthPercentageAuto::Calc(handle) = style.inset.left.expand()
        else {
            panic!("expected calculation");
        };
        handle
    }

    #[test]
    fn released_styles_release_calculation_handles() {
        let mut style = crate::StyleProps::default();
        let env = Env::default();
        style.left = Dimension::parse_calc("min(37px, 50%)").unwrap();
        let detached = style.left.clone();
        let survivor = style.to_taffy(crate::NodeType::View, &env);
        let retained = survivor.clone();
        let live = left_handle(&survivor);
        let mut released = Vec::new();
        for i in 0..16_000 {
            for text in [format!("calc({i}% - 6px)"), format!("min({i}px, 51%)")] {
                style
                    .set_dynamic(crate::StyleId::Left, &crate::StyleValue::Text(text.into()))
                    .unwrap();
                let lowered = style.to_taffy(crate::NodeType::View, &env);
                released.push(left_handle(&lowered) as usize);
            }
        }
        drop(style);
        drop(survivor);
        for handle in released {
            assert!(HANDLES.lock().unwrap().get(handle >> 3).is_none());
            assert_eq!(resolve_calc(handle as *const (), 100.), 0.);
        }
        assert_eq!(detached.at_basis(&env, 100.), 37.);
        drop(detached);
        assert_eq!(resolve_calc(live, 100.), 37.);
        assert_eq!(resolve_calc(live, 40.), 20.);
        drop(retained);
        assert!(HANDLES.lock().unwrap().get(live as usize >> 3).is_none());
        assert_eq!(resolve_calc(live, 100.), 0.);
        style = crate::StyleProps::default();
        style.left = Dimension::Calc(3., 7.);
        let next = style.to_taffy(crate::NodeType::View, &env);
        assert_ne!(left_handle(&next), live);
        assert_eq!(
            resolve_calc(live, 100.),
            0.,
            "released IDs never alias new values"
        );
    }

    #[test]
    fn retained_grid_styles_keep_calculations_after_the_authored_tracks_drop() {
        let mut style = crate::StyleProps::default();
        style.rare.grid_template_columns =
            crate::style::GridTracks::parse("min(31px, 50%)").unwrap();
        let lowered = style.to_taffy(crate::NodeType::View, &Env::default());
        let taffy::style::GridTemplateComponent::Single(track) =
            lowered.rare.grid_template_columns[0]
        else {
            panic!("single track")
        };
        let taffy::style::ExpandedMinTrackSizingFunction::Calc(handle) = track.min.expand() else {
            panic!("calculation track")
        };
        drop(style);
        let copy = lowered.clone();
        drop(lowered);
        assert_eq!(resolve_calc(handle, 100.), 31.);
        assert_eq!(resolve_calc(handle, 40.), 20.);
        drop(copy);
        assert!(HANDLES.lock().unwrap().get(handle as usize >> 3).is_none());
    }

    #[test]
    fn handles_preserve_bits_floor_and_added_length() {
        let _lowering = Lowering::new();
        let mut pairs = std::collections::HashSet::new();
        for (percent, points, floor, added) in [
            (0.0, 0.0, false, 0.0),
            (-0.0, 0.0, false, 0.0),
            (0.0, -0.0, false, 0.0),
            (0.0, 0.0, true, 0.0),
            (0.0, 0.0, false, -0.0),
            (f32::from_bits(0x7fc0_0001), 0.0, false, 0.0),
            (f32::from_bits(0x7fc0_0002), 0.0, false, 0.0),
        ] {
            let handle = calc_handle_in(percent, points, floor, added);
            assert_eq!(handle, calc_handle_in(percent, points, floor, added));
            assert!(pairs.insert(handle));
        }
        let Dimension::Compare(tree) = Dimension::parse_calc("min(50%, -60px)").unwrap() else {
            panic!("a comparison tree");
        };
        let mut trees = std::collections::HashSet::new();
        for (floor, added) in [(false, 0.0), (true, 0.0), (false, -0.0), (true, 7.0)] {
            let handle = tree_handle(tree.clone(), floor, added);
            assert_eq!(handle, tree_handle(tree.clone(), floor, added));
            assert!(trees.insert(handle));
            assert!(!pairs.contains(&handle));
        }
        for (floor, added, pair_value, tree_value) in
            [(false, 7.0, -3.0, -53.0), (true, 7.0, 7.0, 7.0)]
        {
            assert_eq!(
                resolve_calc(calc_handle_in(50.0, -60.0, floor, added), 100.0),
                pair_value,
            );
            assert_eq!(
                resolve_calc(tree_handle(tree.clone(), floor, added), 100.0),
                tree_value
            );
        }
    }
}
