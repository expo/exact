//! The terminal itself: raw mode, the bytes in, the bytes out, and always
//! putting it back.
//!
//! @ref LLP 1101 D6 (input), D7 (restoration), D9 (vte parses input; the
//! output is constructed here; rustix for termios and the window size),
//! D10 (full screen and inline)
//!
//! Inline, the app lives on the normal screen like a shell's output: each
//! settled transcript entry is printed once and becomes the terminal's own
//! scrollback — scrolled, selected and copied by the terminal, in tmux too —
//! and only the live tail below it (a streaming reply, a spinner, the input)
//! is redrawn. Full screen, the app owns the alternate screen and the mouse.

use crate::host::{Host, Key, Mode};
use crate::image::{protocol_bytes, Protocol};
use exact_runner::DataSource;
use std::io::{IsTerminal, Write};
use vte::{Params, Parser, Perform};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{run, size};

/// Terminal size discovery is available only on Unix.
#[cfg(not(unix))]
pub fn size() -> Option<(usize, usize)> {
    None
}

/// Interactive terminal I/O is available only on Unix; headless use is portable.
#[cfg(not(unix))]
pub fn run<D: DataSource>(_host: &mut Host<D>) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the interactive terminal host runs on Unix only",
    ))
}

/// Whether stdin and stdout are terminals.
pub fn is_terminal() -> bool {
    std::io::stdout().is_terminal() && std::io::stdin().is_terminal()
}

// stderr can share stdout's nonblocking open file description. Drop a
// diagnostic on write failure; waiting for a stopped terminal would stall input
// and signal handling.
pub(crate) fn diagnostic(args: std::fmt::Arguments<'_>) {
    let _ = writeln!(std::io::stderr().lock(), "{args}");
}

/// What the input decodes to.
#[derive(Debug, PartialEq)]
pub enum Input {
    /// A key.
    Key(Key),
    /// A mouse press at a cell.
    Click(i32, i32),
    /// A wheel at a cell, rows (negative is up).
    Wheel(i32, i32, i32),
    /// A bracketed paste, whole.
    Paste(String),
    /// The terminal's answer to a cursor position query: row, column (1-based).
    CursorAt(usize, usize),
}

/// The input decoder: its state (a paste in progress) outlives a read.
#[derive(Default)]
pub struct Decoder {
    out: Vec<Input>,
    paste: Option<String>,
    /// A lone ESC ended the last read: when it becomes the Escape key, in
    /// milliseconds on the caller's clock.
    escape: Option<f64>,
}

impl Decoder {
    fn key(&mut self, k: Key) {
        self.out.push(Input::Key(k));
    }

    /// When a held ESC becomes the Escape key, if one is held.
    pub fn escape_due(&self) -> Option<f64> {
        self.escape
    }
}

fn ctrl_of(byte: u8) -> Option<Key> {
    match byte {
        0x0d | 0x0a => Some(Key::Named("Enter")),
        0x09 => Some(Key::Named("Tab")),
        0x08 => Some(Key::Named("Backspace")),
        0x01..=0x1a => Some(Key::Ctrl((b'a' + byte - 1) as char)),
        0x1c => Some(Key::Ctrl('\\')),
        _ => None,
    }
}

/// The modifiers a key's modifier parameter holds. The parameter is 1 plus
/// the sum of shift 1, alt 2, ctrl 4, super 8, hyper 16, meta 32, caps_lock
/// 64 and num_lock 128 (the kitty keyboard protocol, "Modifiers",
/// <https://sw.kovidgoyal.net/kitty/keyboard-protocol/#modifiers>; xterm's
/// PC-style function keys use the first three alike, ctlseqs,
/// <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h3-PC-Style-Function-Keys>).
/// The locks are state, not part of a chord, and come with every key that
/// types no text while they are on, so they are dropped here.
fn modifiers(param: u16) -> u16 {
    param.saturating_sub(1) & !(64 | 128)
}

/// The chord's modifier words, in Playwright's spelling. Super (the Command
/// or Windows key), hyper and meta are all `Meta`: a key event has one such
/// modifier, as DOM's `metaKey`.
fn chord_words(mods: u16) -> String {
    let mut chord = String::new();
    for (bits, word) in [(4, "Control+"), (2, "Alt+"), (1, "Shift+"), (56, "Meta+")] {
        if mods & bits != 0 {
            chord.push_str(word);
        }
    }
    chord
}

