use crate::support::prepare;
use exact_textflow::{balanced_breaks, balanced_width, Options, BALANCE_MAX_LINES};

// Eight units a character: "aaaa bb cc dd" greedy at 88 is "aaaa bb cc " /
// "dd"; balanced it is two lines of seven and five characters.
#[test]
fn balance_finds_the_narrowest_width_that_keeps_the_line_count() {
    let p = prepare("aaaa bb cc dd", Options::default());
    assert_eq!(p.count_lines(88.0), 2);
    let width = balanced_width(88.0, |w| p.count_lines(w)).unwrap();
    assert_eq!(width, 56.0, "\"aaaa bb\" is the widest line that keeps two");
    assert_eq!(p.count_lines(width), 2);
    assert_eq!(p.count_lines(width - 1.0 / 64.0), 3);
}

#[test]
fn one_line_and_more_than_six_stay_greedy() {
    let short = prepare("aa bb", Options::default());
    assert_eq!(balanced_width(88.0, |w| short.count_lines(w)), None);
    let words = ["aa"; 2 * (BALANCE_MAX_LINES + 1)].join(" ");
    let long = prepare(&words, Options::default());
    assert_eq!(long.count_lines(40.0), BALANCE_MAX_LINES + 1);
    assert_eq!(balanced_width(40.0, |w| long.count_lines(w)), None);
    let six = ["aa"; 2 * BALANCE_MAX_LINES].join(" ");
    let six = prepare(&six, Options::default());
    assert_eq!(six.count_lines(40.0), BALANCE_MAX_LINES);
    assert!(balanced_width(40.0, |w| six.count_lines(w)).is_some());
    assert_eq!(balanced_width(f32::INFINITY, |_| 2), None);
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
    let breaks = balanced_breaks(width, font_size, false, &opportunities, lines).unwrap();
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
        balanced_breaks(88.0, 16.0, false, &opportunities("aaaa bb cc dd", 8.0), 3),
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

// A line fits when at most 1/64 px wider than the box, from unrounded
// positions, as Blink's AvailableWidthToFit: two words a line at 61px while a
// pair measures 61 + 1/128 or 61 + 1/64, not at 61 + 5/256.
#[test]
fn balance_fits_lines_a_layout_unit_wider_than_the_box() {
    assert_eq!(
        balanced_breaks(61.0, 16.0, false, &four_words(30.0 + 1.0 / 256.0), 2),
        Some(vec![1])
    );
    assert_eq!(
        balanced_breaks(61.0, 16.0, false, &four_words(30.0 + 1.0 / 128.0), 2),
        Some(vec![1])
    );
    assert_eq!(
        balanced_breaks(61.0, 16.0, false, &four_words(30.0 + 5.0 / 512.0), 2),
        None
    );
}

// Blink scores only five candidates or more, its start sentinel included,
// and bisects below that (ScoreLineBreaker::Optimize, then
// LineBreakStrategy::Balance): "aa aa aa" at 5px keeps "aa aa / aa" where
// scoring, with its orphans penalty, would take "aa / aa aa".
#[test]
fn balance_bisects_fewer_than_four_opportunities() {
    let three = opportunities("aa aa aa", 1.0);
    assert_eq!(greedy_lines(&three, 5.0), 2);
    assert_eq!(balanced_breaks(5.0, 16.0, false, &three, 2), None);
    assert_eq!(balanced_width(5.0, |w| greedy_lines(&three, w)), Some(5.0));
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
    assert_eq!(balanced_breaks(64.0, 16.0, false, &soft, 2), Some(vec![2]));
    // Justified, the hyphen costs a quarter as much (512) and lines nothing:
    // "aa a- / a aa aa" scores 576.75 + 512 + 64.25, under the 1,153.5 of
    // "aa aa / aa aa".
    assert_eq!(balanced_breaks(64.0, 16.0, true, &soft, 2), Some(vec![1]));
}

// Scores are f32, as Blink's LineBreakScore: breaking after "aa aa" or after
// "aa aa aa" differs by about 6e-6 in f64, which f32 rounds to a tie, so the
// later break wins. 14 - 2^-20 is the f32 nearest 13.999999.
#[test]
fn balance_scores_in_f32() {
    let end = 14.0 - 1.0 / 1_048_576.0;
    let near = [
        (2.0, 3.0, false),
        (5.0, 6.0, false),
        (8.0, 9.0, false),
        (11.0, 12.0, false),
        (end, end, false),
    ];
    assert_eq!(balanced_breaks(8.0, 16.0, false, &near, 2), Some(vec![2]));
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
    assert_eq!(balanced_breaks(5.0, 1.0, false, &same, 2), Some(vec![0]));
}
