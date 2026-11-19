//! @ref LLP 1153 D6, LLP 1123 D5 — `var(--name)` in a style row names, in
//! this order:
//!
//! - The host's own custom property (`var(--exact-safe-area-inset-top)`,
//!   `var(--exact-keyboard-inset-bottom)`, LLP 1123 D7, D9), which is left
//!   for the kernel.
//! - The design's token `name`, written as the name before anything reads
//!   the row (scoping, the types, the design's retention): by the loader for
//!   a file, and by `contract::compile` for a source text. It lowers exactly
//!   as the name does: folded, or on the web the token's `var(--x-name)`
//!   where it varies. A fallback is never used, and the app is warned once
//!   (`forma-var-fallback`).
//! - In a length row or a declaration, a runtime custom property some node
//!   of the app declares (`--gap="8px"`), which the kernel reads at run
//!   time, its fallback a value checked as the row's own; any other row
//!   refuses one. A name nothing declares, which can only take its fallback,
//!   is warned with one (`lower-var-undeclared`), as CSS admits it.
//! - Nothing else: the call is refused, so a typo never ships its fallback
//!   (`forma-unknown-token`, naming the design's tokens of the kinds the row
//!   takes). A name with a `.`, which no node can declare, is a token's. The
//!   refused row is dropped, so the passes after the loader say nothing more
//!   of it.
//!
//! A component's argument, a `fn`'s result and a `derive`'s value that is
//! one `var()` naming a token is that token, as a row reads it once the
//! component is expanded or the call made. Any other argument is left as
//! written, since it may be text; where it reaches a style row, a `var()` in
//! it naming no token is refused there ([`carried`]).

use crate::{tags, LowerError};
use contract_syntax::{Attr, DesignSource, Expr, File, Node, Span, TemplatePart, ThemeDecl};
use exact_forma as forma;
use exact_kernel::style::safe_area_properties::is_author_name;
use exact_kernel::StyleId;

/// The tokens an app declares, as a `var()` may name them.
#[derive(Debug, Clone, Default)]
pub struct Tokens {
    /// Each token's name as written, with its kind when the design checks.
    names: Vec<(String, Option<forma::Kind>)>,
    /// Whether the app has a design (`app.forma`, a package's, an inline
    /// `theme`).
    designed: bool,
    /// Whether the design parsed. One that does not is refused where the
    /// design is read, so its `var()`s are written as names and refused no
    /// further.
    parsed: bool,
    /// The runtime custom properties the app's nodes declare (LLP 1123 D5),
    /// each name after its `--`.
    runtime: std::collections::BTreeSet<String>,
}

impl Tokens {
    /// The tokens of the app's design (`app.forma`'s text), its packages'
    /// designs and an inline `theme`.
    pub fn of(
        design: Option<&str>,
        packages: &[DesignSource],
        theme: Option<&ThemeDecl>,
    ) -> Tokens {
        let (own, parsed) = match design {
            Some(text) => match forma::parse(text) {
                Ok(d) => (Some(d), true),
                Err(_) => (None, false),
            },
            None => (theme.map(crate::theme::inline_design), true),
        };
        let designed = design.is_some() || theme.is_some() || !packages.is_empty();
        let Some(mut merged) = crate::theme::packages::merge(own, packages).0 else {
            return Tokens {
                names: Vec::new(),
                designed,
                parsed,
                runtime: Default::default(),
            };
        };
        let names: Vec<String> = merged.tokens.iter().map(|t| t.name.value.clone()).collect();
        crate::theme::stand_in_profiles(&mut merged);
        // A folding's tokens are the design's, in its order.
        let kinds = forma::check_design(merged, &crate::theme::vocab())
            .ok()
            .map(|f| f.tokens.into_iter().map(|t| t.kind).collect::<Vec<_>>())
            .filter(|kinds| kinds.len() == names.len());
        let names = names
            .into_iter()
            .enumerate()
            .filter(|(_, n)| !n.starts_with(crate::params::PREFIX))
            .map(|(i, n)| (n, kinds.as_ref().map(|k| k[i])))
            .collect();
        Tokens {
            names,
            designed,
            parsed,
            runtime: Default::default(),
        }
    }

    /// These tokens with the runtime custom properties `files` declare.
    pub fn with_runtime<'a>(mut self, files: impl IntoIterator<Item = &'a File>) -> Tokens {
        for file in files {
            self.runtime.extend(declared_properties(file));
        }
        self
    }

    fn declares(&self, name: &str) -> bool {
        !self.parsed || self.names.iter().any(|(n, _)| n == name)
    }

    /// The one token or declared property `name` most plausibly misspells.
    fn near(&self, name: &str) -> Option<String> {
        let tokens = self.names.iter().map(|(n, _)| n.as_str());
        contract_syntax::suggestion(name, tokens.chain(self.runtime.iter().map(String::as_str)))
            .map(str::to_string)
    }
}

