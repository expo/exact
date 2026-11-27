//! Canvas text (LLP 1056 D8): the CSS `font` shorthand as the canvas parses
//! and serialises it, the text engine each host supplies, and the metrics
//! `measureText` returns.
//!
//! @ref LLP 1056 D8 (text through each host's engine, measured where the
//! module's code runs)
//!
//! The recorder resolves alignment and baselines itself from one
//! measurement ([`RawMetrics`]: the run at `textAlign = "left"`,
//! `textBaseline = "alphabetic"`), with Chrome's formulas
//! (`TextMetrics::Update`, `GetFontBaseline`), so a replayer only draws a run
//! from its left end on its alphabetic baseline. The engine is the host's:
//! Core Text on Apple, Parley on Linux, the browser's own context on
//! the web; the replayer draws with the same engine, so what was measured is
//! what is drawn.

use std::fmt::Write as _;

/// `fontVariantCaps`'s values; the list's `caps` operand is the index.
pub const CAPS: [&str; 7] = [
    "normal",
    "small-caps",
    "all-small-caps",
    "petite-caps",
    "all-petite-caps",
    "unicase",
    "titling-caps",
];
/// `fontKerning`'s values.
pub const KERNING: [&str; 3] = ["auto", "normal", "none"];
/// `textRendering`'s values.
pub const RENDERING: [&str; 4] = [
    "auto",
    "optimizeSpeed",
    "optimizeLegibility",
    "geometricPrecision",
];
/// `fontStretch`'s keywords and their percentages.
pub const STRETCH: [(&str, f64); 9] = [
    ("ultra-condensed", 50.0),
    ("extra-condensed", 62.5),
    ("condensed", 75.0),
    ("semi-condensed", 87.5),
    ("normal", 100.0),
    ("semi-expanded", 112.5),
    ("expanded", 125.0),
    ("extra-expanded", 150.0),
    ("ultra-expanded", 200.0),
];
/// `textAlign`'s values.
pub const ALIGN: [&str; 5] = ["start", "end", "left", "right", "center"];
/// `textBaseline`'s values.
pub const BASELINE: [&str; 6] = [
    "top",
    "hanging",
    "middle",
    "alphabetic",
    "ideographic",
    "bottom",
];
/// `direction`'s values.
pub const DIRECTION: [&str; 3] = ["ltr", "rtl", "inherit"];

/// The generic families: written unquoted, resolved by each host.
pub const GENERIC: [&str; 13] = [
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
    "math",
    "emoji",
    "fangsong",
];

/// The size relative units resolve against: a detached canvas's default
/// font, which is what Chrome uses for `em`, `rem`, `%`, `larger` and
/// `smaller` on a canvas outside a document (LLP 1056 §8.2).
pub const BASE_SIZE: f64 = 10.0;

/// The largest size a font takes: Chrome holds a larger one to this, so it
/// draws, measures and reads back at 10000px.
pub const MAX_SIZE: f64 = 10000.0;

/// A canvas font: the `font` shorthand's result, with `fontStretch` and
/// `fontVariantCaps` (which the shorthand also sets) folded in.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// 0 normal, 1 italic, 2 oblique.
    pub style: u8,
    /// 1–1000.
    pub weight: u16,
    /// A percentage: 100 is normal.
    pub stretch: f64,
    /// Index into [`CAPS`].
    pub caps: u8,
    /// CSS px.
    pub size: f64,
    /// The family list, unquoted, in order.
    pub families: Vec<String>,
}

impl Default for Font {
    /// `10px sans-serif`.
    fn default() -> Self {
        Font {
            style: 0,
            weight: 400,
            stretch: 100.0,
            caps: 0,
            size: 10.0,
            families: vec!["sans-serif".into()],
        }
    }
}

impl Font {
    /// Chrome's serialisation: `[italic] [weight] [small-caps] <size>px
    /// <families>`, `bold` for 700, nothing for the defaults and for
    /// `fontStretch` (Chrome omits it). The size has at most six significant
    /// digits, as Chrome writes a CSS number (`10pt` is `13.3333px`).
    pub fn serialize(&self) -> String {
        let mut s = String::new();
        if self.style == 1 {
            s.push_str("italic ");
        }
        match self.weight {
            400 => {}
            700 => s.push_str("bold "),
            w => {
                let _ = write!(s, "{w} ");
            }
        }
        if self.caps == 1 {
            s.push_str("small-caps ");
        }
        let size: f64 = format!("{:.5e}", self.size).parse().unwrap_or(self.size);
        let _ = write!(s, "{}px ", exact_num::js(size));
        let families: Vec<String> = self.families.iter().map(|f| quote_family(f)).collect();
        s.push_str(&families.join(", "));
        s
    }

