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

/// Blink's cost of a final line that holds only the paragraph's last word.
const LAST_WORD_ALONE_PENALTY: f64 = 10_000.0;

/// Blink's cost of each line, as a multiple of the width times the font size.
const LINE_PENALTY_MULTIPLIER: f64 = 4.0;

/// The breaks Chromium balances a paragraph at: Blink's
/// `ScoreLineBreaker::BalanceBreakPoints`, which minimises a score rather
/// than searching for the narrowest width that keeps the line count.
///
/// `opportunities` are the paragraph's break opportunities in order, each as
/// `(end, next)`: where a line breaking there ends (trailing space excluded)
/// and where the next line starts, measured from the paragraph's start. The
/// last entry is the paragraph's end (its `next` unused). A line's score is
/// its unused width squared, the final line's included, plus a line penalty
/// of 4 × `width` × `font_size`, plus 10,000 when the final line holds only
/// the last word. Positions round up to 1/64 px (a layout unit) first, which
/// settles exact ties; the earlier break wins a tie that remains.
///
/// Returns the indices into `opportunities` where lines break (`lines` − 1 of
/// them), or `None`, leaving the paragraph greedy, when the best breaking
/// has other than `lines` lines (the greedy count), when `lines` is outside
/// 2..=[`BALANCE_MAX_LINES`], or when `width` is not finite and positive.
pub fn balanced_breaks(
    width: f32,
    font_size: f32,
    opportunities: &[(f32, f32)],
    lines: usize,
) -> Option<Vec<usize>> {
    if !(width.is_finite() && width > 0.0)
        || !(2..=BALANCE_MAX_LINES).contains(&lines)
        || opportunities.len() < lines
    {
        return None;
    }
    let ceil = |x: f32| (f64::from(x) * 64.0).ceil() / 64.0;
    let width64 = (f64::from(width) * 64.0).floor() / 64.0;
    let line_penalty = LINE_PENALTY_MULTIPLIER * f64::from(width) * f64::from(font_size);
    // Node 0 is the paragraph's start; node `k` breaks at `opportunities[k - 1]`.
    let n = opportunities.len();
    let mut score = vec![f64::INFINITY; n + 1];
    let mut from = vec![0; n + 1];
    let mut count = vec![0; n + 1];
    score[0] = 0.0;
    // Lines from nodes before `active` overflow every later end (Blink's
    // `active`): starts and ends only move forward.
    let mut active = 0;
    for b in 1..=n {
        let end = ceil(opportunities[b - 1].0);
        let first = active;
        for a in first..b {
            let start = if a == 0 {
                0.0
            } else {
                ceil(opportunities[a - 1].1)
            };
            let unused = width64 - (end - start);
            if unused < 0.0 {
                active = a + 1;
                continue;
            }
            if !score[a].is_finite() {
                continue;
            }
            let mut s = score[a] + unused * unused + line_penalty;
            if b == n && a + 1 == n && a > 0 {
                s += LAST_WORD_ALONE_PENALTY;
            }
            if s < score[b] {
                score[b] = s;
                from[b] = a;
                count[b] = count[a] + 1;
            }
        }
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
