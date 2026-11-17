//! Balanced line breaking for `flex-wrap: balance`.
//!
//! Implements the balancing algorithm from the CSS Flexbox Level 2 draft
//! (<https://drafts.csswg.org/css-flexbox-2/#balancing>). Items are divided into exactly
//! `line_count` contiguous sequences (lines), where `line_count` is the number of lines that
//! greedy line breaking would produce (with item sizes floored at zero), raised to the minimum
//! flex line count (`flex-line-count`, clamped to the number of items), such that:
//!
//! - every line holds at least one item;
//! - no line's size exceeds the container's inner main size, unless the line holds a single
//!   (overflowing) item;
//! - a zero-sized item is assigned to the end of the preceding line rather than the beginning
//!   of a line, unless no valid division can glue it there: because extending the preceding
//!   line through the item would make it exceed the container's inner main size (which covers
//!   a preceding single overflowing item), or because gluing leaves the remaining lines
//!   without a valid division of the remaining items;
//! - calling the difference between a line's size and the container's inner main size the
//!   line's *error*, the sum of the squared errors of all lines is minimized;
//! - ties are broken by assigning the most items to the first line, then the most items to the
//!   second line, and so on.
//!
//! Because every division has the same line count, total item size, and total gap size,
//! minimizing the sum of squared errors is equivalent to minimizing the sum of squared *line
//! sizes*, which is what is actually computed: it needs no target size, so an indefinite
//! (infinite) container size needs no special handling — no line overflows, and the
//! minimization equalizes the line sizes. All arithmetic uses `f64`: finite `f32` sizes
//! convert exactly, and the squares and sums involved are far from `f64`'s limits, so
//! equally-balanced divisions compare equal and are resolved by the tie-break rather than by
//! rounding noise.
//!
//! The minimization is a dynamic program over (line count, first item of suffix) accelerated
//! with the divide-and-conquer optimization, running in `O(line_count * item_count *
//! log(item_count))` time; the naive `O(line_count * item_count²)` dynamic program is kept as
//! a test oracle.
use crate::util::sys::{new_vec_with_capacity, Vec};

/// Score assigned to divisions that violate the line size constraint
const INFEASIBLE: f64 = f64::INFINITY;

/// Convert an item size in pixels to an `f64`, flooring negative sizes at zero. (An
/// infinite size is clamped to the largest finite `f32` so that sums stay meaningful.)
fn to_size(value: f32) -> f64 {
    (value as f64).clamp(0.0, f32::MAX as f64)
}

/// Line sizing shared by the scoring and readback phases
struct LineSizes {
    /// Per item, the prefix sum of the item sizes through it (each item also contributes
    /// one trailing gap, so the size of the line holding items `start..=end` is
    /// `sums[end].0 - sums[start - 1].0 - gap_between_items`), paired with whether the
    /// item's (floored) size is zero
    sums: Vec<(f64, bool)>,
    /// The size of the gap between adjacent items on a line
    gap_between_items: f64,
    /// The container's inner main size, which lines may not exceed (unless they hold a
    /// single item)
    limit: f64,
}

impl LineSizes {
    /// The total number of items
    fn item_count(&self) -> usize {
        self.sums.len()
    }

    /// Whether the item at `index` has a (floored) size of zero
    fn is_zero_item(&self, index: usize) -> bool {
        self.sums[index].1
    }

    /// The size of the line holding items `start..=end`
    fn line_size(&self, start: usize, end: usize) -> f64 {
        let start_sum = if start == 0 { 0.0 } else { self.sums[start - 1].0 };
        self.sums[end].0 - start_sum - self.gap_between_items
    }

    /// The squared size of the line holding items `start..=end`, or [`INFEASIBLE`] for a
    /// line of more than one item exceeding the limit
    fn line_cost(&self, start: usize, end: usize) -> f64 {
        let size = self.line_size(start, end);
        if end > start && size > self.limit {
            return INFEASIBLE;
        }
        size * size
    }

