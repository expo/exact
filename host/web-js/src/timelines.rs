//! Timelines on the JS target (LLP 1057.003 D5): whether a plan's timeline
//! names can be drag timelines, which the motion piece resolves, or are all
//! the browser's own.

use exact_kernel::StyleId;
use exact_plan::BindingKind;

/// In a plan with no drag timeline, every timeline is the browser's: a name
/// as well as `scroll()` and `view()` is CSS's own row, paused only where the
/// browser has no `animation-timeline` (css.rs, index.html), and needs no
/// motion piece to hand it over.
const CSS_PLAY_STATE: &str =
    "v=>v==null||/^\\s*(unset|auto|(-exact-)?clock\\(.*\\))?\\s*$/i.test(v)?null:\"var(--exact-timeline-play,paused)\"";
const CSS_TIMELINE: &str =
    "v=>v==null||/^\\s*(unset|auto|(-exact-)?clock\\(.*\\))\\s*$/i.test(v)?null:v";

/// A bound `animation-timeline`'s play state: paused for the drag code where
/// a name may be a drag's, else only where the browser has no timelines.
pub(crate) fn play_state(drag_timelines: bool) -> &'static str {
    if drag_timelines {
        crate::style::TIMELINE_PLAY_STATE
    } else {
        CSS_PLAY_STATE
    }
}

/// `animation-timeline` as CSS's own row: any timeline in a plan with no
/// drag timeline, only `scroll()` and `view()` where a name may be a drag's.
pub(crate) fn timeline_map(drag_timelines: bool) -> &'static str {
    if drag_timelines {
        crate::style::ANONYMOUS_TIMELINE
    } else {
        CSS_TIMELINE
    }
}

/// A bound `animation-timeline`'s writes in a plan with no drag timeline: a
/// name is CSS's own row too, as `css_timelines` makes a literal one, and
/// its consumer is the browser's (`--exact-css-timeline`).
pub(crate) fn css_writes(writes: &mut [(String, String, Option<String>)]) {
    for (name, _, map) in writes {
        match name.as_str() {
            "animation-timeline" => *map = Some(CSS_TIMELINE.into()),
            "animation-play-state" => *map = Some(CSS_PLAY_STATE.into()),
            "--exact-css-timeline" => *map = Some(CSS_MARK.into()),
            _ => {}
        }
    }
}

/// A bound `animation`'s writes after its shorthand, which resets the
/// timeline and the range: both again, read live through `timeline` and
/// `range` (the bound rows' getters), unless the shorthand is `none`.
pub(crate) fn after_shorthand(
    writes: &mut Vec<(String, String, Option<String>)>,
    drag_timelines: bool,
    timeline: &str,
    range: Option<String>,
) {
    let live = "v=>v==null||/^\\s*none\\s*$/i.test(v)";
    let map = timeline_map(drag_timelines);
    writes.push((
        "animation-timeline".into(),
        String::new(),
        Some(format!("v=>({live})(v)?null:({map})(({timeline})())")),
    ));
    if let Some(range) = range {
        writes.push((
            "animation-range".into(),
            String::new(),
            Some(format!(
                "v=>({live})(v)?null:({})(({range})())",
                crate::style::ANIMATION_RANGE
            )),
        ));
    }
}

/// `--exact-css-timeline` for any timeline but the clock (index.html).
const CSS_MARK: &str =
    "v=>v==null||/^\\s*(unset|auto|(-exact-)?clock\\(.*\\))\\s*$/i.test(v)?null:\"1\"";

/// Whether any node can declare a drag timeline: then a timeline's name may
/// be a drag's, which the motion piece resolves (motion-glue.js
/// `followTimelines`) and the plan loads.
pub(crate) fn drag_timelines(plan: &exact_plan::Plan) -> bool {
    plan.bindings
        .iter()
        .any(|b| b.kind == BindingKind::Style && b.id == StyleId::DragTimeline as u16)
}

/// @ref LLP 1057.003 D5 — in a plan with no drag timeline (`drag_timelines`),
/// a named consumer's static CSS (css.rs: the name for the drag code, and
/// paused) becomes CSS's own row: the browser runs every timeline there.
pub(crate) fn css_timelines(css: &mut String) {
    const PAUSED: &str = ";animation-play-state:paused;";
    let Some(at) = css.find("--exact-animation-timeline:") else {
        return;
    };
    let start = at + "--exact-animation-timeline:".len();
    let Some(end) = css[start..].find(';').map(|e| start + e) else {
        return;
    };
    if css[end..].starts_with(PAUSED) {
        let name = css[start..end].to_string();
        css.replace_range(
            end..end + PAUSED.len(),
            &format!(";animation-timeline:{name};animation-play-state:var(--exact-timeline-play,paused);--exact-css-timeline:1;"),
        );
    }
}
