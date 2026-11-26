//! LLP 1021 (Menus): HTML's `hr`, a menu's separator row, and the shape a
//! confirmation must have to present natively — refused here where it can
//! be seen, rather than at the tap on the iOS host (`MenusIOS.swift`,
//! `openConfirmation`, which logs `confirmation <id> refused: <why>`). And
//! what its shape implies of its outside (LLP 1115.000 D13.4): an alert has
//! none.

use crate::{err, LowerError, Lowerer};
use contract_syntax::{Attr, BinOp, Component, Expr, Node, Param, Span, Stmt, TemplatePart, UnOp};
use exact_kernel::StyleId;
use std::collections::{BTreeSet, HashSet};

/// LLP 1021 §5.3 and LLP 1133.025 D4, D5: a button's place in the Mac's
/// menu bar, as one menu id; `dock` beside it or alone files it in the Dock
/// menu too; `status` alone presents it as the app's status item, which
/// takes the button's place and so joins no menu.
pub(crate) fn check_menu_placement(tag: &str, attr: &Attr) -> Result<(), LowerError> {
    const MENUS: &[&str] = &[
        "about", "app", "file", "edit", "view", "go", "window", "help", "none",
    ];
    let ids: Vec<&str> = match &attr.value {
        Expr::Str(value, _) if tag == "button" => value.split_whitespace().collect(),
        _ => Vec::new(),
    };
    let known = |id: &&str| MENUS.contains(id) || matches!(*id, "status" | "dock");
    if ids.is_empty() || !ids.iter().all(known) {
        return err(
            "lower-menu-placement",
            "`-exact-menu` belongs to a button and takes a literal about, app, file, edit, view, go, window, help or none, `dock` beside one of them or alone, or `status` alone",
            attr.span,
        );
    }
    if ids.contains(&"status") && ids.len() > 1 {
        return err(
            "lower-menu-status",
            "`-exact-menu=\"status\"` stands alone: the status item takes the button's place, so it joins no menu (LLP 1133.025 D4)",
            attr.span,
        );
    }
    let menus = ids.iter().filter(|id| MENUS.contains(id)).count();
    let docks = ids.iter().filter(|id| **id == "dock").count();
    if menus > 1 || docks > 1 {
        return err(
            "lower-menu-placement",
            "`-exact-menu` names one menu, and `dock` at most once beside it",
            attr.span,
        );
    }
    Ok(())
}

/// LLP 1021 §5.3: a root's bounded opt-out from the Mac's standard items.
/// AppKit's Edit ▸ AutoFill is not one: nothing public identifies it, and the
/// host names no private identifier (LLP 1035.003).
pub(crate) fn check_menu_remove(attrs: &[Attr], root: bool) -> Result<(), LowerError> {
    const IDS: &[&str] = &[
        "close-all",
        "show-tab-bar",
        "show-all-tabs",
        "enter-full-screen",
        "minimize",
        "zoom",
        "bring-all-to-front",
        "show-previous-tab",
        "show-next-tab",
        "move-tab-to-new-window",
        "merge-all-windows",
        "back",
        "forward",
        "develop",
        "paste-and-match-style",
        "find",
        "spelling-and-grammar",
        "substitutions",
        "transformations",
        "app-help",
        "actual-size",
        "zoom-in",
        "zoom-out",
        // The host's Quit at the end of the status item's menu (LLP 1133.025 D7).
        "status-quit",
    ];
    for attr in attrs.iter().filter(|a| a.name == "-exact-menu-remove") {
        if !root
            || !matches!(&attr.value, Expr::Str(value, _) if value.split_whitespace().all(|id| IDS.contains(&id)))
        {
            return err("lower-menu-remove", format!(
                "`-exact-menu-remove` belongs to a root and takes a literal whitespace-separated list from: {}",
                IDS.join(", ")
            ), attr.span);
        }
    }
    Ok(())
}

/// LLP 1021 §5.3.3: a root's chord for File ▸ Close Window, the one host item
/// that takes a declared chord, `close-window=chord` (one `aria-keyshortcuts`
/// chord with `Meta`, not one of the host's or the system's standard chords)
/// or `close-window=none`.
pub(crate) fn check_menu_keys(attrs: &[Attr], root: bool) -> Result<(), LowerError> {
    const IDS: &[&str] = &["close-window"];
    for attr in attrs.iter().filter(|a| a.name == "-exact-menu-keys") {
        let pairs: Option<Vec<(&str, &str)>> = match &attr.value {
            Expr::Str(value, _) if root => value
                .split_whitespace()
                .map(|pair| pair.split_once('='))
                .collect(),
            _ => None,
        };
        let valid = pairs.is_some_and(|pairs| {
            pairs.iter().enumerate().all(|(i, (id, chord))| {
                IDS.contains(id)
                    && (*chord == "none" || menu_chord(chord).is_some_and(|c| !standard_chord(&c)))
                    && !pairs[..i].iter().any(|(earlier, _)| earlier == id)
            })
        });
        if !valid {
            return err("lower-menu-keys", format!(
                "`-exact-menu-keys` belongs to a root and takes a literal whitespace-separated list of id=chord, each id once, \
                 the chord `none` or one `aria-keyshortcuts` chord with Meta or Mod (`Mod+Shift+W`) that is not one of the host's or \
                 the system's own (⌘Q, ⌘W, ⌘H, ⌘M, ⌘-comma and the like); Mod cannot be combined with Meta or Control; the ids: {}",
                IDS.join(", ")
            ), attr.span);
        }
    }
    Ok(())
}

