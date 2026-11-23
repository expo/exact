//! Shared JavaScript expressions and mount functions. A call supplies the
//! operands that differ between uses of the same body in one lexical scope.
use std::{collections::HashMap, fmt::Write as _, rc::Rc};

#[derive(Clone, Hash, Eq, PartialEq)]
enum Value {
    Js(Rc<str>),
    /// The getter ignores its caller's instruction. Any other use may supply it.
    NoPc,
    Param(usize),
    /// A name bound within the function being rendered.
    Local(Rc<str>),
    /// A getter or state object captured from an enclosing arm.
    Binding(usize, Rc<str>),
}

impl Value {
    fn resolve<'a>(&'a self, args: &'a [Value]) -> &'a Value {
        match self {
            Self::Param(i) => &args[*i],
            value => value,
        }
    }
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct Call {
    id: usize,
    base: Option<u32>,
    args: Vec<Value>,
    parent: Option<Box<str>>,
    /// A slot mount follows the DOM parent in the surrounding expression.
    takes_slot: bool,
}

#[derive(Clone, Hash, Eq, PartialEq)]
enum Part {
    Js(Box<str>),
    Site(u32),
    /// The DOM parent at which a frame mounts its caller's children.
    Slot(Box<str>),
    Value(Value),
    GetterArgs {
        shared: bool,
        pc: Value,
    },
    Call(Call),
}

impl Part {
    fn site(&mut self) -> Option<&mut u32> {
        match self {
            Self::Site(n) | Self::Call(Call { base: Some(n), .. }) => Some(n),
            _ => None,
        }
    }
    fn values(&mut self) -> &mut [Value] {
        match self {
            Self::Value(value) | Self::GetterArgs { pc: value, .. } => std::slice::from_mut(value),
            Self::Call(call) => &mut call.args,
            _ => &mut [],
        }
    }
}

struct Template {
    body: Rc<[Part]>,
    /// Mounts also take DOM parameters. An expression factory takes only its captures.
    params: Option<Rc<str>>,
    site: bool,
    columns: Vec<Column>,
}

struct Column {
    fixed: Option<Value>,
    /// First occurrence with the same argument at every call seen so far.
    first: usize,
}

impl Template {
    fn captures(&self) -> impl Iterator<Item = usize> + '_ {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(i, column)| (column.fixed.is_none() && column.first == i).then_some(i))
    }

    fn observe(&mut self, args: &[Value]) {
        let mut firsts = HashMap::new();
        for (i, (column, arg)) in self.columns.iter_mut().zip(args).enumerate() {
            if *arg != Value::NoPc {
                if column.fixed == Some(Value::NoPc) {
                    column.fixed = Some(arg.clone());
                } else if column.fixed.as_ref() != Some(arg) {
                    column.fixed = None;
                }
            }
            column.first = *firsts.entry((column.first, arg)).or_insert(i);
        }
    }
}

#[derive(Default)]
pub(super) struct Templates {
    /// Shared functions are declared here. A deeper binding must come from their caller.
    declaration_depth: usize,
    pub(super) scope_depth: usize,
    /// A shared computation takes this argument at the point it is evaluated.
    argument: Option<&'static str>,
    bodies: Vec<Template>,
    index: HashMap<Option<Rc<str>>, HashMap<Rc<[Part]>, usize>>,
    calls: Vec<Call>,
}

#[derive(Clone, Copy)]
enum Base {
    Absolute(u32),
    Enclosing(u32),
    Relative(u32),
}

impl Base {
    fn plus(self, n: u32) -> Base {
        match self {
            Self::Absolute(base) => Self::Absolute(base + n),
            Self::Enclosing(base) => Self::Enclosing(base + n),
            Self::Relative(base) => Self::Relative(base + n),
        }
    }

    fn js(self) -> String {
        match self {
            Self::Absolute(n) => n.to_string(),
            Self::Enclosing(n) => format!("\0s{n}\0"),
            Self::Relative(0) => "$site".into(),
            Self::Relative(n) => format!("$site+{n}"),
        }
    }
}

impl Templates {
    pub fn mounts(depth: usize) -> Self {
        Self {
            declaration_depth: depth,
            scope_depth: depth,
            ..Self::default()
        }
    }

    pub fn computations(depth: usize, argument: &'static str) -> Self {
        Self {
            argument: Some(argument),
            ..Self::mounts(depth)
        }
    }

