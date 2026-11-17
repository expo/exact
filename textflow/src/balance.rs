//! CSS `text-wrap-style: balance` (CSS Text 4 §6.1.2), bounded as Chromium
//! bounds it. @ref LLP 1001 §6 "Balanced wrapping"
//!
//! Balancing is a search over the host's own greedy breaker: the narrowest
//! width that keeps the line count the box's width gives. Each host runs it
//! over its paragraph's lines up to the first forced break (Chrome balances
//! only those, and breaks the rest greedily at the box's width), measuring
//! with the shaping it paints.

/// The most lines a paragraph balances; one longer breaks greedily, as in
/// Chromium (Firefox balances up to ten).
pub const BALANCE_MAX_LINES: usize = 6;

/// The width a balanced paragraph breaks at: the narrowest, to 1/64 px (a
/// browser layout unit), at which `lines(width)` (the greedy line count of
/// the lines being balanced) is still what it is at `width`. `None` leaves
/// the paragraph greedy: no finite positive width, one line, or more than
/// [`BALANCE_MAX_LINES`]. The box keeps `width`; only the breaks move, so
/// alignment and justification still fill `width`.
pub fn balanced_width(width: f32, mut lines: impl FnMut(f32) -> usize) -> Option<f32> {
    if !(width.is_finite() && width > 0.0) {
        return None;
    }
    let count = lines(width);
    if !(2..=BALANCE_MAX_LINES).contains(&count) {
        return None;
    }
    // In 1/64 px units: `lines(lo)` breaks into more lines, `lines(hi)` no more.
    let (mut lo, mut hi) = (0u32, (width * 64.0).floor().min(u32::MAX as f32) as u32);
    let at = |units: u32| units as f32 / 64.0;
    if lines(at(hi)) > count {
        return Some(width);
    }
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if lines(at(mid)) > count {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(at(hi))
}

/// Blink's `kOrphansPenalty`, so the final line rarely holds one word.
const ORPHANS_PENALTY: f32 = 10_000.0;

/// The fewest break candidates, Blink's start sentinel included, that Blink
/// scores (`kMinCandidates` + 2); fewer bisect.
const MIN_CANDIDATES: usize = 5;

/// The breaks Chromium balances a paragraph at: Blink's
/// `ScoreLineBreaker::BalanceBreakPoints`, which minimises a score rather
/// than searching for the narrowest width that keeps the line count.
///
/// `opportunities` are the paragraph's break opportunities in order, each as
/// `(end, next, hyphen)`: where a line breaking there ends (trailing space
/// excluded, a hyphen's advance included), where the next line starts, both
/// measured from the paragraph's start, and whether breaking there
/// hyphenates (a soft or automatic hyphen). The last entry is the paragraph's
/// end (its `next` and `hyphen` unused). `width` floors to 1/64 px (a layout
/// unit); positions stay unrounded, as Blink's text break candidates do. A
/// line fits when it is at most one layout unit wider than `width`
/// (`AvailableWidthToFit`, matching Blink's greedy `LineBreaker`).
///
/// All of the arithmetic is `f32`, as Blink's. A line's score is its unused
/// width (from that fit) squared, the final line's included, plus a line
/// penalty of 4 times `width` times `font_size`; a line ending at a hyphen
/// adds half that (`SetupParameters`, `LineBreaker::AppendCandidates`). A
/// `justified` paragraph (`text-align: justify`) has no line penalty, and its
/// hyphen penalty is a quarter as much, half of `width` times `font_size`. A
/// line ending at the last break, or at a break followed only by hyphens,
/// adds 10,000 (`Optimize`'s orphans penalty). As in `ComputeScores`, a
/// line's start is skipped when its score plus the last scored start's
/// unused width squared already reaches the best (`best_hope`), and is
/// otherwise taken when its total is `<=` the best, so a tie goes to the
/// later break.
///
/// Returns the indices into `opportunities` where lines break (`lines` - 1 of
/// them), or `None` where Blink falls back to bisection ([`balanced_width`],
/// which keeps the paragraph greedy where Blink does): fewer than four
/// opportunities, the best breaking having other than `lines` lines (the
/// greedy count), `lines` outside 2..=[`BALANCE_MAX_LINES`], or `width` not
/// finite and positive.
pub fn balanced_breaks(
    width: f32,
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
    let width = (width * 64.0).floor() / 64.0;
    let fit = width + 1.0 / 64.0;
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
        // Blink's `start_edge`: a line starting before it overflows.
        let edge = opportunities[b - 1].0 - fit;
        let (mut best, mut best_from, mut hope) = (f32::INFINITY, 0, 0.0f32);
        let first = active;
        for a in first..b {
            if score[a] + hope >= best {
                continue;
            }
            let start = if a == 0 { 0.0 } else { opportunities[a - 1].1 };
            let unused = start - edge;
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
