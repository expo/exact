//! CSS `text-wrap-style: balance` (CSS Text 4 §6.1.2) as Chromium does it.
//! @ref LLP 1001 §6 "Balanced wrapping"
//!
//! Blink balances the lines before a block's first forced break, when they
//! are two to six and none overflows the box (`LineBreakStrategy::Balance`,
//! `inline_layout_algorithm.cc`). It scores them ([`balanced_breaks`]); where
//! that gives nothing and the block has no forced break, it bisects the
//! width ([`balanced_width`]); otherwise the lines stay greedy. Each host
//! runs both over its own breaker and the shaping it paints. The box keeps
//! its width either way; only the breaks move, so alignment and
//! justification still fill it.

/// The most lines a paragraph balances; one longer breaks greedily, as in
/// Chromium (Firefox balances up to ten).
pub const BALANCE_MAX_LINES: usize = 6;

/// The width Blink's bisection breaks a balanced paragraph at
/// (`ParagraphLineBreaker::AttemptParagraphBalancing`), for a paragraph of
/// `count` greedy lines at `width` whose widths sum to `total`. `fits(w)`
/// says whether the paragraph breaks at `w` into no more than `count` lines,
/// none overflowing. The search runs in 1/64 px (a layout unit) from 0.8 of
/// the average line width up to `width` and stops within 1 px, so the
/// result is at most 1 px wider than the narrowest width that fits. `None`
/// leaves the paragraph greedy: no finite positive width, `count` outside
/// 2..=[`BALANCE_MAX_LINES`], or no narrower width found.
pub fn balanced_width(
    width: f32,
    count: usize,
    total: f32,
    mut fits: impl FnMut(f32) -> bool,
) -> Option<f32> {
    if !(width.is_finite() && width > 0.0) || !(2..=BALANCE_MAX_LINES).contains(&count) {
        return None;
    }
    let box_width = (width * 64.0).floor().min(i32::MAX as f32) as i64;
    let average = ((total.max(0.0) * 64.0).floor().min(i32::MAX as f32) as i64) / count as i64;
    let (mut lower, mut upper) = ((average as f32 * 0.8).round() as i64, box_width);
    while lower + 64 < upper {
        let middle = (upper + lower) / 2;
        if fits(middle as f32 / 64.0) {
            upper = middle;
        } else {
            lower = middle;
        }
    }
    (upper < box_width).then_some(upper as f32 / 64.0)
}

/// Blink's `kOrphansPenalty`, so the final line rarely holds one word.
const ORPHANS_PENALTY: f32 = 10_000.0;

/// The fewest break candidates, Blink's start sentinel included, that Blink
/// scores (`kMinCandidates` + 2); fewer bisect.
const MIN_CANDIDATES: usize = 5;

