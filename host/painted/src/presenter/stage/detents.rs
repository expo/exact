//! Sheet detent parsing and content-height ceiling resolution (LLP 1075.003 §9.11, §9.13).

use exact_kernel::PropId;

/// The room a sheet's drag handle takes at its top, points: Material's
/// handle, 4 tall with 22 above and below.
pub const SHEET_HANDLE: f32 = 48.0;
pub(crate) fn sheet_handle(route: &exact_kernel::NodeRef<'_>) -> f32 {
    if route.props.str(PropId::NavigationGrabber) == Some("hidden") {
        0.0
    } else {
        SHEET_HANDLE
    }
}
/// Material 3's widest sheet.
const SHEET_MAX_WIDTH: f32 = 640.0;
/// A centered Material sheet's left margin and width in the page.
pub(crate) fn sheet_span(page_width: f32) -> (f32, f32) {
    let width = page_width.min(SHEET_MAX_WIDTH);
    ((page_width - width) / 2.0, width)
}

/// Whether `detent` names `fit-content`.
pub(super) fn fits(detent: Option<&str>) -> bool {
    detent.is_some_and(|d| d.split_whitespace().any(|w| w == "fit-content"))
}

/// Resolve `detent` into ascending `(alias_words, height)` stops plus any
/// preceding stop words omitted because a terminal `fit-content` ceiling lay at
/// or below them. `natural_tallest` / `natural_foot` / `natural_fit` exclude
/// keyboard shrinkage and IME lift so opening the keyboard on a short sheet
/// never introduces a spurious stop; `tallest` / `foot` / `display_fit` produce
/// the rendered heights. Stops that tie within 0.5 dp share one entry keeping
/// every collapsed word so initial/resting-stop lookup stays exact.
pub(super) fn resolve_stops(
    detent: Option<&str>,
    tallest: f32,
    foot: f32,
    natural_tallest: f32,
    natural_foot: f32,
    natural_fit: f32,
    display_fit: f32,
) -> (Vec<(Vec<String>, f32)>, Vec<String>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Points,
        Share,
        Content,
        Large,
    }
    let mut kept: Vec<(&str, Kind, f32)> = Vec::new();
    for word in detent.unwrap_or("large").split_whitespace() {
        let parsed = match word {
            "large" => Some((Kind::Large, 100.0)),
            "medium" => Some((Kind::Share, 50.0)),
            "fit-content" => Some((Kind::Content, 0.0)),
            d => match d.strip_suffix('%') {
                Some(p) => p
                    .parse::<f32>()
                    .ok()
                    .filter(|p| p.is_finite() && *p > 0.0 && *p <= 100.0)
                    .map(|p| {
                        if p >= 100.0 {
                            (Kind::Large, 100.0)
                        } else {
                            (Kind::Share, p)
                        }
                    }),
                None => d
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .map(|v| (Kind::Points, v)),
            },
        };
        let Some((kind, raw)) = parsed else {
            continue;
        };
        let ok = match kept.last() {
            None => true,
            Some((_, last_kind, last_raw)) => match (*last_kind, kind) {
                (Kind::Large, _) => false,
                (Kind::Content, Kind::Large) => kept.len() == 1,
                (Kind::Content, _) => false,
                (_, Kind::Large | Kind::Content) => true,
                (a, b) => a == b && raw > *last_raw,
            },
        };
        if ok {
            kept.push((word, kind, raw));
        }
    }
    let cap = tallest.max(1.0);
    let nat_cap = natural_tallest.max(1.0);
    let nat_fit = natural_fit.clamp(1.0, nat_cap);
    let disp_fit = display_fit.clamp(1.0, cap);
    let resolve_in = |kind: Kind, raw: f32, max_h: f32, pad: f32, fit: f32| -> f32 {
        match kind {
            Kind::Large => max_h,
            Kind::Share => (max_h * raw / 100.0 + pad).clamp(1.0, max_h),
            Kind::Points => (raw + pad).clamp(1.0, max_h),
            Kind::Content => fit,
        }
    };
    let push_stop = |out: &mut Vec<(Vec<String>, f32)>, word: &str, h: f32| {
        if let Some(prev) = out.last_mut() {
            if (h - prev.1).abs() < 0.5 {
                prev.0.push(word.to_string());
                return;
            }
        }
        out.push((vec![word.to_string()], h));
    };
    let terminal_fit = kept.len() > 1 && kept.last().is_some_and(|(_, k, _)| *k == Kind::Content);
    let mut out: Vec<(Vec<String>, f32)> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    if terminal_fit {
        for &(word, kind, raw) in &kept[..kept.len() - 1] {
            let nat_h = resolve_in(kind, raw, nat_cap, natural_foot, nat_fit);
            if nat_h < nat_fit - 0.5 {
                let disp_h = resolve_in(kind, raw, cap, foot, disp_fit);
                push_stop(&mut out, word, disp_h);
            } else {
                dropped.push(word.to_string());
            }
        }
        push_stop(&mut out, "fit-content", disp_fit);
    } else {
        for &(word, kind, raw) in &kept {
            let h = resolve_in(kind, raw, cap, foot, disp_fit);
            push_stop(&mut out, word, h);
        }
    }
    if out.is_empty() {
        out.push((vec!["large".to_string()], cap));
    }
    (out, dropped)
}