/// A named key with modifiers (bits as [`modifiers`] returns them), as the
/// decoder and a drive's `key` both make it.
pub(crate) fn with_mods(name: &'static str, mods: u16) -> Key {
    if mods == 0 {
        return Key::Named(name);
    }
    if mods == 1 && name == "Tab" {
        return Key::BackTab;
    }
    Key::Chord(chord_words(mods) + name)
}

/// A text key with modifiers. The protocol sends the unshifted key with
/// Shift as a modifier, so Shift alone capitalizes a letter; Control with
/// or without Shift is the control key.
pub(crate) fn text_with_mods(c: char, mods: u16) -> Key {
    match mods & !1 {
        0 if mods == 1 => Key::Char(c.to_ascii_uppercase()),
        0 => Key::Char(c),
        4 => Key::Ctrl(c.to_ascii_lowercase()),
        m => Key::Chord(chord_words(m) + c.encode_utf8(&mut [0; 4])),
    }
}

/// The key a kitty keyboard protocol key number names, with modifiers, or
/// `None` for a key that types nothing here. The keypad's keys and the
/// other functional keys have numbers in the Private Use Area, 57344 to
/// 63743 (the protocol's "Functional key definitions"; under "Disambiguate
/// escape codes" each keypad key that types no text is sent as its number:
/// <https://sw.kovidgoyal.net/kitty/keyboard-protocol/#functional-key-definitions>).
fn kitty_key(number: u16, mods: u16) -> Option<Key> {
    let name = match number {
        27 => "Escape",
        13 | 57414 => "Enter",
        9 => "Tab",
        127 => "Backspace",
        57417 => "ArrowLeft",
        57418 => "ArrowRight",
        57419 => "ArrowUp",
        57420 => "ArrowDown",
        57421 => "PageUp",
        57422 => "PageDown",
        57423 => "Home",
        57424 => "End",
        57426 => "Delete",
        _ => {
            let c = match number {
                57399..=57408 => (b'0' + (number - 57399) as u8) as char,
                57409 => '.',
                57410 => '/',
                57411 => '*',
                57412 => '-',
                57413 => '+',
                57415 => '=',
                57416 => ',',
                57344..=63743 => return None,
                n => char::from_u32(n as u32).filter(|c| !c.is_control())?,
            };
            return Some(text_with_mods(c, mods));
        }
    };
    Some(with_mods(name, mods))
}

impl Perform for Decoder {
    fn print(&mut self, c: char) {
        match &mut self.paste {
            Some(p) => p.push(c),
            None => self.key(Key::Char(c)),
        }
    }

