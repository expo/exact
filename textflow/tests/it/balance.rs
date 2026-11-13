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
fn opportunities(text: &str, advance: f32) -> Vec<(f32, f32)> {
    let at = |chars: usize| chars as f32 * advance;
    let mut result: Vec<_> = text
        .match_indices(' ')
        .map(|(i, _)| (at(i), at(i + 1)))
        .collect();
    result.push((at(text.len()), at(text.len())));
    result
}

// The greedy line count over `opportunities`: each line ends at the last
// opportunity that fits.
fn greedy_lines(opportunities: &[(f32, f32)], width: f32) -> usize {
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
    let breaks = balanced_breaks(width, font_size, &opportunities, lines).unwrap();
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
        balanced_breaks(88.0, 16.0, &opportunities("aaaa bb cc dd", 8.0), 3),
        None
    );
}
