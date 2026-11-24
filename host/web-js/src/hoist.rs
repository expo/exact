//! Region arms as sibling functions (LLP 1071, "Arms as siblings"). V8
//! flushes the bytecode of functions idle for a while, and compiling a
//! function again reads all the source inside it: a nested `else` chain then
//! rereads every later arm at each level, and a `for` body every page it
//! holds.
//!
//! The emitter writes each `when`/`match` arm, `for` row, slot's content,
//! shared mount, and a virtualized list's row text, its regions and fixed
//! mounts, as an arrow whose body sits between `OPEN` and `CLOSE`. `lift`
//! moves each such body of `MIN` bytes or more that another one encloses to
//! the top of the view as `const $h<n>=…`. It takes, after its own
//! parameters, the names it reads from the arrows and blocks between it and
//! the top, under the same names, so its text is unchanged; the arrow left
//! in its place passes them when it is called. A body stays where an
//! argument could differ from what the closure would read: a `let` or
//! `var`, a name written anywhere, a binding declared further on or still
//! initializing where the arm is, a loop head's or a pattern's name, or a
//! head that is more than names. A view with what the emitter does not
//! write (`UNMODELLED`, a destructuring assignment or default) stays whole.
use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};

/// The byte before an arm's body. Never in emitted string literals, which
/// JSON-escape control characters.
pub const OPEN: char = '\u{1}';
/// The byte after it.
pub const CLOSE: char = '\u{2}';
/// Bodies shorter than this stay in place: a function that keeps them reads
/// a few KB of arms it may not run, which costs microseconds to compile.
pub const MIN: usize = 4096;
/// More than this many arguments and a body stays: V8 refuses a function
/// of 65,535 parameters.
const MAX_PARAMS: usize = 1000;

pub fn mark(body: &str) -> String {
    format!("{OPEN}{body}{CLOSE}")
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Block,
    /// An arrow's body block.
    Arrow,
    /// A concise arrow body, which ends at a `,`, `;` or `:` of its own
    /// depth, or at a closer below it.
    Expression,
    /// A loop head's declarations, until the block around the loop ends.
    Head,
}

struct Scope<'a> {
    kind: Kind,
    depth: usize,
    /// Its place in the order scopes open, the same in both passes.
    ordinal: usize,
    names: Vec<&'a str>,
    params: Vec<&'a str>,
    /// An arrow body's head, `(…)=>` or `name=>`, from this offset, when
    /// it is a plain list of names.
    head: Option<usize>,
    /// A concise body's `?`s still waiting for their `:`.
    ternaries: usize,
}

/// Where a binding stands at the point being read.
#[derive(Clone, Copy, PartialEq)]
enum State {
    /// Declared further on in its scope.
    Later,
    /// Its initializer is running.
    Pending,
    Ready,
}

#[derive(Clone, Copy)]
struct Binding {
    scope: usize,
    mutable: bool,
    state: State,
}

/// A marked body being read.
struct Arm {
    /// The scope its own parameters are declared in.
    scope: usize,
    /// Where its arrow's head and its body start in the output.
    head: usize,
    start: usize,
    params: Vec<String>,
    /// Names it reads from outside itself, below the view's top: the scope
    /// declaring each, and whether an argument could differ from what the
    /// closure reads (a mutable binding, a write, or a binding not yet
    /// initialized where the arm is).
    free: HashMap<String, (usize, bool)>,
    fixed: bool,
}

#[derive(Default)]
struct Lifted {
    declarations: String,
    count: usize,
    /// By the hash of a declaration's text after its name: where that text
    /// is in `declarations`, and its name.
    seen: HashMap<u64, Vec<(usize, usize, usize)>>,
}

