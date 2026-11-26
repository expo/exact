//! A test's drag, `tap "id" drag …`, read as the driver's `tap … drag` is
//! (`scripts/agent-drag.mjs` `parseDrag`), and `expect drag` (LLP 1094.000
//! D10): what a node's drag carries, as its admitted props declare it, so a
//! test reads the same on every host.

use super::steps::during_read;
use super::*;

/// The most `then` legs a drag takes, as the driver's `dragArguments` bounds them.
const MOST_LEGS: usize = 7;

impl Parser {
    /// After `tap "id" drag`: `to "B" [at x y]` or `dx dy`, then the options
    /// in any order. `from`, `mouse`, `modifiers` and `press` are the first
    /// leg's; each `then dx dy` starts a leg whose `over`, `hold` and `during`
    /// follow it.
    pub(super) fn drag_step(&mut self, target: String, span: Span) -> R<Step> {
        // `drag to "B" [at x y]` (LLP 1094 D12) or `drag dx dy`.
        let (dx, dy, to) = if self.at_ident("to") {
            self.next();
            let to = self.str_lit("the testId the drag ends on")?;
            let at = if self.at_ident("at") {
                self.next();
                let x = self.step_number("the end's x in the node's box")?;
                let y = self.step_number("the end's y in the node's box")?;
                Some((x, y))
            } else {
                None
            };
            (0.0, 0.0, Some((to, at)))
        } else {
            let dx = self.step_number("the drag's dx in points")?;
            let dy = self.step_number("the drag's dy in points")?;
            (dx, dy, None)
        };
        let (mut press, mut from, mut mouse, mut into, mut modifiers) =
            (None, None, false, None, None);
        let mut first = DragLeg {
            dx,
            dy,
            over: None,
            hold: None,
            during: Vec::new(),
        };
        let mut path: Vec<DragLeg> = Vec::new();
        while let TokenKind::Ident(w) = self.peek_kind().clone() {
            let in_leg = !path.is_empty();
            let leg = path.last_mut().unwrap_or(&mut first);
            let twice = match w.as_str() {
                "from" => from.is_some(),
                "mouse" => mouse,
                "into" => into.is_some(),
                "modifiers" => modifiers.is_some(),
                "press" => press.is_some(),
                "over" => leg.over.is_some(),
                "hold" => leg.hold.is_some(),
                "during" => !leg.during.is_empty(),
                "then" => false,
                _ => break,
            };
            if twice {
                let whose = if !in_leg
                    || ["from", "mouse", "into", "modifiers", "press"].contains(&w.as_str())
                {
                    "one drag"
                } else {
                    "one leg"
                };
                return self.err(
                    "syntax-expected-step",
                    format!("`{w}` is given twice in {whose}"),
                );
            }
            if in_leg && ["from", "mouse", "modifiers", "press"].contains(&w.as_str()) {
                return self.err(
                    "syntax-expected-step",
                    format!("`{w}` is the drag's, written before its first `then`; a `then` leg takes `over`, `hold` and `during`"),
                );
            }
            self.next();
            match w.as_str() {
                "then" => {
                    if to.is_some() {
                        return self.err("syntax-expected-step", "`drag to` takes no `then`: write the legs as `drag dx dy … then dx dy …`");
                    }
                    if path.len() == MOST_LEGS {
                        return self.err(
                            "syntax-expected-step",
                            format!("a drag takes at most {MOST_LEGS} `then` legs"),
                        );
                    }
                    let dx = self.step_number("the leg's dx in points")?;
                    let dy = self.step_number("the leg's dy in points")?;
                    path.push(DragLeg {
                        dx,
                        dy,
                        over: None,
                        hold: None,
                        during: Vec::new(),
                    });
                }
                "mouse" => mouse = true,
                "into" => {
                    self.expect_word("keyboard")?;
                    into = Some("keyboard".to_string());
                }
                "from" => {
                    let x = self.step_number("the start's x in the node's box")?;
                    let y = self.step_number("the start's y in the node's box")?;
                    from = Some((x, y));
                }
                "modifiers" => {
                    modifiers = Some(self.str_lit("the modifiers held, as \"Shift+Meta\"")?)
                }
                "press" => press = Some(self.step_number("milliseconds")?),
                "over" => leg.over = Some(self.step_number("milliseconds")?),
                "hold" => {
                    if matches!(
                        self.peek_kind(),
                        TokenKind::Newline | TokenKind::Eof | TokenKind::Ident(_)
                    ) {
                        return self.err("syntax-expected-step", "a drag lifts at its end; use `hold <milliseconds> during \"expect …\"` to check it before the lift");
                    }
                    leg.hold = Some(self.step_number("milliseconds")?);
                }
                "during" => {
                    while let TokenKind::Str(op) = self.peek_kind().clone() {
                        let span = self.next().span;
                        leg.during.push(Self::during_op(op, span)?);
                    }
                    if self.at_ident("as") {
                        return self.err(
                            "syntax-expected-step",
                            "`during` has no named captures; use a quoted expectation",
                        );
                    }
                    if leg.during.is_empty() {
                        return self.err(
                            "syntax-expected-step",
                            "`during` takes quoted ops, as during \"clock +500\"",
                        );
                    }
                }
                _ => unreachable!("the words above"),
            }
        }
        self.newline()?;
        Ok(Step::Drag {
            target,
            dx,
            dy,
            to,
            from,
            mouse,
            into,
            press,
            over: first.over,
            hold: first.hold,
            during: first.during,
            modifiers: modifiers.unwrap_or_default(),
            path,
            span,
        })
    }