/// A `var()` of a name no token or node declares (LLP 1123 D5): the call as
/// written, and the one declared name it most plausibly misspells.
#[derive(Debug, Clone, PartialEq)]
pub struct Undeclared {
    /// The call as written.
    pub call: String,
    /// The token or declared property it most plausibly misspells.
    pub near: Option<String>,
}

/// What [`rewrite`] reports: the rows it refused, the fallbacks it
/// dropped, and the `var()`s of names nothing declares.
#[derive(Debug, Default)]
pub struct Rewritten {
    /// Each style row refused, and dropped from the file.
    pub refused: Vec<LowerError>,
    /// Each `var()` whose fallback was dropped: where, and the call as
    /// written.
    pub fallbacks: Vec<(Span, String)>,
    /// Each `var()` with a fallback of a name no token or node declares,
    /// and where.
    pub undeclared: Vec<(Span, Undeclared)>,
}

/// The runtime custom properties `file`'s nodes and styles declare (LLP
/// 1123 D5), each name after its `--`.
pub fn declared_properties(file: &File) -> std::collections::BTreeSet<String> {
    // Every call here names no other compiler function (`from_iter`, not `new`): the walk
    // census (`contract-checking`) matches calls by name and counts a walk that could recurse
    let mut out = std::collections::BTreeSet::from_iter([]);
    for s in &file.styles {
        author_names(&s.attrs, &mut out);
    }
    for c in &file.components {
        view_names(&c.view, &mut out);
    }
    if let Some(c) = &file.members {
        view_names(&c.view, &mut out);
    }
    out
}

/// Each custom property `view`'s elements declare, after its `--`, added to
/// `out`.
fn view_names(tree_nodes: &[Node], declared_names: &mut std::collections::BTreeSet<String>) {
    for n in Node::walk(tree_nodes) {
        if let Node::Element {
            attrs: own_attrs, ..
        } = n
        {
            author_names(own_attrs, declared_names);
        }
    }
}

/// Each custom property `list` declares, after its `--`, added to `out`.
fn author_names(list: &[Attr], out: &mut std::collections::BTreeSet<String>) {
    for a in list {
        if is_author_name(&a.name) {
            out.insert(a.name[2..].to_owned());
        }
    }
}

/// The one warning for every `var()` of a name nothing declares, at the
/// first (LLP 1123 D5).
pub fn undeclared_warning(found: &[(Span, Undeclared)]) -> Option<LowerError> {
    let (span, Undeclared { call, near }) = found.first()?;
    let name = call_name(call);
    let near = near
        .as_ref()
        .map(|n| format!("; did you mean `--{n}`?"))
        .unwrap_or_default();
    let others = match found.len() - 1 {
        0 => String::new(),
        _ => "; other `var()`s in this app name undeclared properties likewise".into(),
    };
    Some(LowerError {
        id: "lower-var-undeclared",
        message: format!("`{call}`: no node of this app declares `--{name}` and no token of the design is named so, so the fallback always stands in; declare it (`--{name}=\"…\"`) or write the fallback itself{near}{others}"),
        span: *span,
    })
}

/// Write each `var()` of `file`'s style rows that names a token as its
/// name, leave each that a length or a declaration reads at run time, refuse
/// and drop each row with another, and write a component argument that is
/// one such `var()` as the name (the module's order).
pub fn rewrite(file: &mut File, tokens: &Tokens) -> Rewritten {
    let mut out = Rewritten::default();
    for s in file
        .styles
        .iter_mut()
        .chain(&mut file.decorations)
        .chain(&mut file.atoms)
    {
        rows(&mut s.attrs, tokens, &mut out);
    }
    for k in &mut file.keyframes {
        for frame in &mut k.frames {
            rows(&mut frame.attrs, tokens, &mut out);
        }
    }
    // A `fn`'s or a `derive`'s value that is one `var()` naming a token, through `?:` and `match`, is that token,
    // as a component's argument is: what reaches a row by a call (`background-color=typeColor(t)`) names it there.
    let mut refused = None;
    for f in &mut file.fns {
        value(&mut f.body, tokens, Mode::Whole, &mut out, &mut refused);
    }
    for c in file.components.iter_mut().chain(file.members.as_mut()) {
        for d in &mut c.derives {
            value(&mut d.expr, tokens, Mode::Whole, &mut out, &mut refused);
        }
        nodes(&mut c.view, tokens, &mut out);
    }
    out
}

/// An app's design as [`Tokens::of`] reads it: `app.forma`'s text, its
/// packages' designs and an inline `theme`; and the runtime custom
/// properties its nodes declare.
type Noted = (
    Option<String>,
    Vec<DesignSource>,
    Option<ThemeDecl>,
    std::collections::BTreeSet<String>,
);

