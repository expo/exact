//! Keyframe values evaluated with the runtime's pure operations (LLP 1062 D9).
//!
//! A keyframe is constant (LLP 1055 D5). Functions of literal arguments
//! fold to the literal the plan's `@keyframes` rule carries; executed
//! runtime inputs are refused by name.

use contract_syntax::{BinOp, Expr, FnDecl, TemplatePart, TypeExpr, UnOp};
use contract_types::{records, Ref, Scope, Shapes, Ty};
use exact_plan::{asm::Asm, builder::PlanBuilder, Opcode, Stdlib, TypeKind, Value};
use exact_runner::vm;

/// The app's checked types and functions, without runtime values.
pub(crate) struct Context<'a> {
    fns: Vec<&'a FnDecl>,
    scope: &'a Scope,
    shapes: &'a Shapes,
}

impl<'a> Context<'a> {
    pub(crate) fn new(
        fns: impl IntoIterator<Item = &'a FnDecl>,
        scope: &'a Scope,
        shapes: &'a Shapes,
    ) -> Self {
        Self {
            fns: fns.into_iter().collect(),
            scope,
            shapes,
        }
    }

    pub(crate) fn template_constant(&self, e: &Expr, string_only: bool) -> Option<String> {
        if matches!(e, Expr::Ident(..)) {
            return None;
        }
        let ty = contract_types::infer(e, self.scope, self.shapes).ok()?;
        if let Expr::Typed(inner, ..) = e {
            if let Expr::Str(s, _) = inner.as_ref() {
                return Some(s.clone());
            }
        }
        (ty == Ty::String || (!string_only && ty == Ty::Number)).then(|| self.constant(e).ok())?
    }

    /// Evaluate literal text with the runtime VM, refusing executed input reads.
    pub(crate) fn constant(&self, e: &Expr) -> Result<String, String> {
        let mut compiler = Constant {
            b: PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 0),
            fns: &self.fns,
            inputs: self.scope,
            shapes: self.shapes,
            remaining: FOLD_STEPS,
            depth: 0,
            call_depth: 0,
            values: Vec::new(),
            list_steps: 0,
            deferred: false,
            failure: None,
        };
        let value = compiler.evaluate(e, self.scope);
        let text = value.and_then(|(_, value)| match value {
            Value::Number(n) => Some(exact_num::js(n)),
            Value::Bool(b) => Some(b.to_string()),
            v => v.as_str().map(str::to_string),
        });
        text.ok_or_else(|| {
            compiler
                .failure
                .unwrap_or_else(|| "the expression has no constant number or string value".into())
        })
    }
}

impl crate::Lowerer<'_> {
    pub(crate) fn constants<'a>(&'a self, scope: &'a Scope) -> Context<'a> {
        Context::new(
            self.fns.values().map(|(f, _)| *f),
            scope,
            &self.types.shapes,
        )
    }
}

// Bound expansion and list work across VM evaluations; each evaluation also
// enforces the runtime's value limits.
const FOLD_STEPS: usize = 4096;
// Bound the compiler's stack separately from function-call depth. A source
// expression alone may have 100 levels, before function expansion.
const EXPANSION_DEPTH: usize = 128;

struct Constant<'a> {
    b: PlanBuilder,
    fns: &'a [&'a FnDecl],
    inputs: &'a Scope,
    shapes: &'a Shapes,
    remaining: usize,
    depth: usize,
    call_depth: u16,
    values: Vec<Value>,
    list_steps: u32,
    deferred: bool,
    failure: Option<String>,
}

