use crate::support::prepare;
use exact_textflow::{balanced_breaks, balanced_width, Options, BALANCE_MAX_LINES};

// Blink bisects between 0.8 of the average greedy line and the box, to
// within 1px. Eight units a character, "aaaa bb cc dd" at 88 is "aaaa bb cc"
// / "dd" greedy; two lines need 56 ("aaaa bb"), and the search stops at
// 56 + 14/64.
#[test]
fn balance_bisects_to_within_a_pixel_of_the_narrowest_width() {
    let p = prepare("aaaa bb cc dd", Options::default());
    assert_eq!(p.count_lines(88.0), 2);
    let width = balanced_width(88.0, 2, 80.0 + 16.0, |w| p.count_lines(w) <= 2).unwrap();
    assert_eq!(width, 56.0 + 14.0 / 64.0);
    assert_eq!(p.count_lines(56.0), 2);
    assert_eq!(p.count_lines(56.0 - 1.0 / 64.0), 3);
}

// The 1px stop is Blink's, and it shows: words of 11, 10.5 and 10.8 with 1px
// spaces fit two lines from 22.3 ("11" / "10.5 10.8"), but the search from
// a 30px box stops at 22.6875, where the greedy "11 10.5" / "10.8" still
// fits.
#[test]
fn balance_bisection_stops_a_pixel_short_of_the_narrowest_breaks() {
    let words = [
        (11.0, 12.0, false),
        (22.5, 23.5, false),
        (34.3, 34.3, false),
    ];
    assert_eq!(greedy_lines(&words, 30.0), 2);
    assert_eq!(greedy_lines(&words, 22.3), 2);
    assert_eq!(greedy_lines(&words, 22.3 - 1.0 / 64.0), 3);
    let width = balanced_width(30.0, 2, 22.5 + 10.8, |w| greedy_lines(&words, w) <= 2);
    assert_eq!(width, Some(22.6875));
}

#[test]
fn one_line_and_more_than_six_stay_greedy() {
    assert_eq!(balanced_width(88.0, 1, 40.0, |_| true), None);
    assert_eq!(
        balanced_width(40.0, BALANCE_MAX_LINES + 1, 200.0, |_| true),
        None
    );
    assert!(balanced_width(40.0, BALANCE_MAX_LINES, 200.0, |_| true).is_some());
    assert_eq!(balanced_width(f32::INFINITY, 2, 40.0, |_| true), None);
    // No narrower width fits: the paragraph stays as it breaks at the box's.
    assert_eq!(balanced_width(40.0, 2, 70.0, |_| false), None);
}

// The break opportunities of space-separated `text` at `advance` a character:
// where a line ending at each space ends and where the next line starts, then
// the paragraph's end.
fn opportunities(text: &str, advance: f32) -> Vec<(f32, f32, bool)> {
    let at = |chars: usize| chars as f32 * advance;
    let mut result: Vec<_> = text
        .match_indices(' ')
        .map(|(i, _)| (at(i), at(i + 1), false))
        .collect();
    result.push((at(text.len()), at(text.len()), false));
    result
}

// The greedy line count over `opportunities`: each line ends at the last
// opportunity that fits.
fn greedy_lines(opportunities: &[(f32, f32, bool)], width: f32) -> usize {
    let (mut count, mut start, mut i) = (0, 0.0, 0);
    while i < opportunities.len() {
        let mut last = i;
        while last + 1 < opportunities.len() && opportunities[last + 1].0 - start <= width {
            last += 1;
        }
        count += 1;
        start = opportunities[last].1;
        i = last + 1;
    }
    count
}

fn balanced_lines(text: &str, advance: f32, width: f32, font_size: f32) -> Vec<&str> {
    let spaces: Vec<usize> = text.match_indices(' ').map(|(i, _)| i).collect();
    let opportunities = opportunities(text, advance);
    let lines = greedy_lines(&opportunities, width);
    let breaks = balanced_breaks(width, 0.0, font_size, false, &opportunities, lines).unwrap();
    assert_eq!(breaks.len() + 1, lines);
    let mut result = Vec::new();
    let mut start = 0;
    for b in breaks {
        result.push(&text[start..spaces[b]]);
        start = spaces[b] + 1;
    }
    result.push(&text[start..]);
    result
}