/// A chord as the Mac host's menu holds it: its modifiers, in the order
/// Meta, Control, Alt, Shift, and its key, lowercased.
type MenuChord = (Vec<&'static str>, String);

/// One chord as `aria-keyshortcuts` spells it, with `Meta`, parsed as the
/// Mac host's shortcuts parse it (`ShortcutsMac.swift`, `Shortcut`): one
/// character, a named key or `F1` to `F35`, after any of `Meta`, `Control`,
/// `Alt` and `Shift`, which may repeat, as the host lets them.
fn menu_chord(chord: &str) -> Option<MenuChord> {
    let chord = exact_plan::shortcut::resolve(chord, "Meta")?;
    let (held, key) = match chord.strip_suffix("++") {
        Some(held) => (held, "Plus"),
        None => chord.rsplit_once('+')?,
    };
    const NAMED: &[&str] = &[
        "Enter",
        "Tab",
        "Escape",
        "Space",
        "Backspace",
        "Delete",
        "Insert",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Plus",
    ];
    let function = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=35).contains(&n) && key == format!("F{n}"));
    if !(key.chars().count() == 1 || NAMED.contains(&key) || function) {
        return None;
    }
    let mut modifiers = Vec::new();
    for name in ["Meta", "Control", "Alt", "Shift"] {
        if held.split('+').any(|m| m == name) {
            modifiers.push(name);
        }
    }
    let all_known = held
        .split('+')
        .all(|m| matches!(m, "Meta" | "Control" | "Alt" | "Shift"));
    if !all_known || !modifiers.contains(&"Meta") {
        return None;
    }
    let key = if key == "+" {
        "plus".to_owned()
    } else {
        key.to_lowercase()
    };
    Some((modifiers, key))
}

/// Whether `chord` is one the host's menu bar or macOS itself holds: the
/// app menu's, File's, Edit's (with Find and Spelling), View's, Go's,
/// Window's, Help's and Develop's (`DevMenuMac.swift`), AppKit's own Close
/// All and Minimize All, and the system's (app switching, Spotlight,
/// screenshots, Force Quit, Lock Screen, Log Out, the Dock, Emoji &
/// Symbols). A chord with Shift is also the one that types its shifted
/// character on a US layout, as AppKit matches a menu item by the typed
/// character: ⇧⌘; is ⌘:, and ⇧⌘= is ⌘+.
fn standard_chord(chord: &MenuChord) -> bool {
    const STANDARD: &[&str] = &[
        "Meta+q",
        "Meta+h",
        "Meta+Alt+h",
        "Meta+n",
        "Meta+o",
        "Meta+Shift+o",
        "Meta+w",
        "Meta+Alt+w",
        "Meta+m",
        "Meta+Alt+m",
        "Meta+,",
        "Meta+z",
        "Meta+Shift+z",
        "Meta+x",
        "Meta+c",
        "Meta+v",
        "Meta+Alt+Shift+v",
        "Meta+a",
        "Meta+f",
        "Meta+Alt+f",
        "Meta+g",
        "Meta+Shift+g",
        "Meta+e",
        "Meta+j",
        "Meta+:",
        "Meta+;",
        "Meta+0",
        "Meta+plus",
        "Meta+=",
        "Meta+-",
        "Meta+Control+s",
        "Meta+Control+f",
        "Meta+[",
        "Meta+?",
        "Meta+Shift+/",
        "Meta+r",
        "Meta+d",
        "Meta+Alt+t",
        "Meta+tab",
        "Meta+Shift+tab",
        "Meta+`",
        "Meta+space",
        "Meta+Alt+space",
        "Meta+Shift+3",
        "Meta+Shift+4",
        "Meta+Shift+5",
        "Meta+Alt+escape",
        "Meta+Control+q",
        "Meta+Control+space",
        "Meta+Shift+q",
        "Meta+Alt+Shift+q",
        "Meta+Shift+`",
        "Meta+Control+Shift+3",
        "Meta+Control+Shift+4",
        "Meta+Alt+d",
    ];
    const SHIFTED: &[(&str, &str)] = &[
        ("1", "!"),
        ("2", "@"),
        ("3", "#"),
        ("4", "$"),
        ("5", "%"),
        ("6", "^"),
        ("7", "&"),
        ("8", "*"),
        ("9", "("),
        ("0", ")"),
        ("-", "_"),
        ("=", "plus"),
        ("[", "{"),
        ("]", "}"),
        ("\\", "|"),
        (";", ":"),
        ("'", "\""),
        (",", "<"),
        (".", ">"),
        ("/", "?"),
        ("`", "~"),
    ];
    let (modifiers, key) = chord;
    let spelled = |modifiers: &[&str], key: &str| {
        STANDARD.contains(&format!("{}+{key}", modifiers.join("+")).as_str())
    };
    if spelled(modifiers, key) {
        return true;
    }
    let unshifted: Vec<&str> = modifiers
        .iter()
        .copied()
        .filter(|m| *m != "Shift")
        .collect();
    unshifted.len() < modifiers.len()
        && SHIFTED
            .iter()
            .any(|(base, shifted)| key == base && spelled(&unshifted, shifted))
}

/// HTML honours `popovertarget` and `commandfor` on a button only (HTML
/// §6.12, "the popover target attributes"; "the commandfor attribute"): on
/// any other node no host opens anything, so it is refused where it is
/// written.
pub(crate) fn check_invoker(tag: &str, attrs: &[Attr]) -> Result<(), LowerError> {
    if tag == "button" {
        return Ok(());
    }
    let Some(a) = attrs
        .iter()
        .find(|a| matches!(a.name.as_str(), "popovertarget" | "commandfor"))
    else {
        return Ok(());
    };
    err(
        "lower-invoker-tag",
        format!(
            "`{}` opens nothing on a `{tag}`: HTML honours it on a button only. Make the invoker a \
             `button` (`button appearance=\"none\"` keeps a composite's own look: its children \
             and layout, with the button's press, focus and role)",
            a.name
        ),
        a.span,
    )
}

