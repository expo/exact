//! Inline parsing: one line of Markdown to the ordered styled runs the
//! kernel measures.
//!
//! The runs are what an Exact paragraph is made of (LLP 1033): a `text` node
//! whose children are `text` nodes, each with its own weight, slant, family
//! and `href`. Nothing here knows about layout, and nothing here does I/O.

/// One styled span of a paragraph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Run {
    /// The characters.
    pub text: String,
    /// `**bold**`.
    pub bold: bool,
    /// `*italic*`.
    pub italic: bool,
    /// A `` `code` `` span: the reader gives it a monospaced family.
    pub code: bool,
    /// Where a link goes — empty for text that is not a link.
    pub href: String,
}

impl Run {
    /// A plain run of `text`.
    pub fn text(text: impl Into<String>) -> Run {
        Run {
            text: text.into(),
            ..Run::default()
        }
    }
}

// A stopped scan stays stopped, even if the caller's generation changes again.
// Used by block construction and inline indexes, without a parser-global token.
pub(crate) struct Scan<'a> {
    cancel: &'a dyn Fn() -> bool,
    stopped: std::cell::Cell<bool>,
}

impl<'a> Scan<'a> {
    pub(crate) fn new(cancel: &'a dyn Fn() -> bool) -> Self {
        Self {
            cancel,
            stopped: std::cell::Cell::new(false),
        }
    }

    pub(crate) fn cancelled(&self) -> bool {
        if !self.stopped.get() && (self.cancel)() {
            self.stopped.set(true);
        }
        self.stopped.get()
    }
}

/// A parse frame borrows a range of the original paragraph. Nested labels and
/// emphasis share bracket/backtick indexes and emit directly into the final runs;
/// neither source nor descendant runs are copied per nesting level.
struct Frame {
    start: usize,
    end: usize,
    resume: usize,
    first_run: usize,
    bold: bool,
    italic: bool,
    href: std::rc::Rc<str>,
    plain_wrapper: bool,
    last_plain: bool,
    // Code spans must close inside this frame, so each frame owns its cache.
    closers: [CloserCache; 4],
}

type CloserCache = std::collections::HashMap<(usize, usize), Option<usize>>;

struct EmphasisSearch {
    start: usize,
    marker: u8,
    count: usize,
    // Each nested emphasis bounds the tokens in its content, then resumes
    // after its closer. Searches use a stack so malformed input cannot recurse.
    scopes: Vec<(usize, usize)>,
    visited: Vec<(usize, usize)>,
}

impl EmphasisSearch {
    fn new(start: usize, marker: u8, count: usize, end: usize) -> Self {
        Self {
            start,
            marker,
            count,
            scopes: vec![(end, end)],
            visited: Vec::new(),
        }
    }

    fn index(&self) -> usize {
        usize::from(self.marker == b'_') * 2 + self.count - 1
    }
}

struct Parser<'a> {
    text: &'a str,
    scan: &'a Scan<'a>,
    html: crate::Html<'a>,
    src: &'a [u8],
    at: usize,
    runs: Vec<Run>,
    pending: String,
    frame: Frame,
    parents: Vec<Frame>,
    brackets: Option<Vec<usize>>,
    parens: Option<Vec<usize>>,
    ticks: Option<Vec<(usize, usize)>>,
    angles: Option<Vec<Angle>>,
    url: Option<std::ops::Range<usize>>,
}

// Remember the last relevant characters in each interval ending at `>`.
// Both parsing and emphasis lookahead can revisit an interval from nested labels.
struct Angle {
    close: usize,
    mail: usize,
    invalid: usize,
}