    // The JS writer quotes authored strings, so source NULs arrive escaped.
    fn parts(&self, source: &str) -> Vec<Part> {
        let mut result = Vec::new();
        let mut slices = source.split('\0');
        while let Some(js) = slices.next() {
            if !js.is_empty() {
                result.push(Part::Js(js.into()));
            }
            if let Some(op) = slices.next() {
                result.push(if let Some(parent) = op.strip_prefix("slot:") {
                    Part::Slot(parent.into())
                } else if let Some(site) = op.strip_prefix('s') {
                    Part::Site(site.parse().expect("site"))
                } else if let Some(js) = op.strip_prefix("v:") {
                    Part::Value(Value::Js(js.into()))
                } else if let Some(binding) = op.strip_prefix('l') {
                    let (depth, name) = binding.split_once(':').expect("lexical binding");
                    let depth = depth.parse().expect("binding depth");
                    if self.scope_depth < depth {
                        // The surrounding arm introduces this name in its own body.
                        Part::Js(name.into())
                    } else {
                        Part::Value(Value::Binding(depth, name.into()))
                    }
                } else if let Some(pc) = op.strip_prefix("g:").or_else(|| op.strip_prefix("b:")) {
                    Part::GetterArgs {
                        shared: op.starts_with('b'),
                        pc: if pc.is_empty() {
                            Value::NoPc
                        } else {
                            Value::Js(pc.into())
                        },
                    }
                } else if op == "no-pc" {
                    Part::Value(Value::NoPc)
                } else {
                    let i: usize = op.parse().expect("template call");
                    Part::Call(self.calls[i].clone())
                });
            }
        }
        result
    }

    pub fn mount(
        &mut self,
        params: &str,
        source: String,
        parent: Option<&str>,
        parent_site: Option<u32>,
    ) -> String {
        self.intern(Some(params), source, parent, parent_site)
    }

    pub fn slot_frame(&mut self, body: String) -> String {
        let call = self.mount("p,$slot", body, Some("p"), None);
        self.calls.last_mut().unwrap().takes_slot = true;
        call
    }

    pub fn expression(&mut self, source: String) -> String {
        self.intern(None, source, None, None)
    }

    /// A list effect depends on the fields read inside its getters as well as in its own code.
    pub fn field_mask(&self, scope: &crate::code::Scope, source: &str) -> Option<u32> {
        if !scope.frames.iter().any(|frame| frame.item.is_some()) {
            return None;
        }
        if !source.contains('\0') {
            return crate::reads::field_mask(source);
        }
        let inline = Render {
            templates: self,
            names: &[],
            enclosing: false,
        };
        let mut expanded = String::new();
        inline.expand(&self.parts(source), Base::Absolute(0), &[], &mut expanded);
        crate::reads::field_mask(&expanded)
    }

    fn intern(
        &mut self,
        params: Option<&str>,
        source: String,
        parent: Option<&str>,
        parent_site: Option<u32>,
    ) -> String {
        let mut body = self.parts(&source);
        // A region subject can report its enclosing element. Two uses under
        // the same element move their descendants while keeping that site.
        if let Some(parent) = parent_site {
            for part in &mut body {
                if matches!(part, Part::Site(n) if *n == parent) {
                    *part = Part::Value(Value::Js(parent.to_string().into()));
                }
            }
        }
        let base = body.iter_mut().filter_map(Part::site).map(|n| *n).min();
        if let Some(base) = base {
            for site in body.iter_mut().filter_map(Part::site) {
                *site -= base;
            }
        }

        let mut args = Vec::new();
        for value in body.iter_mut().flat_map(Part::values) {
            if let Value::Binding(depth, name) = value {
                if *depth > self.scope_depth {
                    // The parent declares this binding before calling the child.
                    *value = Value::Local(name.clone());
                    continue;
                }
            }
            if matches!(value, Value::Local(_)) {
                continue;
            }
            let i = args.len();
            args.push(std::mem::replace(value, Value::Param(i)));
        }
        let params: Option<Rc<str>> = params.map(Into::into);
        let index = self.index.entry(params.clone()).or_default();
        let id = match index.get(body.as_slice()) {
            Some(&id) => id,
            None => {
                let id = self.bodies.len();
                let body: Rc<[Part]> = body.into();
                index.insert(body.clone(), id);
                self.bodies.push(Template {
                    body,
                    params,
                    site: base.is_some(),
                    columns: args
                        .iter()
                        .map(|js| Column {
                            fixed: match js {
                                Value::Binding(depth, _) if *depth > self.declaration_depth => None,
                                _ => Some(js.clone()),
                            },
                            first: 0,
                        })
                        .collect(),
                });
                id
            }
        };
        self.bodies[id].observe(&args);
        let call = self.calls.len();
        self.calls.push(Call {
            id,
            base,
            args,
            parent: parent.map(Into::into),
            takes_slot: false,
        });
        format!("\0{call}\0")
    }