    /// The families joined by commas, as the list's `Font` record carries
    /// them.
    pub fn family_list(&self) -> String {
        self.families.join(",")
    }
}

/// The keywords a lone family name may not be, so Chrome quotes it.
const CSS_WIDE: [&str; 6] = [
    "inherit",
    "initial",
    "unset",
    "revert",
    "revert-layer",
    "default",
];
/// The generics Chrome's parser takes as keywords: a family of one of these
/// names in another case was written quoted, and is written quoted.
const KEYWORDS: [&str; 7] = [
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "math",
];

/// A family as Chrome writes it: bare when it is one identifier that is no
/// keyword of the property, quoted otherwise (`"Arial Black"`, `"--a"`,
/// `"-1a"`, `"Serif"`, `"inherit"`). A generic is stored lower-case, so a
/// name that differs from one only in case was written quoted.
fn quote_family(f: &str) -> String {
    let lower = f.to_ascii_lowercase();
    let keyword =
        CSS_WIDE.contains(&lower.as_str()) || (KEYWORDS.contains(&lower.as_str()) && f != lower);
    let mut chars = f.chars();
    let start = match chars.next() {
        Some('-') => chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()),
        Some(c) => !c.is_ascii_digit(),
        None => false,
    };
    let plain = !keyword
        && start
        && f.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii());
    if plain {
        f.to_string()
    } else {
        format!("\"{}\"", f.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn tokens(s: &str) -> Option<Vec<String>> {
    // Whitespace-separated, with quoted strings and `/` kept whole.
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' => {
                // A string is a token of its own (`a"b"` and `"a"b` are not
                // names); the end of the input closes it, and an escape keeps
                // its backslash for [`unescape`] (CSS Syntax 3 §4.3.5).
                if !cur.is_empty() {
                    return None;
                }
                cur.push(c);
                loop {
                    match chars.next() {
                        None => {
                            cur.push(c);
                            break;
                        }
                        Some('\\') => {
                            cur.push('\\');
                            cur.extend(chars.next());
                        }
                        Some(d) => {
                            cur.push(d);
                            if d == c {
                                break;
                            }
                        }
                    }
                }
                if chars
                    .peek()
                    .is_some_and(|n| !n.is_whitespace() && !matches!(n, ',' | '/'))
                {
                    return None;
                }
                out.push(std::mem::take(&mut cur));
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            '/' | ',' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Some(out)
}

/// A CSS length in px, with relative units against [`BASE_SIZE`].
pub fn length_px(t: &str, allow_percent: bool) -> Option<f64> {
    let t = t.to_ascii_lowercase();
    let units: [(&str, f64); 11] = [
        ("px", 1.0),
        ("pt", 4.0 / 3.0),
        ("pc", 16.0),
        ("in", 96.0),
        ("cm", 96.0 / 2.54),
        ("mm", 96.0 / 25.4),
        ("q", 96.0 / 101.6),
        ("rem", BASE_SIZE),
        ("em", BASE_SIZE),
        ("ex", BASE_SIZE / 2.0),
        ("ch", BASE_SIZE / 2.0),
    ];
    if allow_percent {
        if let Some(n) = t.strip_suffix('%') {
            return num(n).map(|v| v * BASE_SIZE / 100.0);
        }
    }
    for (u, per) in units {
        if let Some(n) = t.strip_suffix(u) {
            // `rem` before `em`: the order above.
            return num(n).map(|v| v * per);
        }
    }
    None
}

/// A number as CSS Syntax 3 reads one (§4.3.13): a sign, digits, a `.` only
/// before a digit, and an exponent of an `e` and digits, so not `1.` or `1e`.
fn num(t: &str) -> Option<f64> {
    let b = t.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i > start
    };
    let whole = digits(&mut i);
    if b.get(i) == Some(&b'.') {
        i += 1;
        if !digits(&mut i) {
            return None;
        }
    } else if !whole {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if !digits(&mut i) {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    let v = exact_num::parse_f64(t).ok()?;
    v.is_finite().then_some(v)
}

fn font_size(t: &str) -> Option<f64> {
    let keyword = match t.to_ascii_lowercase().as_str() {
        "xx-small" => Some(9.0),
        "x-small" => Some(10.0),
        "small" => Some(13.0),
        "medium" => Some(16.0),
        "large" => Some(18.0),
        "x-large" => Some(24.0),
        "xx-large" => Some(32.0),
        "xxx-large" => Some(48.0),
        "larger" => Some(BASE_SIZE * 1.2),
        "smaller" => Some(BASE_SIZE / 1.2),
        _ => None,
    };
    // A length needs a unit, except zero (CSS Values 4 §6.1).
    let zero = num(t).filter(|v| *v == 0.0).map(|_| 0.0);
    let v = keyword.or(zero).or_else(|| length_px(t, true))?;
    (v >= 0.0).then_some(v)
}

/// A string's or identifier's escapes resolved (CSS Syntax 3 §4.3.7): `\"`
/// is `"`, `\62 ` is `b`, and an escaped newline or a backslash at the end is
/// dropped.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut hex = 0u32;
        let mut n = 0;
        while n < 6 {
            let Some(d) = it.peek().and_then(|d| d.to_digit(16)) else {
                break;
            };
            hex = hex * 16 + d;
            n += 1;
            it.next();
        }
        if n == 0 {
            if let Some(d) = it.next().filter(|d| *d != '\n') {
                out.push(d);
            }
            continue;
        }
        if it.peek().is_some_and(|d| d.is_whitespace()) {
            it.next();
        }
        out.push(
            char::from_u32(hex)
                .filter(|_| hex != 0)
                .unwrap_or('\u{fffd}'),
        );
    }
    out
}

/// One word of an unquoted family name, its escapes resolved: an identifier
/// (CSS Syntax 3 §4.3.11), so not `a.b`, `3d`, `-1a` or `-`.
fn ident(w: &str) -> Option<String> {
    let b = w.as_bytes();
    let start = match b.first()? {
        b'-' => matches!(b.get(1), Some(c) if !c.is_ascii_digit()),
        c => !c.is_ascii_digit(),
    };
    // Name characters, or an escape of any one character.
    let mut it = w.chars();
    let mut chars = true;
    while let Some(c) = it.next() {
        chars &= match c {
            '\\' => it.next().is_some(),
            c => c.is_ascii_alphanumeric() || matches!(c, '-' | '_') || !c.is_ascii(),
        };
    }
    (start && chars).then(|| unescape(w))
}

fn families(toks: &[String]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut quoted: Option<String> = None;
    let mut finish = |cur: &mut Vec<&str>, quoted: &mut Option<String>| -> Option<()> {
        if let Some(q) = quoted.take() {
            if !cur.is_empty() {
                return None;
            }
            out.push(q);
            return Some(());
        }
        if cur.is_empty() {
            return None;
        }
        let words = cur.iter().map(|w| ident(w)).collect::<Option<Vec<_>>>()?;
        let first = words[0].to_ascii_lowercase();
        // A CSS-wide keyword or `default` is no name alone, and a generic
        // keyword cannot start one (`serif font`), as Chrome reads them.
        let lone = [
            "inherit",
            "initial",
            "unset",
            "revert",
            "revert-layer",
            "default",
        ];
        let generic = [
            "serif",
            "sans-serif",
            "monospace",
            "cursive",
            "fantasy",
            "system-ui",
            "math",
        ];
        if (words.len() == 1 && lone.contains(&first.as_str()))
            || (words.len() > 1 && generic.contains(&first.as_str()))
        {
            return None;
        }
        let name = words.join(" ");
        let lower = name.to_ascii_lowercase();
        out.push(if GENERIC.contains(&lower.as_str()) {
            lower
        } else {
            name
        });
        cur.clear();
        Some(())
    };
    for t in toks {
        if t == "," {
            finish(&mut cur, &mut quoted)?;
            continue;
        }
        if t.starts_with('"') || t.starts_with('\'') {
            if !cur.is_empty() || quoted.is_some() || t.len() < 2 {
                return None;
            }
            quoted = Some(unescape(&t[1..t.len() - 1]));
            continue;
        }
        if quoted.is_some() || t == "/" {
            return None;
        }
        cur.push(t);
    }
    finish(&mut cur, &mut quoted)?;
    Some(out)
}

#[cfg(test)]
mod family_tests {
    use super::*;

    /// Family names as Chrome reads them (headless Chrome, 2026-11-27).
    #[test]
    fn family_names_are_read_as_chrome_reads_them() {
        let f = |v: &str| parse(v).map(|f| f.families);
        assert_eq!(f("12px 'a\\'b'"), Some(vec!["a'b".to_string()]));
        assert_eq!(f("12px \"a\\\\b\""), Some(vec!["a\\b".to_string()]));
        assert_eq!(f("12px 'Open Sans"), Some(vec!["Open Sans".to_string()]));
        assert_eq!(f("12px a\\.b"), Some(vec!["a.b".to_string()]));
        assert_eq!(f("12px a inherit"), Some(vec!["a inherit".to_string()]));
        assert_eq!(f("12px x serif"), Some(vec!["x serif".to_string()]));
        for bad in [
            "12px a.b",
            "12px a!b",
            "12px 'a'b",
            "12px a'b'",
            "12px 'a' 'b'",
            "12px -1a",
            "12px -",
            "12px inherit",
            "12px revert-layer",
            "12px serif font",
            "12px Math x",
        ] {
            assert_eq!(f(bad), None, "{bad}");
        }
        assert_eq!(
            parse("12px \"a\\\\b\"").unwrap().serialize(),
            "12px \"a\\\\b\""
        );
    }
}

/// Parse the CSS `font` shorthand as a canvas `font` assignment does, or
/// `None` when it does not parse (the assignment is then ignored).
pub fn parse(input: &str) -> Option<Font> {
    let toks = tokens(input.trim())?;
    let mut font = Font {
        families: Vec::new(),
        ..Font::default()
    };
    let (mut style, mut variant, mut weight, mut stretch) = (false, false, false, false);
    let mut i = 0;
    // Up to four optional properties, in any order, each at most once.
    while i < toks.len() && i < 4 {
        let t = toks[i].to_ascii_lowercase();
        if t == "normal" {
            i += 1;
            continue;
        }
        if !style && (t == "italic" || t == "oblique") {
            style = true;
            font.style = if t == "italic" { 1 } else { 2 };
            i += 1;
            // `oblique <angle>`.
            if t == "oblique" {
                if let Some(next) = toks.get(i) {
                    let n = next.to_ascii_lowercase();
                    if ["deg", "grad", "rad", "turn"]
                        .iter()
                        .any(|u| n.strip_suffix(u).and_then(num).is_some())
                    {
                        i += 1;
                    }
                }
            }
            continue;
        }
        if !variant && t == "small-caps" {
            variant = true;
            font.caps = 1;
            i += 1;
            continue;
        }
        if !weight {
            let w = match t.as_str() {
                "bold" | "bolder" => Some(700),
                "lighter" => Some(100),
                // Chrome reads `1e2` as 100 and writes `500.6` as 500.
                _ => num(&t)
                    .filter(|v| (1.0..=1000.0).contains(v))
                    .map(|v| v.trunc() as u16),
            };
            if let Some(w) = w {
                weight = true;
                font.weight = w;
                i += 1;
                continue;
            }
        }
        if !stretch {
            if let Some((_, pct)) = STRETCH.iter().find(|(k, _)| *k == t) {
                stretch = true;
                font.stretch = *pct;
                i += 1;
                continue;
            }
        }
        break;
    }
    font.size = font_size(toks.get(i)?)?.min(MAX_SIZE);
    i += 1;
    if toks.get(i).map(String::as_str) == Some("/") {
        let lh = toks.get(i + 1)?;
        let lower = lh.to_ascii_lowercase();
        if lower != "normal" && num(&lower).is_none() && length_px(&lower, true).is_none() {
            return None;
        }
        i += 2;
    }
    font.families = families(toks.get(i..)?)?;
    Some(font)
}

/// One run to measure: a font, the text (whitespace already normalised),
/// and the text attributes that change its shape.
#[derive(Debug, Clone, Copy)]
pub struct TextRun<'a> {
    /// The font.
    pub font: &'a Font,
    /// The text.
    pub text: &'a str,
    /// Right-to-left base direction.
    pub rtl: bool,
    /// `letterSpacing` in px.
    pub letter_spacing: f64,
    /// `wordSpacing` in px.
    pub word_spacing: f64,
    /// Index into [`KERNING`].
    pub kerning: u8,
}