    fn execute(&mut self, byte: u8) {
        if let Some(p) = &mut self.paste {
            match byte {
                b'\n' | b'\r' => p.push('\n'),
                b'\t' => p.push('\t'),
                _ => {}
            }
            return;
        }
        if let Some(k) = ctrl_of(byte) {
            self.key(k);
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        // ESC followed by a key: Alt+key in the legacy encoding.
        if intermediates.is_empty() && self.paste.is_none() && byte.is_ascii_graphic() {
            self.key(Key::Chord(format!("Alt+{}", byte as char)));
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let p: Vec<u16> = params
            .iter()
            .map(|p| p.first().copied().unwrap_or(0))
            .collect();
        let first = p.first().copied().unwrap_or(0);
        let mods = modifiers(p.get(1).copied().unwrap_or(1));
        if action == '~' && (first == 200 || first == 201) {
            if first == 200 {
                self.paste = Some(String::new());
            } else if let Some(text) = self.paste.take() {
                self.out.push(Input::Paste(text));
            }
            return;
        }
        if self.paste.is_some() {
            return;
        }
        match (intermediates, action) {
            (b"<", 'M') | (b"<", 'm') => {
                let (b, x, y) = (
                    first,
                    p.get(1).copied().unwrap_or(1) as i32 - 1,
                    p.get(2).copied().unwrap_or(1) as i32 - 1,
                );
                if b & 64 != 0 {
                    // Buttons 6 and 7 are horizontal; this host scrolls only rows.
                    if b & 2 == 0 {
                        self.out
                            .push(Input::Wheel(x, y, if b & 1 == 0 { -3 } else { 3 }));
                    }
                } else if action == 'M' && b & 32 == 0 && b & 3 == 0 {
                    self.out.push(Input::Click(x, y));
                }
            }
            (_, 'A') => self.key(with_mods("ArrowUp", mods)),
            (_, 'B') => self.key(with_mods("ArrowDown", mods)),
            (_, 'C') => self.key(with_mods("ArrowRight", mods)),
            (_, 'D') => self.key(with_mods("ArrowLeft", mods)),
            (_, 'H') => self.key(with_mods("Home", mods)),
            (_, 'F') => self.key(with_mods("End", mods)),
            (_, 'Z') => self.key(Key::BackTab),
            (b"", 'R') if p.len() == 2 => {
                self.out.push(Input::CursorAt(p[0] as usize, p[1] as usize))
            }
            (_, '~') => match first {
                1 | 7 => self.key(with_mods("Home", mods)),
                4 | 8 => self.key(with_mods("End", mods)),
                3 => self.key(with_mods("Delete", mods)),
                5 => self.key(with_mods("PageUp", mods)),
                6 => self.key(with_mods("PageDown", mods)),
                _ => {}
            },
            // The kitty keyboard protocol: a code point and its modifiers.
            (_, 'u') => {
                if let Some(k) = kitty_key(first, mods) {
                    self.key(k);
                }
            }
            _ => {}
        }
    }
}

/// The legacy Alt+key for an ESC followed by this byte, when the byte is
/// DEL or a C0 key: xterm's `metaSendsEscape` prefixes the key's own byte
/// with ESC (ctlseqs, "Alt and Meta Keys",
/// <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h3-Alt-and-Meta-Keys>).
fn alt_of(byte: u8) -> Option<Key> {
    let key = if byte == 0x7f {
        Key::Named("Backspace")
    } else {
        ctrl_of(byte)?
    };
    Some(match key {
        Key::Named(name) => with_mods(name, 2),
        Key::Ctrl(c) => text_with_mods(c, 6),
        other => other,
    })
}

/// Decode a read: DEL, and ESC before DEL or a C0 key, by hand, everything
/// else through vte. vte ignores DEL, and it executes a C0 byte inside an
/// escape sequence without leaving it, which would make the key after it
/// Alt+key. A read ending in a lone ESC is held: it is the Escape key only
/// if nothing follows within [`ESC_WAIT_MS`] of `now`, the read's time
/// (`flush_escape`), and otherwise the start of a sequence split across
/// reads.
pub fn decode(parser: &mut Parser, decoder: &mut Decoder, bytes: &[u8], now: f64) -> Vec<Input> {
    let held = decoder.escape.take().is_some();
    let joined;
    let bytes = if held {
        joined = [&[0x1b][..], bytes].concat();
        &joined[..]
    } else {
        bytes
    };
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let (key, len) = match bytes[i] {
            0x7f => (Some(Key::Named("Backspace")), 1),
            0x1b => match bytes.get(i + 1).and_then(|b| alt_of(*b)) {
                Some(k) => (Some(k), 2),
                None => (None, 1),
            },
            _ => (None, 1),
        };
        let Some(key) = key else {
            i += 1;
            continue;
        };
        parser.advance(decoder, &bytes[start..i]);
        if decoder.paste.is_none() {
            decoder.key(key);
            start = i + len;
        } else if len == 1 {
            // A paste keeps no DEL.
            start = i + 1;
        } else {
            // A paste's ESC and control byte are vte's, as any other text.
            start = i;
        }
        i += len;
    }
    let rest = &bytes[start..];
    if rest.last() == Some(&0x1b) && decoder.paste.is_none() {
        parser.advance(decoder, &rest[..rest.len() - 1]);
        decoder.escape = Some(now + ESC_WAIT_MS);
    } else {
        parser.advance(decoder, rest);
    }
    std::mem::take(&mut decoder.out)
}

/// How long a lone ESC waits for the rest of a sequence, timed from the
/// ESC whatever else wakes the loop, as ncurses' `ESCDELAY` and tmux's
/// `escape-time` are.
pub const ESC_WAIT_MS: f64 = 25.0;

/// The held ESC, once nothing has followed it for [`ESC_WAIT_MS`]: the
/// Escape key. Before then, nothing.
pub fn flush_escape(decoder: &mut Decoder, now: f64) -> Vec<Input> {
    match decoder.escape {
        Some(due) if now >= due => {
            decoder.escape = None;
            vec![Input::Key(Key::Named("Escape"))]
        }
        _ => Vec::new(),
    }
}

/// The inline writer's memory of what it already put on the screen.
pub(crate) struct Inline {
    /// Transcript children already printed into scrollback, by identity,
    /// and the `id` of the last one, which the app reads to retire them.
    printed: std::collections::HashSet<exact_kernel::NodeKey>,
    through: String,
    /// Which log `through` belongs to: one more for each new log, so an
    /// app can tell a new transcript's `"1"` from the old one's.
    logs: u64,
    /// The (log, through) last published to the app.
    published: (u64, String),
    /// The log node the printed children belong to, and whether it has
    /// printed any.
    log: Option<exact_kernel::NodeKey>,
    printed_any: bool,
    /// The live region's height and the cursor's row inside it.
    live: usize,
    cursor_row: usize,
    /// What the last frame showed: generation, size.
    shown: Option<(u64, usize, usize)>,
    protocol: Protocol,
    /// The live region as last written.
    last: Option<crate::grid::Grid>,
    /// Mouse reporting is on (only while a dialog is open, LLP 1101.001),
    /// and the cursor row inside the region at each position query still
    /// unanswered: the answer places the presented painting on the screen.
    mouse: bool,
    queries: std::collections::VecDeque<usize>,
}

impl Inline {
    /// A writer that has written nothing.
    pub(crate) fn new(protocol: Protocol) -> Inline {
        Inline {
            printed: std::collections::HashSet::new(),
            through: String::new(),
            logs: 0,
            published: (0, String::new()),
            log: None,
            printed_any: false,
            live: 0,
            cursor_row: 0,
            shown: None,
            protocol,
            last: None,
            mouse: false,
            queries: std::collections::VecDeque::new(),
        }
    }

