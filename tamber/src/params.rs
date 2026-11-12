//! Component params in the design (LLP 1121 D15, stage 7 B). A param is a
//! token its Contract component owns, named below the IR by its dotted token
//! name and above it by its full name (`post.contract#PostButton.radius`) or
//! short one (`PostButton.radius`). The app's value of one is a line of the
//! design's `params` block. The compiler, the design server's `plan`,
//! `resolve` and `describe` all fold a design the same way: each param as a
//! token of its kind, then each `params` line in place of its default.

use crate::{At, Design, Token};
use std::collections::{HashMap, HashSet};

/// A param as the design's folds take it: its token name, full name, kind
/// and default (another param named by its token name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// `param.f3f9a01c2.PostButton.radius`.
    pub token: String,
    /// `post.contract#PostButton.radius`.
    pub full: String,
    /// Its kind as its component writes it (`length`, `icon`).
    pub kind: String,
    /// Its default.
    pub default: String,
    /// Declared in the file being folded: read its current default from
    /// that design's component block instead of the loaded default.
    pub from_design: bool,
}

impl Param {
    /// `PostButton.radius`.
    pub fn short(&self) -> &str {
        self.full
            .rsplit_once('#')
            .map_or(self.full.as_str(), |(_, s)| s)
    }
}

/// Where a param's token is said to be written, past any design's own
/// lines (as the compiler numbers it).
pub const LINE: u32 = 1 << 29;

/// The app's params and every component it has (its identity, with params
/// or none): a short name is ambiguous where two components share a name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params {
    /// Every param.
    pub list: Vec<Param>,
    /// Every component's identity (`post.contract#PostButton`).
    pub components: Vec<String>,
    /// Tokens folded beside the design's own: its packages' (LLP 1121 D11),
    /// numbered as the compiler numbers them. One the design defines is the
    /// design's, whole.
    pub joined: Vec<Token>,
    /// Icons folded beside the design's own: its packages' (stage 13),
    /// numbered as the compiler numbers them. One the design defines is the
    /// design's, whole.
    pub joined_icons: Vec<Token>,
}

impl Params {
    /// No params and nothing joined: the design folds alone.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.joined.is_empty() && self.joined_icons.is_empty()
    }
}

/// No params: an app without them.
pub static NONE: Params = Params {
    list: Vec::new(),
    components: Vec::new(),
    joined: Vec::new(),
    joined_icons: Vec::new(),
};

/// Why a name names no single param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unnamed {
    /// No param of the app is called that.
    Unknown,
    /// A short name of a component name two components share, whichever of
    /// them has the param, or neither.
    Ambiguous,
}

/// A component's name in its identity.
fn component(identity: &str) -> &str {
    identity.rsplit_once('#').map_or(identity, |(_, c)| c)
}

/// The identities of the components called `name`, each once.
pub fn called<'a>(params: &'a Params, name: &str) -> Vec<&'a str> {
    let mut out: Vec<&str> = params
        .list
        .iter()
        .filter_map(|p| p.full.rsplit_once('.').map(|(identity, _)| identity))
        .chain(params.components.iter().map(String::as_str))
        .filter(|identity| component(identity) == name)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// [`named`] for many names at once: the params by full name and by short
/// name (each the first), and how many components share each component
/// name, made once (the checker's work in param counts: a scan per name
/// was a square, a scan per param a cube). Owned, so a caller can keep it
/// beside the [`Params`] it indexes.
#[derive(Debug, Clone, Default)]
pub struct Index {
    full: HashMap<String, usize>,
    short: HashMap<String, usize>,
    sharing: HashMap<String, usize>,
}

impl Index {
    /// The index of `params`.
    pub fn new(params: &Params) -> Self {
        let (mut full, mut short) = (HashMap::new(), HashMap::new());
        for (i, p) in params.list.iter().enumerate() {
            full.entry(p.full.clone()).or_insert(i);
            short.entry(p.short().to_string()).or_insert(i);
        }
        let identities: std::collections::BTreeSet<&str> = params
            .list
            .iter()
            .filter_map(|p| p.full.rsplit_once('.').map(|(identity, _)| identity))
            .chain(params.components.iter().map(String::as_str))
            .collect();
        let mut sharing = HashMap::new();
        for identity in identities {
            *sharing.entry(component(identity).to_string()).or_insert(0) += 1;
        }
        Index {
            full,
            short,
            sharing,
        }
    }