/// Unobscured natural and displayed `fit-content` heights for a sheet.
pub(super) struct FitBounds {
    pub(super) natural_tallest: f32,
    pub(super) natural_foot: f32,
    pub(super) natural_fit: Option<f32>,
    pub(super) display_fit: Option<f32>,
}

/// Compute natural (unobscured) and displayed `fit-content` bounds, excluding
/// both `resizes-content` shrinkage and `overlays-content` IME lift when
/// comparing preceding stops against a terminal `fit-content` ceiling
/// (LLP 1075.003 §9.11).
#[allow(clippy::too_many_arguments)]
pub(super) fn fit_bounds(
    h: f32,
    tallest: f32,
    status: f32,
    foot: f32,
    lift: f32,
    keyboard_height: f32,
    window_bottom: Option<(f32, f32)>,
    measured: Option<f32>,
) -> FitBounds {
    let unobscured_h = h + (keyboard_height - lift).max(0.0);
    let natural_tallest = (unobscured_h - status).max(1.0);
    let natural_foot = match window_bottom {
        Some((frame_top, bottom)) => (frame_top + unobscured_h - bottom)
            .max(0.0)
            .min(unobscured_h),
        None => foot,
    };
    FitBounds {
        natural_tallest,
        natural_foot,
        natural_fit: measured.map(|c| (c + natural_foot).clamp(1.0, natural_tallest)),
        display_fit: measured.map(|c| (c + foot.max(lift)).clamp(1.0, tallest)),
    }
}

/// Resolved sheet stops plus word tables and `fit-content` index for `sync_stage`.
pub(super) struct SheetStops {
    pub(super) heights: Vec<f32>,
    pub(super) words: Vec<Vec<String>>,
    pub(super) dropped: Vec<String>,
    pub(super) fit_idx: Option<usize>,
    pub(super) fit_pair: Option<(f32, f32)>,
}

/// Resolve a sheet's stops and `fit-content` bookkeeping for one layout pass.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_sheet_stops(
    presents: bool,
    detent: Option<&str>,
    h: f32,
    tallest: f32,
    status: f32,
    foot: f32,
    lift: f32,
    keyboard_height: f32,
    window_bottom: Option<(f32, f32)>,
    measured: Option<f32>,
) -> SheetStops {
    let bounds = fit_bounds(
        h,
        tallest,
        status,
        foot,
        lift,
        keyboard_height,
        window_bottom,
        measured,
    );
    let (stops, dropped) = if presents {
        resolve_stops(
            detent,
            tallest,
            foot,
            bounds.natural_tallest,
            bounds.natural_foot,
            bounds.natural_fit.unwrap_or(bounds.natural_tallest),
            bounds.display_fit.unwrap_or(tallest),
        )
    } else {
        (vec![(vec!["large".to_string()], h)], Vec::new())
    };
    let words: Vec<Vec<String>> = stops.iter().map(|(w, _)| w.clone()).collect();
    let heights: Vec<f32> = stops.iter().map(|(_, v)| *v).collect();
    let fit_idx = words
        .iter()
        .position(|ws| ws.iter().any(|w| w == "fit-content"));
    SheetStops {
        heights,
        words,
        dropped,
        fit_idx,
        fit_pair: bounds.natural_fit.zip(bounds.display_fit),
    }
}

/// Choose the initial resting detent index for a newly presented sheet.
pub(super) fn initial_rest(
    initial: Option<&str>,
    words: &[Vec<String>],
    dropped: &[String],
    fit_idx: Option<usize>,
    keyboard: bool,
) -> usize {
    let at = match initial.filter(|s| !s.is_empty()) {
        Some(word) => words
            .iter()
            .position(|ws| ws.iter().any(|w| w == word))
            .or_else(|| {
                dropped
                    .iter()
                    .any(|w| w == word)
                    .then_some(fit_idx)
                    .flatten()
            })
            .unwrap_or(0),
        None => 0,
    };
    match keyboard && fit_idx != Some(at) {
        true => words.len() - 1,
        false => at,
    }
}

/// Reconcile an existing sheet's active detent index when its stops, keyboard
/// state, or measured `fit-content` height changes.
pub(super) fn reconcile_index(
    current_at: usize,
    prev_words: Option<&Vec<String>>,
    words: &[Vec<String>],
    dropped: &[String],
    fit_idx: Option<usize>,
    keyboard: bool,
) -> usize {
    let was_fit = prev_words.is_some_and(|ws| ws.iter().any(|w| w == "fit-content"));
    if keyboard && !was_fit {
        words.len() - 1
    } else if was_fit {
        fit_idx.unwrap_or_else(|| current_at.min(words.len() - 1))
    } else if let Some(pws) = prev_words {
        words
            .iter()
            .position(|ws| ws.iter().any(|w| pws.iter().any(|p| p == w)))
            .or_else(|| {
                pws.iter()
                    .any(|p| dropped.iter().any(|d| d == p))
                    .then_some(fit_idx)
                    .flatten()
            })
            .unwrap_or_else(|| current_at.min(words.len() - 1))
    } else {
        current_at.min(words.len() - 1)
    }
}