    /// The record to publish once this frame's bytes are out, when what is
    /// printed changed since the last one (LLP 1101.002 §0 P5).
    pub(crate) fn printed(
        &mut self,
        mode: Mode,
        colors: crate::color::ColorDepth,
    ) -> Option<String> {
        let now = (self.logs, self.through.clone());
        if now == self.published {
            return None;
        }
        self.published = now;
        Some(record(
            mode,
            self.protocol,
            colors,
            self.logs,
            &self.through,
        ))
    }

    /// Position queries asked and not yet answered.
    pub(crate) fn unanswered(&self) -> usize {
        self.queries.len()
    }
}

impl Inline {
    pub(crate) fn frame<D: DataSource>(&mut self, host: &mut Host<D>, out: &mut String) {
        let (cols, rows) = host.size();
        if self.shown == Some((host.generation, cols, rows)) {
            return;
        }
        let resized = self.shown.is_some_and(|(_, c, r)| (c, r) != (cols, rows));
        self.shown = Some((host.generation, cols, rows));
        let total = host.document_rows();
        // A new log node is a new transcript: clear the screen and the
        // scrollback, as `/clear` does in a terminal's own shell.
        let log = host.log_key();
        // Printed children are retired as they go, so remember that this log
        // printed at all, not which children it still has.
        let cleared = self.log.is_some() && log != self.log && self.printed_any;
        if log != self.log {
            self.printed_any = false;
            self.through.clear();
            self.logs += 1;
        }
        self.log = log;
        if cleared {
            if self.cursor_row > 0 {
                out.push_str(&format!("\x1b[{}A", self.cursor_row));
            }
            // The screen only: the terminal's scrollback holds the person's
            // own history too, which a new transcript has no right to erase.
            out.push_str("\x1b[H\x1b[2J");
            self.printed.clear();
            self.cursor_row = 0;
            self.last = None;
        }
        let children = host.transcript();
        // Printed children are known by identity (LLP 1101.001 P1): those the
        // app has since retired, or cleared, are forgotten.
        self.printed
            .retain(|k| children.iter().any(|c| c.key == *k));
        let lead = children
            .iter()
            .take_while(|c| self.printed.contains(&c.key))
            .count();
        // Rows are printed once: none settles while a picture is still
        // being read, or it would go to the scrollback as its alt text.
        let settled = match host.images.loading() {
            true => lead,
            false => children
                .iter()
                .position(|c| c.busy)
                .unwrap_or(children.len()),
        };
        let bottom_of = |n: usize| if n == 0 { 0 } else { children[n - 1].bottom };
        out.push_str("\x1b[?2026h\x1b[?25l");
        // Back to the live region's top-left. After a resize this is still
        // `cursor_row` rows up: every region row ends in a hard line break,
        // which a reflowing terminal never rejoins, so a wider window keeps
        // the rows; a narrower one may wrap them into more, and moving up
        // too few leaves a stale fragment above rather than erasing printed
        // rows (the safer error; LLP 1101.002 code review).
        if self.cursor_row > 0 {
            out.push_str(&format!("\x1b[{}A", self.cursor_row));
        }
        out.push('\r');
        let mut fresh = resized || cleared;
        let mut through = lead;
        if settled > lead {
            fresh = true;
            out.push_str("\x1b[J");
            let (from, to) = (bottom_of(lead), bottom_of(settled));
            // Scrollback can arrive in a single large burst. Rasterize it
            // in bounded strips instead of allocating a cell for every
            // column of the entire transcript before writing any of it.
            for top in (from..to).step_by(128) {
                let painted = host.render_settled(top, (to - top).min(128));
                self.print_rows(&painted, &host.images, out);
            }
            self.printed
                .extend(children[lead..settled].iter().map(|c| c.key));
            self.printed_any = true;
            through = settled;
            self.through = children[settled - 1].id.clone();
        }
        let top = bottom_of(through);
        let live_total = total.saturating_sub(top);
        // An open dialog goes below the live content, anchored to the
        // region's bottom; the region grows down into new rows to hold it,
        // never up over what is printed.
        let layer = host.layer_rows().unwrap_or(0);
        let show = (live_total + layer).min(rows.saturating_sub(1)).max(1);
        let content = show.saturating_sub(layer);
        let from = total.saturating_sub(content).max(top);
        let painted = host.render(from, show);
        // The same region as last time: write only the cells that changed
        // (a spinner's tick is a few bytes). Otherwise repaint it whole.
        let row = match self
            .last
            .as_ref()
            .filter(|last| !fresh && last.rows == show && last.cols == cols)
        {
            Some(last) => painted.grid.diff_relative(last, out),
            None => {
                out.push_str("\x1b[J");
                for y in 0..show {
                    out.push_str(&painted.grid.row_sgr(y));
                    if y + 1 < show {
                        out.push_str("\r\n");
                    }
                }
                out.push('\r');
                show - 1
            }
        };
        self.live = show;
        // The cursor from `row` to the caret, or to the region's last row.
        let (target_row, col) = match painted.grid.cursor {
            Some((cx, cy)) => (cy.min(show - 1), Some(cx)),
            None => (show - 1, None),
        };
        if target_row > row {
            out.push_str(&format!("\x1b[{}B", target_row - row));
        } else if row > target_row {
            out.push_str(&format!("\x1b[{}A", row - target_row));
        }
        if let Some(cx) = col {
            out.push_str(&format!("\x1b[{}G\x1b[5 q\x1b[?25h", cx + 1));
        }
        self.cursor_row = target_row;
        host.present(&painted, None);
        self.last = Some(painted.grid);
        // Clicks only while a dialog is open; scroll and selection are the
        // terminal's the rest of the time.
        let want = host.has_layer();
        if want != self.mouse {
            out.push_str(if want {
                "\x1b[?1000h\x1b[?1006h"
            } else {
                "\x1b[?1006l\x1b[?1000l"
            });
            self.mouse = want;
        }
        // Where the region is on the screen: asked on every frame while
        // clicks matter, so the answer after a resize re-anchors them, and
        // never otherwise.
        if self.mouse {
            out.push_str("\x1b[6n");
            self.queries.push_back(self.cursor_row);
        }
        out.push_str("\x1b[?2026l");
    }