// Chrome (Edge 157) balances with Blink's ScoreLineBreaker, not at the
// narrowest width that keeps the line count. 10px Menlo advances 6.02 a
// character; at the narrowest width these would be "Seven studies / in live /
// typography" and "A road / that ended / at the sea".
#[test]
fn balance_breaks_as_chrome_does_not_at_the_narrowest_width() {
    let seven = "Seven studies in live typography";
    assert_eq!(greedy_lines(&opportunities(seven, 6.02), 96.0), 3);
    assert_eq!(
        balanced_lines(seven, 6.02, 96.0, 10.0),
        ["Seven", "studies in", "live typography"]
    );
    let road = "A road that ended at the sea";
    assert_eq!(greedy_lines(&opportunities(road, 6.02), 90.0), 3);
    assert_eq!(
        balanced_lines(road, 6.02, 90.0, 10.0),
        ["A road that", "ended at", "the sea"]
    );
    let p = prepare("aaaa bb cc dd", Options::default());
    assert_eq!(
        greedy_lines(&opportunities("aaaa bb cc dd", 8.0), 88.0),
        p.count_lines(88.0)
    );
    assert_eq!(
        balanced_lines("aaaa bb cc dd", 8.0, 88.0, 16.0),
        ["aaaa bb", "cc dd"]
    );
    // A line count other than the greedy one keeps the greedy breaks.
    assert_eq!(
        balanced_breaks(
            88.0,
            0.0,
            16.0,
            false,
            &opportunities("aaaa bb cc dd", 8.0),
            3
        ),
        None
    );
}

// Equal scores go to the later break, as in Blink's ComputeScores (`<=`):
// "aa aa / aa aa aa" and "aa aa aa / aa aa" each leave 3 and 0 unused.
#[test]
fn balance_ties_go_to_the_later_break() {
    assert_eq!(
        balanced_lines("aa aa aa aa aa", 1.0, 8.0, 1.0),
        ["aa aa aa", "aa aa"]
    );
}

// Blink's text positions are layout units, each the advances before it
// summed and rounded up to 1/64 px (ShapeResult::CachedPositionForOffset), so
// equal words are not equally wide everywhere and breakings that would tie
// do not. Chromium 159 leaves the odd word of nine or eleven alone on the
// line before the last in 10px Menlo (1233/2048 em a character) at 32px;
// unrounded, both leave it on the third. In 10px DejaVu Sans ("a" 1255/2048
// em, a space 651/2048) at 34px it is on the first line of five words and
// the fourth of eleven; unrounded, the last but one of each.
#[test]
fn balance_rounds_positions_up_to_layout_units() {
    let pair = "aa aa";
    let menlo = 12330.0 / 2048.0;
    assert_eq!(
        balanced_lines(&["aa"; 9].join(" "), menlo, 32.0, 10.0),
        [pair, pair, pair, "aa", pair]
    );
    assert_eq!(
        balanced_lines(&["aa"; 11].join(" "), menlo, 32.0, 10.0),
        [pair, pair, pair, pair, "aa", pair]
    );
    let dejavu = |words: usize| {
        let (mut position, mut result) = (0.0, Vec::new());
        for word in 0..words {
            position += 2.0 * 12550.0 / 2048.0;
            let end = position;
            if word + 1 < words {
                position += 6510.0 / 2048.0;
            }
            result.push((end, position, false));
        }
        balanced_breaks(34.0, 0.0, 10.0, false, &result, words.div_ceil(2))
    };
    assert_eq!(dejavu(5), Some(vec![0, 2]));
    assert_eq!(dejavu(11), Some(vec![1, 3, 5, 6, 8]));
}

// Four words of `word` px with 1px spaces between them.
fn four_words(word: f32) -> Vec<(f32, f32, bool)> {
    (1..=4)
        .map(|i| {
            (
                i as f32 * (word + 1.0) - 1.0,
                i as f32 * (word + 1.0),
                false,
            )
        })
        .collect()
}

// A line fits when at most 1/64 px wider than the box, between positions
// rounded up to 1/64 px, as Blink's AvailableWidthToFit: two words a line at
// 61px while a pair measures 61 + 1/128 or 61 + 1/64, not at 61 + 5/256.
#[test]
fn balance_fits_lines_a_layout_unit_wider_than_the_box() {
    assert_eq!(
        balanced_breaks(61.0, 0.0, 16.0, false, &four_words(30.0 + 1.0 / 256.0), 2),
        Some(vec![1])
    );
    assert_eq!(
        balanced_breaks(61.0, 0.0, 16.0, false, &four_words(30.0 + 1.0 / 128.0), 2),
        Some(vec![1])
    );
    assert_eq!(
        balanced_breaks(61.0, 0.0, 16.0, false, &four_words(30.0 + 5.0 / 512.0), 2),
        None
    );
}