    pub fn render(self, view: String, enclosing: bool, prefix: &str) -> (String, String) {
        if self.bodies.is_empty() && !view.contains('\0') {
            return (String::new(), view);
        }
        let view = self.parts(&view);
        let mut uses = vec![0usize; self.bodies.len()];
        for part in &view {
            if let Part::Call(call) = part {
                uses[call.id] += 1;
            }
        }
        // Children are interned first. An inlined parent copies its children
        // to every use. A shared parent emits them once.
        let all_names: Vec<_> = (0..self.bodies.len())
            .map(|i| Some(format!("{prefix}{i}")))
            .collect();
        let called = Render {
            templates: &self,
            names: &all_names,
            enclosing: false,
        };
        let mut names = vec![None; self.bodies.len()];
        for (i, t) in self.bodies.iter().enumerate().rev() {
            let n = uses[i];
            let name = all_names[i].as_ref().unwrap();
            if n > 1 {
                let threshold = (8 + (n + 1) * name.len()) / (n - 1);
                let literal: usize = t
                    .body
                    .iter()
                    .map(|p| if let Part::Js(js) = p { js.len() } else { 1 })
                    .sum();
                let share = literal > threshold
                    || t.body.iter().any(|p| matches!(p, Part::Call(_))) && {
                        // A small adapter may call several other mounts. Count what
                        // those calls emit, including their bindings and site offsets.
                        let values: Vec<_> = t
                            .columns
                            .iter()
                            .map(|c| c.fixed.clone().unwrap_or_else(|| Value::Local("$c".into())))
                            .collect();
                        let mut js = String::new();
                        called.expand(&t.body, Base::Relative(0), &values, &mut js);
                        js.len() > threshold
                    };
                if share {
                    names[i] = Some(name.clone());
                }
            }
            for part in t.body.iter() {
                if let Part::Call(call) = part {
                    uses[call.id] += if names[i].is_some() { 1 } else { n };
                }
            }
        }
        let render = Render {
            templates: &self,
            names: &names,
            enclosing,
        };
        let mut declarations = String::new();
        for (i, name) in names.iter().enumerate() {
            if let Some(name) = name {
                let t = &self.bodies[i];
                let mut params = Vec::new();
                if t.site {
                    params.push("$site".to_string());
                }
                let mut captures = HashMap::new();
                let single = t.captures().take(2).count() == 1;
                let env = format!("$c{}", self.declaration_depth);
                let values: Vec<_> = t
                    .columns
                    .iter()
                    .map(|column| {
                        if let Some(value) = &column.fixed {
                            value.clone()
                        } else {
                            let next = captures.len();
                            let index = captures.entry(column.first).or_insert(next);
                            let operand = if single {
                                env.clone()
                            } else {
                                format!("{env}[{index}]")
                            };
                            Value::Local(operand.into())
                        }
                    })
                    .collect();
                if !captures.is_empty() {
                    params.push(env);
                }
                params.extend(self.argument.map(str::to_owned));
                params.extend(t.params.iter().map(ToString::to_string));
                write!(declarations, "const {name}=({})=>", params.join(",")).unwrap();
                if t.params.is_some() {
                    declarations.push('{');
                }
                render.expand(&t.body, Base::Relative(0), &values, &mut declarations);
                if t.params.is_some() {
                    declarations.push('}');
                }
                declarations.push(';');
            }
        }
        let base = if enclosing {
            Base::Enclosing(0)
        } else {
            Base::Absolute(0)
        };
        let mut expanded = String::new();
        render.expand(&view, base, &[], &mut expanded);
        (declarations, expanded)
    }
}

/// Slot operands and diagnostic sites resolve in the caller, including when the frame is inlined.
#[derive(Clone, Copy)]
struct Slot<'a> {
    call: &'a Call,
    base: Base,
    args: &'a [Value],
}

struct Render<'a> {
    templates: &'a Templates,
    names: &'a [Option<String>],
    enclosing: bool,
}