/// A run's measurement at `textAlign = "left"`, `textBaseline =
/// "alphabetic"`, in CSS px: what each host's engine answers.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RawMetrics {
    /// The advance.
    pub width: f64,
    /// `actualBoundingBoxLeft`: the ink's reach left of the start.
    pub left: f64,
    /// `actualBoundingBoxRight`.
    pub right: f64,
    /// `actualBoundingBoxAscent`: the ink above the baseline.
    pub ascent: f64,
    /// `actualBoundingBoxDescent`.
    pub descent: f64,
    /// The font's ascent (`fontBoundingBoxAscent`).
    pub font_ascent: f64,
    /// The font's descent.
    pub font_descent: f64,
    /// The em box's top above the baseline: the typographic ascent
    /// normalised to the em (Chrome's `NormalizedTypoAscent`).
    pub em_ascent: f64,
    /// The em box's bottom below it.
    pub em_descent: f64,
    /// The hanging baseline above the alphabetic one.
    pub hanging: f64,
    /// The ideographic baseline above it (negative: below).
    pub ideographic: f64,
}

impl RawMetrics {
    /// The eleven numbers in field order, as a seam carries them.
    pub fn to_array(self) -> [f64; 11] {
        [
            self.width,
            self.left,
            self.right,
            self.ascent,
            self.descent,
            self.font_ascent,
            self.font_descent,
            self.em_ascent,
            self.em_descent,
            self.hanging,
            self.ideographic,
        ]
    }