/// The breaks Blink's scorer balances a paragraph at
/// (`ScoreLineBreaker::BalanceBreakPoints`): it minimises a score rather
/// than searching for a width.
///
/// `opportunities` are the paragraph's break opportunities in order, each as
/// `(end, next, hyphen)`: where a line breaking there ends (trailing space
/// excluded, a hyphen's advance included), where the next line starts, both
/// measured from the paragraph's start, and whether breaking there
/// hyphenates (a soft or automatic hyphen). The last entry is the paragraph's
/// end (its `next` and `hyphen` unused). `width` and `indent`, the first
/// line's `text-indent`, floor to 1/64 px (a layout unit), and each position
/// rounds up to one, as the text positions Blink's candidates read do
/// (`ShapeResult::CachedPositionForOffset`). A line fits when it is at most
/// one layout unit wider than its room, `width` less `indent` on the first
/// line (`AvailableWidthToFit`, matching Blink's greedy `LineBreaker`).
///
/// All of the arithmetic is `f32`, as Blink's. A line's score is its unused
/// width (from that fit) squared, the final line's included, plus a line
/// penalty of 4 times `width` times `font_size`, the block's; a line ending
/// at a hyphen adds half that (`SetupParameters`,
/// `LineBreaker::AppendCandidates`). A `justified` paragraph (`text-align:
/// justify`) has no line penalty, and its hyphen penalty is a quarter as
/// much, half of `width` times `font_size`. A line ending at the last break,
/// or at a break followed only by hyphens, adds 10,000 (`Optimize`'s orphans
/// penalty). As in `ComputeScores`, a line's start is skipped when its score
/// plus the last scored start's unused width squared already reaches the
/// best (`best_hope`), and is otherwise taken when its total is `<=` the
/// best, so a tie goes to the later break. Breakings whose lines are as wide
/// in another order tie but for rounding, of the positions and of the `f32`
/// scores, so both decide which of them wins.
///
/// Blink scores only a paragraph whose greedy lines all fit, so a line that
/// overflows is never taken here.
///
/// Returns the indices into `opportunities` where lines break (`lines` - 1 of
/// them), or `None` where Blink's scorer gives nothing: fewer than four
/// opportunities, the best breaking having other than `lines` lines (the
/// greedy count), `lines` outside 2..=[`BALANCE_MAX_LINES`], or `width` not
/// finite and positive.
pub fn balanced_breaks(
    width: f32,
    indent: f32,
    font_size: f32,
    justified: bool,
    opportunities: &[(f32, f32, bool)],
    lines: usize,
) -> Option<Vec<usize>> {
    // Node 0 is the paragraph's start; node `k` breaks at `opportunities[k - 1]`.
    let n = opportunities.len();
    if !(width.is_finite() && width > 0.0)
        || !(2..=BALANCE_MAX_LINES).contains(&lines)
        || n + 1 < MIN_CANDIDATES
    {
        return None;
    }
    let floor = |px: f32| (px * 64.0).floor() / 64.0;
    let ceil = |px: f32| (px * 64.0).ceil() / 64.0;
    let width = floor(width);
    let fit = width + 1.0 / 64.0;
    let first_fit = (width - floor(indent)).max(0.0) + 1.0 / 64.0;
    let (hyphen_penalty, line_penalty) = if justified {
        (width * font_size / 2.0, 0.0)
    } else {
        (width * font_size * 2.0, width * font_size * 4.0)
    };
    let mut penalty: Vec<f32> = opportunities
        .iter()
        .enumerate()
        .map(|(k, &(_, _, hyphen))| {
            if hyphen && k + 1 < n {
                hyphen_penalty
            } else {
                0.0
            }
        })
        .collect();
    for k in (0..n - 1).rev() {
        penalty[k] += ORPHANS_PENALTY;
        if !opportunities[k].2 {
            break;
        }
    }
    let mut score = vec![0.0f32; n + 1];
    let mut from = vec![0; n + 1];
    let mut count = vec![0; n + 1];
    // Lines from nodes before `active` overflow every later end (Blink's
    // `active`): starts and ends only move forward.
    let mut active = 0;
    for b in 1..=n {
        let end = ceil(opportunities[b - 1].0);
        let (mut best, mut best_from, mut hope) = (f32::INFINITY, 0, 0.0f32);
        let first = active;
        for a in first..b {
            // The first line's room differs by the indent: Blink starts its
            // `best_hope` again where the room changes.
            if a == 1 && first == 0 && first_fit != fit {
                hope = 0.0;
            }
            if score[a] + hope >= best {
                continue;
            }
            let start = if a == 0 {
                0.0
            } else {
                ceil(opportunities[a - 1].1)
            };
            // Blink's `delta`, from its `start_edge`: a line starting before
            // the edge overflows.
            let unused = start - (end - if a == 0 { first_fit } else { fit });
            if unused < 0.0 {
                active = a + 1;
                continue;
            }
            hope = unused * unused;
            if score[a] + hope <= best {
                best = score[a] + hope;
                best_from = a;
            }
        }
        score[b] = best + penalty[b - 1] + line_penalty;
        from[b] = best_from;
        count[b] = count[best_from] + 1;
    }
    if !score[n].is_finite() || count[n] != lines {
        return None;
    }
    let mut breaks = Vec::with_capacity(lines - 1);
    let mut node = from[n];
    while node > 0 {
        breaks.push(node - 1);
        node = from[node];
    }
    breaks.reverse();
    Some(breaks)
}