    fn during_op(op: String, span: Span) -> R<During> {
        let refuse = |message: String| SyntaxError {
            id: "syntax-expected-step",
            message,
            span,
        };
        let mut words = op.split_whitespace();
        let head = words.next();
        // Keep quoted filenames and expected text opaque when looking for a capture.
        let capture = Lexer::tokenize_at(op.trim(), span).is_ok_and(|tokens| {
            let mut words = tokens
                .iter()
                .rev()
                .map(|t| &t.kind)
                .filter(|t| !matches!(t, TokenKind::Eof | TokenKind::Newline));
            matches!(words.next(), Some(TokenKind::Ident(_) | TokenKind::Str(_)))
                && matches!(words.next(), Some(TokenKind::Ident(word)) if word == "as")
        });
        if head == Some("capture") || capture {
            return Err(refuse(
                "`during` has no named captures; use a quoted expectation".into(),
            ));
        }
        if head == Some("expect") {
            if words.next() == Some("perf") {
                return Err(refuse(format!(
                    "`during` cannot check {op:?}: `expect perf` needs a completed perf block"
                )));
            }
            let mut parser = Parser::new(Lexer::tokenize_at(op.trim(), span)?);
            let step = parser.step()?;
            if !matches!(parser.peek_kind(), TokenKind::Eof) {
                return Err(refuse(
                    "`during` takes one expectation per quoted operation".into(),
                ));
            }
            Ok(During::Expect(Box::new(step)))
        } else if during_read(&op) {
            Ok(During::Read(op))
        } else {
            Err(refuse(format!("`during` {op:?} is not a read or the clock (tree, layout, state, logs, screenshot, clock); a quoted `expect` checks a read")))
        }
    }

    /// After `expect drag`: the node's `testId`, then `none`, `missing
    /// "<type>"`, or `has "<type>"` with an optional `== "<value>"` for a
    /// string and `name "<name>"` for a file.
    pub(super) fn expect_drag(&mut self, span: Span) -> R<Step> {
        const USAGE: &str = "`expect drag \"card-1016\" has \"image/jpeg\" name \"1016-thumb.jpg\"`, `has \"text/plain\" == \"Mount Fuji\"`, `missing \"text/plain\"` or `none`";
        let target = self.str_lit("a testId")?;
        let (kind, present) = if self.at_ident("none") {
            self.next();
            (None, false)
        } else if self.at_ident("has") || self.at_ident("missing") {
            let present = self.at_ident("has");
            self.next();
            (Some(self.str_lit("a type, as \"text/plain\"")?), present)
        } else {
            return self.err("syntax-expected-step", USAGE);
        };
        let (mut value, mut name) = (None, None);
        if present && self.at_punct("==") {
            self.next();
            value = Some(self.str_lit("the string the drag carries")?);
        }
        if present && self.at_ident("name") {
            self.next();
            name = Some(self.str_lit("the file's own name, as \"1016-thumb.jpg\"")?);
        }
        Ok(Step::ExpectDrag {
            target,
            kind,
            present,
            value,
            name,
            span,
        })
    }
}