impl Lifted {
    /// The name declared as `text`, declaring it if it is new.
    fn name(&mut self, text: &str) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let same = self.seen.entry(hasher.finish()).or_default();
        if let Some(&(_, _, n)) = same
            .iter()
            .find(|(from, to, _)| &self.declarations[*from..*to] == text)
        {
            return n;
        }
        let n = self.count;
        self.count += 1;
        write!(self.declarations, "const $h{n}").unwrap();
        let from = self.declarations.len();
        self.declarations.push_str(text);
        same.push((from, self.declarations.len(), n));
        n
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Token {
    /// A name, an operand, or `)`/`]`: a `/` after it divides.
    Operand,
    /// An operator or opening punctuation: a `/` after it starts a regex.
    Operator,
}

const REGEX_AFTER: [&str; 14] = [
    "return",
    "typeof",
    "case",
    "do",
    "else",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "instanceof",
    "await",
    "yield",
];

fn word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Whether a name between the code `before` and `after` it is assigned.
fn writes(before: &[u8], after: &[u8]) -> bool {
    const ASSIGN: [&[u8]; 17] = [
        b"++", b"--", b"+=", b"-=", b"*=", b"/=", b"%=", b"&=", b"|=", b"^=", b"**=", b"??=",
        b"||=", b"&&=", b"<<=", b">>=", b">>>=",
    ];
    let before = before.trim_ascii_end();
    let after = after.trim_ascii_start();
    before.ends_with(b"++")
        || before.ends_with(b"--")
        || (after.first() == Some(&b'=') && !matches!(after.get(1), Some(b'=' | b'>')))
        || ASSIGN.iter().any(|op| after.starts_with(op))
}

/// The view with its marked bodies lifted and the markers gone. Its
/// declarations go first, inside the block that encloses the whole view
/// when there is one.
pub fn lift(view: &str, min: usize) -> Result<String, String> {
    if !view.contains(OPEN) {
        return Ok(view.to_string());
    }
    // The root's shared mounts are in a block around the view: lifted
    // bodies go inside it, where they can name them.
    if let Some(inner) = view
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .filter(|inner| encloses(inner))
    {
        let (declarations, body) = Lifter::lift(inner, min)?;
        return Ok(format!("{{{declarations}{body}}}"));
    }
    let (declarations, body) = Lifter::lift(view, min)?;
    Ok(declarations + &body)
}

/// Whether `{` + `inner` + `}` is one block: outside literals, its braces
/// balance and never close below zero.
fn encloses(inner: &str) -> bool {
    let mut depth = 0i64;
    let mut l = Lexer::new(inner);
    while let Some((c, _)) = l.next_code() {
        match c {
            b'{' => depth += 1,
            b'}' if depth == 0 => return false,
            b'}' => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

struct Lexer<'a> {
    b: &'a [u8],
    i: usize,
    last: Token,
    /// Per open `{`: whether it opened a template substitution.
    braces: Vec<bool>,
    /// A substitution's `${`, read as a `[` before the code inside it.
    queued: Option<(u8, usize)>,
    /// Per open `(`: whether it is a control statement's head, after which
    /// a `/` starts a regex.
    parens: Vec<bool>,
    /// The last word was `if`, `while`, `for` or `with`.
    control: bool,
    /// Whether the last `+` or `-` followed an operand, so that its pair
    /// (`x++`) is one too.
    sign_after_operand: bool,
}

impl<'a> Lexer<'a> {
    fn new(js: &'a str) -> Self {
        Lexer {
            b: js.as_bytes(),
            i: 0,
            last: Token::Operator,
            braces: Vec::new(),
            queued: None,
            parens: Vec::new(),
            control: false,
            sign_after_operand: false,
        }
    }

    /// A word's text: its bytes are ASCII.
    fn text(&self, from: usize, to: usize) -> &'a str {
        std::str::from_utf8(&self.b[from..to]).unwrap()
    }

    fn quoted(&mut self, q: u8) {
        self.i += 1;
        while self.i < self.b.len() && self.b[self.i] != q {
            self.i += if self.b[self.i] == b'\\' { 2 } else { 1 };
        }
        self.i += 1;
    }

    /// Reads template text from `self.i` to its end or to a `${`.
    fn template(&mut self) {
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'\\' => self.i += 2,
                b'`' => {
                    self.i += 1;
                    self.last = Token::Operand;
                    return;
                }
                b'$' if self.b.get(self.i + 1) == Some(&b'{') => {
                    self.queued = Some((b'[', self.i));
                    self.i += 2;
                    self.braces.push(true);
                    self.last = Token::Operator;
                    return;
                }
                _ => self.i += 1,
            }
        }
    }