    /// For every suffix of the items, the number of lines that greedy line breaking
    /// produces for it: each line collects consecutive items until the next item no longer
    /// fits, and if even the first item of a line doesn't fit, that line takes just the one
    /// (overflowing) item. (The entry for the empty suffix is zero.)
    ///
    /// This is also the *fewest* lines each suffix can validly be divided into, and a
    /// suffix can validly be divided into any number of lines from that count up to its
    /// item count: dividing at greedy line ends never violates the zero-sized-item rule
    /// (a line that can hold no more of the following items can never glue them), and
    /// from any in-range line count the same holds with line ends capped to leave one
    /// item per remaining line.
    fn suffix_greedy_line_counts(&self) -> Vec<u32> {
        let item_count = self.item_count();
        let mut counts: Vec<u32> = new_vec_with_capacity(item_count + 1);
        counts.extend(core::iter::repeat(0).take(item_count + 1));
        // The (exclusive) end of the greedy first line of the suffix, which only moves
        // down as the suffix grows leftwards since lines starting earlier are larger
        let mut line_end = item_count;
        for start in (0..item_count).rev() {
            while line_end > start + 1 && self.line_size(start, line_end - 1) > self.limit {
                line_end -= 1;
            }
            counts[start] = 1 + counts[line_end];
        }
        counts
    }

    /// For every `start`, the largest `end` such that the line holding items `start..=end`
    /// does not exceed the limit (or `start` itself if even that one item overflows)
    fn fit_ends(&self) -> Vec<u32> {
        let item_count = self.item_count();
        let mut fit_ends: Vec<u32> = new_vec_with_capacity(item_count);
        // Lines starting later are smaller, so the fit end only moves up
        let mut fit_end = 0;
        for start in 0..item_count {
            if fit_end < start {
                fit_end = start;
            }
            while fit_end + 1 < item_count && self.line_size(start, fit_end + 1) <= self.limit {
                fit_end += 1;
            }
            fit_ends.push(fit_end as u32);
        }
        fit_ends
    }
}

/// One row of the balancing dynamic program: divisions of item suffixes into exactly
/// `lines` lines
struct Row<'a> {
    /// The line sizing for the items being divided
    sizes: &'a LineSizes,
    /// See [`LineSizes::fit_ends`]
    fit_ends: &'a [u32],
    /// The previous row: `prev[start]` is the minimum score of any valid division of items
    /// `start..` into exactly `lines - 1` lines ([`INFEASIBLE`] if there is none)
    prev: &'a [f64],
    /// The line ends that precede a non-zero item, in ascending order and capped to
    /// `max_end`. Line ends preceding a *zero* item are constrained by the
    /// zero-sized-item rule and handled separately: for a first line starting at `start`,
    /// the only end preceding a zero item that can be part of a valid division is
    /// `min(fit_ends[start], max_end)` — the line extended as far as the limit and the
    /// remaining lines' one-item-each budget allow. Ending earlier is forbidden by the
    /// rule (the zero item could be glued: the extended line fits and, per
    /// [`LineSizes::suffix_greedy_line_counts`], what remains after even the longest glued
    /// line divides validly into the remaining lines whenever what remains after the
    /// shorter line does, which it must for the division to be feasible at all), and
    /// ending later exceeds the limit on a line of more than one item.
    nonzero_ends: &'a [u32],
    /// The largest possible end of the first line: the remaining `lines - 1` lines need
    /// one item each
    max_end: usize,
}