thread_local! {
    /// The design a `var()` may name while the app's styles are lowered,
    /// read into [`Tokens`] only when a row needs them ([`carried`]).
    static DESIGN: std::cell::RefCell<Option<Noted>> = const { std::cell::RefCell::new(None) };
}

/// Note `file`'s design for the rows [`carried`] checks, as lowering starts.
pub(crate) fn note(file: &File) {
    let design = file.design.as_ref().map(|d| d.text.clone());
    let noted = (
        design,
        file.package_designs.clone(),
        file.theme.clone(),
        declared_properties(file),
    );
    DESIGN.with(|d| *d.borrow_mut() = Some(noted));
}

/// A style row's `var()` that reached it through a component's argument,
/// which the loader writes only when the argument is one `var()` naming a
/// token: one the row cannot read is refused as the loader refuses it in a
/// row. A `var()` among other words of an argument, naming a token, is left
/// for the row's own check.
pub(crate) fn carried(a: &Attr) -> Result<(), LowerError> {
    fn mentions(e: &Expr) -> bool {
        let named = |s: &str| {
            s.as_bytes()
                .windows(4)
                .any(|w| w.eq_ignore_ascii_case(b"var("))
        };
        match e {
            Expr::Str(s, _) => named(s),
            Expr::Template(parts, _) => parts.iter().any(|p| match p {
                TemplatePart::Text(t) => named(t),
                TemplatePart::Expr(e) => mentions(e),
            }),
            Expr::Ternary(_, yes, no, _) => mentions(yes) || mentions(no),
            Expr::Match { some, none, .. } => mentions(some) || mentions(none),
            Expr::Let { value, body, .. } => mentions(value) || mentions(body),
            Expr::Binary(contract_syntax::BinOp::Add, l, r, _) => mentions(l) || mentions(r),
            _ => false,
        }
    }
    if !tags::style(&a.name) || !mentions(&a.value) {
        return Ok(());
    }
    let Some((design, packages, theme, runtime)) = DESIGN.with(|d| d.borrow().clone()) else {
        return Ok(());
    };
    let mut tokens = Tokens::of(design.as_deref(), &packages, theme.as_ref());
    tokens.runtime = runtime;
    let (mut out, mut refused) = (Rewritten::default(), None);
    value(
        &mut a.value.clone(),
        &tokens,
        row_mode(a),
        &mut out,
        &mut refused,
    );
    match refused {
        Some((why, literal, span)) => Err(refusal(a, why, &literal, span, &tokens)),
        None => Ok(()),
    }
}

/// The one warning for every fallback [`rewrite`] dropped, at the first.
pub fn fallback_warning(fallbacks: &[(Span, String)]) -> Option<LowerError> {
    let (span, call) = fallbacks.first()?;
    let name = call_name(call);
    let others = match fallbacks.len() - 1 {
        0 => String::new(),
        1 => "; one other `var()` in this app has a fallback, ignored likewise".into(),
        n => format!("; {n} other `var()`s in this app have fallbacks, ignored likewise"),
    };
    Some(LowerError {
        id: "forma-var-fallback",
        message: format!("`{call}`: `{name}` is a token of the design, so the fallback is never used and is ignored (Exact never leaves a token undefined): write `var(--{name})`{others}"),
        span: *span,
    })
}

fn call_name(call: &str) -> &str {
    let inner = call.get(4..).unwrap_or_default().trim_start();
    let inner = inner.strip_prefix("--").unwrap_or(inner);
    inner.split([',', ')']).next().unwrap_or_default().trim()
}

fn nodes(list: &mut [Node], tokens: &Tokens, out: &mut Rewritten) {
    for node in list {
        match node {
            Node::Element {
                attrs, children, ..
            } => {
                rows(attrs, tokens, out);
                nodes(children, tokens, out);
            }
            Node::Use { args, children, .. } => {
                for a in args.iter_mut() {
                    let mut refused = None;
                    value(&mut a.value, tokens, Mode::Whole, out, &mut refused);
                }
                nodes(children, tokens, out);
            }
            node => {
                for list in node.child_lists_mut() {
                    nodes(list, tokens, out);
                }
            }
        }
    }
}