/// HTML's `hr`, as the platform draws a separator (LLP 1115 §3, wave 1; LLP
/// 1115.000 D10.3): one hairline, a single solid top edge one device pixel
/// thick (`-exact-hairline`) in the separator role (`-exact-separator`:
/// `separatorColor` on Apple), with no margin of its own, as `Divider()` and
/// a table's separator have none and the common resets zero an `hr`'s. In
/// place of the UA stylesheet's `margin: 0.5em auto` and its two-toned
/// `inset` 1px box, which the web's sheet resets (`host/web/index.html`);
/// its `overflow: hidden` stays. The edge is `currentcolor`, so an author's
/// `color`, `border-*` or `margin` still wins (LLP 1021 D1).
pub(crate) const HR: &[(StyleId, &str)] = &[
    (StyleId::BorderStyleTop, "solid"),
    (StyleId::BorderWidthTop, "-exact-hairline"),
    (StyleId::TextColor, "-exact-separator"),
    (StyleId::OverflowX, "hidden"),
    (StyleId::OverflowY, "hidden"),
];

/// Whether an element's `role` attribute can evaluate to `"alertdialog"`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlertRole {
    Never,
    Maybe,
    Always,
}

/// What [`Lowerer::check_menu_shapes`] established about an element: whether
/// its role is unconditionally `"alertdialog"`, and whether its rows have the
/// alert (rather than chooser) shape.
#[derive(Clone, Copy, Default)]
pub(crate) struct AlertShape {
    pub(crate) always_alertdialog: bool,
    pub(crate) alert: bool,
}

const MAX_ROLE_VALUES: usize = 32;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RoleVal {
    Bool(bool),
    Str(String),
    None,
    /// `some(v)`, where `None` means the wrapped payload is unknown.
    Some(Option<Box<RoleVal>>),
}

#[derive(Clone, Default)]
struct ValSet {
    values: BTreeSet<RoleVal>,
    unknown: bool,
}

impl ValSet {
    fn unknown() -> Self {
        Self {
            values: BTreeSet::new(),
            unknown: true,
        }
    }

    fn one(value: RoleVal) -> Self {
        let mut values = BTreeSet::new();
        values.insert(value);
        Self {
            values,
            unknown: false,
        }
    }

    fn add(&mut self, value: RoleVal) {
        if self.values.contains(&value) {
            return;
        }
        if self.values.len() < MAX_ROLE_VALUES {
            self.values.insert(value);
        } else {
            self.unknown = true;
        }
    }

    fn union_with(&mut self, other: Self) {
        self.unknown |= other.unknown;
        for value in other.values {
            self.add(value);
        }
    }

    fn may_be_true(&self) -> bool {
        self.unknown || self.values.contains(&RoleVal::Bool(true))
    }

    fn may_be_false(&self) -> bool {
        self.unknown || self.values.contains(&RoleVal::Bool(false))
    }

    fn may_be_some(&self) -> bool {
        self.unknown || self.values.iter().any(|v| matches!(v, RoleVal::Some(_)))
    }

    fn may_be_none(&self) -> bool {
        self.unknown || self.values.contains(&RoleVal::None)
    }

    fn wrap_some(self) -> Self {
        let mut out = Self::default();
        for value in self.values {
            out.add(RoleVal::Some(Some(Box::new(value))));
        }
        if self.unknown {
            out.add(RoleVal::Some(None));
        }
        out
    }

    fn unwrap_some(&self) -> Self {
        let mut out = Self {
            values: BTreeSet::new(),
            unknown: self.unknown,
        };
        for value in &self.values {
            match value {
                RoleVal::Some(Some(inner)) => out.add((**inner).clone()),
                RoleVal::Some(None) => out.unknown = true,
                _ => {}
            }
        }
        out
    }

    fn to_alert_role(&self) -> AlertRole {
        let has_alert = self
            .values
            .iter()
            .any(|v| matches!(v, RoleVal::Str(s) if s == "alertdialog"));
        if !has_alert {
            AlertRole::Never
        } else if !self.unknown && self.values.len() == 1 {
            AlertRole::Always
        } else {
            AlertRole::Maybe
        }
    }
}

fn role_val_eq(a: &RoleVal, b: &RoleVal) -> Option<bool> {
    match (a, b) {
        (RoleVal::Bool(x), RoleVal::Bool(y)) => Some(x == y),
        (RoleVal::Str(x), RoleVal::Str(y)) => Some(x == y),
        (RoleVal::None, RoleVal::None) => Some(true),
        (RoleVal::Some(_), RoleVal::None) | (RoleVal::None, RoleVal::Some(_)) => Some(false),
        (RoleVal::Some(Some(x)), RoleVal::Some(Some(y))) => role_val_eq(x, y),
        _ => None,
    }
}

fn template_parts_may_match_alertdialog(parts: &[(Vec<String>, bool)]) -> bool {
    const TARGET: &str = "alertdialog";
    let mut reachable: u16 = 1;
    for (known, is_unknown) in parts {
        let mut next: u16 = 0;
        for pos in 0..=TARGET.len() {
            if reachable & (1 << pos) == 0 {
                continue;
            }
            for s in known {
                if TARGET[pos..].starts_with(s.as_str()) {
                    next |= 1 << (pos + s.len());
                }
            }
            if *is_unknown {
                for end in pos..=TARGET.len() {
                    next |= 1 << end;
                }
            }
        }
        reachable = next;
        if reachable == 0 {
            return false;
        }
    }
    reachable & (1 << TARGET.len()) != 0
}

