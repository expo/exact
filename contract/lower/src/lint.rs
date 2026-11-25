//! Refusals that need no types: an element's tag, its attributes' names,
//! and its literal style values against their rows.

use crate::{dataset, native, shorthands, svg, tags, values, LowerError, MAX_REFUSALS};
use contract_syntax::{Attr, Component, Expr, File, Node, Span};
use contract_types::{Scope, Ty};
use exact_kernel::StyleId;
use exact_motion::animation::LONGHANDS;

/// Check literal references in the expanded view, where component arguments
/// have their values. Computed names resolve against the mounted elements.
pub(crate) fn action_targets(checked: &contract_types::Checked<'_>, out: &mut Vec<LowerError>) {
    let checking = contract_checking::Checking::new(checked);
    let view = checking.expanded();
    if !view.walk().any(|node| {
        node.element()
            .is_some_and(|(_, _, attrs)| attrs.iter().any(|a| a.name == "aria-actions"))
    }) {
        return;
    }
    let mut refs = Vec::new();
    let mut targets = std::collections::BTreeMap::new();
    let branches =
        view.structure()
            .walk_regions_with((Vec::new(), false), |node, (arms, inline)| {
                [false, true].map(|other| {
                    let mut path = arms.clone();
                    if node.is_choice() {
                        path.push((node.site(), other));
                    }
                    Some((
                        path,
                        node.element()
                            .map_or(*inline, |(tag, _, _)| crate::prose::text_container(tag)),
                    ))
                })
            });
    for (node, (arms, inline)) in branches {
        let Some((tag, _, attrs)) = node.element() else {
            continue;
        };
        let attr = |name: &str| attrs.iter().find(|a| a.name == name);
        let id = attr("id").map(|a| &a.value);
        if let Some(actions) = attr("aria-actions") {
            if let Expr::Str(names, _) = &actions.value {
                refs.push((names, id, actions.span, arms.clone()));
            }
        }
        if let Some(Expr::Str(id, _)) = id {
            let presses = attr("press").is_some()
                || (matches!(tag, "link" | "button") || inline && crate::prose::phrasing(tag))
                    && attr("href")
                        .is_some_and(|a| !matches!(&a.value, Expr::Str(s, _) if s.is_empty()));
            targets
                .entry(id.as_str())
                .or_insert_with(Vec::new)
                .push((arms, presses));
        }
    }
    for (names, own, span, arms) in refs {
        for name in names.split_ascii_whitespace() {
            let reason = if matches!(own, Some(Expr::Str(id, _)) if id == name) {
                "names the element itself"
            } else {
                // Only opposite arms of the same choice exclude a target.
                let mut mounted = targets
                    .get(name)
                    .into_iter()
                    .flatten()
                    .filter(|(path, _)| {
                        arms.iter()
                            .zip(path)
                            .take_while(|(a, b)| a.0 == b.0)
                            .all(|(a, b)| a.1 == b.1)
                    })
                    .peekable();
                if mounted.peek().is_none() {
                    "names no element in this view with that literal id"
                // A conditional target may disappear. An unconditional earlier id wins.
                } else if mounted
                    .take_while(|(path, presses)| *presses || !arms.starts_with(path))
                    .any(|(_, presses)| *presses)
                {
                    continue;
                } else {
                    "names an element with no press"
                }
            };
            out.push(LowerError {
                id: "lower-aria-action-target",
                message: format!("`aria-actions` id `{name}` {reason}"),
                span,
            });
            if out.len() >= MAX_REFUSALS {
                return;
            }
        }
    }
}

pub(crate) fn unknown_tag(tag: &str, span: Span) -> LowerError {
    let hint = tags::html_tag(tag)
        .or_else(|| svg::refused_tag(tag))
        .map(|spelled| format!("; {spelled}"))
        .or_else(|| tags::similar_tag(tag).map(|n| format!("; did you mean `{n}`?")))
        .unwrap_or_default();
    LowerError {
        id: "lower-unknown-tag",
        message: format!("unknown tag `{tag}`{hint}"),
        span,
    }
}

/// CSS's fragmentation properties exact2 knows and does not implement: the
/// spanning element and paged media's breaks (LLP 1093 §1, §5). Multi-column
/// layout and its `break-*`, `widows` and `orphans` are rows (LLP 1093); these
/// are refused by what they would need, not as misspellings.
pub(crate) fn fragmentation(name: &str) -> Option<&'static str> {
    Some(match name {
        "column-span" => "is CSS Multi-column Layout's spanning element, which exact2 does not implement (only `column-span: none` exists, LLP 1093 §1); end the column flow and put the spanning content after it",
        "page-break-before" | "page-break-after" | "page-break-inside" => "controls where content breaks across printed pages, and exact2 does not print (LLP 1093 §5); in a multi-column flow `break-before`, `break-after` and `break-inside` take `column`, `avoid-column` and `avoid`",
        _ => return None,
    })
}