/// A list's style rows rewritten; one with a `var()` no token answers is
/// refused and dropped, as is a custom property named as a token is.
fn rows(list: &mut Vec<Attr>, tokens: &Tokens, out: &mut Rewritten) {
    list.retain_mut(|a| {
        if !tags::style(&a.name) {
            return true;
        }
        let name = a.name.strip_prefix("--").unwrap_or_default();
        if tokens.parsed && tokens.names.iter().any(|(n, _)| n == name) {
            let why = Why::Token(name.to_string());
            out.refused.push(refusal(a, why, "", a.span, tokens));
            return false;
        }
        let mut refused = None;
        let mode = row_mode(a);
        value(&mut a.value, tokens, mode, out, &mut refused);
        match refused {
            None => true,
            Some((why, literal, span)) => {
                out.refused.push(refusal(a, why, &literal, span, tokens));
                false
            }
        }
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// A style row's value: every `var()` written or refused; whether the
    /// row reads a runtime custom property ([`runtime_row`]).
    Row(bool),
    /// A custom property's declaration: a row that reads runtime properties,
    /// whose token calls stay as written for lowering to write each
    /// token's value ([`crate::values::call_tokens`]).
    Declaration,
    /// A component's argument: a literal that is one `var()` naming a
    /// token, written; the rest left.
    Whole,
}

/// Why a `var()` is refused.
#[derive(Debug, PartialEq)]
enum Why {
    /// It names no token; whether it has a fallback.
    Unknown(String, bool),
    /// Its argument is no custom property name.
    Unnamed,
    /// Its name is computed (a template's hole).
    Computed,
    /// It names a runtime custom property in a row that reads none.
    Runtime(String),
    /// A node declares a custom property a token of the design is named.
    Token(String),
    /// A token of another kind than a length stands where a custom property
    /// holds one.
    NotLength(String),
}

/// Whether `a`'s rows read a runtime custom property: lengths, which the
/// kernel substitutes per node, and declarations (LLP 1123 D5).
fn runtime_row(a: &Attr) -> bool {
    use exact_kernel::StyleCodec;
    let rows = tags::rows(a);
    !rows.is_empty()
        && !rows.iter().any(|r| local_inset(*r))
        && rows.iter().all(|r| {
            matches!(
                r.codec(),
                StyleCodec::Dimension | StyleCodec::SafeAreaProperties
            )
        })
}

/// Whether `row` overrides a content owner's local inset, which the kernel
/// resolves before any custom property, so it reads none.
fn local_inset(row: StyleId) -> bool {
    matches!(
        row,
        StyleId::LocalInsetTop
            | StyleId::LocalInsetRight
            | StyleId::LocalInsetBottom
            | StyleId::LocalInsetLeft
    )
}

/// The first refusal in a value: why, the literal's text, where.
type Refused = Option<(Why, String, Span)>;

/// Rewrite a value's literals in place, through `?:`, `match`, `let`, `+`
/// and a template's text.
fn value(e: &mut Expr, tokens: &Tokens, mode: Mode, out: &mut Rewritten, refused: &mut Refused) {
    if let Expr::Template(parts, _) = e {
        if completed(parts, tokens) {
            return;
        }
    }
    match e {
        Expr::Str(s, span) => {
            if mode == Mode::Whole && whole_var(s).is_none() {
                return;
            }
            match text(
                s,
                tokens,
                false,
                &mut None,
                runtime(mode),
                mode == Mode::Declaration,
            ) {
                Ok(Some((next, fallbacks, undeclared))) => {
                    out.fallbacks
                        .extend(fallbacks.into_iter().map(|f| (*span, f)));
                    out.undeclared
                        .extend(undeclared.into_iter().map(|f| (*span, f)));
                    *s = next;
                }
                Ok(None) => {}
                Err(why)
                    if matches!(mode, Mode::Row(_) | Mode::Declaration) && refused.is_none() =>
                {
                    *refused = Some((why, s.clone(), *span));
                }
                Err(_) => {}
            }
        }
        Expr::Template(parts, span) if matches!(mode, Mode::Row(_) | Mode::Declaration) => {
            let mut quote = None;
            let n = parts.len();
            for i in 0..n {
                let hole = i + 1 < n && matches!(parts[i + 1], TemplatePart::Expr(_));
                match &mut parts[i] {
                    TemplatePart::Text(s) => match text(
                        s,
                        tokens,
                        hole,
                        &mut quote,
                        runtime(mode),
                        mode == Mode::Declaration,
                    ) {
                        Ok(Some((next, fallbacks, undeclared))) => {
                            out.fallbacks
                                .extend(fallbacks.into_iter().map(|f| (*span, f)));
                            out.undeclared
                                .extend(undeclared.into_iter().map(|f| (*span, f)));
                            *s = next;
                        }
                        Ok(None) => {}
                        Err(why) => {
                            if refused.is_none() {
                                *refused = Some((why, s.clone(), *span));
                            }
                        }
                    },
                    TemplatePart::Expr(e) => value(e, tokens, mode, out, refused),
                }
            }
        }
        Expr::Ternary(_, yes, no, _) => {
            value(yes, tokens, mode, out, refused);
            value(no, tokens, mode, out, refused);
        }
        Expr::Match { some, none, .. } => {
            value(some, tokens, mode, out, refused);
            value(none, tokens, mode, out, refused);
        }
        Expr::Let { value: v, body, .. } => {
            value(v, tokens, mode, out, refused);
            value(body, tokens, mode, out, refused);
        }
        Expr::Binary(contract_syntax::BinOp::Add, left, right, _) => {
            value(left, tokens, mode, out, refused);
            value(right, tokens, mode, out, refused);
        }
        _ => {}
    }
}

/// Write a template that is one `var()` whose name its holes complete
/// (`` `var(--type-${t})` ``) as that name (`` `type-${t}` ``), which the
/// runtime reads against the design as it reads a name from data, when a
/// token of the design has the name's written start and end. Whether it was
/// written.
fn completed(parts: &mut Vec<TemplatePart>, tokens: &Tokens) -> bool {
    let n = parts.len();
    let (Some(TemplatePart::Text(first)), Some(TemplatePart::Text(last))) =
        (parts.first(), parts.last())
    else {
        return false;
    };
    if n < 3 {
        return false;
    }
    let first = first.trim_start();
    let Some(start) = first
        .get(..6)
        .filter(|p| p.eq_ignore_ascii_case("var(--"))
        .map(|_| &first[6..])
    else {
        return false;
    };
    let Some(end) = last.trim_end().strip_suffix(')') else {
        return false;
    };
    let name = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
    // Only the name's own text between the holes: a space, a comma (a
    // fallback) or another call makes it more than a name.
    let inner = parts[1..n - 1].iter().all(|p| match p {
        TemplatePart::Text(t) => t.chars().all(name),
        TemplatePart::Expr(_) => true,
    });
    if !inner || !start.chars().all(name) || !end.chars().all(name) {
        return false;
    }
    if !tokens
        .names
        .iter()
        .any(|(t, _)| t.starts_with(start) && t.ends_with(end) && t.len() > start.len() + end.len())
    {
        return false;
    }
    let (start, end) = (start.to_string(), end.to_string());
    if let Some(TemplatePart::Text(t)) = parts.first_mut() {
        *t = start;
    }
    if let Some(TemplatePart::Text(t)) = parts.last_mut() {
        *t = end;
    }
    // As the parser writes a template, with no empty text.
    parts.retain(|p| !matches!(p, TemplatePart::Text(t) if t.is_empty()));
    true
}

/// `text` as one `var()` call and nothing else, its parentheses balanced:
/// the name it gives and whether it writes a fallback; `None` for any other
/// text.
pub fn whole_var(text: &str) -> Option<(String, bool)> {
    let text = text.trim();
    match calls(text, false, &mut None).as_slice() {
        [Found::Call { at, name, fallback }] if at.start == 0 && at.end == text.len() => {
            Some((name.clone(), fallback.is_some()))
        }
        _ => None,
    }
}

/// A `var(` in a literal's text.
#[derive(Debug, PartialEq)]
enum Found {
    /// A whole call: its bytes, its name and its fallback's bytes.
    Call {
        at: std::ops::Range<usize>,
        name: String,
        fallback: Option<std::ops::Range<usize>>,
    },
    /// One with no name, or whose name a template's hole computes.
    Bad(Why),
}

/// Whether a value in `mode` may read a runtime custom property.
fn runtime(mode: Mode) -> bool {
    matches!(mode, Mode::Row(true) | Mode::Declaration)
}

/// How a style row's value is rewritten: a declaration's apart from any
/// other row's.
fn row_mode(a: &Attr) -> Mode {
    match tags::rows(a).as_slice() {
        [StyleId::SafeAreaProperties] => Mode::Declaration,
        _ => Mode::Row(runtime_row(a)),
    }
}

/// A text's `var()`s rewritten: the text, each call whose fallback was
/// dropped, and each call of a name nothing declares.
type Written = (String, Vec<String>, Vec<Undeclared>);

/// `text` with each `var()` naming a token written as the name, each call
/// whose fallback it dropped, and each of a name nothing declares, which
/// its fallback answers (`runtime`: the row reads runtime custom
/// properties); `None` when it has no `var()`. `hole`: a template's hole
/// follows the text. `quote`: the quote a template's earlier text left
/// open, and this text leaves open.
fn text(
    text: &str,
    tokens: &Tokens,
    hole: bool,
    quote: &mut Option<char>,
    runtime: bool,
    keep: bool,
) -> Result<Option<Written>, Why> {
    let found = calls(text, hole, quote);
    if found.is_empty() {
        return Ok(None);
    }
    let (mut out, mut fallbacks, mut undeclared, mut at) =
        (String::new(), Vec::new(), Vec::new(), 0);
    for f in found {
        match f {
            Found::Call {
                at: range,
                name,
                fallback,
            } => {
                // The host's observation stays for the kernel, and its
                // fallback is checked as an author property's is (LLP 1123 D5)
                if host_property(&name) {
                    if let Some(f) = fallback {
                        let written = (&mut out, &mut fallbacks, &mut undeclared, &mut at);
                        fallback_value(text, f, tokens, runtime, written)?;
                    }
                    continue;
                }
                if !tokens.declares(&name) {
                    // Exact reserves dotted names for design tokens, so no node declares one
                    let property = !name.contains('.');
                    match tokens.runtime.contains(&name) {
                        true if runtime => {}
                        true => return Err(Why::Runtime(name)),
                        false if runtime && property && fallback.is_some() => {
                            let near = tokens.near(&name);
                            let call = text[range].to_string();
                            undeclared.push(Undeclared { call, near });
                        }
                        false => return Err(Why::Unknown(name, fallback.is_some())),
                    }
                    // The call stays for the kernel, its fallback a value of its own
                    if let Some(f) = fallback {
                        let written = (&mut out, &mut fallbacks, &mut undeclared, &mut at);
                        fallback_value(text, f, tokens, runtime, written)?;
                    }
                    continue;
                }
                // A design that does not parse names no token: its refusal
                // fails the build, and no fallback is said to be unused.
                if fallback.is_some() && tokens.parsed {
                    fallbacks.push(text[range.clone()].to_string());
                }
                // A declaration's or a runtime fallback's token call stays for
                // lowering to write as its value
                if keep {
                    let kind = tokens
                        .names
                        .iter()
                        .find(|(n, _)| *n == name)
                        .and_then(|(_, k)| *k);
                    if kind.is_some_and(|k| k != forma::Kind::Length) {
                        return Err(Why::NotLength(name));
                    }
                    continue;
                }
                out.push_str(&text[at..range.start]);
                out.push_str(&name);
                at = range.end;
            }
            Found::Bad(why) => return Err(why),
        }
    }
    out.push_str(&text[at..]);
    Ok(Some((out, fallbacks, undeclared)))
}

/// Check the fallback `f` of a call that stays for the kernel as a value of
/// its own, writing what it lowers to in place of its bytes of `text`: the
/// output, the dropped fallbacks, the undeclared calls and the position
/// written to.
fn fallback_value(
    text: &str,
    f: std::ops::Range<usize>,
    tokens: &Tokens,
    runtime: bool,
    (out, fallbacks, undeclared, at): (
        &mut String,
        &mut Vec<String>,
        &mut Vec<Undeclared>,
        &mut usize,
    ),
) -> Result<(), Why> {
    let inner = (self::text)(&text[f.clone()], tokens, false, &mut None, runtime, true)?;
    if let Some((inner, mut dropped, mut unknown)) = inner {
        out.push_str(&text[*at..f.start]);
        out.push_str(&inner);
        *at = f.end;
        fallbacks.append(&mut dropped);
        undeclared.append(&mut unknown);
    }
    Ok(())
}

/// Whether `name` (after its `--`) is a custom property the host sets: a
/// box's observation of a family on an edge, the safe area's
/// (`exact-safe-area-inset-top`), the keyboard's
/// (`exact-keyboard-inset-bottom`, LLP 1123 D7, D9) or a layout region's
/// (`exact-layout-margins-inset-left`, D8, D10), or a native target's raw
/// safe area (`exact-native-2-safe-area-inset-top`, D9).
fn host_property(name: &str) -> bool {
    exact_kernel::style::calc::Family::observed(&format!("--{name}")).is_some()
}

/// Each `var(` in `text` outside a quoted string, in order.
fn calls(text: &str, hole: bool, quote: &mut Option<char>) -> Vec<Found> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let skip = |j: usize| j + text[j..].len() - text[j..].trim_start().len();
    let mut found = Vec::new();
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        if let Some(q) = *quote {
            if c == q {
                *quote = None;
            }
            i += c.len_utf8();
            continue;
        }
        if c == '"' || c == '\'' {
            *quote = Some(c);
            i += 1;
            continue;
        }
        // A comment holds no call
        if text[i..].starts_with("/*") {
            i = text[i + 2..]
                .find("*/")
                .map_or(text.len(), |e| i + 2 + e + 2);
            continue;
        }
        let opens = text
            .get(i..i + 4)
            .is_some_and(|s| s.eq_ignore_ascii_case("var("))
            && !text[..i].chars().next_back().is_some_and(word);
        if !opens {
            i += c.len_utf8();
            continue;
        }
        let start = skip(i + 4);
        // The name ends where its characters do; a name the hole computes
        // reaches the text's end with a hole after it.
        let ends_open = |j: usize| j >= text.len() && hole;
        let Some(rest) = text[start..].strip_prefix("--") else {
            found.push(Found::Bad(if ends_open(start) {
                Why::Computed
            } else {
                Why::Unnamed
            }));
            i = start.max(i + 4);
            continue;
        };
        let name_at = text.len() - rest.len();
        let name_end = name_at
            + rest
                .find(|c: char| !(word(c) || c == '.'))
                .unwrap_or(rest.len());
        let name = &text[name_at..name_end];
        let after = skip(name_end);
        match text.as_bytes().get(after) {
            Some(b')') if !name.is_empty() => {
                found.push(Found::Call {
                    at: i..after + 1,
                    name: name.to_string(),
                    fallback: None,
                });
                i = after + 1;
            }
            Some(b',') if !name.is_empty() => match close(text, after + 1) {
                Some(end) => {
                    found.push(Found::Call {
                        at: i..end + 1,
                        name: name.to_string(),
                        fallback: Some(after + 1..end),
                    });
                    i = end + 1;
                }
                None => {
                    found.push(Found::Bad(if hole { Why::Computed } else { Why::Unnamed }));
                    i = text.len();
                }
            },
            _ => {
                found.push(Found::Bad(if ends_open(after) {
                    Why::Computed
                } else {
                    Why::Unnamed
                }));
                i = after.max(i + 4);
            }
        }
    }
    found
}