    /// As [`named`] answers, among the `params` this indexes.
    pub fn named<'p>(&self, params: &'p Params, name: &str) -> Result<&'p Param, Unnamed> {
        if let Some(&i) = self.full.get(name) {
            return Ok(&params.list[i]);
        }
        let Some(&i) = self.short.get(name) else {
            return Err(Unnamed::Unknown);
        };
        let p = &params.list[i];
        let owner = p.short().rsplit_once('.').map_or("", |(c, _)| c);
        match self.sharing.get(owner) {
            Some(1) => Ok(p),
            _ => Err(Unnamed::Ambiguous),
        }
    }
}

/// The param `name` names: its full name, or its short one where one
/// component of the app has that component name.
pub fn named<'a>(params: &'a Params, name: &str) -> Result<&'a Param, Unnamed> {
    if let Some(p) = params.list.iter().find(|p| p.full == name) {
        return Ok(p);
    }
    let Some(p) = params.list.iter().find(|p| p.short() == name) else {
        return Err(Unnamed::Unknown);
    };
    let owner = p.short().rsplit_once('.').map_or("", |(c, _)| c);
    match called(params, owner).len() {
        1 => Ok(p),
        _ => Err(Unnamed::Ambiguous),
    }
}

fn scoped(value: &str, prefix: &str) -> String {
    crate::lex::rewrite_names(value, |name| {
        name.strip_prefix("param.")
            .filter(|name| crate::lex::is_ident(name))
            .map(|name| format!("{prefix}.{name}"))
    })
}

/// Add authored component defaults as tokens, independently of overrides.
/// These token names need only distinguish components within this file.
pub(crate) fn defaults(mut design: Design) -> Design {
    for c in &design.components {
        let prefix = format!("param.{}", c.name.value);
        for p in &c.params {
            let mut name = p.name.clone();
            name.value = format!("{prefix}.{}", p.name.value);
            let mut value = p.value.clone();
            value.value = scoped(&value.value, &prefix);
            let icon = p.kind == "icon";
            let token = Token {
                name,
                kind: (!icon).then(|| p.kind.clone()),
                value,
                whens: Vec::new(),
                meta: Vec::new(),
            };
            match icon {
                true => design.icons.push(token),
                false => design.tokens.push(token),
            }
        }
    }
    design
}

