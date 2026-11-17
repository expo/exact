//! The test step `expect layout`: a bound on a node's laid-out width or
//! height, read from the agent's `layout <target>` (LLP 1012).

use super::*;

impl Parser {
    /// After `expect layout`: a testId, `width` or `height`, a comparison and
    /// a size in points.
    pub(super) fn expect_layout(&mut self, span: Span) -> R<Step> {
        let target = self.str_lit("a testId")?;
        let dimension = match self.peek_kind() {
            TokenKind::Ident(w) if w == "width" || w == "height" => w.clone(),
            other => {
                return self.err(
                    "syntax-expected-step",
                    format!(
                        "`expect layout` reads `width` or `height`, not {}: `expect layout \"<testId>\" height <= 24`",
                        describe(other)
                    ),
                )
            }
        };
        self.next();
        let comparison = if self.eat_punct("==") {
            StateComparison::Equal
        } else if self.eat_punct("<=") {
            StateComparison::AtMost
        } else if self.eat_punct(">=") {
            StateComparison::AtLeast
        } else {
            return self.err(
                "syntax-expected-step",
                format!(
                    "`expect layout` takes `==`, `<=` or `>=`, found {}",
                    describe(self.peek_kind())
                ),
            );
        };
        let value = match *self.peek_kind() {
            TokenKind::Number(n) => n,
            ref other => {
                return self.err(
                    "syntax-expected-step",
                    format!(
                        "`expect layout` compares with a size of 0 or more, in points, found {}",
                        describe(other)
                    ),
                )
            }
        };
        self.next();
        if let TokenKind::Ident(unit) = self.peek_kind() {
            return self.err(
                "syntax-expected-step",
                format!("`expect layout` compares in points, written bare: `{value}`, not `{value}{unit}`"),
            );
        }
        Ok(Step::ExpectLayout {
            target,
            dimension,
            comparison,
            value,
            span,
        })
    }
}