/// The index of the `)` that closes a function whose argument starts at
/// `from`, past nested parentheses and quoted strings.
fn close(text: &str, from: usize) -> Option<usize> {
    let (mut depth, mut quote) = (0usize, None);
    for (k, c) in text[from..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') if depth == 0 => return Some(from + k),
            (None, ')') => depth -= 1,
            _ => {}
        }
    }
    None
}

/// A row's refusal: what its `var()` names, or why it names nothing.
fn refusal(a: &Attr, why: Why, literal: &str, span: Span, tokens: &Tokens) -> LowerError {
    let written = format!("`{}=\"{literal}\"`", a.name);
    let (id, message) = match why {
        Why::Unnamed => (
            "lower-attr-value",
            format!("{written}: a `var()` names a token of the design as a custom property, `var(--name)`"),
        ),
        Why::Computed => (
            "lower-attr-value",
            format!("{written}: a `var()` names its token whole; a token's name from data is the value itself (`{}=name`), which the runtime reads against the design", a.name),
        ),
        Why::NotLength(name) => (
            "lower-attr-value",
            format!("{written}: `--{name}` is a design token of another kind than a length, and a custom property's value, or a fallback a length reads at run time, is a length"),
        ),
        Why::Token(name) => (
            "lower-attr-value",
            format!("`{}`: `{name}` is a token of this app's design, and `var(--{name})` reads the token, so no node declares a custom property of that name; name the property otherwise", a.name),
        ),
        Why::Runtime(name) if tags::rows(a).into_iter().any(local_inset) => (
            "lower-attr-value",
            format!("{written}: `{}` sets the content owner's inset, which the kernel resolves before any custom property, so it reads none; write the length `--{name}` holds", a.name),
        ),
        Why::Runtime(name) => (
            "lower-attr-value",
            format!("{written}: `--{name}` is a custom property a node declares, which a length (`padding`, `width`, …) or another declaration reads; `{}` names a token of the design (`var(--name)` of a token, or a role)", a.name),
        ),
        Why::Unknown(name, fallback) => {
            let kinds = kinds(a);
            let said = kinds
                .iter()
                .map(|k| kind_words(*k))
                .collect::<Vec<_>>()
                .join(" or ");
            let declared: Vec<&str> = tokens
                .names
                .iter()
                .filter(|(_, k)| k.is_none_or(|k| kinds.contains(&k)))
                .map(|(n, _)| n.as_str())
                .collect();
            let listed = match declared.len() {
                0 if !tokens.designed => "this app has no design: a `var(--name)` names a token its `app.forma` declares (`bun exact.mjs forma import css <file>` writes a stylesheet's custom properties there)".to_string(),
                0 => format!("the design declares no {said} token"),
                n => {
                    let shown: Vec<String> = declared.iter().take(20).map(|n| format!("`{n}`")).collect();
                    let more = if n > 20 { format!(" and {} more", n - 20) } else { String::new() };
                    format!("its {said} tokens are {}{more}", shown.join(", "))
                }
            };
            let dotted = tokens
                .names
                .iter()
                .find(|(n, _)| n.contains('.') && n.replace('.', "-") == name)
                .map(|(n, _)| format!("; `var(--{n})` names `{n}`"))
                .unwrap_or_default();
            let role = crate::theme::role_named(&format!("-{name}"))
                .map(|(_, row)| format!("; `{row}` is a role, which a row names bare (`{}=\"{row}\"`)", a.name))
                .unwrap_or_default();
            let fallback = if fallback {
                "; the fallback is not used for a name the design lacks, so a misspelt name never ships it"
            } else {
                ""
            };
            // A length reads a custom property a node declares too (LLP 1123 D5)
            let (nodes, near) = match runtime_row(a) {
                true => (
                    " and no node of the app declares it",
                    (tokens.near(&name).filter(|_| dotted.is_empty()))
                        .map(|n| format!("; did you mean `--{n}`?"))
                        .unwrap_or_default(),
                ),
                false => ("", String::new()),
            };
            (
                "forma-unknown-token",
                format!("{written}: `--{name}` is no token of this app's design{nodes}; {listed}{dotted}{role}{near}{fallback}"),
            )
        }
    };
    LowerError { id, message, span }
}