impl Constant<'_> {
    fn evaluate(&mut self, e: &Expr, scope: &Scope) -> Option<(Ty, Value)> {
        let mut asm = Asm::new();
        let ty = self.emit(&mut asm, e, scope, &mut 0)?;
        let (result, accounting) = vm::eval_constant(&asm.finish(), self.b.plan(), &self.values);
        self.list_steps = self.list_steps.saturating_add(accounting.list_steps);
        if self.list_steps > vm::MAX_LIST_STEPS {
            self.failure = Some("constant evaluation exceeded the list step limit".into());
            return None;
        }
        match result {
            Ok(value) => Some((ty, value)),
            Err(failure) => {
                self.failure = Some(match failure.read {
                    Some(name) => format!("read `{name}`, which is not constant"),
                    None => format!(
                        "constant evaluation failed: {:?}{}",
                        failure.trap,
                        failure.trap.bound()
                    ),
                });
                None
            }
        }
    }

    // A runtime input has a name in the plan but no value in the constant
    // environment. The VM refuses only if execution reaches its load.
    fn input(&mut self, asm: &mut Asm, name: &str, ty: Ty) -> Ty {
        let kind = self.b.primitive(TypeKind::Number);
        let init = self.b.constant(&Value::Number(0.0));
        let slot = self.b.slot(name, kind, init);
        asm.load_slot(slot);
        ty
    }

    fn parameter(&mut self, value: Value) -> Ref {
        let i = self.values.len();
        self.values.push(value);
        Ref::Param(i as u32)
    }

    fn branch_type(&self, ty: Ty, e: &Expr, scope: &Scope) -> Option<Ty> {
        if ty.is_complete() {
            Some(ty)
        } else {
            // A selected `[]` or `none` takes its item type from both arms.
            // Inference reads function signatures without expanding calls.
            contract_types::infer(e, scope, self.shapes).ok()
        }
    }

    fn bind(
        &mut self,
        asm: &mut Asm,
        e: &Expr,
        scope: &Scope,
        locals: &mut u16,
    ) -> Option<(Ref, Ty)> {
        if !self.deferred {
            let (ty, value) = self.evaluate(e, scope)?;
            Some((self.parameter(value), ty))
        } else {
            let ty = self.emit(asm, e, scope, locals)?;
            asm.bind_local();
            let reference = Ref::Local(*locals as u32);
            *locals += 1;
            Some((reference, ty))
        }
    }

    fn emit(&mut self, asm: &mut Asm, e: &Expr, scope: &Scope, locals: &mut u16) -> Option<Ty> {
        if self.remaining == 0 || self.depth >= EXPANSION_DEPTH {
            self.failure =
                Some("constant evaluation exceeded the expression expansion limit".into());
            return None;
        }
        self.remaining -= 1;
        self.depth += 1;
        let result = self.emit_inner(asm, e, scope, locals);
        self.depth -= 1;
        result
    }

    fn emit_inner(
        &mut self,
        asm: &mut Asm,
        e: &Expr,
        scope: &Scope,
        locals: &mut u16,
    ) -> Option<Ty> {
        Some(match e {
            Expr::Str(s, _) => {
                asm.str(self.b.str(s));
                Ty::String
            }
            Expr::Number(n, _) => {
                asm.number(*n);
                Ty::Number
            }
            Expr::Bool(b, _) => {
                asm.bool(*b);
                Ty::Bool
            }
            Expr::None(_) => {
                asm.simple(Opcode::None);
                Ty::Option(Box::new(Ty::Unknown))
            }
            Expr::Some(value, _) => {
                let ty = self.emit(asm, value, scope, locals)?;
                asm.simple(Opcode::Some);
                Ty::Option(Box::new(ty))
            }
            Expr::List(items, _) => {
                let mut ty = Ty::Unknown;
                for item in items {
                    ty = ty.unify(&self.emit(asm, item, scope, locals)?)?;
                }
                asm.list(items.len() as u32);
                Ty::List(Box::new(ty))
            }
            Expr::Ident(name, _) => {
                let Some((reference, ty)) = scope.lookup(name) else {
                    return Some(self.input(asm, name, Ty::Unknown));
                };
                match reference {
                    Ref::Local(i) => asm.load_local(i as u16),
                    Ref::Param(i) => asm.load_param(i as u16),
                    _ => return Some(self.input(asm, name, ty.clone())),
                };
                ty.clone()
            }
            Expr::Member(base, field, _) => {
                let ty = self.emit(asm, base, scope, locals)?;
                let (index, ty) = match ty {
                    Ty::Record(shape) => self.shapes.field(&shape, field)?,
                    // An unavailable base traps before this field is reached.
                    Ty::Unknown => (0, &Ty::Unknown),
                    _ => return None,
                };
                asm.field(index as u16);
                ty.clone()
            }
            Expr::Template(parts, _) => {
                asm.str(self.b.str(""));
                for part in parts {
                    match part {
                        TemplatePart::Text(s) => {
                            asm.str(self.b.str(s));
                        }
                        TemplatePart::Expr(e) => {
                            self.emit(asm, e, scope, locals)?;
                            asm.call(Stdlib::ToString);
                        }
                    }
                    asm.simple(Opcode::Concat);
                }
                Ty::String
            }
            Expr::Ternary(c, yes, no, _) => {
                if !self.deferred {
                    let (_, Value::Bool(condition)) = self.evaluate(c, scope)? else {
                        return None;
                    };
                    let ty = self.emit(asm, if condition { yes } else { no }, scope, locals)?;
                    return self.branch_type(ty, e, scope);
                }
                self.emit(asm, c, scope, locals)?;
                let otherwise = asm.label();
                let end = asm.label();
                asm.jump_if_false(otherwise);
                let ty = self.emit(asm, yes, scope, locals)?;
                asm.jump(end).place(otherwise);
                let other = self.emit(asm, no, scope, locals)?;
                asm.place(end);
                ty.unify(&other)?
            }
            Expr::Match {
                subject,
                var,
                some,
                none,
                ..
            } => {
                if !self.deferred {
                    let (Ty::Option(ty), Value::Option(value)) = self.evaluate(subject, scope)?
                    else {
                        return None;
                    };
                    let ty = match value {
                        Some(value) => {
                            let mut inner = scope.clone();
                            inner.push(vec![(var.clone(), self.parameter((*value).clone()), *ty)]);
                            self.emit(asm, some, &inner, locals)
                        }
                        None => self.emit(asm, none, scope, locals),
                    }?;
                    return self.branch_type(ty, e, scope);
                }
                let Ty::Option(ty) = self.emit(asm, subject, scope, locals)? else {
                    return None;
                };
                let otherwise = asm.label();
                let end = asm.label();
                asm.jump_if_none(otherwise)
                    .simple(Opcode::Unwrap)
                    .bind_local();
                let mut inner = scope.clone();
                inner.push(vec![(var.clone(), Ref::Local(*locals as u32), *ty)]);
                *locals += 1;
                let ty = self.emit(asm, some, &inner, locals)?;
                *locals -= 1;
                asm.drop_local()
                    .jump(end)
                    .place(otherwise)
                    .simple(Opcode::Pop);
                let other = self.emit(asm, none, scope, locals)?;
                asm.place(end);
                ty.unify(&other)?
            }
            Expr::Let {
                name, value, body, ..
            } => {
                let (reference, ty) = self.bind(asm, value, scope, locals)?;
                let mut inner = scope.clone();
                inner.push(vec![(name.clone(), reference, ty)]);
                let ty = self.emit(asm, body, &inner, locals)?;
                if matches!(reference, Ref::Local(_)) {
                    *locals -= 1;
                    asm.drop_local();
                }
                ty
            }
            Expr::Typed(value, ty, _) => {
                annotated(self.emit(asm, value, scope, locals)?, ty, self.shapes)?
            }
            Expr::Unary(op, value, _) => {
                let ty = self.emit(asm, value, scope, locals)?;
                asm.simple(match op {
                    UnOp::Neg => Opcode::Neg,
                    UnOp::Not => Opcode::Not,
                });
                ty
            }
            Expr::Binary(op @ (BinOp::And | BinOp::Or), a, b, _) => {
                if !self.deferred {
                    let (_, Value::Bool(value)) = self.evaluate(a, scope)? else {
                        return None;
                    };
                    if value == (*op == BinOp::Or) {
                        asm.bool(value);
                        return Some(Ty::Bool);
                    }
                    return self.emit(asm, b, scope, locals);
                }
                self.emit(asm, a, scope, locals)?;
                let otherwise = asm.label();
                let end = asm.label();
                asm.bind_local().load_local(*locals);
                if *op == BinOp::Or {
                    asm.simple(Opcode::Not);
                }
                asm.jump_if_false(otherwise).drop_local();
                self.emit(asm, b, scope, locals)?;
                asm.jump(end)
                    .place(otherwise)
                    .load_local(*locals)
                    .drop_local()
                    .place(end);
                Ty::Bool
            }
            Expr::Binary(op, a, b, _) => {
                let ty = self.emit(asm, a, scope, locals)?;
                self.emit(asm, b, scope, locals)?;
                asm.simple(match op {
                    BinOp::Add if ty.is_string() => Opcode::Concat,
                    BinOp::Add => Opcode::Add,
                    BinOp::Sub => Opcode::Sub,
                    BinOp::Mul => Opcode::Mul,
                    BinOp::Div => Opcode::Div,
                    BinOp::Rem => Opcode::Rem,
                    BinOp::Eq => Opcode::Eq,
                    BinOp::Ne => Opcode::Ne,
                    BinOp::Lt => Opcode::Lt,
                    BinOp::Le => Opcode::Le,
                    BinOp::Gt => Opcode::Gt,
                    BinOp::Ge => Opcode::Ge,
                    BinOp::And | BinOp::Or => unreachable!(),
                });
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => ty,
                    _ => Ty::Bool,
                }
            }
            Expr::Call(name, args, _) if records::is_record_call(name, self.shapes) => {
                let ty = match contract_types::infer(e, scope, self.shapes) {
                    Ok(ty) => ty,
                    Err(error) => {
                        self.failure = Some(error.message);
                        return None;
                    }
                };
                self.record(asm, name, args, scope, locals)?;
                ty
            }
            Expr::Call(name, args, _) => {
                if let Some(f) = self.fns.iter().find(|f| f.name == *name) {
                    if f.params.len() != args.len() {
                        return None;
                    }
                    let mut bound = Vec::new();
                    for (p, a) in f.params.iter().zip(args) {
                        let (reference, ty) = self.bind(asm, a, scope, locals)?;
                        let ty = match &p.ty {
                            Some(annotation) => annotated(ty, annotation, self.shapes)?,
                            None => ty,
                        };
                        bound.push((p.name.clone(), reference, ty));
                    }
                    let count = bound
                        .iter()
                        .filter(|(_, r, _)| matches!(r, Ref::Local(_)))
                        .count();
                    let mut inner = self.inputs.clone();
                    inner.push(bound);
                    if self.call_depth >= vm::MAX_CALL_DEPTH {
                        self.failure =
                            Some("constant evaluation exceeded the call depth limit".into());
                        return None;
                    }
                    self.call_depth += 1;
                    let result = self.emit(asm, &f.body, &inner, locals);
                    self.call_depth -= 1;
                    let ty = result?;
                    for _ in 0..count {
                        *locals -= 1;
                        asm.drop_local();
                    }
                    annotated(ty, &f.ret, self.shapes)?
                } else {
                    let Some(f) = Stdlib::from_name(name) else {
                        let ty =
                            contract_types::infer(e, scope, self.shapes).unwrap_or(Ty::Unknown);
                        return Some(self.input(asm, &format!("{name}()"), ty));
                    };
                    if f == Stdlib::T {
                        return Some(self.input(asm, "t()", Ty::String));
                    }
                    if let (Stdlib::Map | Stdlib::Filter, [list, callback]) = (f, args.as_slice()) {
                        let listed = self.emit(asm, list, scope, locals)?;
                        let op = if f == Stdlib::Map {
                            Opcode::Map
                        } else {
                            Opcode::Filter
                        };
                        let result = self.callback(asm, op, &listed, callback, scope, locals)?;
                        return Some(if f == Stdlib::Map {
                            Ty::List(Box::new(result))
                        } else {
                            listed
                        });
                    }
                    if let (Stdlib::SortBy, [list, callback, rest @ ..]) = (f, args.as_slice()) {
                        let descending = match rest {
                            [] => false,
                            [Expr::Str(s, _)] if s == "asc" => false,
                            [Expr::Str(s, _)] if s == "desc" => true,
                            _ => return None,
                        };
                        let listed = self.emit(asm, list, scope, locals)?;
                        asm.bind_local().load_local(*locals).load_local(*locals);
                        *locals += 1;
                        self.callback(asm, Opcode::Map, &listed, callback, scope, locals)?;
                        *locals -= 1;
                        asm.bool(descending).call(f).drop_local();
                        return Some(listed);
                    }
                    // Infer from operand types without expanding function bodies.
                    let mut typed = Scope::default();
                    let mut arguments = Vec::new();
                    for (i, a) in args.iter().enumerate() {
                        let ty = self.emit(asm, a, scope, locals)?;
                        let name = i.to_string();
                        typed.push(vec![(name.clone(), Ref::Local(i as u32), ty)]);
                        arguments.push(match a {
                            Expr::Str(..) | Expr::Number(..) | Expr::Bool(..) | Expr::None(..) => {
                                a.clone()
                            }
                            _ => Expr::Ident(name, a.span()),
                        });
                    }
                    let call = Expr::Call(name.clone(), arguments, e.span());
                    let ty = contract_types::infer(&call, &typed, self.shapes).ok()?;
                    for default in f.omitted(args.len()) {
                        match default {
                            exact_plan::StdlibDefault::Number(n) => {
                                asm.number(*n);
                            }
                            exact_plan::StdlibDefault::Str(s) => {
                                asm.str(self.b.str(s));
                            }
                        }
                    }
                    asm.call(f);
                    ty
                }
            }
            _ => return None,
        })
    }

    fn record(
        &mut self,
        asm: &mut Asm,
        name: &str,
        args: &[Expr],
        scope: &Scope,
        locals: &mut u16,
    ) -> Option<()> {
        let fields = &self.shapes.map[name];
        let base = if let Some(value) = records::base(args) {
            self.emit(asm, value, scope, locals)?;
            asm.bind_local();
            *locals += 1;
            Some(*locals - 1)
        } else {
            None
        };
        for (i, (field, _)) in fields.iter().enumerate() {
            let value = args.iter().find_map(|arg| match arg {
                Expr::NamedArg(n, value, _) if n == field => Some(value.as_ref()),
                _ => None,
            });
            if let Some(value) = value {
                self.emit(asm, value, scope, locals)?;
            } else {
                asm.load_local(base?).field(i as u16);
            }
        }
        // The VM reads only the field count from this type. Inference checks
        // field types before emission.
        let placeholder = self.b.primitive(TypeKind::Number);
        let fields: Vec<_> = fields
            .iter()
            .map(|(field, _)| (field.as_str(), placeholder))
            .collect();
        asm.record(self.b.record(name, &fields));
        if base.is_some() {
            *locals -= 1;
            asm.drop_local();
        }
        Some(())
    }

    fn callback(
        &mut self,
        asm: &mut Asm,
        op: Opcode,
        listed: &Ty,
        callback: &Expr,
        scope: &Scope,
        locals: &mut u16,
    ) -> Option<Ty> {
        let Expr::Arrow { params, body, .. } = callback else {
            return None;
        };
        let Ty::List(item) = listed else { return None };
        if params.len() > 2 {
            return None;
        }
        let end = asm.label();
        asm.each_item(op, end);
        let mut inner = scope.clone();
        inner.push(
            params
                .iter()
                .zip([(**item).clone(), Ty::Number])
                .enumerate()
                .map(|(i, (p, ty))| (p.clone(), Ref::Local(*locals as u32 + i as u32), ty))
                .collect(),
        );
        *locals += 2;
        // An empty list or an untaken callback branch executes no bindings.
        // Leave their evaluation, including closed arguments, to the VM.
        let deferred = self.deferred;
        self.deferred = true;
        let ty = self.emit(asm, body, &inner, locals)?;
        self.deferred = deferred;
        *locals -= 2;
        asm.place(end);
        Some(ty)
    }
}