fn join_state_assigns<'a>(
    stmts: &'a [Stmt],
    target_name: &str,
    env: &mut Vec<(&'a str, ValSet)>,
    acc: &mut ValSet,
    depth: usize,
    seen: &mut HashSet<(&'static str, &'a str)>,
    root: &'a Component,
    lookup_fn: &dyn Fn(&str) -> Option<(&'a [Param], &'a Expr)>,
) {
    let base = env.len();
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, expr, .. } => {
                let value = eval_values(expr, env, depth + 1, seen, root, lookup_fn);
                env.push((name.as_str(), value));
            }
            Stmt::Assign { target, expr, .. } if target == target_name => {
                acc.union_with(eval_values(expr, env, depth + 1, seen, root, lookup_fn));
            }
            Stmt::If {
                cond,
                then,
                otherwise,
                ..
            } => {
                let cv = eval_values(cond, env, depth + 1, seen, root, lookup_fn);
                if cv.may_be_true() {
                    join_state_assigns(then, target_name, env, acc, depth, seen, root, lookup_fn);
                }
                if cv.may_be_false() {
                    join_state_assigns(
                        otherwise,
                        target_name,
                        env,
                        acc,
                        depth,
                        seen,
                        root,
                        lookup_fn,
                    );
                }
            }
            Stmt::Match {
                subject,
                some,
                none,
                ..
            } => {
                let sv = eval_values(subject, env, depth + 1, seen, root, lookup_fn);
                if sv.may_be_some() {
                    let saved = env.len();
                    env.push((some.0.as_str(), sv.unwrap_some()));
                    join_state_assigns(
                        &some.1,
                        target_name,
                        env,
                        acc,
                        depth,
                        seen,
                        root,
                        lookup_fn,
                    );
                    env.truncate(saved);
                }
                if sv.may_be_none() {
                    join_state_assigns(none, target_name, env, acc, depth, seen, root, lookup_fn);
                }
            }
            Stmt::Call { body, .. } => {
                join_state_assigns(body, target_name, env, acc, depth, seen, root, lookup_fn);
            }
            _ => {}
        }
    }
    env.truncate(base);
}