/// WAI-ARIA 1.2 and the proposed aria-actions (w3c/aria#1805): an unknown `aria-*` name is told
/// whether ARIA has it, and which of these Contract carries (`tags::attr`).
const ARIA: &[&str] = &[
    "aria-actions",
    "aria-activedescendant",
    "aria-atomic",
    "aria-autocomplete",
    "aria-braillelabel",
    "aria-brailleroledescription",
    "aria-busy",
    "aria-checked",
    "aria-colcount",
    "aria-colindex",
    "aria-colindextext",
    "aria-colspan",
    "aria-controls",
    "aria-current",
    "aria-describedby",
    "aria-description",
    "aria-details",
    "aria-disabled",
    "aria-dropeffect",
    "aria-errormessage",
    "aria-expanded",
    "aria-flowto",
    "aria-grabbed",
    "aria-haspopup",
    "aria-hidden",
    "aria-invalid",
    "aria-keyshortcuts",
    "aria-label",
    "aria-labelledby",
    "aria-level",
    "aria-live",
    "aria-modal",
    "aria-multiline",
    "aria-multiselectable",
    "aria-orientation",
    "aria-owns",
    "aria-placeholder",
    "aria-posinset",
    "aria-pressed",
    "aria-readonly",
    "aria-relevant",
    "aria-required",
    "aria-roledescription",
    "aria-rowcount",
    "aria-rowindex",
    "aria-rowindextext",
    "aria-rowspan",
    "aria-selected",
    "aria-setsize",
    "aria-sort",
    "aria-valuemax",
    "aria-valuemin",
    "aria-valuenow",
    "aria-valuetext",
];

/// The physical longhands for a CSS logical box property (r37 t1 wrote `padding-block`),
/// in a left-to-right, top-to-bottom flow.
fn logical(name: &str) -> Option<String> {
    let (base, rest) = ["padding", "margin", "inset"]
        .into_iter()
        .find_map(|b| name.strip_prefix(b).map(|r| (b, r)))?;
    let side = |s: &str| {
        if base == "inset" {
            format!("`{s}`")
        } else {
            format!("`{base}-{s}`")
        }
    };
    Some(match rest {
        "-block" => format!("{} and {}", side("top"), side("bottom")),
        "-inline" => format!(
            "{} and {} in a left-to-right flow",
            side("left"),
            side("right")
        ),
        "-block-start" => side("top"),
        "-block-end" => side("bottom"),
        "-inline-start" => format!("{} in a left-to-right flow", side("left")),
        "-inline-end" => format!("{} in a left-to-right flow", side("right")),
        _ => return None,
    })
}

/// What is written for a React Native style name that has no one CSS name.
pub(crate) fn react_native(name: &str) -> Option<&'static str> {
    Some(match name {
        "paddingHorizontal" => "; React Native's `paddingHorizontal` is `padding-left` and `padding-right` here, or `padding=\"<vertical> <horizontal>\"`",
        "paddingVertical" => "; React Native's `paddingVertical` is `padding-top` and `padding-bottom` here, or `padding=\"<vertical> <horizontal>\"`",
        "marginHorizontal" => "; React Native's `marginHorizontal` is `margin-left` and `margin-right` here, or `margin=\"<vertical> <horizontal>\"`",
        "marginVertical" => "; React Native's `marginVertical` is `margin-top` and `margin-bottom` here, or `margin=\"<vertical> <horizontal>\"`",
        "shadowColor" | "shadowOpacity" | "shadowRadius" | "shadowOffset" | "elevation" => {
            "; React Native's shadow is one `box-shadow=\"<x>px <y>px <2 × shadowRadius>px <shadowColor at shadowOpacity>\"` here"
        }
        "selectable" => "; selectable text is `user-select=\"text\"` here",
        _ => return None,
    })
}