/// Compute `cur[start]` and `opts[start]` for `start` in `start_lo..=start_hi`, where
/// `cur[start]` is the minimum over valid ends `end` of
/// `row.sizes.line_cost(start, end) + row.prev[end + 1]` and `opts[start]` is the largest
/// `end` achieving that minimum. Ends preceding a non-zero item are minimized over
/// `row.nonzero_ends[col_lo..col_hi]`; the (at most one) end preceding a zero item allowed
/// by the zero-sized-item rule is merged in afterwards.
///
/// Uses the divide-and-conquer dynamic programming optimization: over the ends preceding
/// non-zero items, the cost function satisfies the concave quadrangle inequality (it is
/// the square of a quantity that increases in `end` and decreases in `start`, plus a term
/// depending on `end` alone, with infeasibility monotone in line length), so the largest
/// minimizing end is non-decreasing in `start`. Solving the middle `start` therefore
/// splits the end range in two, and each recursion level scans each candidate end a
/// bounded number of times. (The ends constrained by the zero-sized-item rule are *not*
/// monotone this way, which is why they are excluded from the scan and merged in
/// separately.)
fn fill_row(
    row: &Row,
    cur: &mut [f64],
    opts: &mut [u32],
    start_lo: usize,
    start_hi: usize,
    col_lo: usize,
    col_hi: usize,
) {
    if start_lo > start_hi {
        return;
    }
    let start = start_lo + (start_hi - start_lo) / 2;

    // Minimize over the in-range ends preceding a non-zero item, starting from `start`
    // (the line must hold at least one item)
    let first_col = col_lo + row.nonzero_ends[col_lo..col_hi].partition_point(|&end| (end as usize) < start);
    let mut min_cost = INFEASIBLE;
    let mut min_col = None;
    for col in first_col..col_hi {
        let end = row.nonzero_ends[col] as usize;
        let line_cost = row.sizes.line_cost(start, end);
        if line_cost == INFEASIBLE {
            // Every longer line also exceeds the limit
            break;
        }
        let cost = line_cost + row.prev[end + 1];
        // `<=` keeps the *largest* minimizing end, assigning the most items to the
        // earliest lines as the tie-break requires
        if cost <= min_cost {
            min_cost = cost;
            min_col = Some(col);
        }
    }

    // Merge the single end preceding a zero item that the zero-sized-item rule allows
    let mut min_end = min_col.map(|col| row.nonzero_ends[col] as usize);
    let zero_end = (row.fit_ends[start] as usize).min(row.max_end);
    if row.sizes.is_zero_item(zero_end + 1) {
        let cost = row.sizes.line_cost(start, zero_end) + row.prev[zero_end + 1];
        if cost < min_cost || (cost == min_cost && min_end.map_or(true, |end| end < zero_end)) {
            min_cost = cost;
            min_end = Some(zero_end);
        }
    }

    // Infeasible states are excluded from the range by the caller, and every feasible
    // state has a valid transition (see [`LineSizes::suffix_greedy_line_counts`])
    debug_assert!(min_cost.is_finite());
    cur[start] = min_cost;
    opts[start] = min_end.unwrap_or(start) as u32;

    // The zero-rule end is excluded from the narrowing: only the non-zero ends' minima
    // are monotone
    if start > start_lo {
        let col_hi = min_col.map_or(col_hi, |col| col + 1);
        fill_row(row, cur, opts, start_lo, start - 1, col_lo, col_hi);
    }
    if start < start_hi {
        let col_lo = min_col.unwrap_or(first_col);
        fill_row(row, cur, opts, start + 1, start_hi, col_lo, col_hi);
    }
}

