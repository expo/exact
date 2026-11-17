//! A CSS pseudo-element part: `::placeholder` (LLP 1104 D3, Charlie
//! 2026-10-12) or `::backdrop` (LLP 1035.001 D11, Charlie 2026-11-14) on its
//! own line, then an indented block of the part's declarations, in a `style`
//! (literal) or under its element (computed, as its own attributes are).
//! Each declaration is carried as an attribute named for the part and the
//! property (`::placeholder color`, [`part_attr`]), so a class's and the
//! node's merge as any row does and the node's own wins. Lowering admits a
//! placeholder's `color` and a backdrop's `background-color` alone, and on a
//! route `::view-transition-new`'s and `::view-transition-old`'s `animation`
//! alone (LLP 1013.001 D1).

use super::*;

impl Parser {
    /// Whether the line starts a part: `::` and a name.
    pub(super) fn at_part(&self) -> bool {
        matches!(
            (self.peek_kind(), self.peek2()),
            (TokenKind::Punct(":"), TokenKind::Punct(":"))
        )
    }

    /// `::name`, through its block, its declarations added to `attrs`;
    /// `literal` in a `style`. `owner` names the style or element.
    pub(super) fn part(&mut self, owner: &str, literal: bool, attrs: &mut Vec<Attr>) -> R<()> {
        let span = self.next().span;
        self.next();
        let TokenKind::Ident(name) = self.peek_kind().clone() else {
            return Err(SyntaxError {
                id: "syntax-part",
                message: format!(
                    "`::` begins a part, `::placeholder` or `::backdrop`; found {}",
                    describe(self.peek_kind())
                ),
                span,
            });
        };
        let span = Span {
            end_col: self.next().span.end_col,
            ..span
        };
        let part = format!("::{name}");
        let refusal = match name.as_str() {
            "placeholder" | "backdrop" | "view-transition-new" | "view-transition-old" => None,
            "view-transition-group" | "view-transition-image-pair" | "view-transition" => Some(format!(
                "`{part}` has nothing to style: a route's transition is its picture alone, so its parts are `::view-transition-new` (its entrance) and `::view-transition-old` (its exit), each with `animation` (LLP 1013.001)"
            )),
            "before" | "after" => Some(format!(
                "`{part}` has no generated box in Contract: write what it would show as a child (a `text`, an `image`), which every host lays out, draws and reads aloud"
            )),
            _ => Some(format!(
                "`{part}` is not a part Contract has; it has `::placeholder` (its `color`), on an `input` or a `textarea`, `::backdrop` (its `background-color`), on a `dialog`, and `::view-transition-new` and `::view-transition-old` (their `animation`), on a route"
            )),
        };
        if let Some(message) = refusal {
            return Err(SyntaxError {
                id: "syntax-part",
                message,
                span,
            });
        }
        self.newline()?;
        let within = format!("{owner} {part}");
        let mut declared = Vec::new();
        self.block(|p| {
            if literal {
                declared.extend(p.literal_line(&within)?);
            } else {
                let mut positional = Vec::new();
                p.attr_line(&mut positional, &mut declared, true)?;
                p.newline()?;
            }
            Ok(())
        })?;
        if declared.is_empty() {
            return Err(SyntaxError {
                id: "syntax-empty-block",
                message: format!(
                    "`{part}` needs its declarations indented under it: {}",
                    match name.as_str() {
                        "placeholder" => "`color=\"#6b7280\"`",
                        "backdrop" => "`background-color=\"#00000066\"`",
                        _ => "`animation=\"fade-in 180ms ease-out both\"`",
                    }
                ),
                span,
            });
        }
        for a in declared {
            let name = part_attr(&part, &a.name);
            if attrs.iter().any(|b| b.name == name) {
                return Err(SyntaxError {
                    id: "syntax-duplicate-attr",
                    message: format!("`{}` appears twice in `{part}` of {owner}", a.name),
                    span: a.span,
                });
            }
            attrs.push(Attr { name, ..a });
        }
        Ok(())
    }
}