/// The edit for HTML's `style=`: a node's own look is its attributes, and a
/// look nodes share is a `style` block named by `class=`. CSS declarations
/// (`"padding: 8px; color: red"`) are written back as the attributes, each
/// one the hint can say in Contract: a name that is an attribute identifier
/// (a custom property is not), a plain number unquoted unless the attribute
/// takes a shorthand string, `!important` dropped. A bare word is `class=`
/// unless it is reserved (`none`, `if`).
///
/// The rewrite is written only for distinct properties that cannot overlap:
/// CSS resolves a property written twice, or a shorthand beside its longhand,
/// by importance and order, and Contract's attributes do not, so such a set
/// gets the hint without a rewrite rather than one that means something else.
fn style_hint(value: &contract_syntax::Expr) -> String {
    let shared = "a look nodes share is a `style Name` block in the file, named with `class=Name`";
    let contract_syntax::Expr::Str(text, _) = value else {
        return format!("; a node's look is its attributes (`padding=8 color=\"-exact-secondary-label\"`), and {shared}");
    };
    // Each declaration: its name, and its value as a string literal's text
    // or a bare number.
    let decls: Vec<(&str, String)> = declarations(text)
        .into_iter()
        .filter_map(|d| {
            let (name, value) = d.split_once(':')?;
            let name = name.trim();
            if !attribute_name(name) {
                return None;
            }
            // A string literal of the value: CSS's runs of whitespace as one
            // space, which mean the same, and every other control character,
            // a quote and a backslash escaped.
            let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
            let value = match value.strip_suffix("important").map(str::trim_end) {
                Some(v) if v.ends_with('!') => v[..v.len() - 1].trim_end(),
                _ => value.as_str(),
            };
            let string_shorthand = matches!(tags::attr(name), Some(tags::AttrTarget::Shorthand))
                && !matches!(name, "column-count" | "column-rule-width")
                // Quoting an invalid count can let `columns` parse it as a width.
                && (name != "columns" || value.parse::<u16>().is_ok_and(|n| n > 0));
            // A number token begins with a digit (`.5` is not one).
            if !string_shorthand
                && value.starts_with(|c: char| c.is_ascii_digit())
                && value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                && value.parse::<f64>().is_ok()
            {
                return Some((name, value.to_string()));
            }
            let value: String = value
                .chars()
                .map(|c| match c {
                    '\\' | '"' => format!("\\{c}"),
                    c if c.is_control() => format!("\\u{{{:x}}}", c as u32),
                    c => c.to_string(),
                })
                .collect();
            Some((name, format!("\"{value}\"")))
        })
        .collect();
    let distinct = decls
        .iter()
        .enumerate()
        .all(|(i, a)| decls[i + 1..].iter().all(|b| !overlap(a.0, b.0)));
    let word = text.trim();
    if !decls.is_empty() && distinct {
        let attrs: Vec<String> = decls.iter().map(|(n, v)| format!("{n}={v}")).collect();
        format!(
            "; write the declarations as attributes, `{}`: a node's look is its attributes, and {shared}",
            attrs.join(" ")
        )
    } else if !text.contains(':') && attribute_name(word) && !contract_syntax::is_reserved(word) {
        format!("; write `class={word}`: {shared}, and a node's own look is its attributes")
    } else {
        format!("; {shared}, and a node's own look is its attributes")
    }
}

/// Whether two CSS properties may set the same thing: one name, one a prefix
/// of the other at a hyphen (`flex` and `flex-grow`, `border` and
/// `border-top-left-radius`), or a shorthand whose longhands are named
/// otherwise. Shared rows overlap unless the properties compose independent
/// parts. Name checks retain CSS relationships the rows do not represent,
/// such as `font`/`line-height`.
fn overlap(a: &str, b: &str) -> bool {
    let prefix = |short: &str, long: &str| {
        long.len() > short.len() && long.starts_with(short) && long.as_bytes()[short.len()] == b'-'
    };
    let family = |short: &str, long: &str| match short {
        "font" => matches!(long, "line-height"),
        "inset" => matches!(long, "top" | "right" | "bottom" | "left"),
        "gap" => matches!(long, "row-gap" | "column-gap"),
        "columns" => matches!(long, "column-width" | "column-count"),
        "flex-flow" => matches!(long, "flex-direction" | "flex-wrap"),
        "place-items" => matches!(long, "align-items" | "justify-items"),
        "place-content" => matches!(long, "align-content" | "justify-content"),
        "place-self" => matches!(long, "align-self" | "justify-self"),
        "grid-area" => long.starts_with("grid-row") || long.starts_with("grid-column"),
        "border-radius" => long.starts_with("border-") && long.ends_with("-radius"),
        _ => false,
    };
    let rows = |name: &str| -> &[StyleId] {
        match tags::attr(name) {
            Some(tags::AttrTarget::Styles(rows)) => rows,
            Some(tags::AttrTarget::Shorthand) => shorthands::rows(name),
            Some(tags::AttrTarget::Flex) => {
                &[StyleId::FlexGrow, StyleId::FlexShrink, StyleId::FlexBasis]
            }
            _ => &[],
        }
    };
    // Animation longhands set independent components of their shared row.
    let animation_parts = LONGHANDS.contains(&a) && LONGHANDS.contains(&b);
    a == b
        || prefix(a, b)
        || prefix(b, a)
        || family(a, b)
        || family(b, a)
        || rows(a)
            .iter()
            .any(|row| rows(b).contains(row) && (*row != StyleId::Animation || !animation_parts))
}