/// Parse one paragraph's text into runs. `link` resolves targets beside the
/// document. Indexes are built only when their delimiter is actually parsed;
/// ordinary prose allocates no delimiter index.
pub(crate) fn runs(
    text: &str,
    link: &dyn Fn(&str) -> String,
    scan: &Scan<'_>,
    html: crate::Html<'_>,
) -> Vec<Run> {
    let mut p = Parser {
        text,
        scan,
        html,
        src: text.as_bytes(),
        at: 0,
        runs: Vec::new(),
        pending: String::new(),
        frame: Frame {
            start: 0,
            end: text.len(),
            resume: text.len(),
            first_run: 0,
            bold: false,
            italic: false,
            href: "".into(),
            plain_wrapper: true,
            last_plain: false,
            closers: Default::default(),
        },
        parents: Vec::new(),
        brackets: None,
        parens: None,
        ticks: None,
        angles: None,
        url: None,
    };
    p.run(link);
    p.runs
}

impl Parser<'_> {
    fn peek(&self, ahead: usize) -> u8 {
        if self.at + ahead < self.frame.end {
            self.src[self.at + ahead]
        } else {
            0
        }
    }

    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.pending);
        // Preserve the old parser's run boundaries: only pending plain text
        // coalesces, and only with a run plain in this frame BEFORE inheritance.
        if self.frame.last_plain {
            self.runs.last_mut().unwrap().text.push_str(&text);
        } else {
            self.runs.push(Run {
                text,
                bold: self.frame.bold,
                italic: self.frame.italic,
                href: self.frame.href.to_string(),
                code: false,
            });
        }
        self.frame.last_plain = true;
    }

    fn push(&mut self, mut run: Run) {
        self.flush();
        if run.text.is_empty() {
            return;
        }
        self.frame.last_plain = !run.bold && !run.italic && !run.code && run.href.is_empty();
        run.bold |= self.frame.bold;
        run.italic |= self.frame.italic;
        if run.href.is_empty() {
            run.href = self.frame.href.to_string();
        }
        self.runs.push(run);
    }

    fn enter(
        &mut self,
        start: usize,
        end: usize,
        resume: usize,
        bold: bool,
        italic: bool,
        href: String,
    ) {
        self.flush();
        let child = Frame {
            start,
            end,
            resume,
            first_run: self.runs.len(),
            bold: bold || self.frame.bold,
            italic: italic || self.frame.italic,
            plain_wrapper: !bold && !italic && href.is_empty(),
            href: if href.is_empty() {
                self.frame.href.clone()
            } else {
                href.into()
            },
            last_plain: false,
            closers: Default::default(),
        };
        self.parents.push(std::mem::replace(&mut self.frame, child));
        self.at = start;
    }

    fn run(&mut self, link: &dyn Fn(&str) -> String) {
        let mut checkpoint = 0;
        loop {
            if self.at >= checkpoint {
                if self.scan.cancelled() {
                    return;
                }
                checkpoint = self.at + 4096;
            }
            if self.at >= self.frame.end {
                self.flush();
                let Some(parent) = self.parents.pop() else {
                    break;
                };
                let child = std::mem::replace(&mut self.frame, parent);
                if self.runs.len() > child.first_run {
                    self.frame.last_plain = child.last_plain && child.plain_wrapper;
                }
                self.at = child.resume;
                continue;
            }
            let c = self.peek(0);
            match c {
                b'\\' if self.peek(1).is_ascii_punctuation() => {
                    self.pending.push(self.peek(1) as char);
                    self.at += 2;
                }
                b'`' if self.code_span() => {}
                b'!' if self.peek(1) == b'[' => {
                    if let Some((close, end)) = self.bracketed(self.at + 1) {
                        let label = &self.text[self.at + 2..close];
                        let target = target(&self.text[close + 2..end]);
                        self.pending
                            .push_str(if label.is_empty() { target } else { label });
                        self.at = end + 1;
                    } else {
                        self.pending.push('!');
                        self.at += 1;
                    }
                }
                b'[' => {
                    if let Some((close, end)) = self.bracketed(self.at) {
                        let href = link(target(&self.text[close + 2..end]));
                        self.enter(self.at + 1, close, end + 1, false, false, href);
                    } else {
                        self.pending.push('[');
                        self.at += 1;
                    }
                }
                b'<' if self.html_image() => {}
                b'<' if self.autolink(link) => {}
                b'_' if self.intraword_underscores() => {}
                b'*' | b'_' if self.emphasis(c) => {}
                b'h' if self.bare_url(link) => {}
                _ => {
                    // Copy ordinary text in bounded chunks. Only ASCII can
                    // begin syntax, so the scan can skip whole UTF-8 scalars.
                    let first = (self.at + utf8_width(c)).min(self.frame.end);
                    let mut limit = (first + 4096).min(self.frame.end);
                    while !self.text.is_char_boundary(limit) {
                        limit -= 1;
                    }
                    let end = self.src[first..limit]
                        .iter()
                        .position(|c| {
                            matches!(c, b'\\' | b'`' | b'!' | b'[' | b'<' | b'*' | b'_' | b'h')
                        })
                        .map_or(limit, |offset| first + offset);
                    self.pending.push_str(&self.text[self.at..end]);
                    self.at = end;
                }
            }
        }
    }

    fn code_span(&mut self) -> bool {
        let Some((open, close, end)) = self.code_bounds() else {
            return false;
        };
        let text = &self.text[open..close];
        let text = text
            .strip_prefix(' ')
            .and_then(|t| t.strip_suffix(' '))
            .unwrap_or(text);
        self.push(Run {
            text: text.to_string(),
            code: true,
            ..Run::default()
        });
        self.at = end;
        true
    }

    fn code_bounds(&mut self) -> Option<(usize, usize, usize)> {
        let ticks = self
            .ticks
            .get_or_insert_with(|| tick_matches(self.src, self.scan));
        let (open, close) = ticks[self.at];
        if close >= self.frame.end {
            return None;
        }
        let count = open - self.at;
        (close + count <= self.frame.end).then_some((open, close, close + count))
    }

    fn bracketed(&mut self, start: usize) -> Option<(usize, usize)> {
        let pairs = self
            .brackets
            .get_or_insert_with(|| matching(self.src, b'[', b']', self.scan));
        let close = pairs[start];
        if close >= self.frame.end || self.src.get(close + 1) != Some(&b'(') {
            return None;
        }
        let pairs = self
            .parens
            .get_or_insert_with(|| matching(self.src, b'(', b')', self.scan));
        let end = pairs[close + 1];
        (end < self.frame.end).then_some((close, end))
    }

    fn autolink(&mut self, link: &dyn Fn(&str) -> String) -> bool {
        let Some((close, is_mail)) = self.autolink_bounds() else {
            return false;
        };
        let inner = &self.text[self.at + 1..close];
        let href = if is_mail {
            format!("mailto:{inner}")
        } else {
            link(inner)
        };
        self.push(Run {
            text: inner.to_string(),
            href,
            ..Run::default()
        });
        self.at = close + 1;
        true
    }

    fn autolink_bounds(&mut self) -> Option<(usize, bool)> {
        let angles = self
            .angles
            .get_or_insert_with(|| angle_matches(self.src, self.scan));
        let angle = angles.get(angles.partition_point(|a| a.close < self.at))?;
        let close = angle.close;
        if close >= self.frame.end {
            return None;
        }
        let inner = &self.text[self.at + 1..close];
        let is_url = inner.starts_with("http://") || inner.starts_with("https://");
        let is_mail = !is_url && angle.mail > self.at && angle.invalid <= self.at;
        (is_url || is_mail).then_some((close, is_mail))
    }

    /// An `<img>` or `<picture>` (LLP 1033 D9): the caller's run for each
    /// of its images, or each one's alt text.
    fn html_image(&mut self) -> bool {
        let Some(markup) = self.html_markup() else {
            return false;
        };
        for image in markup.images {
            match self.html.inline {
                Some(run) => self.push(run(image)),
                None => self.pending.push_str(&image.alt),
            }
        }
        self.at += markup.len;
        true
    }

    fn html_markup(&self) -> Option<crate::html::Markup> {
        let rest = &self.text[self.at..self.frame.end];
        let links = self.html.links();
        if !self.html.images || !crate::html::starts_image(rest, links.is_some()) {
            return None;
        }
        let mut limit = rest.len().min(crate::html::MARKUP_LIMIT);
        while !rest.is_char_boundary(limit) {
            limit -= 1;
        }
        // The paragraph's end closes a picture left open.
        let ends = limit == rest.len() && self.frame.end == self.text.len();
        crate::html::element(&rest[..limit], self.html.schemes, links, ends)
    }

    fn bare_url(&mut self, link: &dyn Fn(&str) -> String) -> bool {
        let Some(len) = self.bare_url_len() else {
            return false;
        };
        let url = &self.text[self.at..self.at + len];
        self.push(Run {
            text: url.to_string(),
            href: link(url),
            ..Run::default()
        });
        self.at += len;
        true
    }

    fn bare_url_len(&mut self) -> Option<usize> {
        let end = self.bare_url_end()?;
        self.url_len_to(end)
    }

    fn bare_url_end(&mut self) -> Option<usize> {
        let rest = &self.text[self.at..self.frame.end];
        if !rest.starts_with("http://") && !rest.starts_with("https://") {
            return None;
        }
        // Adjacent emphasis frames can start URLs in the same uninterrupted
        // candidate. Find its terminator once, then bound it by each frame.
        if self.url.as_ref().is_none_or(|url| !url.contains(&self.at)) {
            let rest = &self.text[self.at..];
            let len = rest
                .find(|c: char| c.is_whitespace() || c == '<' || c == ')' || c == '"')
                .unwrap_or(rest.len());
            self.url = Some(self.at..self.at + len);
        }
        Some(self.url.as_ref().unwrap().end.min(self.frame.end))
    }

    fn url_len_to(&self, end: usize) -> Option<usize> {
        let url = self.text[self.at..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        (url.len() >= 12).then_some(url.len())
    }

    fn emphasis_url_len(&mut self, marker: u8, count: usize) -> Option<usize> {
        let mut end = self.bare_url_end()?;
        let mut at = self.at;
        while at < end {
            if at.is_multiple_of(4096) && self.scan.cancelled() {
                return None;
            }
            if self.src[at] == marker {
                let first = at;
                while at < end && self.src[at] == marker {
                    at += 1;
                    if at.is_multiple_of(4096) && self.scan.cancelled() {
                        return None;
                    }
                }
                if let Some(close) = self.closing_marker(marker, count, first, at) {
                    end = close;
                    break;
                }
            } else {
                at += 1;
            }
        }
        self.url_len_to(end)
    }

    fn intraword_underscores(&mut self) -> bool {
        if !self.text[self.frame.start..self.at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            return false;
        }
        // Reject the whole run so a second underscore cannot open italic.
        let start = self.at;
        while self.peek(0) == b'_' {
            if self.at.is_multiple_of(4096) && self.scan.cancelled() {
                self.at = self.frame.end;
                return true;
            }
            self.at += 1;
        }
        self.pending.push_str(&self.text[start..self.at]);
        true
    }

    fn emphasis(&mut self, marker: u8) -> bool {
        let count = if self.peek(1) == marker { 2 } else { 1 };
        if matches!(self.peek(count), 0 | b' ' | b'\t') {
            return false;
        }
        let close = self.emphasis_close(marker, count);
        let open = self.at + count;
        let Some(close) = close else {
            return false;
        };
        if close + count > self.frame.end {
            return false;
        }
        self.enter(
            open,
            close,
            close + count,
            count == 2,
            count == 1,
            String::new(),
        );
        true
    }

    fn closing_marker(&self, marker: u8, count: usize, first: usize, end: usize) -> Option<usize> {
        let intraword = marker == b'_'
            && self.text[end..self.frame.end]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric);
        let allowed = first + usize::from(first == 0 || self.src[first - 1] == b' ');
        (!intraword && allowed + count <= end).then_some(allowed)
    }

    // Cache suffix searches by token boundary and scope end. URL recognition
    // uses the same end as parsing, including any nested emphasis closer.
    fn emphasis_close(&mut self, marker: u8, count: usize) -> Option<usize> {
        let start = self.at;
        let end = self.frame.end;
        let mut search = EmphasisSearch::new(start, marker, count, end);
        let mut parents = Vec::new();
        self.at += count;
        let mut checkpoint = self.at;
        let result = 'search: loop {
            self.frame.end = search.scopes.last().unwrap().0;
            let close = loop {
                if self.at >= checkpoint {
                    if self.scan.cancelled() {
                        break 'search None;
                    }
                    checkpoint = self.at + 4096;
                }
                if self.at >= self.frame.end {
                    if search.scopes.len() == 1 {
                        break None;
                    }
                    let (first, resume) = search.scopes.pop().unwrap();
                    self.frame.end = search.scopes.last().unwrap().0;
                    if self.src[first] == search.marker {
                        let mut last = first;
                        while last < self.frame.end && self.src[last] == search.marker {
                            last += 1;
                            if last.is_multiple_of(4096) && self.scan.cancelled() {
                                break 'search None;
                            }
                        }
                        if let Some(close) =
                            self.closing_marker(search.marker, search.count, first, last)
                        {
                            break Some(close);
                        }
                    }
                    self.at = resume;
                    continue;
                }
                let key = (self.frame.end, self.at);
                if let Some(&close) = self.frame.closers[search.index()].get(&key) {
                    match close {
                        Some(_) => break close,
                        None => {
                            self.at = self.frame.end;
                            continue;
                        }
                    }
                }
                search.visited.push(key);
                let c = self.peek(0);
                let opaque_end = match c {
                    b'\\' if self.peek(1).is_ascii_punctuation() => Some(self.at + 2),
                    b'`' => self.code_bounds().map(|(_, _, end)| end),
                    b'!' if self.peek(1) == b'[' => {
                        self.bracketed(self.at + 1).map(|(_, end)| end + 1)
                    }
                    b'[' => self.bracketed(self.at).map(|(_, end)| end + 1),
                    b'<' => self
                        .html_markup()
                        .map(|m| self.at + m.len)
                        .or_else(|| self.autolink_bounds().map(|(end, _)| end + 1)),
                    b'h' => self
                        .emphasis_url_len(search.marker, search.count)
                        .map(|len| self.at + len),
                    _ => None,
                };
                if let Some(end) = opaque_end {
                    self.at = end;
                    continue;
                }
                if matches!(c, b'*' | b'_') {
                    let first = self.at;
                    while self.peek(0) == c {
                        if self.at.is_multiple_of(4096) && self.scan.cancelled() {
                            break 'search None;
                        }
                        self.at += 1;
                    }
                    let last = self.at;
                    if c == search.marker {
                        if let Some(close) = self.closing_marker(c, search.count, first, last) {
                            for at in first..=last - search.count {
                                if at.is_multiple_of(4096) && self.scan.cancelled() {
                                    break 'search None;
                                }
                                self.frame.closers[search.index()]
                                    .insert((self.frame.end, at), Some(at.max(close)));
                            }
                            break Some(close);
                        }
                        // A shorter suffix can open nested italic in a strong
                        // search, so it is not the same failed suffix search.
                        for at in first + 1..last.saturating_sub(search.count - 1) {
                            if at.is_multiple_of(4096) && self.scan.cancelled() {
                                break 'search None;
                            }
                            search.visited.push((self.frame.end, at));
                        }
                    }
                    let count = (last - first).min(2);
                    let intraword = c == b'_'
                        && self.text[self.frame.start..first]
                            .chars()
                            .next_back()
                            .is_some_and(char::is_alphanumeric);
                    let after = self.src.get(first + count).copied().unwrap_or(0);
                    if !intraword
                        && first + count < self.frame.end
                        && !matches!(after, b' ' | b'\t')
                        && (c != search.marker || count != search.count)
                    {
                        let child = EmphasisSearch::new(first, c, count, self.frame.end);
                        parents.push(search);
                        search = child;
                        self.at = first + count;
                        checkpoint = self.at;
                        continue 'search;
                    }
                } else {
                    let first = (self.at + utf8_width(c)).min(self.frame.end);
                    let mut limit = (first + 4096).min(self.frame.end);
                    while !self.text.is_char_boundary(limit) {
                        limit -= 1;
                    }
                    self.at = self.src[first..limit]
                        .iter()
                        .position(|&c| {
                            matches!(c, b'\\' | b'`' | b'!' | b'[' | b'<' | b'h' | b'*' | b'_')
                        })
                        .map_or(limit, |offset| first + offset);
                }
            };
            let index = search.index();
            for (i, (bound, at)) in search.visited.into_iter().enumerate() {
                if i.is_multiple_of(4096) && self.scan.cancelled() {
                    break 'search None;
                }
                self.frame.closers[index]
                    .insert((bound, at), close.filter(|c| c + search.count <= bound));
            }
            let Some(parent) = parents.pop() else {
                break close;
            };
            self.at = search.start + 1;
            if let Some(close) = close {
                self.at = search.start + search.count;
                let scope = (close, close + search.count);
                search = parent;
                search.scopes.push(scope);
            } else {
                search = parent;
            }
            checkpoint = self.at;
        };
        self.at = start;
        self.frame.end = end;
        result
    }
}

