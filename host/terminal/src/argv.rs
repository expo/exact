//! The argument list as a URL (LLP 1101.003 D3): one left-to-right scan
//! that takes the host's flags out and makes the rest the launch location.
//! A word starting with `/` is the location itself; any other word is one
//! path segment; `--name value` is the query parameter `name=value`.

use crate::color::ColorDepth;
use crate::ending;
use crate::host::Mode;

/// What `--color` asked for: `auto` (SGR to a terminal, plain otherwise),
/// or SGR forced at a palette, or plain forced (`never`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Color {
    Auto,
    Depth(ColorDepth),
}

/// One command line, scanned.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Line {
    /// `--print`.
    pub print: bool,
    /// `--json`.
    pub json: bool,
    /// `-h`, `--help`.
    pub help: bool,
    pub color: Option<Color>,
    /// `--columns N`.
    pub columns: Option<usize>,
    /// `--timeout S`, in seconds; zero disables the deadline.
    pub timeout: Option<f64>,
    /// `--size CxR`.
    pub size: Option<(usize, usize)>,
    /// `--inline` or `--fullscreen`.
    pub mode: Option<Mode>,
    /// Everything after `--drive`: the agent's headless operations.
    pub drive: Option<Vec<String>>,
    /// The `/…` word, if one was given.
    location: Option<String>,
    /// The other positional words, each one path segment, encoded.
    segments: Vec<String>,
    /// The query parameters, in order, encoded.
    query: Vec<(String, String)>,
}

/// `COLSxROWS`.
pub(crate) fn parse_size(s: &str) -> Option<(usize, usize)> {
    let (c, r) = s.split_once('x')?;
    Some((c.parse().ok()?, r.parse().ok()?))
}

impl Line {
    /// The launch location the words make: the `/…` word through
    /// `location_of` (LLP 1038 D8), else `/` and the segments, then the
    /// query flags after the location's own query, before any fragment, so
    /// `searchParam` finds the location's value first.
    pub fn location(&self) -> String {
        let mut location = match &self.location {
            Some(word) => exact_route::location_of(word),
            None => format!("/{}", self.segments.join("/")),
        };
        if self.query.is_empty() {
            return location;
        }
        let fragment = location.find('#').map(|at| location.split_off(at));
        for (name, value) in &self.query {
            location.push(if location.contains('?') { '&' } else { '?' });
            location.push_str(&format!("{name}={value}"));
        }
        location.push_str(&fragment.unwrap_or_default());
        location
    }

    /// Whether the line asked for print mode by a flag.
    pub fn print_flag(&self) -> bool {
        self.print || self.json
    }
}

/// A usage error (exit 2), and whether `--json` came before it, which
/// shapes the report.
#[derive(Debug)]
pub(crate) struct Refused {
    pub json: bool,
    pub message: String,
}

/// Scan `args`.
pub(crate) fn scan(args: Vec<String>) -> Result<Line, Refused> {
    let mut line = Line::default();
    let mut words = args.into_iter();
    let mut literal = false;
    let refuse = |line: &Line, message: String| Refused {
        json: line.json,
        message,
    };
    while let Some(word) = words.next() {
        if !literal {
            if let Some(message) = flag(&mut line, &word, &mut words, &mut literal) {
                return Err(refuse(&line, message));
            }
            if let Some(ops) = &mut line.drive {
                ops.extend(words.by_ref());
                continue;
            }
            if literal || word.starts_with('-') {
                continue;
            }
        }
        if !literal && word.starts_with('/') {
            if line.location.is_some() {
                let message = format!("a second location {word:?}: give one `/…` word");
                return Err(refuse(&line, message));
            }
            line.location = Some(word);
        } else {
            match exact_route::encode_route_segment(&word) {
                Ok(segment) => line.segments.push(segment),
                Err(e) => return Err(refuse(&line, format!("{word:?}: {}", e.message))),
            }
        }
        if line.location.is_some() && !line.segments.is_empty() {
            let message = "a `/…` location and other words don't combine: give the whole location";
            return Err(refuse(&line, message.into()));
        }
    }
    Ok(line)
}