/// CSS declarations split at `;` outside quotes and parentheses
/// (`font-family: "A; B"`, `url(data:;base64,…)`), empty ones left out.
fn declarations(text: &str) -> Vec<&str> {
    let (mut out, mut from, mut depth, mut quote) = (Vec::new(), 0, 0usize, None::<char>);
    let mut chars = text.char_indices();
    while let Some((i, c)) = chars.next() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, ';') if depth == 0 => {
                out.push(&text[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[from..]);
    out.into_iter()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .collect()
}

/// Whether `name` is an attribute identifier as the lexer reads one
/// (contract-grammar.md, "Lexical rules"): a letter or `_` (`-exact-…` may
/// lead with one hyphen), then alphanumerics, `_`, or a hyphen before a
/// letter. A custom property (`--gap`) is not.
fn attribute_name(name: &str) -> bool {
    let body = name.strip_prefix('-').unwrap_or(name);
    let mut chars = body.chars().peekable();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return false;
    }
    while let Some(c) = chars.next() {
        match c {
            '-' if chars.peek().is_some_and(char::is_ascii_alphabetic) => {}
            c if c.is_ascii_alphanumeric() || c == '_' => {}
            _ => return false,
        }
    }
    true
}

pub(crate) fn active_migration_hint(name: &str, target: &str) -> String {
    let property = target.trim_start_matches(":active ");
    let migration = if matches!(name, "-exact-press-background-color" | "-exact-press-color") {
        "; `contract migrate <file.contract>` moves this attribute"
    } else {
        ""
    };
    format!("; write `{property}` in a `:active` block{migration}")
}

/// `lower-host-name-declared` (LLP 1123 D10): an `--exact-*` name is the host's, read with
/// `var()` where the host defines it, and declared nowhere, a class style included.
pub(crate) fn host_name_declared(a: &Attr) -> Option<LowerError> {
    use exact_kernel::style::{calc::host_name, safe_area_properties::HOST_PREFIX};
    if !a.name.starts_with(HOST_PREFIX) {
        return None;
    }
    let read = if host_name(&a.name) {
        format!("what the host reports, read with `var({})`", a.name)
    } else {
        "no name a host defines".to_string()
    };
    Some(LowerError {
        id: "lower-host-name-declared",
        message: format!(
            "`{}` is {read}: `--exact-*` names are the host's, and an app declares none. Declare a custom property under a name of your own, as `--gap=\"12px\"`, and read it with `var(--gap)`",
            a.name
        ),
        span: a.span,
    })
}

