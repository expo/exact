//! A world's message carries what caused it (LLP 1133.022 D6, D10 item 6).
//!
//! A message the runner posts from a commit that was not a person's or the
//! app's own (a tool invocation, a wake, a package's event) arrives with
//! the origins that caused it in front of the text,
//! [`ORIGIN_MARK`]`<word>,<word>\u{1}`. The world keeps the union of the
//! origins posted into it since its fresh start, and every message it emits
//! carries that set back, so the canvas's `message=` handler runs under
//! every one's restrictions: a tool that posts into a world cannot start a
//! device through the world's later emissions, and a background action's
//! post keeps its foreground rule beside a tool's. The set is simulation
//! state, saved and restored with the world; only a restart that builds a
//! fresh world from its arguments clears it, since a tool's message can
//! leave state that emits much later and the world's code is not traced.
//!
//! A mark the world cannot read names every restricted origin: never a way
//! to raise one. The save writes the set only when a world has one, so a
//! save of a world no tool reached is byte-for-byte what it was before.

use super::*;

/// What precedes a marked message's text; the runner writes and reads the
/// same (`runner/src/runner/world_origin.rs`).
pub const ORIGIN_MARK: &str = "\u{1}origin:";

/// The restricted origins a mark names, a bit each, in the order a mark
/// writes them; the app's is no mark.
const WORDS: [&str; 4] = ["device", "wake", "tool", "action"];

/// Every restricted origin: what a mark the world cannot read names.
const EVERY: u8 = (1 << WORDS.len()) - 1;

/// `text`'s set of origins and the text after its mark; 0 for none. A
/// word that is not a restricted origin's (`app` included), or a mark with
/// no end, makes the set every restricted origin.
pub fn unmark(text: &str) -> (u8, &str) {
    let Some(rest) = text.strip_prefix(ORIGIN_MARK) else {
        return (0, text);
    };
    // A mark with no end names nothing readable.
    let Some((words, text)) = rest.split_once('\u{1}') else {
        return (EVERY, "");
    };
    let set = words.split(',').try_fold(0, |set, word| {
        let bit = WORDS.iter().position(|w| *w == word)?;
        Some(set | 1 << bit)
    });
    (set.unwrap_or(EVERY), text)
}

/// The words of set `set`, joined by commas.
fn words(set: u8) -> String {
    let named = WORDS.iter().enumerate().filter(|(i, _)| set & 1 << i != 0);
    named.map(|(_, w)| *w).collect::<Vec<_>>().join(",")
}

/// `text` marked with set `set`; unchanged for none.
pub fn mark(set: u8, text: String) -> String {
    match set {
        0 => text,
        _ => format!("{ORIGIN_MARK}{}\u{1}{text}", words(set)),
    }
}

impl<G: Game> Sim<G> {
    /// A posted event: a message's mark is taken off its text and joins the
    /// world's.
    pub(super) fn unmarked(&mut self, event: InputEvent) -> InputEvent {
        match event {
            InputEvent::Message {
                text,
                at_ms,
                player,
            } => {
                let (set, rest) = unmark(&text);
                if set & !self.origin != 0 {
                    self.origin |= set;
                    self.world
                        .log(format_args!("posted by a {} commit", words(set)));
                }
                let text = if set == 0 && rest.len() == text.len() {
                    text
                } else {
                    rest.to_owned()
                };
                InputEvent::Message {
                    text,
                    at_ms,
                    player,
                }
            }
            event => event,
        }
    }

    /// The world's emitted messages, each with its mark.
    pub(super) fn marked(&self, messages: Vec<String>) -> Vec<String> {
        let set = self.origin;
        messages.into_iter().map(|m| mark(set, m)).collect()
    }

    /// The world's set of origins as its mark writes them, `app` for none.
    pub fn origin(&self) -> String {
        match self.origin {
            0 => "app".to_owned(),
            set => words(set),
        }
    }

    /// `,"origin":"<words>"` for the agent's `state` of a marked world.
    pub(crate) fn origin_json(&self) -> String {
        match self.origin {
            0 => String::new(),
            _ => format!(",\"origin\":\"{}\"", self.origin()),
        }
    }
}

impl Data for Saved {
    fn write(&self, w: &mut dyn crate::Writer) {
        w.begin_struct();
        w.field("game");
        self.game.write(w);
        w.field("world");
        self.world.write(w);
        w.field("args");
        self.args.write(w);
        w.field("input");
        self.input.write(w);
        w.field("queue");
        self.queue.write(w);
        w.field("world_us");
        self.world_us.write(w);
        w.field("journal");
        self.journal.write(w);
        w.field("journal_next");
        self.journal_next.write(w);
        w.field("overflow_logged");
        self.overflow_logged.write(w);
        w.field("published");
        self.published.write(w);
        // Only a marked world's save carries its set (module doc).
        if self.origin != 0 {
            w.field("origins");
            self.origin.write(w);
        }
        w.end_struct();
    }

    fn read(&mut self, r: &mut dyn crate::Reader) -> Result<(), DataError> {
        const FIELDS: [&str; 10] = [
            "game",
            "world",
            "args",
            "input",
            "queue",
            "world_us",
            "journal",
            "journal_next",
            "overflow_logged",
            "published",
        ];
        let mut seen = [false; FIELDS.len()];
        let mut legacy = false;
        self.origin = 0;
        r.begin_struct()?;
        while let Some(field) = r.field()? {
            let at = field.as_str();
            let read = match at {
                "game" => self.game.read(r),
                "world" => self.world.read(r),
                "args" => self.args.read(r),
                "input" => self.input.read(r),
                "queue" => self.queue.read(r),
                "world_us" => self.world_us.read(r),
                "journal" => self.journal.read(r),
                "journal_next" => self.journal_next.read(r),
                "overflow_logged" => self.overflow_logged.read(r),
                "published" => self.published.read(r),
                "origins" => self.origin.read(r),
                // A save from before the mark was a set: its one origin
                // cannot be read as a set, so it names every one.
                "origin" => r.skip().map(|()| legacy = true),
                _ => r.skip(),
            };
            read.map_err(|e| e.at(at))?;
            if let Some(i) = FIELDS.iter().position(|f| *f == at) {
                seen[i] = true;
            }
        }
        if legacy {
            self.origin = EVERY;
        }
        if self.origin & !EVERY != 0 {
            return Err(DataError::new("an origin mark names no origin").at("origins"));
        }
        if r.complete() {
            if let Some(i) = seen.iter().position(|s| !s) {
                return Err(DataError::new("missing field").at(FIELDS[i]));
            }
        }
        Ok(())
    }
}