/// Take one flag into `line`: `Some` refusal, else `None` (a word that is
/// not a flag is left for the caller).
fn flag(
    line: &mut Line,
    word: &str,
    rest: &mut impl Iterator<Item = String>,
    literal: &mut bool,
) -> Option<String> {
    let Some(name) = word.strip_prefix("--") else {
        return match word {
            "-h" => {
                line.help = true;
                None
            }
            w if w.starts_with('-') => Some(format!("unknown flag {w:?}")),
            _ => None,
        };
    };
    let (name, inline) = match name.split_once('=') {
        Some((name, value)) => (name, Some(value.to_string())),
        None => (name, None),
    };
    let bare = |line: &mut Line, set: fn(&mut Line)| match &inline {
        Some(_) => Some(format!("--{name} takes no value")),
        None => {
            set(line);
            None
        }
    };
    match name {
        "" => {
            *literal = true;
            None
        }
        "drive" => {
            line.drive = Some(Vec::new());
            None
        }
        "print" => bare(line, |l| l.print = true),
        "json" => bare(line, |l| l.json = true),
        "help" => bare(line, |l| l.help = true),
        "inline" => bare(line, |l| l.mode = Some(Mode::Inline)),
        "fullscreen" => bare(line, |l| l.mode = Some(Mode::Fullscreen)),
        "color" => {
            // `--color=auto|always|never` (D3), and the palettes the
            // headless screen also takes (`16`, `256`, `truecolor`).
            let value = inline.or_else(|| rest.next()).unwrap_or_default();
            line.color = Some(match value.as_str() {
                "auto" => Color::Auto,
                "never" => Color::Depth(ColorDepth::Monochrome),
                "16" => Color::Depth(ColorDepth::Ansi16),
                "256" => Color::Depth(ColorDepth::Ansi256),
                "always" | "truecolor" => Color::Depth(ColorDepth::TrueColor),
                _ => {
                    return Some(format!(
                    "--color expects auto, always or never (or 16, 256, truecolor), not {value:?}"
                ))
                }
            });
            None
        }
        "columns" | "timeout" | "size" => {
            let value = inline.or_else(|| rest.next()).unwrap_or_default();
            match name {
                "columns" => match value.parse::<usize>() {
                    Ok(n) if n > 0 => line.columns = Some(n),
                    _ => {
                        return Some(format!(
                            "--columns expects a number of columns, not {value:?}"
                        ))
                    }
                },
                "timeout" => {
                    let now = std::time::Instant::now();
                    match value.parse::<f64>() {
                        Ok(s) if s >= 0.0 && ending::forced_end(now, s).is_some() => {
                            line.timeout = Some(s)
                        }
                        _ => return Some(format!("--timeout expects seconds, not {value:?}")),
                    }
                }
                _ => match parse_size(&value) {
                    Some(size) => line.size = Some(size),
                    None => return Some(format!("--size expects COLSxROWS, not {value:?}")),
                },
            }
            None
        }
        _ => {
            let value = inline.or_else(|| rest.next()).unwrap_or_default();
            line.query.push((
                exact_route::encode_uri_component(name),
                exact_route::encode_uri_component(&value),
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(args: &[&str]) -> Result<Line, String> {
        scan(args.iter().map(|a| a.to_string()).collect()).map_err(|e| e.message)
    }

    fn location(args: &[&str]) -> String {
        line(args).unwrap().location()
    }

    /// D3's example table, each row.
    #[test]
    fn the_argument_list_is_a_url() {
        let l = line(&["--print", "--prompt", "why?"]).unwrap();
        assert!(l.print);
        assert_eq!(l.location(), "/?prompt=why%3F");
        assert_eq!(
            location(&["--print", "--prompt", "/model gpt"]),
            "/?prompt=%2Fmodel%20gpt"
        );
        assert_eq!(location(&[]), "/");
        assert_eq!(location(&["active"]), "/active");
        assert_eq!(
            location(&["--print", "/?x=one", "--x", "two"]),
            "/?x=one&x=two"
        );
        let l = line(&["--columns=100", "--print"]).unwrap();
        assert_eq!(l.columns, Some(100));
        assert_eq!(location(&["--", "/tmp/milk"]), "/%2Ftmp%2Fmilk");
        assert!(line(&["/a", "b"]).is_err());
        assert!(line(&["--color=red"]).is_err());
        assert!(line(&[".."]).is_err());
        assert_eq!(location(&["--prompt"]), "/?prompt=");
        assert_eq!(location(&["--prompt="]), "/?prompt=");
    }

    #[test]
    fn host_flags_go_anywhere_and_take_their_values() {
        let l = line(&["--timeout", "5", "/done", "--json", "--color=never"]).unwrap();
        assert_eq!(l.timeout, Some(5.0));
        assert!(l.json);
        assert_eq!(l.color, Some(Color::Depth(ColorDepth::Monochrome)));
        assert_eq!(l.location(), "/done");
        // A query flag takes the next word, even one that starts with `-`.
        assert_eq!(location(&["--q", "-x"]), "/?q=-x");
        assert_eq!(location(&["/a#top", "--q", "1"]), "/a?q=1#top");
        assert!(line(&["--columns", "wide"]).is_err());
        assert!(line(&["--timeout=-1"]).is_err());
        // Too long to be a Duration, or (on Unix, whose Instant is a
        // timespec) a Duration past the last Instant.
        assert!(line(&["--timeout=1e300"]).is_err());
        #[cfg(unix)]
        assert!(line(&["--timeout=1e19"]).is_err());
        assert_eq!(line(&["--timeout=1e9"]).unwrap().timeout, Some(1e9));
        assert!(line(&["-x"]).is_err());
        assert!(line(&["/a", "/b"]).is_err());
        assert!(line(&["--print=yes"]).is_err());
        assert!(line(&[""]).is_err());
        assert!(line(&["-h"]).unwrap().help);
    }

    #[test]
    fn after_drive_every_word_is_an_operation() {
        let l = line(&[
            "--size", "40x12", "/active", "--drive", "tap", "item-1", "--all",
        ])
        .unwrap();
        assert_eq!(l.size, Some((40, 12)));
        assert_eq!(l.location(), "/active");
        assert_eq!(
            l.drive,
            Some(vec!["tap".into(), "item-1".into(), "--all".into()])
        );
    }
}
