//! The declarations whose clauses follow a call or a shape: `resource` (its
//! source, list, context, placeholder and whether its answer is kept),
//! `mutation` (its reply, what it refreshes, its `then`, and whether its sends
//! wait their turn) and `task` (its schedule).

use super::*;

impl Parser {
    /// `mutation NAME as shape T [queue | parallel] [refreshes a, b] [then action]`.
    /// The clauses after the shape may continue on deeper-indented lines, in
    /// the same order (the codeedit diary: `then` on the next line was
    /// `syntax-expected-section`).
    pub(super) fn mutation(&mut self) -> R<MutationDecl> {
        let span = self.expect_word("mutation")?;
        let name = self.named_ident(span)?;
        self.expect_word("as")?;
        self.expect_word("shape")?;
        let shape = self.type_expr()?;
        let mut continued = false;
        // @ref LLP 1092 D1, LLP 1161.000 D1 — keywords only here: every
        // send of the mutation waits for the one before it, or none waits.
        let mode = if self.clause_ahead("queue", &mut continued) {
            MutationMode::Queue
        } else if self.clause_ahead("parallel", &mut continued) {
            MutationMode::Parallel
        } else {
            MutationMode::Newest
        };
        if mode != MutationMode::Newest {
            self.next();
        }
        let mut refreshes = Vec::new();
        if self.clause_ahead("refreshes", &mut continued) {
            self.next();
            loop {
                refreshes.push(self.ident()?);
                if !self.eat_punct(",") {
                    break;
                }
            }
        }
        if !refreshes.is_empty()
            && matches!(self.peek_kind(), TokenKind::Ident(word)
                if !matches!(word.as_str(), "queue" | "parallel" | "refreshes" | "then"))
        {
            return self.err(
                "syntax-expected-newline",
                "separate refreshed resources with commas: `refreshes board, feed`",
            );
        }
        let then = if self.clause_ahead("then", &mut continued) {
            let then_span = self.next().span;
            Some(self.named_ident(then_span).map(|name| (name, then_span))?)
        } else {
            None
        };
        // A clause out of order (r32 t4 wrote `refreshes cards queue`), or
        // both modes.
        if let Some(word) = ["queue", "parallel", "refreshes", "then"]
            .into_iter()
            .find(|w| self.at_ident(w))
        {
            return self.err(
                "syntax-expected-newline",
                format!(
                    "`{word}` is out of place: a mutation's clauses go `as shape T`, then \
                     `queue` or `parallel`, then `refreshes a, b`, then `then action`, each at most once"
                ),
            );
        }
        self.newline()?;
        if continued && !matches!(self.peek_kind(), TokenKind::Dedent | TokenKind::Eof) {
            return self.err(
                "syntax-expected",
                format!(
                    "a mutation's continued lines hold `queue` or `parallel`, `refreshes …` and `then …`, in that order; found {}",
                    describe(self.peek_kind())
                ),
            );
        }
        if continued && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.next();
        }
        Ok(MutationDecl {
            name,
            shape,
            mode,
            refreshes,
            then,
            span,
        })
    }

    pub(super) fn resource(&mut self) -> R<ResourceDecl> {
        let span = self.expect_word("resource")?;
        let name = self.named_ident(span)?;
        self.expect_punct("=")?;
        let source = self.source_ident(span)?;
        self.expect_punct("(")?;
        let mut args = self.call_args()?;
        // @ref LLP 1017.000 P4c (amended 2026-11-04) — one request for each
        // element of a list, keyed as a `for` row is.
        let each = if self.at_ident("for") {
            let at = self.next().span;
            let item = self.named_ident(at)?;
            self.expect_word("in")?;
            let list = self.expr()?;
            let key = if self.at_ident("key") {
                self.next();
                self.expect_punct("=")?;
                Some(self.expr()?)
            } else {
                None
            };
            if self.at_ident("with") {
                return self.err(
                    "syntax-resource-for-with",
                    "a resource over a list takes no `with`: write the context among the call's arguments",
                );
            }
            Some(ResourceEach {
                item,
                list,
                key,
                span: at,
            })
        } else {
            None
        };
        // @ref LLP 1027.005 D1/D6 — context remains ordinary source arguments.
        let identity = if self.at_ident("with") {
            self.next();
            let identity = args.len();
            loop {
                args.push(self.expr()?);
                if !self.eat_punct(",") {
                    break;
                }
            }
            Some(identity)
        } else {
            None
        };
        // @ref LLP 1161.002 D1 — asked only while the condition holds.
        let gate = if self.at_ident("when") {
            if each.is_some() {
                return self.err(
                    "syntax-resource-for-when",
                    "a resource over a list takes no `when`: its list already leaves out what it should not ask",
                );
            }
            self.next();
            Some(self.expr()?)
        } else {
            None
        };
        self.expect_word("as")?;
        self.expect_word("shape")?;
        let shape = self.type_expr()?;
        // @ref LLP 1048.003 D6 — what shows until the source answers.
        let placeholder = if self.at_ident("else") {
            if each.is_some() {
                return self.err(
                    "syntax-resource-for-else",
                    "a resource over a list takes no `else`: each element shows its shape's zero until its answer",
                );
            }
            self.next();
            let at = self.peek().span;
            if !matches!(self.peek_kind(), TokenKind::Ident(_)) {
                // `else []`, `else ""` (app farm round 1): a value, where a call goes.
                return self.err(
                    "syntax-placeholder-call",
                    "a resource's `else` is a call: `else empty()` is its shape's zero (`[]` for a list, `\"\"` for text), with `field=constant` overrides for a record, or `else source(values…)`",
                );
            }
            let source = self.source_ident(at)?;
            self.expect_punct("(")?;
            let args = self.call_args()?;
            Some(Placeholder {
                source,
                args,
                span: at,
            })
        } else {
            None
        };
        // @ref LLP 1156.005 D4 — an answer the device's store never keeps.
        let transient = self.at_ident("transient");
        if transient {
            self.next();
        }
        let keep = self.at_ident("keep");
        if keep {
            self.next();
        }
        self.newline()?;
        Ok(ResourceDecl {
            name,
            source,
            args,
            identity,
            shape,
            placeholder,
            each,
            transient,
            keep,
            gate,
            span,
        })
    }

    /// Whether the next clause is `word`, on this line or on a continued
    /// one: the first continued line opens an indented block (`continued`
    /// records it), each later one is a line of that block.
    pub(super) fn clause_ahead(&mut self, word: &str, continued: &mut bool) -> bool {
        if self.at_ident(word) {
            return true;
        }
        let ahead = |n: usize| &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind;
        let is_word = |k: &TokenKind| matches!(k, TokenKind::Ident(w) if w == word);
        let opens = !*continued
            && matches!(ahead(0), TokenKind::Newline)
            && matches!(ahead(1), TokenKind::Indent)
            && is_word(ahead(2));
        let goes_on = *continued && matches!(ahead(0), TokenKind::Newline) && is_word(ahead(1));
        if opens {
            self.next();
            self.next();
            *continued = true;
        } else if goes_on {
            self.next();
        }
        opens || goes_on
    }

    pub(super) fn task(&mut self) -> R<Task> {
        let span = self.expect_word("task")?;
        let name = self.named_ident(span)?;
        // @ref LLP 1092 D7 — `mount`, or a gate: `when cond [key=expr]`, or
        // `key=expr` alone (`when true`), spelled as `for` spells its key.
        let Some((gate, key)) = self.gate()? else {
            return self.err(
                "syntax-task-start",
                format!("`task {name}` starts at `mount`, or while a condition holds: `when cond`, `when cond key=expr` or `key=expr`"),
            );
        };
        self.newline()?;
        let mut timer = None;
        let mut options = Vec::new();
        self.block(|p| {
            let (f, fspan) = p.ident()?;
            let mut kind = match f.as_str() {
                "every" => TaskKind::Every,
                "after" => TaskKind::After,
                "change" => TaskKind::Change,
                _ => {
                    return p.err(
                        "contract-task-body",
                        "a task body is `every(ms, action)`, `every(frame, action)`, `after(ms, action)` or `change(action)` with `key=`",
                    )
                }
            };
            p.expect_punct("(")?;
            // `every(frame, a)`: `frame` there is a word, not an expression (LLP 1073 D1).
            let frame = p.at_ident("frame") && matches!(p.peek2(), TokenKind::Punct(","));
            let ms = if kind == TaskKind::Change {
                Expr::Number(0.0, fspan)
            } else if frame {
                if kind == TaskKind::After {
                    return p.err(
                        "contract-task-body",
                        "`after` takes milliseconds; `every(frame, action)` fires each frame",
                    );
                }
                kind = TaskKind::Frame;
                let at = p.next().span;
                Expr::Number(0.0, at)
            } else {
                p.expr()?
            };
            if kind != TaskKind::Change {
                p.expect_punct(",")?;
            }
            let action = p.named_ident(fspan)?;
            while p.eat_punct(",") {
                let (name, at) = p.ident()?;
                p.expect_punct("=")?;
                options.push((name, p.expr()?, at));
            }
            p.expect_punct(")")?;
            p.newline()?;
            if timer.is_some() {
                return duplicate("task entry", &f, fspan, span);
            }
            timer = Some((kind, (ms, action, fspan)));
            Ok(())
        })?;
        let (kind, timer) = timer.ok_or(SyntaxError {
            id: "contract-task-body",
            message:
                "a task needs `every(ms, action)`, `every(frame, action)` or `after(ms, action)`"
                    .into(),
            span,
        })?;
        Ok(Task {
            name,
            kind,
            gate,
            key,
            timer,
            options,
            span,
        })
    }
}
