//! The view arms live in separate frames from the recursive dispatcher.

use super::*;

impl Parser {
    pub(super) fn view_range(&mut self, owner: Span, start: Span) {
        if self.view_ranges {
            self.names
                .view_ranges
                .insert(owner, (start, self.tokens[self.pos - 1].span));
        }
    }

    /// `if cond`, with `else if` and `else` as an action's (@ref LLP 1116.015 D1).
    #[inline(never)]
    pub(super) fn node_if(&mut self, span: Span) -> R<Node> {
        self.next();
        let cond = self.expr()?;
        self.view_range(span, span);
        self.newline()?;
        let then = self.required_block(span, "if", |p| p.node())?;
        let mut otherwise = Vec::new();
        if self.at_ident("else") {
            self.next();
            // `else if` is an `else` whose one node is the next `if`.
            if self.at_ident("if") || self.at_ident("when") {
                let retired = self.at_ident("when");
                let first = self.retired.len();
                otherwise = vec![self.node()?];
                if retired {
                    self.retired[first].id = "syntax-else-keyword";
                }
            } else {
                self.after_else("if")?;
                otherwise = self.required_block(span, "else", |p| p.node())?;
            }
        }
        Ok(Node::When {
            tag: 0,
            cond,
            then,
            otherwise,
            span,
        })
    }
    /// `for x in xs key=…`, or `for x, i in xs key=…` (@ref LLP 1116.015 D1).
    #[inline(never)]
    pub(super) fn node_for(&mut self, span: Span) -> R<Node> {
        self.next();
        let var = self.named_ident(span)?;
        let index = if self.at_punct(",") {
            self.next();
            Some(self.named_ident(span)?)
        } else {
            None
        };
        self.expect_word("in")?;
        let list = self.expr()?;
        // @ref LLP 1116.014 D1 — the edit, with the variable the author named. The
        // parser knows no item type, so the field is described, not named.
        if !self.at_ident("key") {
            return self.err(
                "syntax-expected",
                format!(
                    "add `key={var}.<its id field>`, a field no two items share: a `for` keeps \
                     each row's state by its key"
                ),
            );
        }
        self.expect_word("key")?;
        self.expect_punct("=")?;
        let key = self.expr()?;
        self.view_range(span, span);
        self.newline()?;
        let body = self.block(|p| p.node())?;
        Ok(Node::Each {
            tag: 0,
            var,
            index,
            list,
            key,
            body,
            span,
        })
    }
    #[inline(never)]
    pub(super) fn node_match(&mut self, span: Span) -> R<Node> {
        self.next();
        let subject = self.expr()?;
        self.newline()?;
        let platform = match &subject {
            Expr::Member(base, field, _) if field == "platform" => match &**base {
                Expr::Ident(name, _) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        };
        let mut cases = Vec::new();
        let mut some = None;
        let mut none = None;
        if matches!(self.peek_kind(), TokenKind::Indent) {
            self.next();
            loop {
                match self.peek_kind() {
                    TokenKind::Dedent => {
                        self.next();
                        break;
                    }
                    TokenKind::Eof => break,
                    TokenKind::Newline => {
                        self.next();
                    }
                    _ if platform.is_some()
                        && self.at_ident("case")
                        && matches!(self.peek2(), TokenKind::Str(_)) =>
                    {
                        let case = self.next().span;
                        let TokenKind::Str(host) = self.next().kind else {
                            unreachable!()
                        };
                        if !crate::MODULE_PLATFORMS.contains(&host.as_str()) {
                            return Err(SyntaxError {
                                id: "contract-match-platform",
                                message: format!("unknown platform {host:?}"),
                                span: case,
                            });
                        }
                        if let Some((_, first, _)) = cases.iter().find(|(name, _, _)| *name == host)
                        {
                            return duplicate("platform case", &host, case, *first);
                        }
                        self.view_range(case, case);
                        self.newline()?;
                        // Later cases lower to nested else-if nodes.
                        self.view_depth += cases.len();
                        let body = self.required_block(case, "case", |p| p.node());
                        self.view_depth -= cases.len();
                        cases.push((host, case, body?));
                    }
                    _ => {
                        let (var, case_span) = self.match_case_header(span)?;
                        if let Some(var) = var {
                            if some.is_some() {
                                return duplicate("match arm", "some", case_span, span);
                            }
                            some = Some((var, self.block(|p| p.node())?));
                        } else {
                            if none.is_some() {
                                return duplicate("match arm", "none", case_span, span);
                            }
                            none = Some(self.block(|p| p.node())?);
                        }
                    }
                }
            }
        }
        if !cases.is_empty() {
            self.platform_matches
                .push((platform.unwrap().to_owned(), span));
            if some.is_some() || none.is_some() {
                return Err(SyntaxError {
                    id: "contract-match-platform",
                    message: "a platform match takes only quoted platform cases".into(),
                    span,
                });
            }
            let mut otherwise = Vec::new();
            for (host, case, then) in cases.into_iter().rev() {
                otherwise = vec![Node::When {
                    tag: 0,
                    cond: Expr::Binary(
                        BinOp::Eq,
                        Box::new(subject.clone()),
                        Box::new(Expr::Str(host, case)),
                        case,
                    ),
                    then,
                    otherwise,
                    span: case,
                }];
            }
            return Ok(otherwise.pop().unwrap());
        }
        let some = some.ok_or(SyntaxError {
            id: "contract-match-arms",
            message: "`match` needs `case some(x)`".into(),
            span,
        })?;
        let none = none.ok_or(SyntaxError {
            id: "contract-match-arms",
            message: "`match` needs `case none`".into(),
            span,
        })?;
        Ok(Node::Match {
            tag: 0,
            subject,
            some,
            none,
            span,
        })
    }

    #[inline(never)]
    fn match_case_header(&mut self, span: Span) -> R<(Option<String>, Span)> {
        let case_span = self.expect_word("case")?;
        if self.at_ident("some") {
            self.next();
            self.expect_punct("(")?;
            let var = self.named_ident(span)?;
            self.expect_punct(")")?;
            self.case_end(Some(&var), "elements")?;
            Ok((Some(var), case_span))
        } else {
            self.case_word("none")?;
            self.case_end(None, "elements")?;
            Ok((None, case_span))
        }
    }

    /// The end of a block `match`'s `case some(var)` or `case none` line,
    /// whose arm (`what` it holds) is the indented lines under it. `=>` is
    /// the inline `match`'s, so it is named with the form to write.
    pub(super) fn case_end(&mut self, var: Option<&str>, what: &str) -> R<()> {
        if !self.at_punct("=>") {
            return self.newline();
        }
        let case = var.map_or("case none".to_string(), |v| format!("case some({v})"));
        let inline = if what == "statements" {
            "`x = match subject { case some(v) => v, case none => 0 }`"
        } else {
            "`text (match subject { case some(v) => v, case none => \"\" })`"
        };
        self.err(
            "syntax-case-arrow",
            format!(
                "a block `match` takes no `=>`: end the line at `{case}` and write its {what} on \
                 the indented lines under it; `=>` is the inline `match`'s, which is a value: {inline}"
            ),
        )
    }
    #[inline(never)]
    pub(super) fn node_use(&mut self, word: String, span: Span) -> R<Node> {
        self.next();
        self.expect_punct("(")?;
        let args = self.named_args()?;
        self.newline()?;
        let children = self.block(|p| p.node())?;
        Ok(Node::Use {
            name: word,
            args,
            children,
            span,
        })
    }
    #[inline(never)]
    pub(super) fn node_element(&mut self, word: String, span: Span) -> R<Node> {
        self.next();
        let mut positional = Vec::new();
        let mut attrs = Vec::new();
        self.attr_line(&mut positional, &mut attrs, false)?;
        self.newline()?;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Indent) {
            self.next();
            // The attribute list may continue on deeper lines that
            // begin with `name=`; a child never does (LLP 1035.005 D1).
            // So may a part's block, `::placeholder` (parts.rs).
            // And a state part's, `:disabled` (states.rs).
            while self.at_continuation() || self.at_part() || self.at_state() {
                if self.at_part() {
                    self.part(&format!("`{word}`"), false, &mut attrs)?;
                    continue;
                }
                if self.at_state() {
                    self.state_part(&format!("`{word}`"), false, &mut attrs)?;
                    continue;
                }
                self.attr_line(&mut positional, &mut attrs, true)?;
                self.newline()?;
            }
            children = self.block_rest(|p| p.node())?;
        }
        // A label on a container is its first text child on every host
        // (LLP 1017.001 for buttons).
        if matches!(word.as_str(), "button" | "blockquote" | "li") && positional.len() == 1 {
            if self.view_depth >= 256 {
                return self.err("syntax-view-depth", "views nest more than 256 sites deep");
            }
            children.insert(
                0,
                Node::Element {
                    tag: "text".into(),
                    positional: vec![positional.remove(0)],
                    attrs: Vec::new(),
                    children: Vec::new(),
                    span,
                    instance: 0,
                    place: None,
                },
            );
        }
        let tag = heading(word, span, &mut attrs)?;
        Ok(Node::Element {
            tag,
            positional,
            attrs,
            children,
            span,
            instance: 0,
            place: None,
        })
    }
}

/// LLP 1116.017 D1: HTML's heading elements are headings, the level in the
/// name; every host then sees the one form, `text role="heading"`. Apart from
/// `node_element`, whose frame every nested view node costs.
#[inline(never)]
fn heading(word: String, span: Span, attrs: &mut Vec<Attr>) -> R<String> {
    let level = match word.as_str() {
        "h1" => 1.0,
        "h2" => 2.0,
        "h3" => 3.0,
        "h4" => 4.0,
        "h5" => 5.0,
        "h6" => 6.0,
        _ => return Ok(word),
    };
    if let Some(a) = attrs
        .iter()
        .find(|a| matches!(a.name.as_str(), "role" | "aria-level"))
    {
        return Err(SyntaxError {
            id: "syntax-heading-level",
            message: format!(
                "an `{word}` is a heading of level {level}: drop its `{}`, or write `text … role=\"heading\" aria-level=…` for a level the app computes",
                a.name
            ),
            span: a.span,
        });
    }
    for (name, value) in [
        ("role", Expr::Str("heading".into(), span)),
        ("aria-level", Expr::Number(level, span)),
    ] {
        attrs.push(Attr {
            name: name.into(),
            value,
            span,
            written: true,
        });
    }
    Ok("text".into())
}