/// `design` with each param a token of its kind (an icon param among the
/// icons) on line [`LINE`] + its index, and the first line of its `params`
/// block that names it in place of its default and `when` lines (at that
/// line, its name too). A param declared in the app's design reads its
/// current default there. Then each icon whose value names another icon (a
/// param's or the design's) takes that one's sources, the icons block
/// folding only literals. A line naming none, ambiguously, or a param
/// another line already named, is left out: the compiler says why. The
/// compiler, the design server's `plan`, `resolve` and `describe` all lay a
/// design out this way.
pub fn laid(design: &Design, params: &Params) -> Design {
    let mut out = design.clone();
    let own: HashSet<&str> = design
        .tokens
        .iter()
        .map(|t| t.name.value.as_str())
        .collect();
    out.tokens.extend(
        params
            .joined
            .iter()
            .filter(|j| !own.contains(j.name.value.as_str()))
            .cloned(),
    );
    let own: HashSet<&str> = design.icons.iter().map(|t| t.name.value.as_str()).collect();
    out.icons.extend(
        params
            .joined_icons
            .iter()
            .filter(|j| !own.contains(j.name.value.as_str()))
            .cloned(),
    );
    // Each param's first `params` line, by its token, found in one pass.
    let index = Index::new(params);
    let defaults: HashMap<String, &At<String>> = design
        .components
        .iter()
        .flat_map(|c| {
            c.params
                .iter()
                .map(|p| (format!("{}.{}", c.name.value, p.name.value), &p.value))
        })
        .collect();
    let mut first: HashMap<&str, &Token> = HashMap::new();
    for e in &design.params {
        if let Ok(q) = index.named(params, &e.name.value) {
            first.entry(q.token.as_str()).or_insert(e);
        }
    }
    for (i, p) in params.list.iter().enumerate() {
        let at = |value: &str| At {
            value: value.to_string(),
            line: LINE + i as u32,
            col: 1,
        };
        let icon = p.kind == "icon";
        let mut token = Token {
            name: at(&p.token),
            kind: (!icon).then(|| p.kind.clone()),
            value: at(&p.default),
            whens: Vec::new(),
            meta: Vec::new(),
        };
        if let Some(default) = p.from_design.then(|| defaults.get(p.short())).flatten() {
            let prefix = p.token.rsplit_once('.').unwrap().0;
            token.value.value = scoped(&default.value, prefix);
        }
        if let Some(entry) = first.get(p.token.as_str()) {
            token.name.line = entry.name.line;
            token.name.col = entry.name.col;
            token.value = entry.value.clone();
            token.whens = entry.whens.clone();
        }
        match icon {
            true => out.icons.push(token),
            false => out.tokens.push(token),
        }
    }
    crate::icon::aliased(&mut out.icons);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(full: &str, default: &str) -> Param {
        Param {
            token: {
                let (file, short) = full.rsplit_once('#').unwrap();
                format!("param.f{}.{short}", file.split('.').next().unwrap())
            },
            full: full.into(),
            kind: "length".into(),
            default: default.into(),
            from_design: false,
        }
    }

    #[test]
    fn a_params_line_takes_its_params_place() {
        let params = Params {
            list: vec![
                param("a.contract#Card.inset", "4px"),
                param("b.contract#Card.inset", "6px"),
                param("a.contract#Row.gap", "2px"),
                param("a.contract#Bar.gap", "2px"),
            ],
            components: vec!["a.contract#Row".into(), "c.contract#Bar".into()],
            joined: Vec::new(),
            joined_icons: Vec::new(),
        };
        assert_eq!(named(&params, "Card.inset"), Err(Unnamed::Ambiguous));
        assert_eq!(
            named(&params, "b.contract#Card.inset").map(|p| &p.default[..]),
            Ok("6px")
        );
        assert_eq!(named(&params, "Row.gap").map(|p| &p.default[..]), Ok("2px"));
        assert_eq!(named(&params, "Row.nope"), Err(Unnamed::Unknown));
        // A component without params shares the name: ambiguous.
        assert_eq!(named(&params, "Bar.gap"), Err(Unnamed::Ambiguous));
        let design =
            crate::parse("tamber t\n\nparams\n  Row.gap = 9px\n  Card.inset = 1px\n").unwrap();
        let laid = laid(&design, &params);
        let value = |n: &str| {
            laid.tokens
                .iter()
                .find(|t| t.name.value.ends_with(n))
                .map(|t| t.value.value.clone())
        };
        assert_eq!(value("Row.gap").as_deref(), Some("9px"));
        assert_eq!(
            value("Card.inset").as_deref(),
            Some("4px"),
            "an ambiguous line sets nothing"
        );
        assert!(crate::check_design(laid, &crate::Vocab::default()).is_ok());
    }

    #[test]
    fn current_design_defaults_keep_their_scope_and_other_owners_defaults() {
        let mut inset = param("app.tamber#Card.inset", "1px");
        inset.from_design = true;
        let mut edge = param("app.tamber#Card.edge", "2px");
        edge.from_design = true;
        let params = Params {
            list: vec![
                inset,
                edge,
                param("card.contract#Card.inset", "3px"),
                param("pkg/theme.tamber#Card.inset", "4px"),
            ],
            ..Default::default()
        };
        let design = crate::parse(
            "tamber t\n\ncomponent Card\n  params\n    inset: length = 8px\n    edge: length = calc(param.inset * 2)\n",
        )
        .unwrap();
        let laid = laid(&design, &params);
        let folded = crate::check_design(laid, &crate::Vocab::default()).unwrap();
        let values: Vec<_> = folded.tokens.iter().map(|t| t.values[0].css()).collect();
        assert_eq!(values, ["8px", "16px", "3px", "4px"]);
    }
}