    /// From [`RawMetrics::to_array`]'s order; non-finite values are 0.
    pub fn from_slice(v: &[f64]) -> RawMetrics {
        let at = |i: usize| v.get(i).copied().filter(|x| x.is_finite()).unwrap_or(0.0);
        RawMetrics {
            width: at(0),
            left: at(1),
            right: at(2),
            ascent: at(3),
            descent: at(4),
            font_ascent: at(5),
            font_descent: at(6),
            em_ascent: at(7),
            em_descent: at(8),
            hanging: at(9),
            ideographic: at(10),
        }
    }
}

/// A host's text engine, callable from the executor's thread (LLP 1056 D8:
/// never a synchronous call into the UI thread).
pub trait TextEngine: Send + Sync {
    /// Measure one run.
    fn measure(&self, run: &TextRun<'_>) -> RawMetrics;
}

/// The engine when a host supplies none (tests, bake): half an em per
/// character, Helvetica-like vertical metrics. Its alignment is right for
/// the runs it measures; its widths are not a font's.
#[derive(Debug, Default, Clone, Copy)]
pub struct Estimate;

impl TextEngine for Estimate {
    fn measure(&self, run: &TextRun<'_>) -> RawMetrics {
        let size = run.font.size;
        let n = run.text.chars().count() as f64;
        let spaces = run.text.chars().filter(|c| *c == ' ').count() as f64;
        let width = n * size * 0.5 + n * run.letter_spacing + spaces * run.word_spacing;
        RawMetrics {
            width,
            left: 0.0,
            right: width,
            ascent: if n > 0.0 { size * 0.7 } else { 0.0 },
            descent: 0.0,
            font_ascent: size,
            font_descent: size * 0.2,
            em_ascent: size * 0.8,
            em_descent: size * 0.2,
            hanging: size * 0.8,
            ideographic: -size * 0.2,
        }
    }
}

/// `TextMetrics`, as `measureText` returns it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextMetrics {
    /// `width`.
    pub width: f64,
    /// `actualBoundingBoxLeft`.
    pub actual_bounding_box_left: f64,
    /// `actualBoundingBoxRight`.
    pub actual_bounding_box_right: f64,
    /// `fontBoundingBoxAscent`.
    pub font_bounding_box_ascent: f64,
    /// `fontBoundingBoxDescent`.
    pub font_bounding_box_descent: f64,
    /// `actualBoundingBoxAscent`.
    pub actual_bounding_box_ascent: f64,
    /// `actualBoundingBoxDescent`.
    pub actual_bounding_box_descent: f64,
    /// `emHeightAscent`.
    pub em_height_ascent: f64,
    /// `emHeightDescent`.
    pub em_height_descent: f64,
    /// `hangingBaseline`.
    pub hanging_baseline: f64,
    /// `alphabeticBaseline`.
    pub alphabetic_baseline: f64,
    /// `ideographicBaseline`.
    pub ideographic_baseline: f64,
}