fn eval_values<'a>(
    expr: &'a Expr,
    env: &[(&'a str, ValSet)],
    depth: usize,
    seen: &mut HashSet<(&'static str, &'a str)>,
    root: &'a Component,
    lookup_fn: &dyn Fn(&str) -> Option<(&'a [Param], &'a Expr)>,
) -> ValSet {
    if depth > 32 {
        return ValSet::unknown();
    }
    match expr {
        Expr::Bool(b, _) => ValSet::one(RoleVal::Bool(*b)),
        Expr::Str(s, _) => ValSet::one(RoleVal::Str(s.clone())),
        Expr::None(_) => ValSet::one(RoleVal::None),
        Expr::Some(inner, _) => {
            eval_values(inner, env, depth + 1, seen, root, lookup_fn).wrap_some()
        }
        Expr::Typed(inner, _, _) => eval_values(inner, env, depth + 1, seen, root, lookup_fn),
        Expr::Template(parts, _) => {
            let mut single_expr: Option<&Expr> = None;
            let mut expr_count = 0usize;
            let mut has_text = false;
            for part in parts {
                match part {
                    TemplatePart::Text(t) if !t.is_empty() => has_text = true,
                    TemplatePart::Text(_) => {}
                    TemplatePart::Expr(e) => {
                        expr_count += 1;
                        single_expr = Some(e);
                    }
                }
            }
            if expr_count == 1 && !has_text {
                return eval_values(single_expr.unwrap(), env, depth + 1, seen, root, lookup_fn);
            }
            let mut evaluated: Vec<(Vec<String>, bool)> = Vec::with_capacity(parts.len());
            let mut any_unknown = false;
            let mut has_constraint = has_text;
            for part in parts {
                match part {
                    TemplatePart::Text(t) => evaluated.push((vec![t.clone()], false)),
                    TemplatePart::Expr(e) => {
                        let vs = eval_values(e, env, depth + 1, seen, root, lookup_fn);
                        let mut strs = Vec::new();
                        let mut part_unknown = vs.unknown;
                        for value in vs.values {
                            match value {
                                RoleVal::Str(s) => {
                                    if !s.is_empty() {
                                        has_constraint = true;
                                    }
                                    strs.push(s);
                                }
                                RoleVal::Bool(b) => {
                                    has_constraint = true;
                                    strs.push(if b { "true" } else { "false" }.into());
                                }
                                _ => part_unknown = true,
                            }
                        }
                        any_unknown |= part_unknown;
                        evaluated.push((strs, part_unknown));
                    }
                }
            }
            let mut acc = vec![String::new()];
            let mut overflow = false;
            let all_have_known = evaluated.iter().all(|(strs, _)| !strs.is_empty());
            if all_have_known {
                'outer: for (strs, _) in &evaluated {
                    let mut next = Vec::new();
                    for prefix in &acc {
                        for suffix in strs {
                            if next.len() >= MAX_ROLE_VALUES {
                                overflow = true;
                                break 'outer;
                            }
                            let mut combined = String::with_capacity(prefix.len() + suffix.len());
                            combined.push_str(prefix);
                            combined.push_str(suffix);
                            next.push(combined);
                        }
                    }
                    acc = next;
                }
            } else {
                acc.clear();
            }
            let mut out = ValSet {
                values: BTreeSet::new(),
                unknown: any_unknown || overflow,
            };
            if !overflow {
                for s in acc {
                    out.add(RoleVal::Str(s));
                }
            }
            if out.unknown
                && has_constraint
                && !out
                    .values
                    .iter()
                    .any(|v| matches!(v, RoleVal::Str(s) if s == "alertdialog"))
                && template_parts_may_match_alertdialog(&evaluated)
            {
                out.add(RoleVal::Str("alertdialog".into()));
            }
            out
        }
        Expr::Let {
            name, value, body, ..
        } => {
            let val = eval_values(value, env, depth + 1, seen, root, lookup_fn);
            let mut next = env.to_vec();
            next.push((name.as_str(), val));
            eval_values(body, &next, depth + 1, seen, root, lookup_fn)
        }
        Expr::Unary(UnOp::Not, inner, _) => {
            let vs = eval_values(inner, env, depth + 1, seen, root, lookup_fn);
            let mut out = ValSet {
                values: BTreeSet::new(),
                unknown: vs.unknown,
            };
            for value in vs.values {
                match value {
                    RoleVal::Bool(b) => out.add(RoleVal::Bool(!b)),
                    _ => out.unknown = true,
                }
            }
            out
        }
        Expr::Binary(op, l, r, _) => match op {
            BinOp::And => {
                let lv = eval_values(l, env, depth + 1, seen, root, lookup_fn);
                if !lv.may_be_true() {
                    return ValSet::one(RoleVal::Bool(false));
                }
                let rv = eval_values(r, env, depth + 1, seen, root, lookup_fn);
                let mut out = ValSet::default();
                if lv.may_be_false() || rv.may_be_false() {
                    out.add(RoleVal::Bool(false));
                }
                if lv.may_be_true() && rv.may_be_true() {
                    out.add(RoleVal::Bool(true));
                }
                out
            }
            BinOp::Or => {
                let lv = eval_values(l, env, depth + 1, seen, root, lookup_fn);
                if !lv.may_be_false() {
                    return ValSet::one(RoleVal::Bool(true));
                }
                let rv = eval_values(r, env, depth + 1, seen, root, lookup_fn);
                let mut out = ValSet::default();
                if lv.may_be_true() || rv.may_be_true() {
                    out.add(RoleVal::Bool(true));
                }
                if lv.may_be_false() && rv.may_be_false() {
                    out.add(RoleVal::Bool(false));
                }
                out
            }
            BinOp::Eq | BinOp::Ne => {
                let lv = eval_values(l, env, depth + 1, seen, root, lookup_fn);
                let rv = eval_values(r, env, depth + 1, seen, root, lookup_fn);
                if lv.unknown || rv.unknown {
                    return ValSet::unknown();
                }
                let mut out = ValSet::default();
                for a in &lv.values {
                    for b in &rv.values {
                        match role_val_eq(a, b) {
                            Some(eq) => {
                                out.add(RoleVal::Bool(if *op == BinOp::Eq { eq } else { !eq }));
                            }
                            None => out.unknown = true,
                        }
                    }
                }
                out
            }
            _ => ValSet::unknown(),
        },
        Expr::Ternary(cond, yes, no, _) => {
            let cv = eval_values(cond, env, depth + 1, seen, root, lookup_fn);
            let mut out = ValSet::default();
            if cv.may_be_true() {
                out.union_with(eval_values(yes, env, depth + 1, seen, root, lookup_fn));
            }
            if cv.may_be_false() {
                out.union_with(eval_values(no, env, depth + 1, seen, root, lookup_fn));
            }
            out
        }
        Expr::Match {
            subject,
            var,
            some,
            none,
            ..
        } => {
            let sv = eval_values(subject, env, depth + 1, seen, root, lookup_fn);
            let mut out = ValSet::default();
            if sv.may_be_some() {
                let mut next = env.to_vec();
                next.push((var.as_str(), sv.unwrap_some()));
                out.union_with(eval_values(some, &next, depth + 1, seen, root, lookup_fn));
            }
            if sv.may_be_none() {
                out.union_with(eval_values(none, env, depth + 1, seen, root, lookup_fn));
            }
            out
        }
        Expr::Ident(name, _) => {
            if let Some((_, value)) = env.iter().rev().find(|(n, _)| *n == name.as_str()) {
                return value.clone();
            }
            if !seen.insert(("id", name.as_str())) {
                return ValSet::unknown();
            }
            let out = if let Some(d) = root.derives.iter().find(|d| d.name == *name) {
                eval_values(&d.expr, &[], depth + 1, seen, root, lookup_fn)
            } else if let Some(s) = root.states.iter().find(|s| s.name == *name) {
                let mut acc = eval_values(&s.expr, &[], depth + 1, seen, root, lookup_fn);
                if root.persistence(name).is_some() {
                    acc.unknown = true;
                }
                for action in &root.actions {
                    let mut action_env: Vec<(&'a str, ValSet)> = action
                        .params
                        .iter()
                        .map(|p| (p.name.as_str(), ValSet::unknown()))
                        .collect();
                    join_state_assigns(
                        &action.body,
                        name,
                        &mut action_env,
                        &mut acc,
                        depth,
                        seen,
                        root,
                        lookup_fn,
                    );
                }
                // A non-string `state` (bool/option) is mutable state rather than a folded prop
                // constant, so conditional role branches on it remain reachable even before an
                // action mutates it.
                if !acc.values.iter().all(|v| matches!(v, RoleVal::Str(_))) {
                    acc.unknown = true;
                }
                acc
            } else {
                ValSet::unknown()
            };
            seen.remove(&("id", name.as_str()));
            out
        }
        Expr::Call(name, args, _) => {
            let Some((params, body)) = lookup_fn(name) else {
                return ValSet::unknown();
            };
            if params.len() != args.len() {
                return ValSet::unknown();
            }
            let arg_vals: Vec<ValSet> = args
                .iter()
                .map(|a| eval_values(a, env, depth + 1, seen, root, lookup_fn))
                .collect();
            if !seen.insert(("fn", name.as_str())) {
                return ValSet::unknown();
            }
            let fn_env: Vec<(&'a str, ValSet)> = params
                .iter()
                .zip(arg_vals)
                .map(|(p, v)| (p.name.as_str(), v))
                .collect();
            let out = eval_values(body, &fn_env, depth + 1, seen, root, lookup_fn);
            seen.remove(&("fn", name.as_str()));
            out
        }
        _ => ValSet::unknown(),
    }
}

/// Whether `attrs`'s `role` can evaluate to `"alertdialog"`, resolving literal
/// branches, enclosing view bindings, component prop substitutions, `derive`s,
/// `state` initializers and reachable assignments, and pure `fn` calls.
pub(crate) fn alertdialog_role<'a>(
    attrs: &'a [Attr],
    root: &'a Component,
    view_bindings: &[(&'a str, Option<&'a Expr>)],
    lookup_fn: &dyn Fn(&str) -> Option<(&'a [Param], &'a Expr)>,
) -> AlertRole {
    let Some(role) = attrs.iter().rev().find(|a| a.name == "role") else {
        return AlertRole::Never;
    };
    let mut seen = HashSet::new();
    let mut env: Vec<(&'a str, ValSet)> = Vec::with_capacity(view_bindings.len());
    for &(name, subject) in view_bindings {
        let value = match subject {
            Some(expr) => eval_values(expr, &env, 0, &mut seen, root, lookup_fn).unwrap_some(),
            None => ValSet::unknown(),
        };
        env.push((name, value));
    }
    eval_values(&role.value, &env, 0, &mut seen, root, lookup_fn).to_alert_role()
}

/// A `dialog` whose role is `alertdialog` is presented as the platform's
/// confirmation, and its `closedby`, unless written, is what that
/// confirmation does (LLP 1115 D6; LLP 1115.000 D13.4), so every host and
/// the web's `dialog` behave alike. `shape` is whether its role is always
/// `alertdialog` and whether its rows are an alert's
/// ([`Lowerer::check_menu_shapes`]). **An alert has no outside:**
/// `closerequest`, so Escape and Back cancel it and a tap outside does
/// nothing, as `UIAlertController(.alert)`, `NSAlert` and Material's dialog
/// do. Any other shape is the platform's chooser (an action sheet, a menu),
/// which a tap outside dismisses: `any`. The attributes with it added, or
/// `None` when nothing is implied.
pub(crate) fn implied_closedby(
    tag: &str,
    attrs: &[Attr],
    span: Span,
    shape: AlertShape,
) -> Option<Vec<Attr>> {
    if tag != "dialog" || !shape.always_alertdialog || has(attrs, "closedby") {
        return None;
    }
    let mut attrs = attrs.to_vec();
    attrs.push(Attr {
        name: "closedby".into(),
        value: Expr::Str(
            if shape.alert { "closerequest" } else { "any" }.into(),
            span,
        ),
        span,
        written: crate::IMPLIED,
    });
    Some(attrs)
}

/// The most of `rows` that can show at once: rows on different arms of one
/// `if` or `match` never show together, so a choice counts its fullest
/// arm. `depth` is how far along each row's path of arms this looks.
fn most(rows: &[&Row], depth: usize) -> usize {
    let mut here = 0;
    let mut choices: std::collections::BTreeMap<
        usize,
        std::collections::BTreeMap<usize, Vec<&Row>>,
    > = Default::default();
    for row in rows {
        match row.arms.get(depth) {
            None => here += 1,
            Some(&(choice, arm)) => choices
                .entry(choice)
                .or_default()
                .entry(arm)
                .or_default()
                .push(row),
        }
    }
    let fullest = |arms: &std::collections::BTreeMap<usize, Vec<&Row>>| {
        arms.values().map(|rows| most(rows, depth + 1)).max()
    };
    here + choices.values().filter_map(fullest).sum::<usize>()
}

/// Whether a confirmation's rows, as written, are an alert's: the shape
/// rule every host presents by (`exact_kernel::confirmation::is_alert`),
/// read off the source. It counts the most rows that can show at once: a
/// text on any arm is its text, and a button under `for` can be any
/// number of buttons, so it is a chooser's. Where an `if` leaves a state
/// that is not an alert's, the host that meets it presents what it finds.
fn alert_shaped(rows: &[Row]) -> bool {
    let of = |keep: &dyn Fn(&Row) -> bool| rows.iter().filter(|r| keep(r)).collect::<Vec<_>>();
    // A prompt's text field is what it asks, as explanatory text is.
    let texts = of(&|r| r.tag == "text" || text_field(r));
    let actions = of(&|r| r.tag == "button" && has(&r.attrs, "press"));
    let cancels = of(&|r| r.tag == "button" && !has(&r.attrs, "press"));
    !actions.iter().chain(&cancels).any(|r| r.repeats)
        && exact_kernel::confirmation::is_alert(texts.len(), most(&actions, 0), most(&cancels, 0))
}

/// HTML's one-line text fields, what a prompt asks with
/// (`UIAlertController.addTextField`, `NSAlert.accessoryView`): an `input`
/// whose `type` edits a line of text. A `type` known only at run time is
/// the host's to read.
fn text_field(row: &Row) -> bool {
    const TYPES: &[&str] = &[
        "text", "password", "email", "url", "search", "tel", "number",
    ];
    row.tag == "input" && literal(&row.attrs, "type").is_none_or(|t| TYPES.contains(&t))
}

fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    match &attrs.iter().rev().find(|a| a.name == name)?.value {
        Expr::Str(v, _) => Some(v),
        _ => None,
    }
}

fn has(attrs: &[Attr], name: &str) -> bool {
    attrs.iter().any(|a| a.name == name)
}

/// A confirmation's direct rows, as its host reads them: `for`, `if`
/// and `match` are not elements, so the rows they produce are its rows.
/// `repeats` is whether a row is inside a `for`; `arms` is the `if` and
/// `match` arms it is on, outermost first, each as the choice node's address
/// and the arm's index. Addresses, not spans: a component used on two arms
/// is inlined as two nodes with one span.
struct Row<'a> {
    tag: &'a str,
    attrs: Vec<Attr>,
    span: Span,
    repeats: bool,
    arms: Vec<(usize, usize)>,
}