pub(crate) fn unknown_attr(tag: &str, a: &Attr) -> LowerError {
    if let Some(declared) = host_name_declared(a) {
        return declared;
    }
    if matches!(a.name.as_str(), "dragstart" | "drag" | "dragend") {
        return crate::transfer::handler_later(a);
    }
    let hint = match tags::renamed(&a.name) {
        None if crate::fields::part_hint(&a.name).is_some() => {
            crate::fields::part_hint(&a.name).unwrap_or_default()
        }
        None if crate::route_parts::hint(&a.name).is_some() => {
            crate::route_parts::hint(&a.name).unwrap_or_default()
        }
        Some(new @ ("press" | "change" | "input")) => format!(
            "; `{}` is `{new}` here: a handler is named for its event (LLP 1005 §3)",
            a.name
        ),
        // CSS's own shorthand, not a nickname: Contract takes the longhand (app farm wave 1, about twelve builds).
        Some(new @ "background-color") if a.name == "background" => {
            format!("; CSS's `background` shorthand is not taken here: a colour is `{new}` (LLP 1017 §8.1)")
        }
        Some(new) if new.starts_with(":active ") => active_migration_hint(&a.name, new),
        Some(new) => format!(
            "; `{}` is spelled `{new}` here, the web's name (LLP 1017 §8.1)",
            a.name
        ),
        None if fragmentation(&a.name).is_some() => {
            format!(
                ": `{}` {}",
                a.name,
                fragmentation(&a.name).unwrap_or_default()
            )
        }
        None if react_native(&a.name).is_some() => {
            react_native(&a.name).unwrap_or_default().into()
        }
        None if logical(&a.name).is_some() => format!(
            "; CSS's logical `{}` is not admitted: write {}",
            a.name,
            logical(&a.name).unwrap_or_default()
        ),
        None if tag == "dialog" && a.name == "open" => {
            ": in HTML, `open` alone shows a dialog non-modally, which Exact does not support. For a \
             modal, give the dialog an `id` and call `showModal(\"id\")` in an action, or put \
             `commandfor=\"id\" command=\"show-modal\"` on a button; close it with `close(\"id\")` \
             or `commandfor=\"id\" command=\"close\"`. A dialog that state holds open is not \
             available yet"
                .into()
        }
        None if a.name.starts_with("-exact-press-") => {
            "; a pressed look is paint only: write rows in `:active`; `-exact-press-opacity` and \
             `-exact-press-scale` multiply the presented value, and \
             `-exact-press-haptic` names the platform feedback"
                .into()
        }
        None if matches!(a.name.as_str(), "touchcancel" | "lostpointercapture") => {
            "; handle a cancelled contact with `pointercancel`".into()
        }
        // LLP 1055.000 §8 ruling 7: the end of a motion is not an event.
        None if matches!(
            a.name.as_str(),
            "animationstart" | "animationiteration" | "animationend" | "animationcancel" | "transitionend"
        ) =>
        {
            "; a motion tells no handler when it starts or ends: change state after one with a gated task's `after(ms, action)`, its length the one the motion declares (docs/recipes/timers.md)"
                .into()
        }
        None if css_hint(&a.name).is_some() => css_hint(&a.name).unwrap_or_default(),
        // LLP 1048.007 D5: the build chooses.
        None if tag == "image" && a.name == "loading" => {
            "; Exact chooses: an image a `for` repeats loads as its row comes into view, and a lone \
             image loads with the page"
                .into()
        }
        None if a.name == "className" => {
            "; `class` names a `style` declared in this file, as in `class=Card`".into()
        }
        // The euxy port wrote `style=` for a declared style.
        None if a.name == "style" => style_hint(&a.value),
        None if a.name.starts_with("aria-") => {
            let carried: Vec<&str> = ARIA
                .iter()
                .copied()
                .filter(|n| tags::attr(n).is_some())
                .collect();
            let what = if ARIA.contains(&a.name.as_str()) {
                "is ARIA's, and Contract does not carry it yet".to_string()
            } else {
                match tags::similar_attr(&a.name, false) {
                    Some(n) => format!("is not ARIA's (did you mean `{n}`?)"),
                    None => "is not ARIA's".to_string(),
                }
            };
            format!(
                "; `{}` {what}; Contract carries {}",
                a.name,
                carried.join(", ")
            )
        }
        None => tags::similar_attr(&a.name, false)
            .map(|n| format!("; did you mean `{n}`?"))
            .unwrap_or_default(),
    };
    LowerError {
        id: "lower-unknown-attr",
        message: format!("`{tag}` has no attribute `{}`{hint}", a.name),
        span: a.span,
    }
}

/// The names expansion replaces in a child's view: its props and injects,
/// by what its use and the `provide` above it pass, and its derives, by
/// their bodies. It replaces none of the root's: the plan reads those at
/// run time.
#[derive(Clone, Default)]
struct Substituted<'a> {
    values: Vec<&'a str>,
    derives: Vec<(&'a str, &'a Expr)>,
    /// Names that a `for` row or a `case some` binds here, which hide a
    /// prop, inject or derive of the same name.
    hidden: Vec<&'a str>,
    /// The names a `use` brings from a package: one may be a module's event,
    /// heard as an attribute on the root (LLP 1133.004 D6), which only the
    /// package's declaration, read when sources resolve, can say.
    imported: Vec<&'a str>,
}

impl<'a> Substituted<'a> {
    fn hiding(&self, names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut out = self.clone();
        out.hidden.extend(names);
        out
    }
}