fn target(inside: &str) -> &str {
    let inside = inside.trim();
    let target = match inside.find(['"', '\'']) {
        Some(cut) => inside[..cut].trim(),
        None => inside,
    };
    target.trim_start_matches('<').trim_end_matches('>')
}

fn angle_matches(src: &[u8], scan: &Scan<'_>) -> Vec<Angle> {
    let mut angles = Vec::new();
    let mut mail = 0;
    let mut invalid = 0;
    for (at, &byte) in src.iter().enumerate() {
        if at.is_multiple_of(4096) && scan.cancelled() {
            break;
        }
        match byte {
            b'>' => angles.push(Angle {
                close: at,
                mail,
                invalid,
            }),
            b'@' => mail = at,
            b' ' | b'/' => invalid = at,
            _ => {}
        }
    }
    angles
}

// Direct-address indexes have no sorting or per-opener suffix scan. Unmatched
// openers retain a sentinel, shared by every nested frame of this paragraph.
fn matching(src: &[u8], open: u8, close: u8, scan: &Scan<'_>) -> Vec<usize> {
    let mut pairs = vec![src.len(); src.len()];
    let mut stack = Vec::new();
    let mut at = 0;
    let mut checkpoint = 0;
    while at < src.len() {
        if at >= checkpoint {
            if scan.cancelled() {
                break;
            }
            checkpoint = at + 4096;
        }
        match src[at] {
            b'\\' => at += 1,
            c if c == open => stack.push(at),
            c if c == close => {
                if let Some(start) = stack.pop() {
                    pairs[start] = at;
                }
            }
            _ => {}
        }
        at += 1;
    }
    pairs
}

fn tick_matches(src: &[u8], scan: &Scan<'_>) -> Vec<(usize, usize)> {
    let mut pairs = vec![(src.len(), src.len()); src.len()];
    let mut next = vec![src.len(); src.len() + 1];
    let mut end = src.len();
    while end > 0 {
        if end.is_multiple_of(4096) && scan.cancelled() {
            break;
        }
        if src[end - 1] != b'`' {
            end -= 1;
            continue;
        }
        let mut start = end - 1;
        while start > 0 && src[start - 1] == b'`' {
            if start.is_multiple_of(4096) && scan.cancelled() {
                return pairs;
            }
            start -= 1;
        }
        // A failed opener loses ONE tick in the existing grammar. Every suffix
        // of this run can therefore open; only whole later runs can close it.
        for at in start..end {
            if at.is_multiple_of(4096) && scan.cancelled() {
                return pairs;
            }
            pairs[at] = (end, next[end - at]);
        }
        next[end - start] = start;
        end = start;
    }
    pairs
}

fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}