/// Determine the number of items on each line that balances items across lines, such that
/// the largest line is as small as possible, with a minimum of `min_line_count` lines
/// (or one line per item if there are fewer items).
///
/// `item_sizes` must be non-empty. The returned line item counts are all non-zero and sum to
/// the number of items.
///
/// Runs in `O(line_count * item_count * log(item_count))` time using
/// `O(line_count * item_count)` transient memory.
pub(super) fn balanced_line_item_counts(
    item_sizes: impl ExactSizeIterator<Item = f32>,
    line_limit: f32,
    gap_between_items: f32,
    min_line_count: usize,
) -> Vec<usize> {
    let item_count = item_sizes.len();
    debug_assert!(item_count > 0);
    let gap_between_items = to_size(gap_between_items);
    let limit = line_limit as f64;

    let mut sums: Vec<(f64, bool)> = new_vec_with_capacity(item_count);
    let mut sum = 0.0;
    for size in item_sizes {
        let size = to_size(size);
        sum += size + gap_between_items;
        sums.push((sum, size == 0.0));
    }
    let sizes = LineSizes { sums, gap_between_items, limit };

    // `suffix_greedy[start]` is the number of lines greedy line breaking produces for items
    // `start..`, which is also the *fewest* lines the suffix can validly be divided into
    let suffix_greedy = sizes.suffix_greedy_line_counts();
    let line_count = (suffix_greedy[0] as usize).max(min_line_count.clamp(1, item_count));

    let mut item_counts: Vec<usize> = new_vec_with_capacity(line_count);
    if line_count == item_count {
        // One item per line is the only division (this covers `flex-line-count` of at least
        // the item count as well as every item overflowing a line of its own)
        item_counts.extend(core::iter::repeat(1).take(item_count));
        return item_counts;
    }

    let fit_ends = sizes.fit_ends();
    let mut nonzero_ends: Vec<u32> = new_vec_with_capacity(item_count - 1);
    for end in 0..item_count - 1 {
        if !sizes.is_zero_item(end + 1) {
            nonzero_ends.push(end as u32);
        }
    }

    // `prev[start]` is the minimum total score of any valid division of items `start..`
    // into exactly `lines - 1` lines ([`INFEASIBLE`] if there is none), and `cur` is the
    // row being computed for `lines` lines: a division into `lines` lines is a first line
    // `start..=end` plus a division of `end + 1..` into `lines - 1` lines.
    // `opts[(lines - 2) * item_count + start]` records the largest `end` achieving
    // `cur[start]`, from which the chosen division is read back.
    let mut prev: Vec<f64> = new_vec_with_capacity(item_count);
    for start in 0..item_count {
        prev.push(sizes.line_cost(start, item_count - 1));
    }
    let mut cur: Vec<f64> = new_vec_with_capacity(item_count);
    cur.extend(core::iter::repeat(INFEASIBLE).take(item_count));
    let mut opts: Vec<u32> = new_vec_with_capacity((line_count - 1) * item_count);
    opts.extend(core::iter::repeat(0).take((line_count - 1) * item_count));
    for lines in 2..=line_count {
        // The remaining `lines - 1` lines need one item each, bounding this line's end
        let max_end = item_count - lines;
        // Starts whose suffix doesn't fit in `lines` lines even with greedy breaking are
        // infeasible; they form a prefix (a longer suffix never needs fewer lines), and
        // excluding them keeps every state `fill_row` solves feasible
        let mut first_start = 0;
        while first_start < max_end && suffix_greedy[first_start] as usize > lines {
            cur[first_start] = INFEASIBLE;
            first_start += 1;
        }
        let col_hi = nonzero_ends.partition_point(|&end| (end as usize) <= max_end);
        let row = Row { sizes: &sizes, fit_ends: &fit_ends, prev: &prev, nonzero_ends: &nonzero_ends, max_end };
        let opts_row = &mut opts[(lines - 2) * item_count..(lines - 1) * item_count];
        fill_row(&row, &mut cur, opts_row, first_start, max_end, 0, col_hi);
        core::mem::swap(&mut prev, &mut cur);
    }

    // Read the division back out front to back
    debug_assert!(prev[0].is_finite());
    let mut start = 0;
    for lines in (2..=line_count).rev() {
        let end = opts[(lines - 2) * item_count + start] as usize;
        item_counts.push(end - start + 1);
        start = end + 1;
    }
    item_counts.push(item_count - start);
    item_counts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The number of lines that greedy line breaking produces: each line collects
    /// consecutive items until the next item no longer fits, and if even the first item of
    /// a line doesn't fit, that line takes just the one (overflowing) item
    fn greedy_line_count(sizes: &LineSizes) -> usize {
        let item_count = sizes.item_count();
        let mut line_count = 1;
        let mut index = 0;
        while index < item_count {
            let mut next = index;
            while next < item_count && sizes.line_size(index, next) <= sizes.limit {
                next += 1;
            }
            if next == index {
                next = index + 1;
            }
            index = next;
            if index < item_count {
                line_count += 1;
            }
        }
        line_count
    }

    /// The zero-sized-item rule, brute force: a line `start..=end` (of a division of
    /// `start..` into `lines` lines) may only end before a zero-sized item if no division
    /// that extends the line through the item exists — every extension either exceeds the
    /// limit or leaves the remaining items without a valid division into the remaining
    /// lines. (`min_errors` rows below `lines` must already be computed.)
    fn zero_boundary_valid(sizes: &LineSizes, min_errors: &[f64], lines: usize, start: usize, end: usize) -> bool {
        let item_count = sizes.item_count();
        if !sizes.is_zero_item(end + 1) {
            return true;
        }
        for glued_end in end + 1..=item_count - lines {
            if sizes.line_size(start, glued_end) > sizes.limit {
                break;
            }
            if min_errors[(lines - 2) * item_count + glued_end + 1].is_finite() {
                return false;
            }
        }
        true
    }

    /// The naive `O(line_count * item_count²)` dynamic program from the spec, used as a
    /// test oracle for the divide-and-conquer optimized implementation
    fn naive_line_item_counts(
        item_sizes: &[f32],
        line_limit: f32,
        gap_between_items: f32,
        min_line_count: usize,
    ) -> Vec<usize> {
        let item_count = item_sizes.len();
        let gap_between_items = to_size(gap_between_items);
        let limit = line_limit as f64;

        let mut sums: Vec<(f64, bool)> = new_vec_with_capacity(item_count);
        let mut sum = 0.0;
        for size in item_sizes {
            let size = to_size(*size);
            sum += size + gap_between_items;
            sums.push((sum, size == 0.0));
        }
        let sizes = LineSizes { sums, gap_between_items, limit };

        let line_count = greedy_line_count(&sizes).max(min_line_count.clamp(1, item_count));

        // `min_errors[(lines - 1) * item_count + start]` is the minimum total score of any
        // valid division of items `start..` into exactly `lines` lines
        let mut min_errors: Vec<f64> = new_vec_with_capacity(line_count * item_count);
        for start in 0..item_count {
            min_errors.push(sizes.line_cost(start, item_count - 1));
        }
        for lines in 2..=line_count {
            let row = (lines - 1) * item_count;
            for start in 0..item_count {
                let mut min_error = INFEASIBLE;
                let mut end = start;
                while end + lines <= item_count {
                    let line_cost = sizes.line_cost(start, end);
                    if line_cost == INFEASIBLE {
                        break;
                    }
                    if zero_boundary_valid(&sizes, &min_errors, lines, start, end) {
                        let error = line_cost + min_errors[row - item_count + end + 1];
                        if error < min_error {
                            min_error = error;
                        }
                    }
                    end += 1;
                }
                min_errors.push(min_error);
            }
        }

        // For each line, the *largest* end whose error plus the minimum remaining error
        // adds up to the (feasible) minimum is chosen
        assert!(min_errors[(line_count - 1) * item_count].is_finite());
        let mut item_counts: Vec<usize> = new_vec_with_capacity(line_count);
        let mut start = 0;
        for lines_after in (0..line_count).rev() {
            let target = min_errors[lines_after * item_count + start];
            let mut end = item_count - 1 - lines_after;
            loop {
                if sizes.line_cost(start, end) != INFEASIBLE
                    && (lines_after == 0 || zero_boundary_valid(&sizes, &min_errors, lines_after + 1, start, end))
                {
                    let remaining_error =
                        if lines_after == 0 { 0.0 } else { min_errors[(lines_after - 1) * item_count + end + 1] };
                    if sizes.line_cost(start, end) + remaining_error == target {
                        break;
                    }
                }
                assert!(end > start);
                end -= 1;
            }
            item_counts.push(end - start + 1);
            start = end + 1;
        }
        assert_eq!(start, item_count);
        item_counts
    }

    #[test]
    fn zero_sized_item_rule() {
        // The zero item joins the first line even though the gap it brings makes the score
        // worse: [70, 0] / [20] has line sizes 80 / 20 (score 6800), while [70] / [0, 20]
        // would have line sizes 70 / 30 (score 5800)
        let counts = balanced_line_item_counts([70.0, 0.0, 20.0].iter().copied(), 100.0, 10.0, 1);
        assert_eq!(counts, [2, 1]);
        // The zero item is not glued to a single overflowing item's line
        let counts = balanced_line_item_counts([150.0, 0.0, 30.0].iter().copied(), 100.0, 0.0, 1);
        assert_eq!(counts, [1, 2]);
        // Zero items are glued as far as the requested line count allows
        let counts = balanced_line_item_counts([70.0, 0.0, 0.0].iter().copied(), 100.0, 0.0, 2);
        assert_eq!(counts, [2, 1]);
    }

    #[test]
    fn matches_naive_dp() {
        // A simple xorshift PRNG so the test is deterministic and dependency-free
        let mut state: u64 = 0x243F6A8885A308D3;
        let mut rand = move |bound: u32| -> u32 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 32) as u32) % bound
        };

        for case in 0..5000 {
            // Mostly small item counts (quantized sizes maximize ties, exercising the
            // tie-break rules), with some larger ones exercising the recursion
            let item_count = if case % 20 == 0 { (rand(60) + 1) as usize } else { (rand(15) + 1) as usize };
            let mut item_sizes: Vec<f32> = new_vec_with_capacity(item_count);
            for _ in 0..item_count {
                let size = match rand(8) {
                    // Frequent zero items exercise the zero-sized-item rule
                    0 | 1 => 0.0,
                    2 => -10.0,
                    _ => rand(8) as f32 * 25.0,
                };
                item_sizes.push(size);
            }
            let line_limit = match rand(5) {
                0 => f32::INFINITY,
                _ => rand(10) as f32 * 30.0,
            };
            let gap_between_items = rand(4) as f32 * 5.0;
            let min_line_count = rand(item_count as u32 + 2) as usize;

            let expected = naive_line_item_counts(&item_sizes, line_limit, gap_between_items, min_line_count);
            let actual = balanced_line_item_counts(
                item_sizes.iter().copied(),
                line_limit,
                gap_between_items,
                min_line_count,
            );
            assert_eq!(
                actual, expected,
                "case {case}: item_sizes={item_sizes:?} line_limit={line_limit} \
                 gap_between_items={gap_between_items} min_line_count={min_line_count}"
            );
        }
    }
}
