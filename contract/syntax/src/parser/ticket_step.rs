//! Held host and package ticket steps (`tap "@target" <choice>`, `type "@target" "<text>"`).

use super::{Parser, SyntaxError, R};
use crate::ast::{Step, TicketAnswer};
use crate::lexer::TokenKind;
use crate::Span;

impl Parser {
    /// Parse a `tap "@<name>" <choice>` or `type "@<name>" "<text>"` held-ticket step.
    pub(super) fn ticket_step(&mut self, raw: &str, span: Span, typed: bool) -> R<Step> {
        let (verb, name) = (if typed { "type" } else { "tap" }, &raw[1..]);
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return Err(SyntaxError {
                id: "syntax-expected-step",
                message: format!(
                    "`{verb}` takes a non-empty held ticket name without spaces, as `tap \"@browser\" dismissed` or `type \"@date\" \"2026-10-10\"`"
                ),
                span,
            });
        }
        if name.bytes().all(|b| b.is_ascii_digit()) {
            return Err(SyntaxError {
                id: "syntax-expected-step",
                message: format!(
                    "authored tests name a held ticket by its capability or target (for example `\"@browser\"`), not by numeric ticket id `{raw}`"
                ),
                span,
            });
        }
        let answer = if typed {
            let text = self.str_lit("the text answering the held ticket")?;
            if text.is_empty() {
                return self.err(
                    "syntax-expected-step",
                    format!("`type \"@{name}\"` requires non-empty answer text"),
                );
            }
            if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof) {
                return self.err(
                    "syntax-expected-step",
                    format!("`type \"@{name}\"` takes only the quoted answer text (no `append` or extra words)"),
                );
            }
            TicketAnswer::Text(text)
        } else {
            let expected = format!(
                "`tap \"@{name}\"` takes one choice word or quoted choice, as `tap \"@browser\" dismissed`"
            );
            let choice = match self.peek_kind().clone() {
                TokenKind::Ident(c) | TokenKind::Str(c) if !c.is_empty() => {
                    self.next();
                    c
                }
                _ => return self.err("syntax-expected-step", expected),
            };
            if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof) {
                return self.err("syntax-expected-step", expected);
            }
            TicketAnswer::Choice(choice)
        };
        Ok(Step::Ticket {
            target: name.to_string(),
            answer,
            span,
        })
    }
}
