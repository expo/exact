//! Move parsed press attributes into their owner's active part, retaining values verbatim.

use crate::lexer::{Lexer, Token, TokenKind};
use crate::{Attr, Expr, File, Node, Span, SyntaxError};

pub(super) fn rewrite(src: &str, file: &File) -> Result<String, Vec<SyntaxError>> {
    let tokens = Lexer::tokenize(src, 1).map_err(|e| vec![e.into()])?;
    let mut starts = vec![0];
    starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
    let mut edit = Paints {
        src,
        tokens,
        starts,
        edits: Vec::new(),
    };
    for style in &file.styles {
        edit.owner(&style.attrs, style.span, false)?;
    }
    for component in file.components.iter().chain(file.members.iter()) {
        edit.nodes(&component.view, false)?;
    }
    edit.edits.sort_by_key(|(start, end, _)| (*start, *end));
    let mut after = src.to_string();
    for (start, end, text) in edit.edits.into_iter().rev() {
        after.replace_range(start..end, &text);
    }
    Ok(after)
}

fn property(name: &str) -> Option<&'static str> {
    match name {
        "-exact-press-background-color" => Some("background-color"),
        "-exact-press-color" => Some("color"),
        _ => None,
    }
}

struct Paints<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    starts: Vec<usize>,
    edits: Vec<(usize, usize, String)>,
}

impl Paints<'_> {
    fn offset(&self, span: Span, end: bool) -> usize {
        self.starts[span.line as usize - 1] + (if end { span.end_col } else { span.col }) as usize
            - 1
    }

    fn nodes(&mut self, nodes: &[Node], in_text: bool) -> Result<(), Vec<SyntaxError>> {
        for node in nodes {
            match node {
                Node::Element {
                    tag,
                    attrs,
                    children,
                    span,
                    ..
                } => {
                    let native = tag != "button"
                        && attrs.iter().any(|a| {
                            a.name == "appearance"
                                && matches!(&a.value, Expr::Str(s, _) if s == "auto")
                        });
                    self.owner(attrs, *span, in_text || native)?;
                    self.nodes(children, matches!(tag.as_str(), "text" | "time" | "option"))?;
                }
                node => {
                    for list in node.child_lists() {
                        self.nodes(list, in_text)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Continued attributes may be deeper than children. Logical line starts
    /// reveal their shared block level without reading indentation inside expressions.
    fn body_indent(&self, start: usize) -> Option<u32> {
        if self.tokens.get(start)?.kind != TokenKind::Indent {
            return None;
        }
        let (mut depth, mut line, mut indent) = (1, true, None);
        for token in &self.tokens[start + 1..] {
            match token.kind {
                TokenKind::Indent => depth += 1,
                TokenKind::Dedent => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                TokenKind::Newline => line = true,
                TokenKind::Eof => break,
                _ => {
                    if line && depth == 1 {
                        indent =
                            Some(indent.map_or(token.span.col - 1, |old: u32| {
                                old.min(token.span.col - 1)
                            }));
                    }
                    line = false;
                }
            }
        }
        indent
    }

    fn owner(
        &mut self,
        attrs: &[Attr],
        owner: Span,
        unsupported: bool,
    ) -> Result<(), Vec<SyntaxError>> {
        let paints: Vec<_> = attrs
            .iter()
            .filter_map(|a| Some((a, property(&a.name)?)))
            .collect();
        if paints.is_empty() {
            return Ok(());
        }
        for (attr, name) in &paints {
            let why = if unsupported {
                Some("this target does not accept an active part; choose a supported box or native parameter")
            } else if attrs.iter().any(|a| a.name == format!(":active {name}")) {
                Some("the same paint is already in :active; keep one value before migrating")
            } else {
                None
            };
            if let Some(why) = why {
                return Err(vec![SyntaxError {
                    id: "syntax-migrate-press",
                    message: format!("cannot move `{}`: {why}", attr.name),
                    span: attr.span,
                }]);
            }
        }
        let active = attrs.iter().find(|a| a.name.starts_with(":active "));
        let ending = if self.src.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let (at, indent, mut block) = if let Some(first) = active {
            (
                self.starts[first.span.line as usize - 1],
                first.span.col - 1,
                String::new(),
            )
        } else {
            let first = self
                .tokens
                .iter()
                .position(|t| t.span == owner)
                .expect("parsed owner");
            let line = first
                + self.tokens[first..]
                    .iter()
                    .position(|t| t.kind == TokenKind::Newline)
                    .expect("parsed line");
            // A logical newline ends the whole value, including multiline
            // expressions and positionals on the owner's opening line.
            let last_attr = attrs
                .iter()
                .map(|a| a.span.line)
                .max()
                .unwrap_or(owner.line);
            let end = self.tokens[line..]
                .iter()
                .find(|t| t.kind == TokenKind::Newline && t.span.line >= last_attr)
                .expect("parsed attribute line");
            let at = self
                .starts
                .get(end.span.line as usize)
                .copied()
                .unwrap_or(self.src.len());
            let prefix = if at > 0 && !self.src[..at].ends_with('\n') {
                ending
            } else {
                ""
            };
            let indent = self.body_indent(line + 1).unwrap_or(owner.col + 1);
            (
                at,
                indent + 2,
                format!("{prefix}{}:active{ending}", " ".repeat(indent as usize)),
            )
        };
        for (attr, name) in paints {
            let first = self
                .tokens
                .iter()
                .position(|t| t.span == attr.span)
                .expect("parsed attribute");
            let end =
                crate::parser::attribute_value_end(&self.tokens, first).map_err(|e| vec![e])?;
            let start = self.offset(attr.span, false);
            let finish = self.offset(end, true);
            let value = &self.src[self.offset(attr.span, true)..finish];
            block.push_str(&format!(
                "{}{name}{value}{ending}",
                " ".repeat(indent as usize)
            ));
            self.edits.push((start, finish, String::new()));
        }
        self.edits.push((at, at, block));
        Ok(())
    }
}
