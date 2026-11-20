//! `expect drag` (LLP 1094.000 D10): what a node's drag carries, as its
//! admitted props declare it, so a test reads the same on every host.

use super::*;

impl Parser {
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