impl Row<'_> {
    /// Whether `self` and `other` are on different arms of one `if` or
    /// `match`, so never both present.
    fn exclusive(&self, other: &Row) -> bool {
        for (a, b) in self.arms.iter().zip(&other.arms) {
            if a.0 != b.0 {
                return false;
            }
            if a.1 != b.1 {
                return true;
            }
        }
        false
    }
}

impl Lowerer<'_> {
    pub(crate) fn under_view_binding<T>(
        &mut self,
        name: &str,
        subject: Option<&Expr>,
        lower: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.view_bindings.push((name.to_owned(), subject.cloned()));
        let out = lower(self);
        self.view_bindings.pop();
        out
    }

    /// `hr` is void; a popover or `dialog` whose role is or can evaluate to
    /// `alertdialog` is a confirmation, which an Apple host presents as the
    /// platform's sheet. Its rows are text, text fields when it is an alert (a
    /// prompt), actions (a `button` with `press` that hides it) and at most one
    /// cancel (a `button` without `press` that hides it), with at least one
    /// action unless it is an alert with no field: text and its one button, the
    /// platform's one-button alert ("Couldn't save." and OK). Any expression
    /// whose reachable values include `"alertdialog"` is checked here; an
    /// opaque runtime value is left to the host. Returns whether its role is
    /// unconditionally `alertdialog` and whether its rows are an alert's
    /// ([`alert_shaped`]), which decides the `closedby` a `dialog` implies and
    /// may take.
    pub(crate) fn check_menu_shapes(
        &self,
        tag: &str,
        attrs: &[Attr],
        children: &[Node],
        span: Span,
    ) -> Result<AlertShape, LowerError> {
        if tag == "hr" {
            if let Some(child) = children.first() {
                return err(
                    "lower-void",
                    "`hr` is a void element, HTML's separator: it takes no children",
                    child.span(),
                );
            }
        }
        let dialog = tag == "dialog";
        let view_bindings: Vec<(&str, Option<&Expr>)> = self
            .view_bindings
            .iter()
            .map(|(n, e)| (n.as_str(), e.as_ref()))
            .collect();
        let role = alertdialog_role(attrs, self.root, &view_bindings, &|name| {
            self.fns
                .get(name)
                .map(|(decl, body)| (decl.params.as_slice(), *body))
        });
        if role == AlertRole::Never || !(dialog || has(attrs, "popover")) {
            return Ok(AlertShape::default());
        }
        let always_alertdialog = role == AlertRole::Always;
        let mut rows = Vec::new();
        self.rows(children, false, &mut Vec::new(), &mut rows);
        let alert = alert_shaped(&rows);
        // A prompt asks one thing (rules/DEFERRED.md), whatever its `id`:
        // fields on arms of one `if` or `match` never show together.
        let fields: Vec<&Row> = rows.iter().filter(|r| text_field(r)).collect();
        if let Some(field) = fields.iter().find(|f| f.repeats) {
            return err(
                "lower-alertdialog",
                "alertdialog has a text field inside `for`, which can repeat, and a prompt has one",
                field.span,
            );
        }
        if most(&fields, 0) > 1 {
            let second = fields
                .iter()
                .enumerate()
                .find(|(i, f)| fields[..*i].iter().any(|e| !e.exclusive(f)))
                .map_or(fields[1], |(_, f)| *f);
            return err(
                "lower-alertdialog",
                "alertdialog has a second text field; a prompt asks one thing (one `input`, or one on each arm of an `if` or `match`)",
                second.span,
            );
        }
        let Some(id) = literal(attrs, "id") else {
            return Ok(AlertShape {
                always_alertdialog,
                alert,
            });
        };
        // An unwritten `closedby` is implied (`implied_closedby`); one written
        // otherwise asks for what the native confirmation of this shape
        // cannot do (LLP 1115.000 D13.4). An alert has no outside and always
        // has its cancel: `closerequest`, or `any` for the platform's
        // dismissible form. A chooser is dismissed by a tap outside: `any`.
        let closedby = attrs.iter().rev().find(|a| a.name == "closedby");
        if let Some(Expr::Str(value, _)) = closedby.filter(|_| dialog).map(|c| &c.value) {
            let refused = match (alert, value.as_str()) {
                (_, "any") | (true, "closerequest") => None,
                (true, "none") => Some("an alert always has its cancel, which Escape and Back press, so it is `closedby=\"closerequest\"` (leave it unwritten: a tap outside does nothing), or `closedby=\"any\"` for the platform's dismissible form, an action sheet or a light-dismiss dialog"),
                (true, _) => Some("an alert's is `closerequest` (leave it unwritten: Escape and Back cancel it, a tap outside does nothing) or `any`, the platform's dismissible form"),
                (false, _) => Some("with no explanatory text, no cancel, more than three buttons or a button under `for` it is the platform's chooser (an action sheet, a menu), which a tap outside dismisses, so it is `closedby=\"any\"` (leave it unwritten)"),
            };
            if let Some(why) = refused {
                return err(
                    "lower-alertdialog",
                    format!(
                        "alertdialog `dialog` `{id}` cannot take `closedby=\"{value}\"`: {why}"
                    ),
                    span,
                );
            }
        }
        // How it hides this one: the popover's `popovertarget` with
        // `popovertargetaction="hide"`, the dialog's `commandfor` with
        // `command="close"`. `None` when a value is not a literal.
        let (target, action, hide) = if dialog {
            ("commandfor", "command", "close")
        } else {
            ("popovertarget", "popovertargetaction", "hide")
        };
        let hides = |attrs: &[Attr]| -> Option<bool> {
            let names = |name| attrs.iter().any(|a| a.name == name);
            if !names(target) {
                return Some(false);
            }
            let to = literal(attrs, target)?;
            let how = if names(action) {
                literal(attrs, action)?
            } else {
                "toggle"
            };
            Some(to == id && how == hide)
        };
        let how = format!("`{target}=\"{id}\" {action}=\"{hide}\"`");
        let refuse = |row: &Row, why: String| {
            err(
                "lower-alertdialog",
                format!("alertdialog `{id}` cannot be presented as a native confirmation: {why}. Its rows are text, text fields (`input`), actions (a `button` with `press` and {how}) and at most one cancel (a `button` without `press`, with {how})"),
                row.span,
            )
        };
        let mut actions = 0;
        let mut cancels: Vec<&Row> = Vec::new();
        for row in &rows {
            match row.tag {
                "text" => {}
                // A prompt (a rename's): the alert with a text field, whose
                // edits are the field's `input` as typing in place is.
                "input" if text_field(row) => {}
                // The hosts present buttons only; a `link` is refused as
                // any other element row is.
                "button" if has(&row.attrs, "press") => {
                    if hides(&row.attrs) == Some(false) {
                        return refuse(row, format!("this action's `press` does not also hide it ({how})"));
                    }
                    actions += 1;
                }
                "button" => match hides(&row.attrs) {
                    Some(false) => {
                        return refuse(row, "this `button` has no `press` and does not hide it, so it is neither an action nor the cancel".into())
                    }
                    _ if row.repeats => {
                        return refuse(row, "a cancel inside `for` can repeat, and there is at most one".into())
                    }
                    _ => cancels.push(row),
                },
                // An author laying a modal out (onboarding's Delete account?
                // in a `column` of rows) wants a sheet: say so.
                other => return refuse(row, format!("a `{other}` row is not text, an action or the cancel; a modal you lay out yourself is a sheet, a route with `navigationPresentation=\"modal\"` (docs/recipes/navigation-stack.md)")),
            }
        }
        for (i, later) in cancels.iter().enumerate() {
            if cancels[..i].iter().any(|earlier| !earlier.exclusive(later)) {
                return refuse(later, "a second cancel; there is at most one".into());
            }
        }
        if let Some(field) = fields.first().filter(|_| !alert) {
            return refuse(field, "a text field belongs to the alert's shape, a prompt: with it one cancel and at most three buttons in all, none under `for`".into());
        }
        // A chooser with nothing to choose is no presentation, and a
        // prompt's answer goes to an action; an alert's one button (text
        // and its cancel) is the platform's own.
        if actions == 0 && (!alert || !fields.is_empty()) {
            return err(
                "lower-alertdialog",
                format!(
                    "alertdialog `{id}` has no action: give it a `button` with `press` and {how}, or make it an alert that tells, with explanatory `text` and its one closing `button`"
                ),
                span,
            );
        }
        Ok(AlertShape {
            always_alertdialog,
            alert,
        })
    }

    /// The element rows `nodes` produce, through `for`, `if` and `match`;
    /// `arms` is the path of arms `nodes` are on.
    fn rows<'n>(
        &self,
        nodes: &'n [Node],
        repeats: bool,
        arms: &mut Vec<(usize, usize)>,
        out: &mut Vec<Row<'n>>,
    ) {
        for node in nodes {
            let choice = std::ptr::from_ref(node) as usize;
            let branches: Vec<&'n [Node]> = match node {
                Node::Element {
                    tag, attrs, span, ..
                } => {
                    let mut all = self
                        .class_rows(attrs)
                        .ok()
                        .flatten()
                        .map_or_else(Vec::new, |(_, rows)| rows);
                    all.extend(attrs.iter().cloned());
                    out.push(Row {
                        tag,
                        attrs: all,
                        span: *span,
                        repeats,
                        arms: arms.clone(),
                    });
                    continue;
                }
                Node::When {
                    then, otherwise, ..
                } => vec![then, otherwise],
                Node::Match { some, none, .. } => vec![&some.1, none],
                Node::Each { body, .. } => {
                    self.rows(body, true, arms, out);
                    continue;
                }
                // A component's use is inlined before lowering; a slot's
                // fill is its caller's to check.
                Node::Use { .. } | Node::Children { .. } => continue,
            };
            for (arm, branch) in branches.into_iter().enumerate() {
                arms.push((choice, arm));
                self.rows(branch, repeats, arms, out);
                arms.pop();
            }
        }
    }
}
