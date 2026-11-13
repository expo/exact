//! `source name(param: type, …) as shape T` and its request's lines (LLP
//! 1162.001 D1). The parser reads each line as a key and its words; lowering
//! says what each key means and refuses what it does not admit.

use super::*;

impl Parser {
    pub(super) fn source_decl(&mut self) -> R<SourceDecl> {
        let span = self.expect_word("source")?;
        let name = self.named_ident(span)?;
        self.expect_punct("(")?;
        let mut params: Vec<Param> = Vec::new();
        while !self.at_punct(")") {
            let (pname, pspan) = self.ident()?;
            if let Some(first) = params.iter().find(|p| p.name == pname) {
                return duplicate("source parameter", &pname, pspan, first.span);
            }
            self.expect_punct(":")?;
            let ty = self.type_expr()?;
            params.push(Param {
                name: pname,
                ty: Some(ty),
                span: pspan,
            });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(")")?;
        self.expect_word("as")?;
        self.expect_word("shape")?;
        let shape = self.type_expr()?;
        self.newline()?;
        let lines = self.required_block(span, "source", |p| p.source_line())?;
        Ok(SourceDecl {
            name,
            params,
            shape,
            lines,
            span,
        })
    }

    /// One line: a key, then its words, each a `"…"` string or a bare word.
    fn source_line(&mut self) -> R<SourceLine> {
        let (key, span) = self.ident()?;
        let mut words = Vec::new();
        loop {
            match self.peek_kind().clone() {
                TokenKind::Str(s) => words.push(s),
                TokenKind::Ident(w) => words.push(w),
                TokenKind::Newline | TokenKind::Eof | TokenKind::Dedent => break,
                other => {
                    return self.err(
                        "syntax-source-line",
                        format!(
                            "a `source`'s line is a key and its words in quotes (`url \"https://…\"`, `header \"Accept\" \"text/csv\"`), found {}",
                            describe(&other)
                        ),
                    )
                }
            }
            self.next();
        }
        if !matches!(self.peek_kind(), TokenKind::Dedent) {
            self.newline()?;
        }
        Ok(SourceLine { key, words, span })
    }
}