// Empty lists and `none` take their item types from the function signature,
// as in normal expression lowering.
fn annotated(ty: Ty, annotation: &TypeExpr, shapes: &Shapes) -> Option<Ty> {
    ty.unify(&shapes.resolve(annotation).ok()?)
}

/// A colour keyframe with the app's design tokens (LLP 1121 D3) replaced by
/// the colours they stand for, as a row reads them: each whole component of
/// the value (a colour, or a shadow's part, split at the top level) that is a
/// token's name, a role's spelling, or an expression over tokens the build
/// folded (`light-dark(chip, #fff)`, a synthetic token named by its text)
/// is that entry's folded colour, a `light-dark()` pair whole, so the frames
/// follow the scheme. A word inside a function (`rgb(0, 0, 0)`) is no
/// token. `Err` names one that varies by an app axis, which a keyframe
/// folded once cannot follow.
pub(crate) fn tokens(value: &str, theme: &[crate::theme::ThemeEntry]) -> Result<String, String> {
    let mut out = String::with_capacity(value.len());
    let take = |part: &str, out: &mut String| -> Result<(), String> {
        match theme
            .iter()
            .find(|e| !part.is_empty() && (e.name == part || e.role.as_deref() == Some(part)))
        {
            Some(e) if !e.slices.is_empty() => Err(format!(
                "`{part}` varies by an app axis, and a keyframe is folded once"
            )),
            Some(e) => {
                out.push_str(&e.value);
                Ok(())
            }
            None => {
                out.push_str(part);
                Ok(())
            }
        }
    };
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if depth == 0 && (c.is_whitespace() || c == ',') => {
                take(&value[start..i], &mut out)?;
                out.push(c);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    take(&value[start..], &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(e: &Expr, fns: &[FnDecl]) -> Result<String, String> {
        let mut shapes = Shapes::default();
        for f in fns {
            let params = f
                .params
                .iter()
                .map(|p| {
                    p.ty.as_ref()
                        .and_then(|ty| shapes.resolve(ty).ok())
                        .unwrap_or(Ty::Unknown)
                })
                .collect();
            let ret = shapes.resolve(&f.ret).unwrap_or(Ty::Unknown);
            shapes.fns.insert(f.name.clone(), (params, ret));
        }
        Context::new(fns, &Scope::default(), &shapes).constant(e)
    }

    #[test]
    fn checked_props_have_types_but_no_constant_values() {
        let mut scope = Scope::default();
        scope.push(vec![("amount".into(), Ref::Prop(0), Ty::Number)]);
        let shapes = Shapes::default();
        let context = Context::new([], &scope, &shapes);
        for (body, expected) in [
            (
                "sum([0].map(i => i == 0 ? 0.5 : min(amount, 1)))",
                Ok("0.5"),
            ),
            (
                "min(amount, 1)",
                Err("read `amount`, which is not constant"),
            ),
        ] {
            let file = contract_syntax::parse(&format!("fn value(): number = {body}\n")).unwrap();
            let result = context.constant(&file.fns[0].body);
            assert_eq!(
                result.as_deref().map_err(String::as_str),
                expected,
                "{body}"
            );
        }
    }

    #[test]
    fn pure_roster_calls_use_the_vms_defaults_and_bounds() {
        for (body, expected) in [
            (
                "sum(range(4).map((x, i) => x + i).filter(x => x > 1))",
                "12",
            ),
            (r#""a,b,c".split(",").slice(1).join("-")"#, "b-c"),
            (r#""WHITE".toLowerCase()"#, "white"),
            (
                r#"match sortBy([2, 1], x => x).at(0) { case some(x) => x, case none => 0 }"#,
                "1",
            ),
            (
                r#"calendarDiff("2000-01-01", "2002-01-01", "years") == some(2)"#,
                "true",
            ),
            ("true ? 0.5 : sum(range(-1))", "0.5"),
            ("false && sum(range(-1)) > 0", "false"),
            ("true || sum(range(-1)) > 0", "true"),
            ("true ? 1 : performanceNow()", "1"),
            ("false ? unknownState : 1", "1"),
            (
                "match some(1) { case some(x) => x, case none => unknownResource }",
                "1",
            ),
            ("true || performanceNow() > 0", "true"),
            ("false && hostCall()", "false"),
            ("sum([0].map(i => i == 0 ? 0.5 : performanceNow()))", "0.5"),
            ("sum([0].map(i => i == 0 ? 0.5 : unknownState))", "0.5"),
            ("sum([0].map(i => i == 0 ? 0.5 : unknownResource))", "0.5"),
            (r#"true ? "0% 0%" : t("key")"#, "0% 0%"),
            (
                r#"[0].map(i => i == 0 ? "0% 0%" : t("key")).join("")"#,
                "0% 0%",
            ),
        ] {
            let file =
                contract_syntax::parse(&format!("fn value(): string = `${{{body}}}`\n")).unwrap();
            assert_eq!(
                constant(&file.fns[0].body, &file.fns).ok().as_deref(),
                Some(expected),
                "{body}"
            );
        }
        for body in [
            "performanceNow()",
            "unknownState",
            "unknownResource",
            "sum(range(65537))",
            "sum(range(-1))",
        ] {
            let file = contract_syntax::parse(&format!("fn value(): number = {body}\n")).unwrap();
            assert_eq!(constant(&file.fns[0].body, &file.fns).ok(), None, "{body}");
        }
    }

    #[test]
    fn function_annotations_type_empty_lists_and_options() {
        for (source, expected) in [
            (
                "fn slot(xs: string[]): string = xs.map(x => x.toUpperCase()).join(\"\")\nfn value(): string = slot([])\n",
                "",
            ),
            (
                "fn slot(x: option<string>): string = match x { case some(s) => s.toUpperCase(), case none => \"white\" }\nfn value(): string = slot(none)\n",
                "white",
            ),
            (
                "fn cells(): string[] = []\nfn value(): string = cells().map(x => x.toUpperCase()).join(\"\")\n",
                "",
            ),
            (
                "fn cell(): option<string> = none\nfn value(): string = match cell() { case some(s) => s.toUpperCase(), case none => \"white\" }\n",
                "white",
            ),
        ] {
            let file = contract_syntax::parse(source).unwrap();
            assert_eq!(
                constant(&file.fns.last().unwrap().body, &file.fns).ok().as_deref(),
                Some(expected),
                "{source}"
            );
        }
    }

    #[test]
    fn a_constant_table_lookup_folds_through_an_option_match() {
        let file = contract_syntax::parse(
            r#"fn slot(i: number): string = match ["0% 0%", "100% 0%", "200% 0%"].at(i) { case some(x) => x, case none => "0% 0%" }
fn value(): string = slot(2)
"#,
        ).unwrap();
        assert_eq!(
            constant(&file.fns[1].body, &file.fns).ok(),
            Some("200% 0%".into())
        );
    }

    #[test]
    fn known_conditions_expand_only_the_selected_calls() {
        for (ty, initial, body, expected) in [
            ("number", "0.5", "x == 0 ? fP(0) : fP(1)", "0.5"),
            (
                "number",
                "0.5",
                "match [0, 1].at(x) { case some(i) => fP(i), case none => fP(0) }",
                "0.5",
            ),
            ("bool", "true", "x == 0 && fP(0) || fP(1)", "true"),
        ] {
            let mut source = format!("fn f0(x: number): {ty} = {initial}\n");
            for n in 1..=9 {
                let body = body.replace("fP", &format!("f{}", n - 1));
                source.push_str(&format!("fn f{n}(x: number): {ty} = {body}\n"));
            }
            for x in [0, 1, 8] {
                let file = contract_syntax::parse(&format!("{source}fn value(): {ty} = f9({x})\n"))
                    .unwrap();
                assert_eq!(
                    constant(&file.fns.last().unwrap().body, &file.fns)
                        .ok()
                        .as_deref(),
                    Some(expected),
                    "{body}: f9({x})"
                );
            }
        }
    }

    #[test]
    fn callback_locals_and_constant_arguments_keep_their_bindings() {
        let file = contract_syntax::parse(
            "fn choose(x: number, y: number): number = x == 0 ? y : y + 10\nfn value(): number = sum([0, 1].map(x => choose(x, 2)))\n",
        )
        .unwrap();
        assert_eq!(
            constant(&file.fns[1].body, &file.fns).ok(),
            Some("14".into())
        );
    }

    #[test]
    fn selected_branches_keep_their_inferred_item_types() {
        for value in [
            r#"(i == 0 ? [] : ["100% 0%"]).map(x => x.toUpperCase()).join("")"#,
            r#"(match some(i) { case some(x) => [], case none => ["100% 0%"] }).map(x => x.toUpperCase()).join("")"#,
            r#"match (i == 0 ? none : some("100% 0%")) { case some(x) => x.toUpperCase(), case none => "" }"#,
            r#"match (match some(i) { case some(x) => none, case none => some("100% 0%") }) { case some(x) => x.toUpperCase(), case none => "" }"#,
            r#"(i == 0 ? [] : cells()).map(x => x.toUpperCase()).join("")"#,
            r#"match (i == 0 ? none : cell()) { case some(x) => x.toUpperCase(), case none => "" }"#,
            r#"(i == 0 ? [[]] : [["100% 0%"]]).map(xs => xs.map(x => x.toUpperCase()).join("")).join("")"#,
        ] {
            let file = contract_syntax::parse(&format!(
                "fn cells(): string[] = [\"100% 0%\"]\nfn cell(): option<string> = some(\"100% 0%\")\nfn slot(i: number): string = {value} == \"\" ? \"0% 0%\" : \"100% 0%\"\nfn value(): string = slot(0)\n"
            ))
            .unwrap();
            assert_eq!(
                constant(&file.fns.last().unwrap().body, &file.fns)
                    .ok()
                    .as_deref(),
                Some("0% 0%"),
                "{value}"
            );
        }
    }

    #[test]
    fn callback_branches_evaluate_arguments_only_when_reached() {
        for body in [
            "sum([0].map(i => i == 0 ? 0.5 : id(sum(range(65537)))))",
            "sum([0].map(i => match [0.5].at(i) { case some(x) => x, case none => id(sum(range(65537))) }))",
            "[0].filter(i => i == 0 || id(sum(range(65537))) > 0).length / 2",
            "[0].filter(i => i != 0 && id(sum(range(65537))) > 0).length + 0.5",
            "sum([0].map(i => i == 0 ? 0.5 : (true ? id(sum(range(65537))) : 0)))",
            "sum([0].map(i => i == 0 ? 0.5 : (sum(range(65537)) > 0 ? 1 : 0)))",
            "sum([0].map(i => i == 0 ? 0.5 : (match range(65537).at(0) { case some(x) => x, case none => 0 })))",
            "sum([0].map(i => i == 0 ? 0.5 : (sum(range(65537)) > 0 && true ? 1 : 0)))",
            "sum([].map(i => id(sum(range(65537))))) + 0.5",
        ] {
            let file = contract_syntax::parse(&format!(
                "fn id(x: number): number = x\nfn slot(): number = {body}\n"
            ))
            .unwrap();
            assert_eq!(
                constant(&file.fns[1].body, &file.fns).ok().as_deref(),
                Some("0.5"),
                "{body}"
            );
        }
    }

    #[test]
    fn repeated_palette_calls_have_a_total_work_bound() {
        let mut source = String::from("fn f0(): number = 1\n");
        for n in 1..=12 {
            let p = n - 1;
            source.push_str(&format!("fn f{n}(): number = f{p}() + f{p}() + f{p}()\n"));
        }
        let file = contract_syntax::parse(&source).unwrap();
        assert_eq!(constant(&file.fns[12].body, &file.fns).ok(), None);
        assert_eq!(
            constant(&file.fns[1].body, &file.fns).ok(),
            Some("3".into())
        );

        let file = contract_syntax::parse(
            "fn add(x: number, y: number): number = x + y\nfn value(): number = add(sum(range(20000)), sum(range(20000)))\n",
        )
        .unwrap();
        assert_eq!(constant(&file.fns[1].body, &file.fns).ok(), None);
    }
}