    /// A position answer: the region's top is the answered row less the
    /// cursor's row in the region when it was asked.
    pub(crate) fn answered<D: DataSource>(&mut self, host: &mut Host<D>, row: usize) {
        if let Some(in_region) = self.queries.pop_front() {
            host.anchor((row as i32 - 1 - in_region as i32).max(0));
        }
    }

    /// Print settled rows into the scrollback, an image protocol's picture
    /// over each image's cells once its last row is out.
    fn print_rows(
        &self,
        painted: &crate::paint::Painted,
        images: &crate::image::Images,
        out: &mut String,
    ) {
        let rows = painted.grid.rows;
        for y in 0..rows {
            out.push_str(&painted.grid.row_sgr(y));
            if self.protocol != Protocol::Blocks {
                for (id, r) in &painted.images {
                    // An image may begin in an earlier strip. Its last
                    // row emits the protocol image over the rows already
                    // printed, exactly once.
                    if r.y + r.h - 1 == y as i32 && r.h > 0 {
                        if let Some(image) = images.of(*id) {
                            let bytes =
                                protocol_bytes(self.protocol, image, r.w as usize, r.h as usize);
                            out.push_str("\x1b7");
                            if r.h > 1 {
                                out.push_str(&format!("\x1b[{}A", r.h - 1));
                            }
                            out.push_str(&format!("\x1b[{}G", r.x + 1));
                            out.push_str(&bytes);
                            out.push_str("\x1b8");
                        }
                    }
                }
            }
            out.push_str("\r\n");
        }
    }