impl Render<'_> {
    fn operand(&self, value: &Value) -> Rc<str> {
        match value {
            Value::Js(js) if self.enclosing => format!("\0v:{js}\0").into(),
            Value::Binding(depth, js)
                if self.enclosing && self.templates.declaration_depth > *depth =>
            {
                crate::code::lexical_operand(js, *depth).into()
            }
            Value::Binding(_, js) => js.clone(),
            Value::Js(js) | Value::Local(js) => js.clone(),
            Value::NoPc if self.enclosing => "\0no-pc\0".into(),
            Value::NoPc => "null".into(),
            Value::Param(_) => unreachable!("unresolved template parameter"),
        }
    }

    fn expand(&self, body: &[Part], base: Base, args: &[Value], out: &mut String) {
        self.expand_with_slot(body, base, args, None, out);
    }

    fn expand_with_slot(
        &self,
        body: &[Part],
        base: Base,
        args: &[Value],
        slot: Option<Slot<'_>>,
        out: &mut String,
    ) {
        let mut parts = body.iter();
        while let Some(part) = parts.next() {
            match part {
                Part::Slot(parent) => match slot {
                    Some(slot) => {
                        self.call(slot.call, slot.base, slot.args, Some(parent), None, out)
                    }
                    None => write!(out, "$slot({parent});").unwrap(),
                },
                Part::Js(js) => out.push_str(js),
                Part::Site(n) => out.push_str(&base.plus(*n).js()),
                Part::Value(value) => out.push_str(&self.operand(value.resolve(args))),
                Part::GetterArgs { shared, pc } => {
                    let (capture, pc) = match pc.resolve(args) {
                        Value::Js(js) => (self.enclosing, Some(js.as_ref())),
                        Value::Local(js) => (false, Some(js.as_ref())),
                        Value::NoPc => (self.enclosing, None),
                        Value::Param(_) | Value::Binding(..) => {
                            unreachable!("invalid getter instruction")
                        }
                    };
                    out.push_str(&crate::code::getter_args(capture, *shared, pc));
                }
                Part::Call(call) => {
                    let content = call.takes_slot.then(|| {
                        let Some(Part::Call(content)) = parts.next() else {
                            unreachable!("a slot frame takes a mount")
                        };
                        Slot {
                            call: content,
                            base,
                            args,
                        }
                    });
                    self.call(call, base, args, call.parent.as_deref(), content, out);
                }
            }
        }
    }

    fn call(
        &self,
        call: &Call,
        base: Base,
        args: &[Value],
        parent: Option<&str>,
        slot: Option<Slot<'_>>,
        out: &mut String,
    ) {
        let t = &self.templates.bodies[call.id];
        if let Some(Some(name)) = self.names.get(call.id) {
            out.push_str(name);
            let mut inputs: Vec<_> = call.base.map(|n| base.plus(n).js()).into_iter().collect();
            let captures: Vec<_> = t
                .captures()
                .map(|i| self.operand(call.args[i].resolve(args)))
                .collect();
            match captures.as_slice() {
                [] => {}
                [one] => inputs.push(one.to_string()),
                many => inputs.push(format!("[{}]", many.join(","))),
            }
            if let Some(parent) = parent {
                inputs.push(parent.to_string());
                write!(out, "({}", inputs.join(",")).unwrap();
                if let Some(slot) = slot {
                    out.push(',');
                    self.call(slot.call, slot.base, slot.args, None, None, out);
                }
                out.push_str(");");
            } else if t.params.is_none() {
                inputs.extend(self.templates.argument.map(str::to_owned));
                write!(out, "({})", inputs.join(",")).unwrap();
            } else if !inputs.is_empty() {
                write!(out, ".bind(null,{})", inputs.join(",")).unwrap();
            }
        } else {
            let values: Vec<_> = call.args.iter().map(|v| v.resolve(args).clone()).collect();
            let base = base.plus(call.base.unwrap_or(0));
            let Some(params) = &t.params else {
                self.expand(&t.body, base, &values, out);
                return;
            };
            match parent {
                Some("p") => out.push('{'),
                // The parent can have the same local name as a child.
                Some(parent) => write!(out, "{{const p={parent};{{").unwrap(),
                None => write!(out, "({params})=>{{").unwrap(),
            }
            self.expand_with_slot(&t.body, base, &values, slot, out);
            out.push('}');
            if parent.is_some_and(|p| p != "p") {
                out.push('}');
            }
        }
    }
}