// Blink scores only five candidates or more, its start sentinel included,
// and bisects below that (ScoreLineBreaker::Optimize, then
// LineBreakStrategy::Balance): "aa aa aa" at 5px keeps "aa aa / aa" where
// scoring, with its orphans penalty, would take "aa / aa aa". No width
// within 1px of the box fits two lines, so the bisection keeps them too.
#[test]
fn balance_bisects_fewer_than_four_opportunities() {
    let three = opportunities("aa aa aa", 1.0);
    assert_eq!(greedy_lines(&three, 5.0), 2);
    assert_eq!(balanced_breaks(5.0, 0.0, 16.0, false, &three, 2), None);
    assert_eq!(
        balanced_width(5.0, 2, 5.0 + 2.0, |w| greedy_lines(&three, w) <= 2),
        None
    );
}

// A soft hyphen's break costs Blink's hyphen penalty, 2 x width x font size
// (SetupParameters, LineBreaker::AppendCandidates), and its line ends past
// the hyphen's advance. "aa a\u{ad}a aa aa" at 8px a character and hyphen
// balances to "aa aa / aa aa" at 64px, not "aa a- / a aa aa".
#[test]
fn balance_penalizes_hyphenated_breaks() {
    let soft = [
        (16.0, 24.0, false),
        (40.0, 32.0, true),
        (40.0, 48.0, false),
        (64.0, 72.0, false),
        (88.0, 88.0, false),
    ];
    assert_eq!(
        balanced_breaks(64.0, 0.0, 16.0, false, &soft, 2),
        Some(vec![2])
    );
    // Justified, the hyphen costs a quarter as much (512) and lines nothing:
    // "aa a- / a aa aa" scores 576.75 + 512 + 64.25, under the 1,153.5 of
    // "aa aa / aa aa".
    assert_eq!(
        balanced_breaks(64.0, 0.0, 16.0, true, &soft, 2),
        Some(vec![1])
    );
}

// Scores are f32, as Blink's LineBreakScore: breaking after "aa aa" scores
// 6/64 less than after "aa aa aa", which f32 loses beside a line penalty of
// 2^22 (a font size of 2^17 in an 8px box), so the tie goes to the later
// break.
#[test]
fn balance_scores_in_f32() {
    let end = 14.0 - 1.0 / 64.0;
    let near = [
        (2.0, 3.0, false),
        (5.0, 6.0, false),
        (8.0, 9.0, false),
        (11.0, 12.0, false),
        (end, end, false),
    ];
    assert_eq!(
        balanced_breaks(8.0, 0.0, 16.0, false, &near, 2),
        Some(vec![1])
    );
    assert_eq!(
        balanced_breaks(8.0, 0.0, 131_072.0, false, &near, 2),
        Some(vec![2])
    );
}

// Coincident breaks, as after a space and an empty inline item, keep the
// first: ComputeScores skips a start whose score plus the last scored line's
// unused width squared (`best_hope`) already reaches the best, so the tie
// never reaches `<=`.
#[test]
fn balance_keeps_the_first_of_coincident_breaks() {
    let same = [
        (1.0, 2.0, false),
        (1.0, 2.0, false),
        (1.0, 2.0, false),
        (3.0, 4.0, false),
        (6.0, 6.0, false),
    ];
    assert_eq!(
        balanced_breaks(5.0, 0.0, 1.0, false, &same, 2),
        Some(vec![0])
    );
}

// The first line's `text-indent` comes off its room alone
// (ScoreLineBreaker::AvailableWidth): six words of two 1px characters in an
// 8px box break "aa aa aa / aa aa aa"; indented by 3px the first line holds
// two words, and the three lines even out as two words each.
#[test]
fn balance_takes_the_indent_from_the_first_line() {
    let six = opportunities("aa aa aa aa aa aa", 1.0);
    assert_eq!(
        balanced_breaks(8.0, 0.0, 1.0, false, &six, 2),
        Some(vec![2])
    );
    assert_eq!(balanced_breaks(8.0, 3.0, 1.0, false, &six, 2), None);
    assert_eq!(
        balanced_breaks(8.0, 3.0, 1.0, false, &six, 3),
        Some(vec![1, 3])
    );
}