    /// The shell may have moved the cursor. Repaint the live region below it,
    /// retaining the transcript already printed into scrollback.
    #[cfg(unix)]
    fn resume(&mut self) {
        self.live = 0;
        self.cursor_row = 0;
        self.shown = None;
        self.last = None;
        self.mouse = false;
        self.queries.clear();
    }

    /// Leave the screen with the last frame in place and the cursor below it.
    #[cfg(unix)]
    fn finish(&self, out: &mut String) {
        let down = self.live.saturating_sub(1 + self.cursor_row);
        if down > 0 {
            out.push_str(&format!("\x1b[{down}B"));
        }
        out.push_str("\r\n");
    }
}

/// A JSON string's contents: quotes, backslashes and controls escaped, so
/// an id round-trips unchanged.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The terminal's record for the app, `exactSurface("terminal")`: `log`
/// counts the logs seen (a new one is a new transcript), and `printed` is
/// the `id` of that log's last child written to scrollback.
pub(crate) fn record(
    mode: Mode,
    images: Protocol,
    colors: crate::color::ColorDepth,
    log: u64,
    printed: &str,
) -> String {
    let mode = if mode == Mode::Inline {
        "inline"
    } else {
        "fullscreen"
    };
    let images = match images {
        Protocol::Kitty => "kitty",
        Protocol::Iterm => "iterm",
        Protocol::Blocks => "blocks",
    };
    let printed = json_string(printed);
    let colors = colors as u8;
    format!(
        "{{\"mode\":\"{mode}\",\"images\":\"{images}\",\"colors\":{colors},\"log\":{log},\"printed\":\"{printed}\"}}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(unix))]
    #[test]
    fn interactive_terminal_is_refused_by_name() {
        for mode in [Mode::Inline, Mode::Fullscreen] {
            let plan =
                contract::compile_terminal("component App\n  view\n    text \"hello\"\n").unwrap();
            let mut host = Host::boot(plan, (), mode, 20, 12).unwrap();
            let error = run(&mut host).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
            assert_eq!(
                error.to_string(),
                "the interactive terminal host runs on Unix only"
            );
        }
    }

    #[test]
    fn protocol_images_cross_paint_strips_without_being_dropped_or_repeated() {
        let path =
            std::env::temp_dir().join(format!("terminal-strip-image-{}.png", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[20, 40, 60])
            .unwrap();
        let source = format!(
            "component App\n  view\n    column\n      text \"before\"\n      image \"{}\" width=\"2ch\" height=\"260lh\"\n      text \"after\"\n",
            path.display()
        );
        let plan = contract::compile_terminal(&source).unwrap();
        let mut host = Host::boot(plan, (), Mode::Inline, 20, 12).unwrap();
        host.wait_images(std::time::Duration::from_secs(5));
        for protocol in [Protocol::Kitty, Protocol::Iterm] {
            let writer = Inline::new(protocol);
            let mut whole = String::new();
            writer.print_rows(
                &host.render_settled(0, host.document_rows()),
                &host.images,
                &mut whole,
            );
            let mut strips = String::new();
            for top in (0..host.document_rows()).step_by(128) {
                writer.print_rows(
                    &host.render_settled(top, (host.document_rows() - top).min(128)),
                    &host.images,
                    &mut strips,
                );
            }
            assert_eq!(strips, whole);
            let command = match protocol {
                Protocol::Kitty => "a=T",
                _ => "1337;File=",
            };
            assert_eq!(strips.matches(command).count(), 1);
        }
        host.set_colors(crate::color::ColorDepth::Monochrome);
        for protocol in [Protocol::Blocks, Protocol::Kitty, Protocol::Iterm] {
            let writer = Inline::new(protocol);
            let painted = host.render_settled(0, host.document_rows());
            assert!(painted.images.is_empty());
            // The fixture is only two columns wide, so its alternate text
            // is clipped to the same image box.
            assert!(painted.grid.text().contains("[i"));
            let mut out = String::new();
            writer.print_rows(&painted, &host.images, &mut out);
            assert!(!out.contains("a=T") && !out.contains("1337;File="));
            assert!(!out.contains("38;") && !out.contains("48;"));
        }
        std::fs::remove_file(path).unwrap();
    }

    fn keys(bytes: &[u8]) -> Vec<Input> {
        decode(&mut Parser::new(), &mut Decoder::default(), bytes, 0.0)
    }

    #[test]
    fn keys_mouse_and_paste_decode() {
        assert_eq!(
            keys(b"a\r"),
            vec![Input::Key(Key::Char('a')), Input::Key(Key::Named("Enter"))]
        );
        assert_eq!(
            keys(b"\x1b[A\x7f"),
            vec![
                Input::Key(Key::Named("ArrowUp")),
                Input::Key(Key::Named("Backspace"))
            ]
        );
        assert_eq!(keys(b"\x1b[<0;5;3M"), vec![Input::Click(4, 2)]);
        assert_eq!(keys(b"\x1b[<65;1;1M"), vec![Input::Wheel(0, 0, 3)]);
        assert_eq!(keys(b"\x1b[99;5u"), vec![Input::Key(Key::Ctrl('c'))]);
        assert_eq!(
            keys(b"\x1b[13;2u"),
            vec![Input::Key(Key::Chord("Shift+Enter".into()))]
        );
        assert_eq!(
            keys(b"\x1b[1;5A"),
            vec![Input::Key(Key::Chord("Control+ArrowUp".into()))]
        );
        let (mut p, mut d) = (Parser::new(), Decoder::default());
        // A lone ESC is the Escape key once ESC_WAIT_MS passed, not before.
        assert_eq!(decode(&mut p, &mut d, b"\x1b", 100.0), vec![]);
        assert_eq!(d.escape_due(), Some(100.0 + ESC_WAIT_MS));
        assert_eq!(flush_escape(&mut d, 110.0), vec![]);
        assert_eq!(
            flush_escape(&mut d, 100.0 + ESC_WAIT_MS),
            vec![Input::Key(Key::Named("Escape"))]
        );
        assert_eq!(d.escape_due(), None);
        // An arrow split across reads stays an arrow.
        assert_eq!(decode(&mut p, &mut d, b"\x1b", 200.0), vec![]);
        assert_eq!(flush_escape(&mut d, 210.0), vec![]);
        assert_eq!(
            decode(&mut p, &mut d, b"[A", 215.0),
            vec![Input::Key(Key::Named("ArrowUp"))]
        );
        assert_eq!(d.escape_due(), None);
        assert_eq!(
            keys(b"\x1b[200~x\ny\x1b[201~"),
            vec![Input::Paste("x\ny".into())]
        );
    }

    #[test]
    fn horizontal_wheel_reports_do_not_become_vertical_input() {
        for modifiers in [0, 4, 8, 16, 28] {
            for (button, rows) in [(64, Some(-3)), (65, Some(3)), (66, None), (67, None)] {
                let report = format!("\x1b[<{};3;2M", button + modifiers);
                let expected = rows.map(|rows| Input::Wheel(2, 1, rows));
                assert_eq!(
                    keys(report.as_bytes()),
                    expected.into_iter().collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn kitty_lock_modifiers_and_keypad_keys_decode_as_the_keys_they_are() {
        let key = |bytes: &[u8]| match &keys(bytes)[..] {
            [Input::Key(k)] => Some(k.clone()),
            [] => None,
            other => panic!("{bytes:?}: {other:?}"),
        };
        // Caps Lock (64) and Num Lock (128) ride on every key that types no text.
        assert_eq!(key(b"\x1b[99;69u"), Some(Key::Ctrl('c')));
        assert_eq!(key(b"\x1b[99;133u"), Some(Key::Ctrl('c')));
        assert_eq!(key(b"\x1b[92;133u"), Some(Key::Ctrl('\\')));
        assert_eq!(key(b"\x1b[27;65u"), Some(Key::Named("Escape")));
        assert_eq!(key(b"\x1b[9;66u"), Some(Key::BackTab));
        assert_eq!(key(b"\x1b[1;129B"), Some(Key::Named("ArrowDown")));
        assert_eq!(key(b"\x1b[3;129~"), Some(Key::Named("Delete")));
        assert_eq!(
            key(b"\x1b[1;133A"),
            Some(Key::Chord("Control+ArrowUp".into()))
        );
        // Super, hyper and meta make a chord, never text.
        assert_eq!(key(b"\x1b[99;9u"), Some(Key::Chord("Meta+c".into())));
        assert_eq!(key(b"\x1b[13;9u"), Some(Key::Chord("Meta+Enter".into())));
        assert_eq!(key(b"\x1b[99;7u"), Some(Key::Chord("Control+Alt+c".into())));
        // The keypad's own numbers are the keys they stand for.
        assert_eq!(key(b"\x1b[57414u"), Some(Key::Named("Enter")));
        assert_eq!(key(b"\x1b[57419u"), Some(Key::Named("ArrowUp")));
        assert_eq!(key(b"\x1b[57426;129u"), Some(Key::Named("Delete")));
        assert_eq!(key(b"\x1b[57400;5u"), Some(Key::Ctrl('1')));
        // Another functional key (here Caps Lock itself) types nothing.
        assert_eq!(key(b"\x1b[57358u"), None);
    }

    #[test]
    fn legacy_alt_with_backspace_or_a_control_key_is_one_chord() {
        let chord = |c: &str| Input::Key(Key::Chord(c.into()));
        let (mut p, mut d) = (Parser::new(), Decoder::default());
        for (bytes, alt) in [
            (&b"\x1b\x7f"[..], "Alt+Backspace"),
            (b"\x1b\x08", "Alt+Backspace"),
            (b"\x1b\r", "Alt+Enter"),
            (b"\x1b\t", "Alt+Tab"),
            (b"\x1b\x01", "Control+Alt+a"),
        ] {
            assert_eq!(decode(&mut p, &mut d, bytes, 0.0), vec![chord(alt)]);
            // The key after it is itself, not Alt+key.
            assert_eq!(
                decode(&mut p, &mut d, b"a", 0.0),
                vec![Input::Key(Key::Char('a'))]
            );
        }
        // Split across reads: the held ESC and the key that follows it.
        assert_eq!(
            decode(&mut p, &mut d, b"x\x1b", 0.0),
            vec![Input::Key(Key::Char('x'))]
        );
        assert_eq!(
            decode(&mut p, &mut d, b"\x7fa", 0.0),
            vec![chord("Alt+Backspace"), Input::Key(Key::Char('a'))]
        );
    }

    #[test]
    fn a_paste_split_across_reads_stays_a_paste() {
        let (mut p, mut d) = (Parser::new(), Decoder::default());
        assert_eq!(decode(&mut p, &mut d, b"\x1b[200~line one\r", 0.0), vec![]);
        assert_eq!(
            decode(&mut p, &mut d, b"q\x7fline two\x1b[201~", 0.0),
            vec![Input::Paste("line one\nqline two".into())]
        );
    }
}