/// A shorthand's arms as lowering judges them: each value it may be,
/// through `?:` and `match`, with what expansion puts in `names`' places
/// (a derive's body; no prop's value, which lowering judges). `font`'s type
/// tokens are judged across the arms, as lowering judges them
/// (`type_tokens`), with what a lint does not know left out: declared fonts
/// and the node's children.
fn shorthand(a: &Attr, names: &Substituted) -> Result<(), LowerError> {
    let mut budget = DERIVE_READS;
    let Some(value) = known_arms(&a.value, names, &mut budget) else {
        return Ok(());
    };
    let a = Attr { value, ..a.clone() };
    if a.name == "font"
        && crate::type_tokens::tokens_named(
            &a,
            exact_forma::Kind::Type,
            &[],
            &Default::default(),
            false,
        )?
        .is_some()
    {
        return Ok(());
    }
    shorthands::component(&a.value, &a.name, 0).map(|_| ())
}

/// A press offset's known derive arms, with local names retaining their scope.
pub(crate) fn press_translation_arms(a: &Attr, root: &Component, scope: &Scope) -> Attr {
    let names = Substituted {
        derives: root.derives.iter().enumerate().filter_map(|(i, d)| {
            matches!(scope.lookup(&d.name), Some((contract_types::Ref::Derive(j), _)) if j as usize == i)
                .then_some((d.name.as_str(), &d.expr))
        }).collect(),
        ..Substituted::default()
    };
    let mut budget = DERIVE_READS;
    let value = known_arms(&a.value, &names, &mut budget).unwrap_or_else(|| a.value.clone());
    Attr { value, ..a.clone() }
}

/// At most this many derive bodies read into one value: a derive read twice
/// in each link of a chain of derives doubles the value at every link.
const DERIVE_READS: usize = 64;

/// `v` with each arm that is a prop or inject taken out and each derive's
/// body in its place, read where the derive is declared; `None` when no arm
/// is left. A derive read inside its own body, or past `budget`, is taken
/// out as a prop is (a cycle is the type pass's refusal).
fn known_arms(v: &Expr, names: &Substituted, budget: &mut usize) -> Option<Expr> {
    match v {
        Expr::Ternary(cond, yes, no, span) => match (
            known_arms(yes, names, budget),
            known_arms(no, names, budget),
        ) {
            (Some(yes), Some(no)) => Some(Expr::Ternary(
                cond.clone(),
                Box::new(yes),
                Box::new(no),
                *span,
            )),
            (one, other) => one.or(other),
        },
        Expr::Match {
            subject,
            var,
            some,
            none,
            span,
        } => match (
            known_arms(some, &names.hiding([var.as_str()]), budget),
            known_arms(none, names, budget),
        ) {
            (Some(some), Some(none)) => Some(Expr::Match {
                subject: subject.clone(),
                var: var.clone(),
                some: Box::new(some),
                none: Box::new(none),
                span: *span,
            }),
            (one, other) => one.or(other),
        },
        Expr::Ident(name, _) if !names.hidden.contains(&name.as_str()) => {
            if names.values.contains(&name.as_str()) {
                return None;
            }
            match names.derives.iter().position(|(d, _)| d == name) {
                Some(_) if *budget == 0 => None,
                Some(i) => {
                    *budget -= 1;
                    let mut declared = Substituted {
                        hidden: Vec::new(),
                        ..names.clone()
                    };
                    let (derive, body) = declared.derives.remove(i);
                    declared.values.push(derive);
                    known_arms(body, &declared, budget)
                }
                None => Some(v.clone()),
            }
        }
        other => Some(other.clone()),
    }
}

/// What an authored element can be refused for without any types: its
/// tag, its attributes' names, and its literal style values against their
/// rows. The driver runs this when an earlier pass refused, so a misspelled
/// tag or a bad colour is reported in the same run as a type error.
pub fn lint(file: &File) -> Vec<LowerError> {
    lint_components(file, |_| true, &Default::default())
}

