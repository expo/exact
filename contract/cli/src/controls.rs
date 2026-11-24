//! Controls (LLP 1126.004): each `controls` row of `app.json` checked
//! against the plan and stored on it with what its tool can do.
//!
//! A tool's effects are walked from its action through the calls the plan
//! holds inline, every branch, the `then` of every mutation it sends, the
//! resources it refreshes or its sends re-ask, and the actions of the tasks
//! and sequences whose gate or key reads a slot it assigns, directly or
//! through a derive, transitively (D3). A send is classified by what it resolves to: the
//! reserved ActivityKit sends by operation, and a package name by its
//! package and declared name, so an aliased `use` is still seen: every
//! name a package grants the microphone, camera or screen to, except
//! `applyConstraints` and `takePhoto`, which only use a track already open. Speech
//! recognition of a `track` subscribes to it, which can reopen an idle one,
//! so it counts as a restart. A media argument the walk cannot read as a
//! literal, or a `let` of one, counts as possibly set.
//! `faces::tool_signature`'s `sends` bit says only whether there is a send.

use crate::manifest::{Control, ControlAction};
use contract_syntax::{Component, Expr, Stmt};
use exact_plan::{Plan, ToolsId};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::collections::{BTreeSet, HashSet};

/// How surely a capture kind is in a tool's reach. Ordered: a reach is
/// the strongest of its sends'.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
enum Level {
    #[default]
    None,
    /// An argument the walk cannot read may ask for it.
    Possible,
    /// A literal asks for it.
    Known,
}

impl Level {
    fn word(self) -> &'static str {
        match self {
            Level::None => "none",
            Level::Possible => "possible",
            Level::Known => "known",
        }
    }
}

#[derive(Default)]
struct Reach {
    /// `start`, `update`, `end`.
    activity: BTreeSet<&'static str>,
    /// The literal kinds a `startActivity` names.
    kinds: BTreeSet<String>,
    /// A `startActivity` whose kind is not a literal.
    dynamic_kind: bool,
    audio: Level,
    /// `startSpeechRecognition` on the microphone, which no Apple intent
    /// conformance qualifies.
    speech: Level,
    camera: Level,
    display: Level,
    /// `startMediaRecorder`: records a stream whose tracks only the run
    /// knows.
    recorder: bool,
    /// `resumeMediaRecorder` or `setTrackEnabled`: may restart capture.
    restart: bool,
}

/// The effects of the action named `action` in the expanded root, as the
/// plan stores them on a control and the bake derives conformances from:
/// `{activity, activityKinds, dynamicKind, audio, speech, camera, display,
/// recorder, restart}`, each capture kind `none`, `possible` or `known`.
pub fn effects(root: &Component, plan: &Plan, action: &str) -> Value {
    let mut reach = Reach::default();
    let mut seen = HashSet::new();
    let mut next = vec![action.to_owned()];
    let mut walker = Walker {
        root,
        plan,
        lets: HashMap::new(),
        assigned: HashSet::new(),
    };
    loop {
        while let Some(name) = next.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(a) = root.actions.iter().find(|a| a.name == name) {
                walker.lets.clear();
                walker.walk(&a.body, &mut reach, &mut next);
            }
        }
        next.extend(walker.armed().into_iter().filter(|a| !seen.contains(a)));
        if next.is_empty() {
            break;
        }
    }
    json!({
        "activity": reach.activity,
        "activityKinds": reach.kinds,
        "dynamicKind": reach.dynamic_kind,
        "audio": reach.audio.word(),
        "speech": reach.speech.word(),
        "camera": reach.camera.word(),
        "display": reach.display.word(),
        "recorder": reach.recorder,
        "restart": reach.restart,
    })
}

/// Whether `effects` can start or restart a capture: a discoverable face's
/// call is refused it (LLP 1133.009 D9), and only a control's press may
/// start audio (D3).
pub fn captures(effects: &Value) -> bool {
    ["audio", "speech", "camera", "display"]
        .iter()
        .any(|k| effects[k] != "none")
        || effects["recorder"] == true
        || effects["restart"] == true
}

struct Walker<'a> {
    root: &'a Component,
    plan: &'a Plan,
    /// The `let`s of the action walked, an inlined call's parameters among
    /// them, so a literal bound to a name still reads as one.
    lets: HashMap<String, Expr>,
    /// The slots the walk assigns.
    assigned: HashSet<String>,
}

