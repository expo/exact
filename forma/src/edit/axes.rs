//! A removal is checked on the transaction's complete design. Cite a surviving
//! condition at its authored line, even when removing the axis shifted that line.

use super::Op;
use crate::{params, Design, Diagnostic, When};
use std::collections::{HashMap, HashSet};

fn owners<'a>(
    d: &'a Design,
    params: &'a params::Params,
) -> impl Iterator<Item = (&'static str, &'a crate::Token)> {
    d.tokens
        .iter()
        .map(|t| ("token", t))
        .chain(d.params.iter().map(|t| ("param", t)))
        .chain(d.icons.iter().map(|t| ("icon", t)))
        .chain(
            params::inherited(&d.tokens, &params.joined)
                .into_iter()
                .map(|t| ("package token", t)),
        )
        .chain(
            params::inherited(&d.icons, &params.joined_icons)
                .into_iter()
                .map(|t| ("package icon", t)),
        )
}

fn conditions<'a>(
    d: &'a Design,
    params: &'a params::Params,
) -> impl Iterator<Item = (&'static str, &'a str, &'a When)> {
    owners(d, params).flat_map(|(kind, t)| {
        t.whens
            .iter()
            .map(move |w| (kind, t.name.value.as_str(), w))
    })
}

/// A rename proves the same values on both sides. Include any modes an earlier
/// edit removed whose conditions a later edit still has to unset. Only the
/// proof uses these declarations. The final design checks without them.
pub(super) fn rename_modes(
    before: &Design,
    after: &Design,
    params: &params::Params,
    removed: &[crate::Axis],
) -> Option<Vec<crate::Axis>> {
    if removed.is_empty() {
        return None;
    }
    let used: HashSet<_> = conditions(before, params)
        .chain(conditions(after, params))
        .flat_map(|(_, _, w)| w.conds.iter().map(|(a, v)| (a.as_str(), v.as_str())))
        .collect();
    let mut axes = before.axes.clone();
    for old in removed.iter().rev() {
        let mut used_values = old
            .values
            .iter()
            .filter(|v| used.contains(&(old.name.value.as_str(), v.as_str())));
        if let Some(current) = axes.iter_mut().find(|a| a.name.value == old.name.value) {
            for v in used_values {
                if !current.values.contains(v) {
                    current.values.push(v.clone());
                }
            }
        } else if used_values.next().is_some() {
            axes.push(old.clone());
        }
    }
    (axes != before.axes).then_some(axes)
}

pub(super) fn check_removals(
    src: &str,
    design: &Design,
    ops: &[Op],
    vocab: &crate::Vocab,
    params: &params::Params,
) -> Result<(), Vec<Diagnostic>> {
    let removed: HashSet<_> = ops
        .iter()
        .filter_map(|op| match op {
            Op::RemoveAxis { name } => Some((name.as_str(), None)),
            Op::RemoveAxisValue { axis, value } => Some((axis.as_str(), Some(value.as_str()))),
            _ => None,
        })
        .collect();
    let declared: HashSet<_> = design
        .axes
        .iter()
        .flat_map(|a| {
            a.values
                .iter()
                .map(move |v| (a.name.value.as_str(), v.as_str()))
        })
        .collect();
    let dangling: Vec<_> = conditions(design, params)
        .filter(|(_, _, w)| {
            w.conds.iter().any(|(a, v)| {
                !declared.contains(&(a.as_str(), v.as_str()))
                    && (removed.contains(&(a.as_str(), None))
                        || removed.contains(&(a.as_str(), Some(v.as_str()))))
            })
        })
        .collect();
    if dangling.is_empty() {
        return Ok(());
    }

    // Cite the disk condition or, for a new branch, its existing declaration.
    // Package lines already carry their source location.
    let original = crate::parse(src)?;
    let roles: Vec<_> = vocab
        .roles
        .iter()
        .map(|r| (r.name.clone(), r.system))
        .collect();
    let key = |kind: &'static str, name: &str| {
        let role = (kind == "token")
            .then(|| super::role_key(&roles, name))
            .flatten();
        (kind, role.unwrap_or_else(|| name.to_owned()))
    };
    let mut sources: HashMap<_, _> = owners(&original, &params::NONE)
        .map(|(kind, t)| {
            let branches: HashMap<_, _> = t
                .whens
                .iter()
                .map(|w| (crate::syntax::conds_key(&w.conds), (w.line, w.col)))
                .collect();
            (
                key(kind, &t.name.value),
                ((t.name.line, t.name.col), branches),
            )
        })
        .collect();
    // A renamed declaration keeps its source. Once a branch or its owner
    // goes, a later set with the same name has no corresponding disk line.
    for op in ops {
        let (owner, unset) = match op {
            Op::RenameToken { from, to } => {
                if let Some(source) = sources.remove(&key("token", from)) {
                    sources.insert(key("token", to), source);
                }
                continue;
            }
            Op::UnsetTokenWhen { name, when } => (key("token", name), when),
            Op::RemoveToken { name } => {
                sources.remove(&key("token", name));
                continue;
            }
            Op::SetParam { name, respell, .. } | Op::UnsetParam { name, respell, .. } => {
                let owner = key("param", name);
                if let Some(old) = respell {
                    if let Some(source) = sources.remove(&key("param", old)) {
                        sources.insert(owner.clone(), source);
                    }
                }
                let Op::UnsetParam { when, .. } = op else {
                    continue;
                };
                (owner, when)
            }
            _ => continue,
        };
        if unset.is_empty() {
            sources.remove(&owner);
        } else if let Some((_, branches)) = sources.get_mut(&owner) {
            branches.remove(&crate::syntax::conds_key(unset));
        }
    }
    Err(dangling
        .into_iter()
        .map(|(kind, name, w)| {
            let source = sources.get(&key(kind, name));
            let place =
                source.and_then(|(_, branches)| branches.get(&crate::syntax::conds_key(&w.conds)));
            let (line, col) = place
                .copied()
                .or_else(|| source.map(|(owner, _)| *owner))
                .unwrap_or((w.line, w.col));
            let added = if place.is_none() && !kind.starts_with("package ") {
                " (condition added by this transaction)"
            } else {
                ""
            };
            let condition = w
                .conds
                .iter()
                .map(|(a, v)| format!("{a}={v}"))
                .collect::<Vec<_>>()
                .join(" ");
            Diagnostic::new(
                "forma-edit",
                format!("{kind} `{name}` still uses `when {condition}`{added}"),
                line,
                col,
            )
        })
        .collect())
}