/// A bad literal value (`color="notacolor"`) in a component of the root's
/// file that nothing the root renders uses (LLP 1156.005 D3). Lowering walks
/// from the root, so without this it built clean and was refused only at the
/// component's first use, far from the edit.
pub fn lint_unused(file: &File, shapes: &contract_types::Shapes) -> Vec<LowerError> {
    let Some(root) = file.components.first() else {
        return Vec::new();
    };
    let mut reached = vec![false; file.components.len()];
    reached[0] = true;
    let mut todo = vec![0];
    while let Some(i) = todo.pop() {
        fn uses<'n>(nodes: &'n [Node], out: &mut Vec<&'n str>) {
            for n in nodes {
                match n {
                    Node::Use { name, children, .. } => {
                        out.push(name);
                        uses(children, out);
                    }
                    node => {
                        for list in node.child_lists() {
                            uses(list, out);
                        }
                    }
                }
            }
        }
        let mut names = Vec::new();
        uses(&file.components[i].view, &mut names);
        for name in names {
            for (j, c) in file.components.iter().enumerate() {
                if c.name == name && !reached[j] {
                    reached[j] = true;
                    todo.push(j);
                }
            }
        }
    }
    let own = root.span.source_id;
    let unused = |i: usize| !reached[i] && file.components[i].span.source_id == own;
    // The lint stands beside a type error; as the only verdict it may judge
    // only what needs no context: a literal value outside SVG (whose
    // attributes lowering renames). An attribute's name, a value read from a
    // prop, and a row whose meaning depends on its element are the caller's.
    fn literal(nodes: &[Node], out: &mut Vec<Span>) {
        for n in nodes {
            match n {
                Node::Element {
                    tag,
                    attrs,
                    children,
                    ..
                } if tag != "svg" => {
                    out.extend(attrs.iter().filter_map(|a| {
                        matches!(
                            a.value,
                            Expr::Str(..) | Expr::Number(..) | Expr::Template(..)
                        )
                        .then_some(a.span)
                    }));
                    literal(children, out);
                }
                Node::Element { .. } => {}
                node => {
                    for list in node.child_lists() {
                        literal(list, out);
                    }
                }
            }
        }
    }
    let mut spans = Vec::new();
    for (i, c) in file.components.iter().enumerate() {
        if unused(i) {
            literal(&c.view, &mut spans);
        }
    }
    if spans.is_empty() {
        return Vec::new();
    }
    let mut errors = lint_components(file, unused, shapes);
    errors.retain(|e| e.id == "lower-attr-value" && spans.contains(&e.span));
    errors
}