/// The token kinds a row takes, by its rows (LLP 1121 D3).
fn kinds(a: &Attr) -> Vec<forma::Kind> {
    use exact_kernel::style::theme::{admits, Kind as K};
    use forma::Kind as T;
    let name = contract_syntax::state::state_of(&a.name).map_or(a.name.as_str(), |(_, row)| row);
    if crate::motion::is_row(name) {
        return vec![T::Duration, T::Easing];
    }
    match name {
        "font" => return vec![T::Type],
        "font-family" => return vec![T::Family],
        _ => {}
    }
    let rows = tags::rows(&Attr {
        name: name.to_string(),
        ..a.clone()
    });
    [
        (K::Color, T::Color),
        (K::Length, T::Length),
        (K::Number, T::Number),
        (K::BoxShadow, T::BoxShadow),
        (K::TextShadow, T::TextShadow),
    ]
    .into_iter()
    .filter(|(k, _)| rows.iter().any(|r| admits(*k, *r)))
    .map(|(_, t)| t)
    .collect()
}

/// A kind in an author's words.
fn kind_words(kind: forma::Kind) -> &'static str {
    match kind {
        forma::Kind::Color => "colour",
        forma::Kind::BoxShadow => "box shadow",
        forma::Kind::TextShadow => "text shadow",
        other => other.name(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(names: &[&str]) -> Tokens {
        Tokens {
            names: names.iter().map(|n| (n.to_string(), None)).collect(),
            designed: true,
            parsed: true,
            runtime: Default::default(),
        }
    }

    fn written(s: &str) -> Result<Option<String>, Why> {
        text(
            s,
            &tokens(&["ink", "space.row"]),
            false,
            &mut None,
            false,
            false,
        )
        .map(|o| o.map(|(s, ..)| s))
    }

    #[test]
    fn a_declared_var_is_its_name_wherever_it_stands() {
        assert_eq!(written("var(--ink)").unwrap().as_deref(), Some("ink"));
        assert_eq!(written("VAR( --ink )").unwrap().as_deref(), Some("ink"));
        assert_eq!(
            written("1px solid var(--ink, #000)").unwrap().as_deref(),
            Some("1px solid ink")
        );
        assert_eq!(
            written("color-mix(in srgb, var(--ink) 50%, var(--ink, rgb(0 0 0)))")
                .unwrap()
                .as_deref(),
            Some("color-mix(in srgb, ink 50%, ink)")
        );
        assert_eq!(
            written("var(--space.row)").unwrap().as_deref(),
            Some("space.row")
        );
        // Quoted text, and a word ending in `var(`, are not calls.
        assert_eq!(written("'var(--x)', serif").unwrap(), None);
        assert_eq!(written("somevar(--x)").unwrap(), None);
    }

    #[test]
    fn any_other_var_is_refused() {
        assert_eq!(
            written("var(--inc)"),
            Err(Why::Unknown("inc".into(), false))
        );
        assert_eq!(
            written("var(--inc, red)"),
            Err(Why::Unknown("inc".into(), true))
        );
        assert_eq!(written("var(ink)"), Err(Why::Unnamed));
        assert_eq!(written("var(--ink"), Err(Why::Unnamed));
        // A name a template's hole completes.
        let t = tokens(&["ink"]);
        assert_eq!(
            text("var(--", &t, true, &mut None, false, false),
            Err(Why::Computed)
        );
    }

    #[test]
    fn the_hosts_safe_area_is_left_for_the_kernel() {
        let row = "calc(-1 * var(--exact-safe-area-inset-top)) var(--ink)";
        assert_eq!(
            written(row).unwrap().as_deref(),
            Some("calc(-1 * var(--exact-safe-area-inset-top)) ink")
        );
        assert_eq!(
            written("var(--exact-safe-area-inset-middle)"),
            Err(Why::Unknown("exact-safe-area-inset-middle".into(), false))
        );
    }

    #[test]
    fn a_fallback_is_kept_for_the_warning() {
        let t = tokens(&["ink"]);
        let (_, fallbacks, _) = text("var(--ink, #111)", &t, false, &mut None, false, false)
            .unwrap()
            .unwrap();
        assert_eq!(fallbacks, ["var(--ink, #111)"]);
        assert_eq!(call_name("var(--ink, #111)"), "ink");
        assert_eq!(whole_var(" var(--ink) "), Some(("ink".into(), false)));
        assert_eq!(
            whole_var("var(--ink, rgb(0 0 0))"),
            Some(("ink".into(), true))
        );
        assert_eq!(whole_var("color: var(--ink);"), None);
        assert_eq!(whole_var("var(--ink, #000) var(--paper)"), None);
    }
}
