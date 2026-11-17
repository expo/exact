//! What of the viewport a plan reads (LLP 1048.006 D4), and the media
//! queries that tell a browser's viewport apart as the plan does (LLP
//! 1048.007 D2).
//!
//! Every code body is walked for `exactViewport` field reads. A field read
//! only to be compared with a constant (`viewport.width >= 640`) is
//! recorded as those comparisons; one read any other way (arithmetic, a
//! record passed whole) as its whole value. Two viewports are then one
//! class when every recorded reading gives the same answer for both, so a
//! page rendered for one is the page for the other.

use crate::viewport::{Viewport, FIELDS, SOURCE};
use crate::vm::instructions;
use exact_plan::{Code, Opcode, Plan, ResourcesId, StrId, TypeKind, Value};
use std::collections::{BTreeMap, BTreeSet};

/// How the plan reads one fact.
#[derive(Debug, Clone, PartialEq)]
enum Read {
    /// Only compared with constants: these comparisons, `fact <op> value`.
    Compared(Vec<(Opcode, Constant)>),
    /// Some other way: the page depends on its value.
    Whole,
}

/// A constant a fact is compared with.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// A number (a size).
    Number(f64),
    /// A keyword (`"dark"`, `"coarse"`).
    Str(String),
    /// A boolean preference.
    Bool(bool),
}

/// What of the viewport a plan reads, fact by fact (in [`FIELDS`]' order).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reads {
    facts: Vec<Option<Read>>,
}

/// One comparison the plan makes, as a media query that matches exactly
/// when the comparison holds in the browser's viewport.
#[derive(Debug, Clone, PartialEq)]
pub struct Media {
    /// The query, for `matchMedia`.
    pub query: String,
    /// The fact compared (one of [`FIELDS`]).
    pub fact: &'static str,
    /// The comparison, the fact on its left.
    pub op: Opcode,
    /// What the fact is compared with.
    pub constant: Constant,
}

/// An operand whose use may still be decided by an executable caller.
#[derive(Clone)]
enum Operand {
    Resource(usize),
    Fact(usize),
    Constant(Constant),
    Other,
}