/// Where the alphabetic baseline sits relative to the `textBaseline` point
/// (Chrome's `GetFontBaseline`): `y + baseline_shift` is the baseline.
pub fn baseline_shift(raw: &RawMetrics, baseline: u8) -> f64 {
    match baseline {
        0 => raw.em_ascent,                          // top
        1 => raw.hanging,                            // hanging
        2 => (raw.em_ascent - raw.em_descent) / 2.0, // middle
        4 => raw.ideographic,                        // ideographic
        5 => -raw.em_descent,                        // bottom
        _ => 0.0,                                    // alphabetic
    }
}

/// How far left of the alignment point the run starts, as a fraction of its
/// width: 0 for left, 1 for right, ½ for center; `start` and `end` by the
/// direction.
pub fn align_fraction(align: u8, rtl: bool) -> f64 {
    match (align, rtl) {
        (4, _) => 0.5,
        (3, _) | (0, true) | (1, false) => 1.0,
        _ => 0.0,
    }
}

/// `measureText`'s answer for a run measured as `raw`.
pub fn metrics(raw: &RawMetrics, align: u8, baseline: u8, rtl: bool) -> TextMetrics {
    let dx = raw.width * align_fraction(align, rtl);
    let b = baseline_shift(raw, baseline);
    TextMetrics {
        width: raw.width,
        actual_bounding_box_left: raw.left + dx,
        actual_bounding_box_right: raw.right - dx,
        font_bounding_box_ascent: raw.font_ascent - b,
        font_bounding_box_descent: raw.font_descent + b,
        actual_bounding_box_ascent: raw.ascent - b,
        actual_bounding_box_descent: raw.descent + b,
        em_height_ascent: raw.em_ascent - b,
        em_height_descent: raw.em_descent + b,
        hanging_baseline: raw.hanging - b,
        alphabetic_baseline: -b,
        ideographic_baseline: raw.ideographic - b,
    }
}