    fn regex(&mut self) {
        self.i += 1;
        let mut class = false;
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'\\' => self.i += 1,
                b'[' => class = true,
                b']' => class = false,
                b'/' if !class => break,
                _ => {}
            }
            self.i += 1;
        }
        self.i += 1;
        while self.i < self.b.len() && word(self.b[self.i]) {
            self.i += 1;
        }
        self.last = Token::Operand;
    }

    /// The next byte of code (a word's first byte stands for the word,
    /// which `self.i` then ends), skipping literals and comments.
    fn next_code(&mut self) -> Option<(u8, usize)> {
        if let Some(open) = self.queued.take() {
            return Some(open);
        }
        loop {
            let at = self.i;
            let c = *self.b.get(at)?;
            match c {
                b'"' | b'\'' => {
                    self.quoted(c);
                    self.last = Token::Operand;
                }
                b'`' => {
                    self.i += 1;
                    self.template();
                    if let Some(open) = self.queued.take() {
                        return Some(open);
                    }
                }
                b'/' if self.b.get(at + 1) == Some(&b'/') => {
                    while self.i < self.b.len() && self.b[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                b'/' if self.b.get(at + 1) == Some(&b'*') => {
                    self.i += 2;
                    while self.i + 1 < self.b.len()
                        && !(self.b[self.i] == b'*' && self.b[self.i + 1] == b'/')
                    {
                        self.i += 1;
                    }
                    self.i += 2;
                }
                b'/' if self.last == Token::Operator => self.regex(),
                // A substitution's `}`: it reads as a `]`, as its `${` does a `[`.
                b'}' if self.braces.last() == Some(&true) => {
                    self.braces.pop();
                    self.i += 1;
                    self.template();
                    return Some((b']', at));
                }
                c if c.is_ascii_whitespace() => self.i += 1,
                c if word(c) => {
                    while self.i < self.b.len() && word(self.b[self.i]) {
                        self.i += 1;
                    }
                    let w = self.text(at, self.i);
                    self.control = matches!(w, "if" | "while" | "for" | "with");
                    self.last = if REGEX_AFTER.contains(&w) {
                        Token::Operator
                    } else {
                        Token::Operand
                    };
                    return Some((c, at));
                }
                c => {
                    self.i += 1;
                    let before = self.last;
                    self.last = if matches!(c, b')' | b']' | b'}') {
                        Token::Operand
                    } else {
                        Token::Operator
                    };
                    match c {
                        b'{' => self.braces.push(false),
                        b'}' => {
                            self.braces.pop();
                        }
                        b'(' => self.parens.push(self.control),
                        // After `if (…)` a statement starts.
                        b')' if self.parens.pop() == Some(true) => self.last = Token::Operator,
                        b'+' | b'-' if at > 0 && self.b[at - 1] == c => {
                            if self.sign_after_operand {
                                self.last = Token::Operand;
                            }
                        }
                        b'+' | b'-' => self.sign_after_operand = before == Token::Operand,
                        _ => {}
                    }
                    self.control = false;
                    return Some((c, at));
                }
            }
        }
    }
}

/// A `const`, `let` or `var` being read.
struct Declaring<'a> {
    /// The depth of its declarators.
    depth: usize,
    /// Whether the next word is a name it declares.
    expect: bool,
    mutable: bool,
    /// A `var`, whose names belong to the function.
    var: bool,
    /// Inside a destructuring pattern, from this depth.
    pattern: Option<usize>,
    /// Its names whose initializer is running, with their scopes.
    pending: Vec<(&'a str, usize)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Paren {
    Plain,
    /// A `catch`'s parameter.
    Catch,
    /// A `for` loop's head.
    For,
}

/// What the emitter does not write and this reader does not follow: a view
/// that has one is left as it is.
const UNMODELLED: [&str; 7] = [
    "function",
    "this",
    "arguments",
    "super",
    "class",
    "with",
    "eval",
];

struct Lifter<'a> {
    src: &'a str,
    min: usize,
    lexer: Lexer<'a>,
    scopes: Vec<Scope<'a>>,
    opened: usize,
    /// Per scope, by ordinal, the names the first pass found it declares
    /// and whether each is mutable (a `let` or `var`, or written anywhere);
    /// the second pass binds them on entry.
    known: Vec<Vec<(&'a str, bool)>>,
    collecting: bool,
    /// Each name bound in an open scope, innermost last.
    bound: HashMap<&'a str, Vec<Binding>>,
    /// Open brackets of every kind.
    depth: usize,
    /// Per open `{`: whether it opened a scope (a pattern's does not).
    braces: Vec<bool>,
    /// Per open `(`: its offset, where the names read inside it start in
    /// `group`, and what it holds.
    parens: Vec<(usize, usize, Paren)>,
    group: Vec<&'a str>,
    /// The last `(…)` closed: its offsets and names, while it may still be
    /// an arrow's head.
    closed: Option<(usize, usize, Vec<&'a str>)>,
    /// Per open `[`: whether it is a member's (`x[k]`), not a pattern's.
    brackets: Vec<bool>,
    /// Whether the last `]` closed a member's `[`.
    member: bool,
    /// The view has what `UNMODELLED` names, or a destructuring
    /// assignment or default.
    unmodelled: bool,
    /// The next `{` opens this body: its kind, parameters, and its arrow's
    /// head when that is a plain list of names.
    pending: Option<(Kind, Vec<&'a str>, Option<usize>)>,
    declaring: Vec<Declaring<'a>>,
    /// The previous word and the one before it.
    words: [&'a str; 2],
    /// The previous code byte, `w` for a word.
    prev: u8,
    /// The previous word's offset, as an arrow's head.
    prev_word_at: usize,
    arms: Vec<Arm>,
    out: String,
    copied: usize,
    /// After a lifted arm's `CLOSE`, the `}` that ends its arrow is dropped.
    drop_brace: bool,
    lifted: Lifted,
}

impl<'a> Lifter<'a> {
    fn new(src: &'a str, min: usize, known: Option<Vec<Vec<(&'a str, bool)>>>) -> Self {
        let mut l = Lifter {
            src,
            min,
            lexer: Lexer::new(src),
            scopes: Vec::new(),
            opened: 0,
            collecting: known.is_none(),
            known: known.unwrap_or_default(),
            bound: HashMap::new(),
            depth: 0,
            braces: Vec::new(),
            parens: Vec::new(),
            group: Vec::new(),
            closed: None,
            brackets: Vec::new(),
            member: false,
            unmodelled: false,
            pending: None,
            declaring: Vec::new(),
            words: ["", ""],
            prev: b';',
            prev_word_at: 0,
            arms: Vec::new(),
            out: String::new(),
            copied: 0,
            drop_brace: false,
            lifted: Lifted::default(),
        };
        l.push(Kind::Block, &[], None);
        l
    }

    /// The view's lifted declarations and the view with its arms replaced.
    /// A first pass finds each scope's declarations and the names written
    /// anywhere, so that the second knows a name declared further on in a
    /// scope it is in, and one written outside the arm that reads it.
    fn lift(src: &'a str, min: usize) -> Result<(String, String), String> {
        let mut first = Lifter::new(src, min, None);
        first.read()?;
        if first.unmodelled {
            return Ok((String::new(), src.replace([OPEN, CLOSE], "")));
        }
        let mut second = Lifter::new(src, min, Some(first.known));
        second.read()?;
        second.copy(src.len());
        Ok((second.lifted.declarations, second.out))
    }

    /// Copies the source up to `to` into the output (the second pass's).
    fn copy(&mut self, to: usize) {
        if !self.collecting {
            self.out.push_str(&self.src[self.copied..to]);
        }
        self.copied = to;
    }

    fn bind(&mut self, at: usize, name: &'a str, mutable: bool, state: State) {
        if self.collecting {
            self.known[self.scopes[at].ordinal].push((name, mutable));
        }
        // Innermost last: a `var` binds in its function, outside the block
        // it is written in.
        let stack = self.bound.entry(name).or_default();
        let below = stack.iter().rposition(|b| b.scope <= at);
        match below.map(|k| &mut stack[k]) {
            Some(b) if b.scope == at => {
                b.mutable |= mutable;
                b.state = state;
            }
            _ => {
                let k = below.map_or(0, |k| k + 1);
                stack.insert(
                    k,
                    Binding {
                        scope: at,
                        mutable,
                        state,
                    },
                );
                self.scopes[at].names.push(name);
            }
        }
    }

    fn push(&mut self, kind: Kind, params: &[&'a str], head: Option<usize>) {
        let ordinal = self.opened;
        self.opened += 1;
        if self.collecting {
            self.known.push(Vec::new());
        }
        self.scopes.push(Scope {
            kind,
            depth: self.depth,
            ordinal,
            names: Vec::new(),
            params: params.to_vec(),
            head,
            ternaries: 0,
        });
        let at = self.scopes.len() - 1;
        if !self.collecting {
            for k in 0..self.known.get(ordinal).map_or(0, Vec::len) {
                let (name, mutable) = self.known[ordinal][k];
                self.bind(at, name, mutable, State::Later);
            }
        }
        for p in params {
            self.bind(at, p, false, State::Ready);
        }
    }

    fn pop(&mut self) {
        let scope = self.scopes.pop().expect("an open scope");
        for name in scope.names {
            if let Some(stack) = self.bound.get_mut(name) {
                stack.pop();
                if stack.is_empty() {
                    self.bound.remove(name);
                }
            }
        }
    }

    fn top(&self) -> &Scope<'a> {
        self.scopes.last().expect("the view's scope")
    }

    /// Ends the concise arrow bodies that a `,` or `;` at this depth ends.
    fn end_expressions(&mut self, depth: usize) {
        while self.top().kind == Kind::Expression && self.top().depth >= depth {
            self.pop();
        }
    }

    /// The names declarator list `d` has been declaring are initialized.
    fn initialized(&mut self, d: usize) {
        for (name, scope) in std::mem::take(&mut self.declaring[d].pending) {
            let bound = self.bound.get_mut(name).into_iter().flatten();
            if let Some(b) = bound.rev().find(|b| b.scope == scope) {
                b.state = State::Ready;
            }
        }
    }

    /// After a closer: ends the pattern it closes, and the declarator
    /// lists below their depth.
    fn closed_bracket(&mut self) {
        if let Some(d) = self.declaring.last_mut() {
            if d.pattern == Some(self.depth + 1) {
                d.pattern = None;
                d.expect = false;
            }
        }
        while self.declaring.last().is_some_and(|d| d.depth > self.depth) {
            let d = self.declaring.len() - 1;
            self.initialized(d);
            self.declaring.pop();
        }
    }

    /// Whether the next bracket opens a pattern the open declaration binds.
    fn opens_pattern(&mut self) -> bool {
        let depth = self.depth;
        match self.declaring.last_mut() {
            Some(d) if d.expect && d.pattern.is_none() && d.depth == depth => {
                d.pattern = Some(depth + 1);
                true
            }
            _ => false,
        }
    }

    fn reference(&mut self, name: &'a str, writes: bool) {
        let Some(&b) = self.bound.get(name).and_then(|s| s.last()) else {
            return;
        };
        if self.collecting {
            // A binding written anywhere is mutable to every arm that reads it.
            if writes {
                let ordinal = self.scopes[b.scope].ordinal;
                if let Some(k) = self.known[ordinal].iter().position(|k| k.0 == name) {
                    self.known[ordinal][k].1 = true;
                }
            }
            return;
        }
        // The view's top is where lifted bodies are: they read it as the
        // closures did.
        if b.scope == 0 {
            return;
        }
        let unlike = b.mutable || writes || b.state != State::Ready;
        if let Some(arm) = self.arms.last_mut().filter(|a| a.scope > b.scope) {
            let entry = arm.free.entry(name.to_string()).or_insert((b.scope, false));
            entry.1 |= unlike;
        }
    }

    fn read(&mut self) -> Result<(), String> {
        while let Some((c, at)) = self.lexer.next_code() {
            if c == OPEN as u8 {
                self.open(at)?;
                continue;
            }
            if c == CLOSE as u8 {
                self.close(at)?;
                continue;
            }
            if self.drop_brace {
                if c != b'}' {
                    return Err(format!("lift: an arm's body is not its arrow's at {at}"));
                }
                self.drop_brace = false;
                self.copied = at + 1;
            }
            self.token(c, at);
        }
        if !self.arms.is_empty() || self.scopes.len() != 1 {
            return Err("lift: the view's arms or braces do not close".into());
        }
        Ok(())
    }

    fn token(&mut self, c: u8, at: usize) {
        if word(c) {
            let w = self.lexer.text(at, self.lexer.i);
            self.word(w, at);
            return;
        }
        let b = self.src.as_bytes();
        match c {
            b'(' => {
                let kind = match (self.prev == b'w', self.words[0]) {
                    (true, "for") => Paren::For,
                    (true, "catch") => Paren::Catch,
                    _ => Paren::Plain,
                };
                self.parens.push((at, self.group.len(), kind));
                self.depth += 1;
            }
            b')' => {
                self.end_expressions(self.depth);
                self.depth = self.depth.saturating_sub(1);
                self.closed_bracket();
                self.closed = None;
                if let Some((open, from, kind)) = self.parens.pop() {
                    let names = self.group.split_off(from);
                    match kind {
                        // Its parameter is the block's.
                        Paren::Catch => self.pending = Some((Kind::Block, names, None)),
                        Paren::Plain => self.closed = Some((open, at, names)),
                        Paren::For => {}
                    }
                }
                if self.parens.is_empty() {
                    self.group.clear();
                }
            }
            b'[' => {
                let member = matches!(self.prev, b'w' | b')' | b']');
                let pattern = self.opens_pattern();
                self.brackets.push(member && !pattern);
                self.depth += 1;
            }
            b']' => {
                self.end_expressions(self.depth);
                self.depth = self.depth.saturating_sub(1);
                self.closed_bracket();
                self.member = self.brackets.pop().unwrap_or(false);
            }
            b'{' => {
                let depth = self.depth;
                let pattern = self.opens_pattern()
                    || (self.declaring.last())
                        .is_some_and(|d| d.pattern.is_some_and(|p| depth >= p));
                self.depth += 1;
                self.braces.push(!pattern);
                if !pattern {
                    let (kind, params, head) =
                        self.pending
                            .take()
                            .unwrap_or((Kind::Block, Vec::new(), None));
                    self.push(kind, &params, head);
                }
            }
            b'}' => {
                self.end_expressions(self.depth);
                self.depth = self.depth.saturating_sub(1);
                self.closed_bracket();
                if self.braces.pop() == Some(true) && self.scopes.len() > 1 {
                    // A loop head written in this block ends with it.
                    while self.top().kind == Kind::Head {
                        self.pop();
                    }
                    self.pop();
                }
            }
            b',' | b';' => {
                self.end_expressions(self.depth);
                if let Some(d) = self.declaring.len().checked_sub(1) {
                    if self.declaring[d].depth == self.depth && self.declaring[d].pattern.is_none()
                    {
                        self.initialized(d);
                        if c == b',' {
                            self.declaring[d].expect = true;
                        } else {
                            self.declaring.pop();
                        }
                    }
                }
                if c == b';' {}
            }
            b'?' if b.get(at + 1) != Some(&b'.')
                && b.get(at + 1) != Some(&b'?')
                && b[at - 1] != b'?' =>
            {
                let depth = self.depth;
                let top = self.scopes.last_mut().unwrap();
                if top.kind == Kind::Expression && top.depth == depth {
                    top.ternaries += 1;
                }
            }
            b':' => {
                // A concise body ends at a `:` that is not its own `?`'s.
                while self.top().kind == Kind::Expression && self.top().depth == self.depth {
                    let top = self.scopes.last_mut().unwrap();
                    if top.ternaries > 0 {
                        top.ternaries -= 1;
                        break;
                    }
                    self.pop();
                }
            }
            b'=' if b.get(at + 1) == Some(&b'>') => {
                // An arrow: its head is the last `(…)` or word.
                let (params, head) = match self.prev {
                    b')' => match self.closed.take() {
                        Some((open, close, names)) => {
                            let plain = plain_names(&self.src[open + 1..close]);
                            (names, plain.then_some(open))
                        }
                        None => (Vec::new(), None),
                    },
                    b'w' => (vec![self.words[0]], Some(self.prev_word_at)),
                    _ => (Vec::new(), None),
                };
                self.lexer.i = at + 2;
                let after = b[at + 2..].iter().find(|c| !c.is_ascii_whitespace());
                if after == Some(&b'{') {
                    self.pending = Some((Kind::Arrow, params, head));
                } else {
                    self.push(Kind::Expression, &params, None);
                }
            }
            // A destructuring assignment, or a default in a pattern, reads
            // and writes names this reader does not follow.
            b'=' if !matches!(b.get(at + 1), Some(b'=' | b'>')) => {
                let depth = self.depth;
                let in_pattern =
                    (self.declaring.last()).is_some_and(|d| d.pattern.is_some_and(|p| depth >= p));
                let assigned = self.prev == b'}' || (self.prev == b']' && !self.member);
                if in_pattern || (assigned && self.declaring.is_empty()) {
                    self.unmodelled = true;
                }
            }
            _ => {}
        }
        self.prev = c;
    }

    fn word(&mut self, w: &'a str, at: usize) {
        let b = self.src.as_bytes();
        // A property's name, not a spread's operand (`...x`).
        let property = at > 0 && b[at - 1] == b'.' && !(at > 2 && &b[at - 3..at] == b"...");
        // An object's key named like a keyword: `{const:x}`.
        let key = b[self.lexer.i..].trim_ascii_start().first() == Some(&b':');
        match w {
            _ if property => {
                // Not a keyword: `p.catch(`, `o.for(`.
                self.words = ["", self.words[0]];
                self.prev = b'w';
                self.prev_word_at = at;
                return;
            }
            "const" | "let" | "var" if !key => {
                // A loop head's declarations are the loop's.
                if self.parens.last().is_some_and(|p| p.2 == Paren::For)
                    && self.top().kind != Kind::Head
                {
                    self.depth -= 1;
                    self.push(Kind::Head, &[], None);
                    self.depth += 1;
                }
                self.declaring.push(Declaring {
                    depth: self.depth,
                    expect: true,
                    mutable: w != "const",
                    var: w == "var",
                    pattern: None,
                    pending: Vec::new(),
                });
            }
            _ if UNMODELLED.contains(&w) => self.unmodelled = true,
            _ => {
                let depth = self.depth;
                let declares = self.declaring.last().filter(|d| {
                    (d.expect && d.depth == depth) || d.pattern.is_some_and(|p| depth >= p)
                });
                if let Some(d) = declares {
                    let (var, pattern, mutable) = (d.var, d.pattern.is_some(), d.mutable);
                    let at = if var {
                        (self.scopes.iter())
                            .rposition(|s| s.kind == Kind::Arrow)
                            .unwrap_or(0)
                    } else {
                        self.scopes.len() - 1
                    };
                    // A loop head's names outlive this reader's idea of the
                    // loop: an arm that reads one stays.
                    let head = self.scopes[at].kind == Kind::Head;
                    let d = self.declaring.last_mut().unwrap();
                    if !pattern {
                        d.expect = false;
                    }
                    d.pending.push((w, at));
                    // A pattern's names, keys and defaults read as names the
                    // reader cannot place: an arm that reads one stays.
                    self.bind(at, w, mutable || pattern || head, State::Pending);
                } else {
                    let writes = writes(&b[..at], &b[self.lexer.i..]);
                    self.reference(w, writes);
                }
                if !self.parens.is_empty() {
                    self.group.push(w);
                }
            }
        }
        self.words = [w, self.words[0]];
        self.prev = b'w';
        self.prev_word_at = at;
    }

    fn open(&mut self, at: usize) -> Result<(), String> {
        let scope = self.scopes.len() - 1;
        let s = &self.scopes[scope];
        if s.kind != Kind::Arrow || at == 0 || self.src.as_bytes()[at - 1] != b'{' {
            return Err(format!("lift: a marked body at {at} is not an arrow's"));
        }
        if self.collecting {
            return Ok(());
        }
        let (head, params) = (s.head, s.params.iter().map(|n| n.to_string()).collect());
        // The arrow's head, `(…)=>{`, ends the output.
        self.copy(at);
        self.copied = at + 1;
        let start = self.out.len();
        self.arms.push(Arm {
            scope,
            head: head.map_or(start, |h| start - (at - h)),
            start,
            params,
            free: HashMap::new(),
            // A head with defaults, a rest or a pattern stays as it is.
            fixed: head.is_none(),
        });
        Ok(())
    }

    fn close(&mut self, at: usize) -> Result<(), String> {
        if self.src.as_bytes().get(at + 1) != Some(&b'}') {
            return Err(format!(
                "lift: an arm's body at {at} does not end its arrow"
            ));
        }
        if self.collecting {
            return Ok(());
        }
        self.copy(at);
        self.copied = at + 1;
        let arm = self
            .arms
            .pop()
            .ok_or("lift: an arm closes that never opened")?;
        let stays = arm.fixed || arm.free.values().any(|f| f.1);
        let mut free: Vec<String> = if stays {
            Vec::new()
        } else {
            arm.free.keys().cloned().collect()
        };
        // What it reads from outside its encloser, the encloser reads too.
        if let Some(parent) = self.arms.last_mut() {
            for (name, (scope, unlike)) in arm.free {
                if scope < parent.scope {
                    parent.free.entry(name).or_insert((scope, false)).1 |= unlike;
                }
            }
        }
        // An arm no other arm encloses is in the mount's function, which
        // runs once: it stays.
        if stays
            || self.arms.is_empty()
            || self.out.len() - arm.start < self.min
            || arm.params.len() + free.len() > MAX_PARAMS
        {
            return Ok(());
        }
        free.sort_unstable();
        let mut params = arm.params;
        params.extend(free.iter().cloned());
        let text = format!("=({})=>{{{}}};", params.join(","), &self.out[arm.start..]);
        let name = format!("$h{}", self.lifted.name(&text));
        // The arrow's head, `(…)=>{`, becomes a call or the name itself.
        let head = self.out[arm.head..arm.start]
            .strip_suffix("=>{")
            .ok_or("lift: an arm's head is not an arrow's")?
            .to_string();
        self.out.truncate(arm.head);
        if free.is_empty() {
            self.out.push_str(&name);
        } else {
            write!(self.out, "{head}=>{name}({})", params.join(",")).unwrap();
        }
        self.drop_brace = true;
        Ok(())
    }
}

/// Whether a parameter list is names alone, without defaults, rests or
/// patterns.
fn plain_names(list: &str) -> bool {
    list.trim().is_empty()
        || list.split(',').all(|n| {
            let n = n.trim().as_bytes();
            !n.is_empty() && !n[0].is_ascii_digit() && n.iter().all(|&c| word(c))
        })
}

#[cfg(test)]
#[path = "hoist_tests.rs"]
mod tests;