impl Reads {
    /// Walk every code body: each `exactViewport` resource's field read is a
    /// comparison with a constant, or not.
    pub fn of(plan: &Plan) -> Reads {
        let mut reads = Reads {
            facts: vec![None; FIELDS.len()],
        };
        // Each viewport resource's fields: field index -> fact index.
        let fields: Vec<(ResourcesId, Vec<Option<usize>>)> = plan
            .resources
            .iter()
            .enumerate()
            .filter(|(_, row)| plan.str(row.source) == SOURCE)
            .map(|(i, row)| {
                let ty = plan.type_(row.ty);
                let names = if ty.kind == TypeKind::Record {
                    ty.fields
                        .iter()
                        .map(|f| {
                            let name = plan.str(plan.field(f).name);
                            FIELDS.iter().position(|fact| *fact == name)
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                (ResourcesId(i as u32), names)
            })
            .collect();
        if fields.is_empty() {
            return reads;
        }
        // Executable rows return operands to their callers. Scanning their
        // returns as independent uses would turn every shared field into Whole.
        // Dependencies precede readers, so summaries need no recursive expansion.
        let mut returned = vec![Operand::Other; plan.derives.len()];
        let mut entries = BTreeMap::new();
        plan.each_code(&mut |code| {
            *entries.entry((code.offset, code.len)).or_insert(0_usize) += 1;
        });
        for (at, row) in plan.derives.iter().enumerate() {
            if row.unsettled {
                returned[at] = reads.scan(plan, row.body, &fields, &returned);
                *entries.get_mut(&(row.body.offset, row.body.len)).unwrap() -= 1;
            }
        }
        for ((offset, len), uses) in entries {
            if uses != 0 {
                let result = reads.scan(plan, Code { offset, len }, &fields, &returned);
                reads.whole(result, &fields);
            }
        }
        reads
    }

    fn scan(
        &mut self,
        plan: &Plan,
        body: Code,
        fields: &[(ResourcesId, Vec<Option<usize>>)],
        returned: &[Operand],
    ) -> Operand {
        let Ok(code) = instructions(plan.code(body)).collect::<Result<Vec<_>, _>>() else {
            // A body the VM would refuse reads nothing; the runner never runs it.
            return Operand::Other;
        };
        let targets: BTreeSet<usize> = code
            .iter()
            .filter(|i| {
                matches!(
                    i.op,
                    Opcode::Jump
                        | Opcode::JumpIfFalse
                        | Opcode::JumpIfNone
                        | Opcode::Map
                        | Opcode::Filter
                )
            })
            .map(|i| i.args[0] as usize)
            .collect();
        let mut stack = Vec::new();
        let mut result = Operand::Other;
        for i in code {
            // Values on opposite sides of a control-flow edge cannot be paired.
            if targets.contains(&i.pc) {
                self.flush(&mut stack, fields);
            }
            let value = match i.op {
                Opcode::Number => Operand::Constant(Constant::Number(i.number)),
                Opcode::Str => {
                    Operand::Constant(Constant::Str(plan.str(StrId(i.args[0] as u32)).into()))
                }
                Opcode::Bool => Operand::Constant(Constant::Bool(i.args[0] != 0)),
                Opcode::LoadResource => fields
                    .iter()
                    .position(|(r, _)| r.0 as u64 == i.args[0])
                    .map_or(Operand::Other, Operand::Resource),
                Opcode::LoadDerive => returned[i.args[0] as usize].clone(),
                Opcode::Field => match stack.pop() {
                    Some(Operand::Resource(resource)) => fields[resource]
                        .1
                        .get(i.args[0] as usize)
                        .copied()
                        .flatten()
                        .map_or(Operand::Other, Operand::Fact),
                    value => {
                        self.whole(value.unwrap_or(Operand::Other), fields);
                        Operand::Other
                    }
                },
                op if comparison(op).is_some() => {
                    let right = stack.pop().unwrap_or(Operand::Other);
                    let left = stack.pop().unwrap_or(Operand::Other);
                    match (left, right) {
                        (Operand::Fact(fact), Operand::Constant(value)) => {
                            self.compared(fact, op, value);
                        }
                        (Operand::Constant(value), Operand::Fact(fact)) => {
                            self.compared(fact, mirror(op), value);
                        }
                        (left, right) => {
                            self.whole(left, fields);
                            self.whole(right, fields);
                        }
                    }
                    Operand::Other
                }
                Opcode::Return if targets.is_empty() => {
                    result = stack.pop().unwrap_or(Operand::Other);
                    self.flush(&mut stack, fields);
                    break;
                }
                _ => {
                    self.flush(&mut stack, fields);
                    Operand::Other
                }
            };
            stack.push(value);
        }
        self.flush(&mut stack, fields);
        result
    }

    fn flush(&mut self, stack: &mut Vec<Operand>, fields: &[(ResourcesId, Vec<Option<usize>>)]) {
        for value in stack.drain(..) {
            self.whole(value, fields);
        }
    }

    fn whole(&mut self, value: Operand, fields: &[(ResourcesId, Vec<Option<usize>>)]) {
        match value {
            Operand::Fact(fact) => self.facts[fact] = Some(Read::Whole),
            Operand::Resource(resource) => {
                for fact in fields[resource].1.iter().flatten() {
                    self.facts[*fact] = Some(Read::Whole);
                }
            }
            _ => {}
        }
    }

    fn compared(&mut self, fact: usize, op: Opcode, value: Constant) {
        let compared = (op, value);
        match &mut self.facts[fact] {
            Some(Read::Whole) => {}
            Some(Read::Compared(all)) => {
                if !all.contains(&compared) {
                    all.push(compared);
                }
            }
            slot => *slot = Some(Read::Compared(vec![compared])),
        }
    }

    /// Whether the plan reads any viewport fact.
    pub fn any(&self) -> bool {
        self.facts.iter().any(Option::is_some)
    }

    /// Whether the plan reads `fact` (one of [`FIELDS`]).
    pub fn reads(&self, fact: &str) -> bool {
        FIELDS
            .iter()
            .position(|f| *f == fact)
            .is_some_and(|at| self.facts[at].is_some())
    }

    /// Each fact the plan reads in `viewport`, as the plan can tell it
    /// apart: equal for two viewports of one class. Empty for a plan that
    /// reads no viewport.
    pub fn key(&self, viewport: &Viewport) -> String {
        let mut key = String::new();
        for (name, read) in FIELDS.iter().zip(&self.facts) {
            let Some(read) = read else { continue };
            let value = viewport.field(name).unwrap_or(Value::Unit);
            key.push_str(name);
            key.push('=');
            match read {
                Read::Whole => key.push_str(&format!("{value:?}")),
                Read::Compared(all) => {
                    for (op, constant) in all {
                        key.push(if holds(*op, &value, constant) {
                            '1'
                        } else {
                            '0'
                        });
                    }
                }
            }
            key.push(';');
        }
        key
    }

    /// Every comparison the plan makes, each as a media query, or `None`
    /// when some reading has none: a fact read whole, a fact no media
    /// feature carries (`gpuTier`, a fold's segments, a gamut, which a
    /// wider one also matches), or an order on a keyword. A fact the web
    /// holds constant needs none: `accessibilityTextSize` is false in every
    /// browser (`facts.js`), as in the render, so `exact:content`'s
    /// `EmptyState`, which reads it, leaves a page free to wait.
    pub fn media(&self) -> Option<Vec<Media>> {
        let mut out = Vec::new();
        for (fact, read) in FIELDS.iter().zip(&self.facts) {
            match read {
                None => {}
                Some(_) if *fact == "accessibilityTextSize" => {}
                Some(Read::Whole) => return None,
                Some(Read::Compared(all)) => {
                    for (op, constant) in all {
                        out.push(Media {
                            query: query(fact, *op, constant)?,
                            fact,
                            op: *op,
                            constant: constant.clone(),
                        });
                    }
                }
            }
        }
        Some(out)
    }
}

impl Media {
    /// Whether the comparison holds in `viewport`, as the VM compares.
    pub fn holds(&self, viewport: &Viewport) -> bool {
        holds(
            self.op,
            &viewport.field(self.fact).unwrap_or(Value::Unit),
            &self.constant,
        )
    }
}

/// The media query that matches exactly when `fact <op> constant` holds, as
/// the web host reads each fact (`navigation.js` `preferences`, `facts.js`).
fn query(fact: &str, op: Opcode, constant: &Constant) -> Option<String> {
    let not = |q: String, holds_when_matches: bool| {
        if holds_when_matches {
            q
        } else {
            format!("not all and {q}")
        }
    };
    let eq = match op {
        Opcode::Eq => true,
        Opcode::Ne => false,
        _ => {
            // Only a size is ordered: `width` is `innerWidth`, the layout
            // viewport with any scrollbar, as the media feature measures.
            let Constant::Number(n) = constant else {
                return None;
            };
            if !matches!(fact, "width" | "height") {
                return None;
            }
            let op = match op {
                Opcode::Lt => "<",
                Opcode::Le => "<=",
                Opcode::Gt => ">",
                _ => ">=",
            };
            return Some(format!("({fact} {op} {n}px)"));
        }
    };
    let feature = |name: &str, value: &str, values: &[&str]| {
        values
            .contains(&value)
            .then(|| format!("({name}: {value})"))
    };
    match (fact, constant) {
        ("width" | "height", Constant::Number(n)) => Some(not(format!("({fact} = {n}px)"), eq)),
        ("prefersReducedMotion", Constant::Bool(b)) => {
            Some(not("(prefers-reduced-motion: reduce)".into(), eq == *b))
        }
        ("prefersReducedTransparency", Constant::Bool(b)) => Some(not(
            "(prefers-reduced-transparency: reduce)".into(),
            eq == *b,
        )),
        ("prefersColorScheme", Constant::Str(s)) => match s.as_str() {
            "dark" => Some(not("(prefers-color-scheme: dark)".into(), eq)),
            "light" => Some(not("(prefers-color-scheme: dark)".into(), !eq)),
            _ => None,
        },
        ("prefersContrast", Constant::Str(s)) => feature(
            "prefers-contrast",
            s,
            &["no-preference", "more", "less", "custom"],
        )
        .map(|q| not(q, eq)),
        ("pointer", Constant::Str(s)) => {
            feature("pointer", s, &["fine", "coarse", "none"]).map(|q| not(q, eq))
        }
        ("hover", Constant::Str(s)) => feature("hover", s, &["hover", "none"]).map(|q| not(q, eq)),
        ("displayMode", Constant::Str(s)) => feature(
            "display-mode",
            s,
            &[
                "standalone",
                "browser",
                "fullscreen",
                "minimal-ui",
                "window-controls-overlay",
                "picture-in-picture",
                "tabbed",
            ],
        )
        .map(|q| not(q, eq)),
        _ => None,
    }
}

/// A comparison opcode, if `op` is one.
fn comparison(op: Opcode) -> Option<Opcode> {
    matches!(
        op,
        Opcode::Lt | Opcode::Le | Opcode::Gt | Opcode::Ge | Opcode::Eq | Opcode::Ne
    )
    .then_some(op)
}

/// `a <op> b` as `b <mirror(op)> a`.
fn mirror(op: Opcode) -> Opcode {
    match op {
        Opcode::Lt => Opcode::Gt,
        Opcode::Le => Opcode::Ge,
        Opcode::Gt => Opcode::Lt,
        Opcode::Ge => Opcode::Le,
        other => other,
    }
}

/// Whether `value <op> constant` holds, as the VM compares.
fn holds(op: Opcode, value: &Value, constant: &Constant) -> bool {
    let order = match (value, constant) {
        (Value::Number(a), Constant::Number(b)) => a.partial_cmp(b),
        (Value::Bool(a), Constant::Bool(b)) => Some(a.cmp(b)),
        (_, Constant::Str(b)) => value.as_str().map(|a| a.cmp(b.as_str())),
        _ => None,
    };
    let Some(order) = order else {
        return op == Opcode::Ne;
    };
    match op {
        Opcode::Lt => order.is_lt(),
        Opcode::Le => order.is_le(),
        Opcode::Gt => order.is_gt(),
        Opcode::Ge => order.is_ge(),
        Opcode::Eq => order.is_eq(),
        _ => order.is_ne(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_plan::{asm::Asm, builder::PlanBuilder};

    #[test]
    fn a_straight_line_body_returns_its_first_return_operand() {
        for executable in [false, true] {
            let mut b = PlanBuilder::new(0, 1);
            let number = b.primitive(TypeKind::Number);
            let boolean = b.primitive(TypeKind::Bool);
            let viewport = b.record("Viewport", &[("width", number)]);
            let resource = b.resource("viewport", SOURCE, &[], viewport, None);
            let mut a = Asm::default();
            a.load_resource(resource)
                .field(0)
                .op(Opcode::Return, &[])
                .number(640.0);
            let body = b.code(a);
            let row = b.derive("width", number, body);
            if executable {
                b.set_derive_unsettled(row, None);
                let mut a = Asm::default();
                a.load_derive(row).number(640.0).op(Opcode::Ge, &[]);
                let body = b.code(a);
                b.derive("wide", boolean, body);
            }
            let reads = Reads::of(&b.finish().unwrap());
            assert!(reads.reads("width"));
            if executable {
                assert_eq!(reads.media().unwrap()[0].query, "(width >= 640px)");
            } else {
                assert!(reads.media().is_none());
                assert_ne!(
                    reads.key(&Viewport::sized(800.0, 900.0)),
                    reads.key(&Viewport::sized(900.0, 900.0))
                );
            }
        }
    }
}