/// The spec's text preparation: every ASCII whitespace character becomes a
/// space, and nothing collapses.
pub fn prepare(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '\t' | '\n' | '\u{c}' | '\r') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod size_tests {
    use super::*;

    /// Chrome holds a font's size to 10000px (headless Chrome, 2026-11-27).
    #[test]
    fn a_size_is_held_to_ten_thousand_pixels() {
        let size = |v: &str| parse(v).map(|f| f.size);
        assert_eq!(size("20000px a"), Some(10000.0));
        assert_eq!(size("1e9px a"), Some(10000.0));
        assert_eq!(size("9999.5px a"), Some(9999.5));
        assert_eq!(parse("20000px a").unwrap().serialize(), "10000px a");
        // Measured, and so drawn, at the held size: the estimate is half an
        // em a character.
        let ctx = crate::Context2d::new();
        ctx.set_font("20000px a");
        assert_eq!(ctx.measure_text("ab").unwrap().width, 10000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A family is bare only when it is one identifier and no keyword of the
    /// property (headless Chrome, 2026-11-27).
    #[test]
    fn a_family_is_quoted_as_chrome_quotes_it() {
        let s = |v: &str| parse(v).map(|f| f.serialize());
        assert_eq!(s("12px 'Serif'").as_deref(), Some("12px \"Serif\""));
        assert_eq!(s("12px SERIF").as_deref(), Some("12px serif"));
        assert_eq!(s("12px 'inherit'").as_deref(), Some("12px \"inherit\""));
        assert_eq!(
            s("12px 'Revert-Layer'").as_deref(),
            Some("12px \"Revert-Layer\"")
        );
        assert_eq!(s("12px --a").as_deref(), Some("12px \"--a\""));
        assert_eq!(s("12px '-1a'").as_deref(), Some("12px \"-1a\""));
        assert_eq!(s("12px '-'").as_deref(), Some("12px \"-\""));
        assert_eq!(s("12px -a, _1, é").as_deref(), Some("12px -a, _1, é"));
        assert_eq!(s("12px 'Arial'").as_deref(), Some("12px Arial"));
    }

    #[test]
    fn the_shorthand_serialises_as_chrome_does() {
        let s = |v: &str| parse(v).map(|f| f.serialize());
        assert_eq!(s("bold 20px Arial").as_deref(), Some("bold 20px Arial"));
        assert_eq!(
            s("italic small-caps 700 15px/2 serif").as_deref(),
            Some("italic bold small-caps 15px serif")
        );
        assert_eq!(
            s("12pt Georgia, 'Times New Roman', serif").as_deref(),
            Some("16px Georgia, \"Times New Roman\", serif")
        );
        assert_eq!(s("2em serif").as_deref(), Some("20px serif"));
        assert_eq!(s("larger serif").as_deref(), Some("12px serif"));
        assert_eq!(
            s("bold italic 1.5rem \"Inter\"").as_deref(),
            Some("italic bold 15px Inter")
        );
        assert_eq!(
            s("condensed 12px sans-serif").as_deref(),
            Some("12px sans-serif")
        );
        assert_eq!(s("oblique 10deg 12px x").as_deref(), Some("12px x"));
        assert_eq!(
            s("normal normal 400 normal 12px a").as_deref(),
            Some("12px a")
        );
        assert_eq!(s("12px"), None);
        assert_eq!(s("bad"), None);
        assert_eq!(s("100 12px a").as_deref(), Some("100 12px a"));
        assert_eq!(s("x-small serif").as_deref(), Some("10px serif"));
        assert_eq!(s("12.5px a").as_deref(), Some("12.5px a"));
        assert_eq!(
            s("italic 12px 'My Font'").as_deref(),
            Some("italic 12px \"My Font\"")
        );
        assert_eq!(parse("condensed 12px a").unwrap().stretch, 75.0);
    }

    /// Chrome writes the size as a CSS number, six significant digits at
    /// most (headless Chrome, 2026-11-26); the font keeps the exact size.
    #[test]
    fn the_size_serialises_with_six_significant_digits() {
        let s = |v: &str| parse(v).map(|f| f.serialize());
        assert_eq!(s("10pt a").as_deref(), Some("13.3333px a"));
        assert_eq!(s("smaller a").as_deref(), Some("8.33333px a"));
        assert_eq!(s("12.3456789px a").as_deref(), Some("12.3457px a"));
        assert_eq!(s("12.000001px a").as_deref(), Some("12px a"));
        assert_eq!(s("0px a").as_deref(), Some("0px a"));
        assert!((parse("10pt a").unwrap().size - 40.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn metrics_follow_chrome_for_top_and_center() {
        // Chrome's 10px sans-serif on this Mac, "Hi there".
        let raw = RawMetrics {
            width: 23.0146484375,
            left: -0.7861328125 + 0.0,
            right: 24.080078125,
            ascent: 7.197265625,
            descent: 0.185546875,
            font_ascent: 10.0,
            font_descent: 2.0,
            em_ascent: 8.15625,
            em_descent: 1.84375,
            hanging: 8.0,
            ideographic: -2.0,
        };
        let m = metrics(&raw, 4, 0, false);
        assert!((m.font_bounding_box_ascent - 1.84375).abs() < 1e-9);
        assert!((m.alphabetic_baseline + 8.15625).abs() < 1e-9);
        assert!((m.hanging_baseline + 0.15625).abs() < 1e-9);
        assert!((m.ideographic_baseline + 10.15625).abs() < 1e-9);
        assert!((m.actual_bounding_box_right - (24.080078125 - 23.0146484375 / 2.0)).abs() < 1e-9);
    }

    /// Numbers as CSS reads them (headless Chrome, 2026-11-27): a unitless
    /// zero is a size, a weight may have an exponent and loses its fraction,
    /// and `1.` is not a number.
    #[test]
    fn the_shorthand_reads_numbers_as_css_does() {
        let s = |v: &str| parse(v).map(|f| f.serialize());
        assert_eq!(s("0 serif").as_deref(), Some("0px serif"));
        assert_eq!(s("-0 serif").as_deref(), Some("0px serif"));
        assert_eq!(s("1e2 12px a").as_deref(), Some("100 12px a"));
        assert_eq!(s("1E+2 12px a").as_deref(), Some("100 12px a"));
        assert_eq!(s("500.6 12px a").as_deref(), Some("500 12px a"));
        assert_eq!(s("999.9 12px a").as_deref(), Some("999 12px a"));
        assert_eq!(s("1.px a"), None);
        assert_eq!(s("100. 12px a"), None);
        assert_eq!(s("12px/1. a"), None);
        assert_eq!(s("0 0 a"), None);
    }
}