impl Walker<'_> {
    fn walk(&mut self, stmts: &[Stmt], reach: &mut Reach, next: &mut Vec<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Send {
                    target,
                    source,
                    args,
                    ..
                } => {
                    if let Some(m) = self.root.mutations.iter().find(|m| &m.name == target) {
                        if let Some((action, _)) = &m.then {
                            next.push(action.clone());
                        }
                        for (resource, _) in &m.refreshes {
                            self.refresh(resource, reach);
                        }
                    }
                    self.classify(source, args, reach);
                }
                Stmt::Command { name, args, .. } => self.classify(name, args, reach),
                Stmt::Refresh { target, .. } => self.refresh(target, reach),
                Stmt::If {
                    then, otherwise, ..
                } => {
                    self.walk(then, reach, next);
                    self.walk(otherwise, reach, next);
                }
                Stmt::Match { some, none, .. } => {
                    self.walk(&some.1, reach, next);
                    self.walk(none, reach, next);
                }
                Stmt::Call { body, .. } => self.walk(body, reach, next),
                Stmt::Let { name, expr, .. } => {
                    self.lets.insert(name.clone(), expr.clone());
                }
                Stmt::Assign { target, .. } => {
                    let slot = target.split(['.', '[']).next().unwrap_or(target);
                    self.assigned.insert(slot.to_owned());
                }
            }
        }
    }

    /// A resource re-asked: its source, with the arguments it holds.
    fn refresh(&self, name: &str, reach: &mut Reach) {
        if let Some(r) = self.root.resources.iter().find(|r| r.name == name) {
            self.classify(&r.source, &r.args, reach);
        }
    }

    /// The actions of the tasks and sequences whose gate or key reads a
    /// slot the walk assigned, or a derive that does.
    fn armed(&self) -> Vec<String> {
        let mut names: HashSet<&str> = self.assigned.iter().map(String::as_str).collect();
        loop {
            let before = names.len();
            for d in &self.root.derives {
                if reads(&d.expr, &names) {
                    names.insert(&d.name);
                }
            }
            if names.len() == before {
                break;
            }
        }
        let gated = |gate: &Option<Expr>, key: &Option<Expr>| {
            [gate, key].into_iter().flatten().any(|e| reads(e, &names))
        };
        let tasks = (self.root.tasks.iter())
            .filter(|t| gated(&t.gate, &t.key))
            .map(|t| t.timer.1.clone());
        let sequences = (self.root.sequences.iter())
            .filter(|q| gated(&q.gate, &q.key))
            .filter_map(|q| q.ended.as_ref().map(|(a, _)| a.clone()));
        tasks.chain(sequences).collect()
    }

    /// `value` with a name bound by a `let` read as what it binds.
    fn resolved<'e>(&'e self, value: Option<&'e Expr>) -> Option<&'e Expr> {
        let mut value = value;
        for _ in 0..8 {
            match value {
                Some(Expr::Ident(name, _)) if self.lets.contains_key(name) => {
                    value = self.lets.get(name);
                }
                _ => break,
            }
        }
        value
    }
}

/// A string literal's text, or a template's without interpolation.
fn literal(value: &Expr) -> Option<String> {
    match value {
        Expr::Str(text, _) => Some(text.clone()),
        Expr::Template(parts, _) => parts
            .iter()
            .map(|p| match p {
                contract_syntax::TemplatePart::Text(t) => Some(t.as_str()),
                contract_syntax::TemplatePart::Expr(_) => None,
            })
            .collect(),
        _ => None,
    }
}

/// Whether `expr` reads one of `names`.
fn reads(expr: &Expr, names: &HashSet<&str>) -> bool {
    match expr {
        Expr::Ident(name, _) => names.contains(name.as_str()),
        _ => expr.operands().any(|e| reads(e, names)),
    }
}

impl Walker<'_> {
    fn classify(&self, source: &str, args: &[Expr], reach: &mut Reach) {
        let plan = self.plan;
        match source {
            "startActivity" => {
                reach.activity.insert("start");
                match self.resolved(args.first()).and_then(literal) {
                    Some(kind) => {
                        reach.kinds.insert(kind);
                    }
                    None => reach.dynamic_kind = true,
                }
                return;
            }
            "updateActivity" => {
                reach.activity.insert("update");
                return;
            }
            "endActivity" => {
                reach.activity.insert("end");
                return;
            }
            _ => {}
        }
        let Some(module) = plan.module_named(source) else {
            return;
        };
        // A package name's options reach the plan as one record of them
        // (`modules.rs`); one the walk cannot open may ask for anything.
        let (fields, opaque) = match args {
            [Expr::Call(_, fields, _)] => (fields.as_slice(), false),
            [] => (args, false),
            _ => (args, !args.iter().all(|a| matches!(a, Expr::NamedArg(..)))),
        };
        let unread = Expr::Ident(String::new(), contract_syntax::Span::default());
        let named = |key: &str| {
            let given = fields.iter().find_map(|a| match a {
                Expr::NamedArg(n, value, _) if n == key => Some(value.as_ref()),
                _ => None,
            });
            self.resolved(given).or(opaque.then_some(&unread)).cloned()
        };
        let flag = |v: Option<Expr>| self.flag(v.as_ref());
        let constraints = |v: Option<Expr>| self.constraints(v.as_ref());
        match (plan.str(module.package), plan.str(module.declared)) {
            ("@exact/media-capture", "getUserMedia") => {
                let audio = flag(named("audio")).max(constraints(named("audioConstraints")));
                let camera = flag(named("video")).max(constraints(named("videoConstraints")));
                reach.audio = reach.audio.max(audio);
                reach.camera = reach.camera.max(camera);
            }
            ("@exact/media-capture", "setTrackEnabled") => {
                reach.restart |= flag(named("enabled")) != Level::None;
            }
            ("@exact/recording", "resumeMediaRecorder") => reach.restart = true,
            ("@exact/recording", "startMediaRecorder") => reach.recorder = true,
            ("@exact/speech-recognition", "startSpeechRecognition") => {
                reach.speech = reach.speech.max(flag(named("microphone")));
                reach.restart |= constraints(named("track")) != Level::None;
            }
            ("@exact/screen-capture", "captureScreens") => reach.display = Level::Known,
            (
                "@exact/camera",
                "takePhoto" | "takePhotoWithCameraApp" | "recordVideoWithCameraApp",
            ) => reach.camera = Level::Known,
            _ => {}
        }
    }

    /// How surely a Boolean option asks for its kind: absent, `none` and
    /// `false` do not, `true` does, and anything computed may.
    fn flag(&self, value: Option<&Expr>) -> Level {
        match self.resolved(value) {
            None | Some(Expr::None(_) | Expr::Bool(false, _)) => Level::None,
            Some(Expr::Some(inner, _)) => self.flag(Some(inner)),
            Some(Expr::Bool(true, _)) => Level::Known,
            Some(_) => Level::Possible,
        }
    }

    /// How surely a constraints option asks for its kind: a shape's
    /// constructor does, `none` does not, and anything computed may.
    fn constraints(&self, value: Option<&Expr>) -> Level {
        match self.resolved(value) {
            None | Some(Expr::None(_)) => Level::None,
            Some(Expr::Some(inner, _)) => self.constraints(Some(inner)),
            Some(Expr::Call(name, ..)) if name.starts_with(char::is_uppercase) => Level::Known,
            Some(_) => Level::Possible,
        }
    }
}

