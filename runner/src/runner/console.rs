//! An app's `console` lines (LLP 1027.009). Each is journaled as
//! `console: …`, as the module's lines are (LLP 1097 D8), and handed with the
//! method that wrote it to the sink the host installed, which writes it to
//! the platform's log in every build (D1, D3), as the journal takes it (D2).
use super::{DataSource, Runner, JOURNAL_RING};

/// The `console` method that wrote an app's line: a host writes the line to
/// its platform's log at the level that matches it (D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Console {
    /// `console.log`.
    Log,
    /// `console.info`.
    Info,
    /// `console.warn`.
    Warn,
    /// `console.error`.
    Error,
    /// `console.debug`.
    Debug,
}

impl Console {
    /// The method named `name` (`"warn"`).
    pub fn named(name: &str) -> Option<Console> {
        Some(match name {
            "log" => Console::Log,
            "info" => Console::Info,
            "warn" => Console::Warn,
            "error" => Console::Error,
            "debug" => Console::Debug,
            _ => return None,
        })
    }

    /// The method's name (`"warn"`).
    pub fn name(self) -> &'static str {
        match self {
            Console::Log => "log",
            Console::Info => "info",
            Console::Warn => "warn",
            Console::Error => "error",
            Console::Debug => "debug",
        }
    }

    /// `line` as a source hands it to the runner ([`DataSource::take_logs`]):
    /// `console.warn: …`. The runner journals it as `console: …`.
    pub fn mark(self, line: &str) -> String {
        let name = self.name();
        let mut marked = String::with_capacity("console.: ".len() + name.len() + line.len());
        marked.push_str("console.");
        marked.push_str(name);
        marked.push_str(": ");
        marked.push_str(line);
        marked
    }

    /// The method and the line of a line [`Console::mark`] marked.
    fn unmark(line: &str) -> Option<(Console, &str)> {
        let (name, text) = line.strip_prefix("console.")?.split_once(": ")?;
        Some((Console::named(name)?, text))
    }
}

/// What a host installs to write its app's `console` lines.
type Sink = Box<dyn FnMut(Console, &str)>;

/// Where a runner's `console` lines go: the host's sink once it is
/// installed, and until then the lines journaled before it.
#[derive(Default)]
pub(super) struct Out {
    sink: Option<Sink>,
    held: std::collections::VecDeque<(Console, String)>,
}

impl<D: DataSource> Runner<D> {
    /// A line of the module's journal into the journal. An app's `console`
    /// line is journaled as `console: …` and handed to the host's sink.
    pub(super) fn journal_module_line(&mut self, line: String) {
        let Some((method, text)) = Console::unmark(&line) else {
            return self.log(line);
        };
        let mut journaled = String::with_capacity("console: ".len() + text.len());
        journaled.push_str("console: ");
        journaled.push_str(text);
        let out = &mut self.console;
        match out.sink.as_mut() {
            Some(sink) => sink(method, text),
            None => {
                out.held.push_back((method, text.to_string()));
                // Held no longer than the journal keeps a line. A host that
                // installs no sink (the web, the terminal) keeps these lines
                // in its journal only, by design: D1 names no log for it.
                if out.held.len() > JOURNAL_RING {
                    out.held.pop_front();
                }
            }
        }
        // The journal only, as an alert is: the sink is this line's way to
        // the platform's log, so the measuring queue must not carry it again.
        self.journal_only(journaled);
    }

    /// Where this runner's app writes its `console` lines from now on, each
    /// once, in order, with the method that wrote it: the platform's log
    /// (D1), in every build (D3). The lines journaled before are written
    /// first. A runner dropped with none installed writes nothing.
    pub fn set_console(&mut self, sink: impl FnMut(Console, &str) + 'static) {
        let mut sink: Sink = Box::new(sink);
        for (method, line) in self.console.held.drain(..) {
            sink(method, &line);
        }
        self.console.sink = Some(sink);
    }
}

#[cfg(test)]
mod tests;