fn lint_components(
    file: &File,
    checked: impl Fn(usize) -> bool,
    shapes: &contract_types::Shapes,
) -> Vec<LowerError> {
    // A theme's names are colours (LLP 1095 D4); the lint asks only whether a
    // row's value is one, so each stands for any.
    crate::theme::bind_axes(file);
    let _theme = crate::theme::lint_scope(file);
    fn walk(
        nodes: &[Node],
        names: &Substituted,
        errors: &mut Vec<LowerError>,
        constants: &crate::keyframes::Context,
    ) {
        for n in nodes {
            match n {
                Node::Element {
                    tag,
                    attrs,
                    children,
                    span,
                    ..
                } => {
                    let mut attrs = std::borrow::Cow::Borrowed(attrs.as_slice());
                    values::fold_attr_templates(&mut attrs, &|e, string_only| {
                        constants.template_constant(e, string_only)
                    });
                    let attrs = attrs.as_ref();
                    if tags::tag(tag).is_none() && !native::is_module_tag(tag) {
                        errors.push(unknown_tag(tag, *span));
                    } else {
                        let coerced = svg::coerce_lengths(tag, false, attrs, None);
                        let attrs = coerced.as_deref().unwrap_or(attrs);
                        // Activity compilation consumes `slot` before lowering (LLP 1126.001 WP8).
                        // `delete=` is lowering's, a list row's swipe (`swipe_rows.rs`).
                        for a in attrs.iter().filter(|a| {
                            a.name != "class"
                                && a.name != "delete"
                                && a.name != "slot"
                                && a.name != crate::row_look::ROW_LOOK
                                && !native::leftover(tag, a)
                                && !native::declared(tag, a)
                                && !names.imported.contains(&a.name.as_str())
                        }) {
                            if let Some(e) = native::refused(tag, a) {
                                errors.push(e);
                                continue;
                            }
                            if dataset::word(&a.name).is_some() {
                                errors.extend(dataset::refused(a));
                                continue;
                            }
                            // A state part's row (`:hover opacity`, LLP 1121 D8): one
                            // its state takes, checked as the row the fold lowers it to.
                            let row;
                            let a = match contract_syntax::state::state_of(&a.name) {
                                Some((state, name)) => {
                                    let written = contract_syntax::Attr {
                                        name: name.to_string(),
                                        ..a.clone()
                                    };
                                    if let Some(e) = crate::states::refusal(state, &written, false)
                                    {
                                        errors.push(e);
                                        continue;
                                    }
                                    row = contract_syntax::Attr {
                                        name: crate::states::checked_as(state, name),
                                        ..written
                                    };
                                    &row
                                }
                                None => a,
                            };
                            let check = |a: &contract_syntax::Attr| match tags::attr_valued(
                                &a.name, &a.value,
                            ) {
                                None => Err(unknown_attr(tag, a)),
                                // A family is resolved against declared fonts.
                                Some(tags::AttrTarget::Styles(rows))
                                    if rows != [StyleId::FontFamily] =>
                                {
                                    values::check_style_value(
                                        a,
                                        rows,
                                        &Ty::Unknown,
                                        &[],
                                        &Default::default(),
                                        &Default::default(),
                                    )
                                }
                                // The shorthand's parts, as lowering checks them: `flex="none"`
                                // was refused here as a grow number beside another error
                                // (authoring bench: three builders).
                                Some(tags::AttrTarget::Flex) => {
                                    [StyleId::FlexGrow, StyleId::FlexShrink, StyleId::FlexBasis]
                                        .into_iter()
                                        .enumerate()
                                        .try_for_each(|(index, row)| {
                                            let value = values::flex_component(&a.value, index)?;
                                            let part = contract_syntax::Attr { value, ..a.clone() };
                                            values::check_style_value(
                                                &part,
                                                &[row],
                                                &Ty::Unknown,
                                                &[],
                                                &Default::default(),
                                                &Default::default(),
                                            )
                                        })
                                }
                                Some(tags::AttrTarget::Shorthand) => shorthand(a, names),
                                Some(_) => Ok(()),
                            };
                            // An expression over a token is folded by the
                            // build (a synthetic token, LLP 1121 D3), not here,
                            // in a choice's arm as in a whole value; the other
                            // arms are checked here.
                            let folded = |t: &str| {
                                t.contains('(')
                                    && crate::theme::words(t).any(exact_kernel::style::theme::names)
                            };
                            let mut leaves = Vec::new();
                            crate::theme::literal_leaves(&a.value, &mut leaves);
                            if leaves.iter().any(|(t, _)| folded(t)) {
                                for (t, span) in leaves.into_iter().filter(|(t, _)| !folded(t)) {
                                    let value = contract_syntax::Expr::Str(t, span);
                                    let leaf = contract_syntax::Attr { value, ..a.clone() };
                                    errors.extend(check(&leaf).err());
                                }
                            } else {
                                errors.extend(check(a).err());
                            }
                        }
                    }
                    walk(children, names, errors, constants);
                }
                node => {
                    for (list, binds) in node.scoped_child_lists() {
                        if binds.is_empty() {
                            walk(list, names, errors, constants);
                        } else {
                            walk(list, &names.hiding(binds.iter()), errors, constants);
                        }
                    }
                }
            }
        }
    }
    let mut errors = Vec::new();
    let imported: Vec<&str> = (file.uses.iter())
        .filter(|u| u.path.starts_with('@'))
        .flat_map(|u| u.names.iter().map(|n| n.local()))
        .collect();
    for (i, c) in file
        .components
        .iter()
        .enumerate()
        .filter(|(i, _)| checked(*i))
    {
        let names = match i {
            0 => Substituted {
                imported: imported.clone(),
                ..Substituted::default()
            },
            _ => Substituted {
                values: c
                    .props
                    .iter()
                    .chain(&c.injects)
                    .map(|p| p.name.as_str())
                    .collect(),
                derives: c
                    .derives
                    .iter()
                    .map(|d| (d.name.as_str(), &d.expr))
                    .collect(),
                hidden: Vec::new(),
                imported: imported.clone(),
            },
        };
        let scope = Scope::default();
        let constants = crate::keyframes::Context::new(&file.fns, &scope, shapes);
        walk(&c.view, &names, &mut errors, &constants);
    }
    // A component's anatomy, which no type is needed to check (LLP 1139 D2).
    for (_, c) in
        (file.components.iter().enumerate()).filter(|(i, c)| checked(*i) && !c.anatomy.is_empty())
    {
        errors.extend(crate::anatomy::check(c, file));
    }
    // A library package's host reads (LLP 1139 D6), said beside the type
    // error a child's resource is.
    if checked(0) {
        errors.extend(crate::anatomy::platform(file));
    }
    errors.truncate(MAX_REFUSALS);
    // A param by its full name, never its token's (LLP 1121 D15).
    let params = crate::params::tokens(file);
    for e in &mut errors {
        e.message = crate::params::shown(&e.message, &params);
    }
    errors
}

/// What to write for a CSS property Exact does not take, on an element or in a `style`.
pub(crate) fn css_hint(name: &str) -> Option<String> {
    match name {
        // CSS 2.1 §17.6.2's collapsing model is not implemented; the separated one with no spacing
        // is what a table drawn with rules between its rows needs.
        "border-collapse" => Some(
            "; tables use the separated model: `border-spacing=0` puts the cells edge to edge, and a \
             `border-bottom` on each cell draws a rule between rows"
                .into(),
        ),
        _ => None,
    }
}