/// Check each control against the plan and add it there (D1): its tool is
/// declared, destroys nothing and needs no argument; its URL is a location
/// the navigation root's routes match.
pub(crate) fn add(
    controls: &[Control],
    root: &Component,
    navigates: bool,
    plan: &mut Plan,
) -> Result<(), (&'static str, String)> {
    for control in controls {
        let kind = &control.kind;
        let (tool, url, effects, signature) = match &control.action {
            ControlAction::Tool(name) => {
                let Some(i) = (plan.tools.iter()).position(|t| plan.str(t.name) == name) else {
                    return Err(("control-tool", format!("control `{kind}` names tool `{name}`, and the app has no `tool {name}` line (LLP 1126.004 D1)")));
                };
                let row = &plan.tools[i];
                if plan.str(row.consequence) == "destroys" {
                    return Err(("control-destroys", format!("control `{kind}`: tool `{name}` destroys, and a control has no confirmation for the person to answer; name a tool that reads or writes (LLP 1126.004 D1)")));
                }
                let action = plan.action(row.action);
                let required = (action.params.iter())
                    .map(|p| plan.param(p))
                    .find(|p| plan.types[p.ty.0 as usize].kind != exact_plan::TypeKind::Option);
                if let Some(p) = required {
                    return Err(("control-arguments", format!("control `{kind}`: tool `{name}`'s parameter `{}` is required, and a press passes no arguments; make it an option, which a press passes as none (LLP 1126.004 D1)", p.declared_name(plan))));
                }
                let action = plan.str(action.name).to_owned();
                let effects = self::effects(root, plan, &action);
                let signature = crate::faces::tool_signature(plan, i);
                (Some(ToolsId(i as u32)), "", effects, signature)
            }
            ControlAction::Url(url) => {
                if !navigates || !opens(plan, url) {
                    return Err(("control-url", format!("control `{kind}` opens {url}, which no route of the navigation root matches; a URL control needs the root component's outermost node to have `navigate=` and a route for its url (LLP 1126.004 D1; docs/recipes/navigation-stack.md)")));
                }
                (None, url.as_str(), json!({}), Value::Null)
            }
        };
        let digest = digest(&control.json(), &signature, &effects);
        plan.add_control(kind, tool, url, &effects.to_string(), &digest);
    }
    Ok(())
}

/// The dispatch digest of a control: hex SHA-256 of its row, its tool's
/// signature (null for a URL) and its effects, as canonical JSON (D1).
pub fn digest(row: &Value, signature: &Value, effects: &Value) -> String {
    let all = json!({ "row": row, "signature": signature, "effects": effects });
    let digest = Sha256::digest(crate::faces::canonical(&all).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether one of the plan's declared routes matches `url`: a `notfound`
/// fallback does not make a literal valid.
fn opens(plan: &Plan, url: &str) -> bool {
    if plan.router.is_none() {
        return false;
    }
    let table = exact_route::Table {
        routes: (plan.routes.iter())
            .map(|r| exact_route::Route {
                name: plan.str(r.name).into(),
                pattern: plan.str(r.pattern).into(),
                parent: r.parent.map(|p| p.0 as usize),
                tab: r.tab,
                notfound: r.notfound,
            })
            .collect(),
    };
    table.matches_pattern(url).is_some()
}
