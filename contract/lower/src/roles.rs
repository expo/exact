//! Literal role results checked against the schema (LLP 1166.002 D3).
use crate::{err, spelling, LowerError};
use contract_syntax::Expr;
use exact_kernel::ARIA_ROLES;

pub(crate) fn check(value: &Expr) -> Result<(), LowerError> {
    match value {
        Expr::Ternary(_, yes, no, _)
        | Expr::Match {
            some: yes,
            none: no,
            ..
        } => {
            check(yes)?;
            check(no)
        }
        Expr::Let { body, .. } => check(body),
        Expr::Str(text, span) => {
            let known = |word: &str| {
                ARIA_ROLES
                    .iter()
                    .any(|role| role.eq_ignore_ascii_case(word))
            };
            // An empty role is no role, as in HTML: one arm of a computed role.
            if text.trim().is_empty() || text.split_ascii_whitespace().any(known) {
                return Ok(());
            }
            const ABSTRACT: &[&str] = &[
                "command",
                "composite",
                "input",
                "landmark",
                "range",
                "roletype",
                "section",
                "sectionhead",
                "select",
                "structure",
                "widget",
                "window",
            ];
            if let Some(role) = text
                .split_ascii_whitespace()
                .find(|word| ABSTRACT.iter().any(|role| role.eq_ignore_ascii_case(word)))
            {
                return err("lower-unknown-role", format!("`role=\"{text}\"`: `{role}` is an abstract role; ARIA forbids authors to write an abstract role"), *span);
            }
            let lower = text.to_ascii_lowercase();
            let hint = spelling::similar_in(lower.split_ascii_whitespace(), |word| {
                ARIA_ROLES.contains(&word)
            })
            .map(|role| format!("; did you mean `{role}`?"))
            .unwrap_or_default();
            err(
                "lower-unknown-role",
                format!("`role=\"{text}\"` has no concrete ARIA role{hint}"),
                *span,
            )
        }
        _ => Ok(()),
    }
}
