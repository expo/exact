//! The expression VM.
//!
//! @ref LLP 1004 D2 (`opcodes.json` is the one authority) / LLP 0508 §4.3
//! (closed expressions; research)
//!
//! A stack machine over [`Value`]s. Type inference in the compiler guarantees
//! stack discipline for compiled plans; the VM still checks everything and
//! traps with a typed reason, because a plan is data from outside and never
//! trusted. There is no closure, no heap of the VM's own, no ambient read that
//! is not an operand: `performanceNow()` reads the clock the runner passes in.

mod column;
pub use column::Column;
mod constants;
mod decoded;
pub(crate) use constants::Constants;
mod list_call;
mod prepared;
use decoded::Decoded;
use list_call::list_call;
pub(crate) use prepared::Prepared;

use crate::machine::{self, Host, Kind, Machine, Num, Val};
pub use crate::machine::{Instruction, Trap};
use crate::stdlib;
use exact_plan::bytes::Reader;
use exact_plan::{Opcode, Operand, Plan, Stdlib, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// The longest string, in UTF-8 bytes, an expression may build or an
/// action may take or keep: doubling a string every few instructions
/// otherwise outruns memory within one evaluation.
pub const MAX_STRING: usize = 1 << 26;

/// The most values a list, record or option the VM builds may hold, a shared
/// one counted once per place it appears: that expanded tree is what
/// equality, shape checks and conversions walk. A list doubled through a
/// local (`[x, x]`, then again) is otherwise exponential in instructions.
/// A `some` is not counted (LLP 1090 D2): the checker refuses an option
/// directly inside an option, so a walk meets at most one per counted value.
pub const MAX_VALUE_NODES: u64 = 1 << 24;

/// The most string bytes such a value may hold, counted the same way: a
/// shared string repeated in a doubled list multiplies what encoding writes.
pub const MAX_VALUE_BYTES: u64 = MAX_STRING as u64;

/// The deepest such a value may nest: [`Value::decode`]'s bound, so that
/// whatever the VM builds can cross the data seam, and a value nested once
/// per action cannot grow past what walking or dropping it recurses through.
pub const MAX_VALUE_DEPTH: u32 = 64;

/// The most list steps one evaluation takes (LLP 1017.003 D3): each run of a
/// `map` or `filter` callback body is a step, nested runs each counted, and
/// each item `join` prints is a step. A body is straight-line code with
/// forward jumps, so this bounds how many instructions an evaluation runs.
pub const MAX_LIST_STEPS: u32 = 1 << 16;

/// Only extents at least this large are remembered: smaller ones cost less
/// to walk again than to look up.
const REMEMBERED: u64 = 64;

/// An instance's own slots — the values of the `state` a child component
/// declared, one set per keyed row or shown `if`/`match` arm (LLP 1017
/// P4c) — shared by the instance and every frame that reaches it, so a
/// read during an update and a write applied after an action's commit see
/// one storage.
pub type RowSlots = Rc<RefCell<BTreeMap<u32, Value>>>;

/// One instance scope: what a `for` row or a `match` arm binds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    /// The `for` item, when this scope is a keyed row.
    pub item: Option<Value>,
    /// The row's position in its list, from 0 (LLP 1062 D8).
    pub index: Option<usize>,
    /// The `match` binding, when this scope is a `some(x)` arm.
    pub bound: Option<Value>,
    /// The region this instance belongs to, including arms with no state.
    pub region: Option<u32>,
    /// The instance's slots, when this scope is a row or an arm owning some.
    pub row: Option<RowSlots>,
}

impl Frame {
    /// The row slots of the innermost frame belonging to `region`.
    pub fn row_of(frames: &[Frame], region: u32) -> Option<&RowSlots> {
        frames
            .iter()
            .rev()
            .find(|f| f.region == Some(region))
            .and_then(|f| f.row.as_ref())
    }
}

/// Everything an expression may read.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    /// The plan the code belongs to.
    pub plan: &'a Plan,
    /// The plan's strings as values ([`Strings`]), made as they are read.
    pub strings: &'a Strings,
    /// Checked route table and shapes; present only for a plan with a router.
    /// @ref LLP 1038 D3/D9 — the same table across all calls in this runner.
    pub router: Option<&'a dyn crate::runner::Routing>,
    /// The list engines, when this artifact links them (LLP 1047.000 §9).
    pub lists: Option<&'static crate::instance::ListLinks>,
    /// The `format` capability, when linked (LLP 1054.000.003 D8).
    pub format: crate::runner::FormatLink,
    /// Geometry reads: present only while an action's body runs
    /// (LLP 1051.000 D2).
    pub geometry: Option<&'a crate::geometry::GeometryEnv<'a>>,
    /// State slots by index.
    pub slots: &'a [Value],
    /// Derives by index; `None` while not yet settled this update.
    pub derives: &'a [Option<Value>],
    /// Resource values by index; `None` while not yet settled this update.
    pub resources: Column<'a, Option<crate::held::Held>>,
    /// Action parameters (empty outside an action).
    pub params: &'a [Value],
    /// Enclosing instance scopes, innermost last.
    pub frames: &'a [Frame],
    /// The clock, in milliseconds.
    pub now_ms: f64,
    /// Whether each resource has a request in flight (LLP 1016 D3).
    pub pending_resources: Column<'a, bool>,
    /// Whether each resource shows an answer to its call arguments (LLP 1161.001).
    pub current_resources: Column<'a, bool>,
    /// Arguments whose latest request failed, by resource (LLP 1054.000.002).
    pub failed_resources: Column<'a, Option<Vec<Value>>>,
    /// Why, while `failed_resources` holds (LLP 1109 D3); may be shorter.
    pub failed_why: Column<'a, Option<crate::failure::Failure>>,
    /// Whether each mutation has a request in flight.
    pub pending_mutations: &'a [bool],
    /// Latest mutation failure; cleared by a new send or assignment.
    pub failed_mutations: &'a [Option<crate::failure::Failure>],
    /// Store dependence of settled derives, for bake provenance propagation.
    pub store_dependent_derives: &'a [bool],
    /// Store dependence of settled resources, for bake provenance propagation.
    pub store_dependent_resources: Column<'a, bool>,
}

/// Take `n` list steps, or trap at `pc`.
fn step(steps: &mut u32, n: usize, pc: usize) -> Result<(), Trap> {
    *steps = steps.saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
    if *steps > MAX_LIST_STEPS {
        return Err(Trap::IterationLimit { pc });
    }
    Ok(())
}

/// A value's expanded extent: its values (a shared one counted once per
/// place), their string bytes, and how deep below it they nest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Extent {
    nodes: u64,
    bytes: u64,
    depth: u32,
}

impl Extent {
    fn scalar(v: &Value) -> Option<Extent> {
        match v {
            Value::List(_) | Value::Record(_) | Value::Option(Some(_)) => None,
            v if v.is_str() => Some(Extent {
                nodes: 1,
                bytes: v.text_len() as u64,
                depth: 0,
            }),
            _ => Some(Extent {
                nodes: 1,
                ..Extent::default()
            }),
        }
    }

    fn check(self, pc: usize) -> Result<Extent, Trap> {
        if self.depth > MAX_VALUE_DEPTH {
            Err(Trap::ValueTooDeep { pc })
        } else if self.nodes > MAX_VALUE_NODES || self.bytes > MAX_VALUE_BYTES {
            Err(Trap::ValueTooLarge { pc })
        } else {
            Ok(self)
        }
    }

    /// Add `part` as one child of a value being built.
    fn with(mut self, part: Extent, pc: usize) -> Result<Extent, Trap> {
        self.nodes += part.nodes;
        self.bytes += part.bytes;
        self.depth = self.depth.max(part.depth + 1);
        self.check(pc)
    }
}

/// The extents of the lists, records and options one evaluation met, by
/// allocation, so building from shared parts costs the parts' count and not
/// their expansion. Each entry pins its value: an address cannot be reused
/// for another while it is remembered.
#[derive(Default)]
struct Extents(exact_kernel::id::IdMap<usize, (Value, Extent)>);

impl Extents {
    fn key(v: &Value) -> Option<usize> {
        match v {
            Value::List(items) | Value::Record(items) => Some(items.addr()),
            Value::Option(Some(inner)) => Some(Rc::as_ptr(inner) as usize),
            _ => None,
        }
    }

    /// The extent of `v`, walked only if this evaluation has not measured
    /// that allocation, and only as far as the bounds.
    fn of(&mut self, v: &Value, pc: usize) -> Result<Extent, Trap> {
        if let Some(e) = Extent::scalar(v) {
            return e.check(pc);
        }
        let key = Self::key(v);
        if let Some((_, e)) = key.and_then(|k| self.0.get(&k)) {
            return Ok(*e);
        }
        let mut total = Extent::default();
        measure(v, 0, &mut total, pc)?;
        self.remember(v, total);
        Ok(total)
    }

    /// The extent of a list or record of `items`.
    fn built(&mut self, items: &[Value], pc: usize) -> Result<Extent, Trap> {
        let mut e = Extent {
            nodes: 1,
            ..Extent::default()
        };
        for item in items {
            e = e.with(self.of(item, pc)?, pc)?;
        }
        Ok(e)
    }

    fn remember(&mut self, v: &Value, e: Extent) {
        if e.nodes >= REMEMBERED {
            if let Some(k) = Self::key(v) {
                self.0.insert(k, (v.clone(), e));
            }
        }
    }
}

/// Count `v`, `depth` below where the walk began, into `total`; stops at the
/// first bound passed, so neither its time nor its recursion outgrows them.
fn measure(v: &Value, depth: u32, total: &mut Extent, pc: usize) -> Result<(), Trap> {
    #[cfg(test)]
    MEASURED.with(|m| m.set(m.get() + 1));
    total.depth = total.depth.max(depth);
    // A `Some` is not a value of its own (LLP 1090 D2), only a level.
    if let Value::Option(Some(inner)) = v {
        total.check(pc)?;
        return measure(inner, depth + 1, total, pc);
    }
    total.nodes += 1;
    match v {
        v if v.is_str() => total.bytes += v.text_len() as u64,
        Value::List(items) | Value::Record(items) => {
            total.check(pc)?;
            for item in items.iter() {
                measure(item, depth + 1, total, pc)?;
            }
        }
        _ => {}
    }
    total.check(pc).map(|_| ())
}

/// What a body produced: its value, slot writes in order, and commands.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// The value left on the stack (`Unit` for statement bodies).
    pub value: Value,
    /// `(slot, value)` writes in execution order.
    pub writes: Vec<(u32, Value)>,
    /// `(slot, value, row)` writes to row slots, with the row they belong to
    /// (LLP 1017 P4c).
    pub row_writes: Vec<(u32, Value, RowSlots)>,
    /// `(name, args)` commands in execution order.
    pub commands: Vec<(String, Vec<Value>)>,
    /// `(mutation, source, args)` sends in execution order (LLP 1016).
    pub sends: Vec<(u32, String, Vec<Value>)>,
    /// Resources to re-request with their current arguments.
    pub refreshes: Vec<u32>,
    /// The evaluated path read a value derived from the durable store.
    pub store_dependent: bool,
}

/// The plan's strings as values (`Env::strings`), each made when first read:
/// a plan has a thousand strings and a screen's bindings read a hundred of
/// them. A string read twice is the same object, inline text or one shared
/// allocation, never a fresh one.
pub struct Strings {
    cells: Vec<std::cell::OnceCell<Value>>,
    constants: Option<std::sync::Arc<Constants>>,
}

impl Strings {
    /// The table `plan`'s strings fill as they are read.
    pub fn new(plan: &Plan) -> Strings {
        let cells = (0..plan.strings.len()).map(|_| std::cell::OnceCell::new());
        Strings {
            cells: cells.collect(),
            constants: None,
        }
    }

    pub(crate) fn with_constants(plan: &Plan, constants: std::sync::Arc<Constants>) -> Self {
        Self {
            constants: (!constants.is_empty()).then_some(constants),
            ..Self::new(plan)
        }
    }

    /// A table holding `values` already (a test's: shorter than the plan's,
    /// or holding what is not text).
    pub fn of(values: impl IntoIterator<Item = Value>) -> Strings {
        Strings {
            cells: values.into_iter().map(std::cell::OnceCell::from).collect(),
            constants: None,
        }
    }

    /// The string with id `id`, as a value; `plan` is the plan the table was
    /// made for.
    pub fn get(&self, plan: &Plan, id: usize) -> Option<&Value> {
        let cell = self.cells.get(id)?;
        Some(cell.get_or_init(|| Value::str(&plan.strings[id])))
    }

    /// How many strings the table covers.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether it covers none.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

/// The plan's strings as values, for [`Env::strings`].
pub fn intern(plan: &Plan) -> Strings {
    Strings::new(plan)
}

/// Decode the instruction at `r`: the one decoder the VM, the dependency
/// table and the retained-binding grammar share.
#[inline]
pub fn decode(r: &mut Reader<'_>) -> Result<Instruction, Trap> {
    let pc = r.position();
    let malformed = |_| Trap::Malformed { pc };
    let op = Opcode::from_wire(r.u8().map_err(malformed)?).ok_or(Trap::Malformed { pc })?;
    let mut args = [0; 3];
    let mut number = 0.0;
    for (i, operand) in op.operands().iter().enumerate() {
        match operand {
            Operand::U8 | Operand::Enum(_) => args[i] = r.u8().map_err(malformed)? as u64,
            Operand::U16 => args[i] = r.u16().map_err(malformed)? as u64,
            Operand::U32 | Operand::Str | Operand::Idx(_) => {
                args[i] = r.u32().map_err(malformed)? as u64
            }
            Operand::F64 => number = r.f64().map_err(malformed)?,
        }
    }
    Ok(Instruction {
        pc,
        op,
        args,
        number,
    })
}

/// Every instruction of `code` in order, without following jumps; stops
/// after the first malformed one.
pub fn instructions(code: &[u8]) -> impl Iterator<Item = Result<Instruction, Trap>> + '_ {
    let mut r = Reader::new(code);
    let mut failed = false;
    std::iter::from_fn(move || {
        if failed || r.is_empty() {
            return None;
        }
        let next = decode(&mut r);
        failed = next.is_err();
        Some(next)
    })
}
/// Instructions reachable through at-read rows, each row scanned once.
/// This is a dependency walk; nested offsets remain local to their bodies.
pub fn transitive_instructions<'a>(
    plan: &'a Plan,
    code: &'a [u8],
) -> impl Iterator<Item = Result<Instruction, Trap>> + 'a {
    let mut bodies = vec![code];
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    while let Some(body) = bodies.pop() {
        for i in instructions(body) {
            if let Ok(i) = &i {
                if i.op == Opcode::LoadDerive {
                    if let Some(row) = plan.unsettled_body(i.args[0] as u32) {
                        if seen.insert(i.args[0]) {
                            bodies.push(plan.code(row.body));
                        }
                    }
                }
            }
            out.push(i);
        }
    }
    out.into_iter()
}

/// Whether `code` is a literal — pushes of values and `Return`, nothing
/// read — so a binding can be told from an expression without running it
/// (LLP 1035.002 D5: a row bound by an expression is `dynamic`). Malformed
/// code is not a literal.
pub fn is_literal(plan: &Plan, code: &[u8]) -> bool {
    transitive_instructions(plan, code).all(|i| {
        i.is_ok_and(|i| {
            matches!(
                i.op,
                Opcode::Number
                    | Opcode::Bool
                    | Opcode::Str
                    | Opcode::None
                    | Opcode::Unit
                    | Opcode::Some
                    | Opcode::Return
            ) || i.op == Opcode::LoadDerive && plan.unsettled_body(i.args[0] as u32).is_some()
        })
    })
}

/// Evaluate `code` in `env`. `allowed_writes` bounds `StoreSlot`; an action
/// passes its `writes` range, an expression passes nothing.
///
/// The interpreter is [`machine::run`], over [`Value`] and this environment.
/// An initial row-field read is fused before the remaining instructions run.
pub fn eval(code: &[u8], env: &Env<'_>, allowed_writes: &[u32]) -> Result<Outcome, Trap> {
    eval_reusing(code, env, allowed_writes, &mut Scratch::default())
}

/// Allocation storage shared by evaluations in one tree update. Values
/// never survive an evaluation, including one that traps inside a callback.
pub(crate) struct Scratch {
    machine: Machine<Value, Extent>,
    extents: Extents,
    decoded: Decoded,
}

impl Default for Scratch {
    fn default() -> Self {
        let mut machine = Machine::new();
        machine.stack = Vec::with_capacity(16);
        Self {
            machine,
            extents: Extents::default(),
            decoded: Decoded::default(),
        }
    }
}

pub(crate) fn eval_reusing(
    code: &[u8],
    env: &Env<'_>,
    allowed_writes: &[u32],
    scratch: &mut Scratch,
) -> Result<Outcome, Trap> {
    if let Some(value) = prepare_projection(code, env, scratch)? {
        return Ok(Outcome {
            value,
            ..Outcome::default()
        });
    }
    eval_machine(code, env, allowed_writes, scratch)
}

/// A binding consumes only the result, with no slot writes admitted.
/// Complete projections need no temporary effect vectors in an `Outcome`.
#[inline]
pub(crate) fn eval_value_reusing(
    code: &[u8],
    env: &Env<'_>,
    scratch: &mut Scratch,
) -> Result<Value, Trap> {
    if let Some(value) = prepare_projection(code, env, scratch)? {
        return Ok(value);
    }
    Ok(eval_machine(code, env, &[], scratch)?.value)
}

#[inline]
fn prepare_projection(
    code: &[u8],
    env: &Env<'_>,
    scratch: &mut Scratch,
) -> Result<Option<Value>, Trap> {
    if let Some(value) = field_projection(code, env) {
        let value = value?;
        if code.get(6..) == Some(&[Opcode::Return as u8]) {
            return Ok(Some(value));
        }
        if let Some(value) = projected_string_default(code, env, &value) {
            return Ok(Some(value));
        }
        // Preserve byte offsets for every jump and trap in the remaining body.
        scratch.machine.stack.push(value);
        scratch.machine.pos = 6;
    }
    Ok(None)
}

// A projected string passed to `s == literal ? replacement : s`, with the
// parameter bound once. Recognize the complete body, including both jump
// targets and the local's lifetime; every other body uses the interpreter.
fn projected_string_default(code: &[u8], env: &Env<'_>, value: &Value) -> Option<Value> {
    if !value.is_str() {
        return None;
    }
    let (literal, replacement) = prepared::string_default(code)?;
    let literal = literal as usize;
    let literal = env.strings.get(env.plan, literal)?;
    if !literal.is_str() {
        return None;
    }
    if value == literal {
        env.strings.get(env.plan, replacement as usize).cloned()
    } else {
        Some(value.clone())
    }
}

// The general interpreter remains the oracle for a projected row field.
fn eval_machine(
    code: &[u8],
    env: &Env<'_>,
    allowed_writes: &[u32],
    scratch: &mut Scratch,
) -> Result<Outcome, Trap> {
    let mut run = Run {
        env,
        code,
        allowed_writes,
        constants: env.strings.constants.as_deref().and_then(|pool| {
            let base = code
                .as_ptr()
                .addr()
                .checked_sub(env.plan.code.as_ptr().addr())?;
            (base.checked_add(code.len())? <= env.plan.code.len()).then_some((pool, base))
        }),
        cursor: Reader::new(code),
        out: Outcome::default(),
        extents: std::mem::take(&mut scratch.extents),
        decoded: std::mem::take(&mut scratch.decoded),
        steps: 0,
        calls: 0,
        depth: 0,
    };
    let value = machine::run(&mut run, &mut scratch.machine);
    scratch.machine.pos = 0;
    scratch.machine.stack.clear();
    scratch.machine.locals.clear();
    scratch.machine.callbacks.clear();
    run.extents.0.clear();
    scratch.extents = run.extents;
    run.decoded.clear();
    scratch.decoded = run.decoded;
    run.out.value = value?;
    Ok(run.out)
}

/// Read an initial row field without cloning its enclosing record. The rest
/// of the expression still runs on the interpreter with its original offsets.
fn field_projection(code: &[u8], env: &Env<'_>) -> Option<Result<Value, Trap>> {
    let &[load, dl, dh, field, il, ih] = code.get(..6)? else {
        return None;
    };
    if (load != Opcode::LoadItem as u8 && load != Opcode::LoadBound as u8)
        || field != Opcode::Field as u8
    {
        return None;
    }
    let depth = u16::from_le_bytes([dl, dh]);
    let index = u16::from_le_bytes([il, ih]);
    Some((|| {
        let frame = frame_at(env.frames, depth as usize, 0)?;
        let value = if load == Opcode::LoadItem as u8 {
            &frame.item
        } else {
            &frame.bound
        };
        let value = value.as_ref().ok_or(Trap::BadScope { pc: 0, depth })?;
        match value {
            Value::Record(fields) => fields
                .get(index as usize)
                .cloned()
                .ok_or(Trap::BadField { pc: 3, index }),
            _ => Err(Trap::TypeMismatch {
                pc: 3,
                op: Opcode::Field,
            }),
        }
    })())
}

fn frame_at(frames: &[Frame], depth: usize, pc: usize) -> Result<&Frame, Trap> {
    frames
        .len()
        .checked_sub(depth + 1)
        .and_then(|i| frames.get(i))
        .ok_or(Trap::BadScope {
            pc,
            depth: depth as u16,
        })
}

#[cfg(test)]
mod projection_tests;

/// The runner's value, as the machine sees it.
impl Val for Value {
    #[inline]
    fn kind(&self) -> Kind<'_, Value> {
        match self {
            Value::Number(n) => Kind::Number(*n),
            Value::Bool(b) => Kind::Bool(*b),
            exact_plan::str_value!() => Kind::Str,
            Value::Unit => Kind::Unit,
            Value::Option(None) => Kind::None,
            Value::Option(Some(v)) => Kind::Some(v),
            Value::List(items) => Kind::List(items),
            Value::Record(items) => Kind::Record(items),
        }
    }

    #[inline]
    fn number(n: f64) -> Value {
        Value::Number(n)
    }

    #[inline]
    fn index(n: usize) -> Value {
        Value::Number(n as f64)
    }

    #[inline]
    fn boolean(b: bool) -> Value {
        Value::Bool(b)
    }

    #[inline]
    fn unit() -> Value {
        Value::Unit
    }

    #[inline]
    fn none() -> Value {
        Value::NONE
    }

    #[inline]
    fn num(op: Num, a: f64, b: f64) -> Value {
        match op {
            Num::Add => Value::Number(a + b),
            Num::Sub => Value::Number(a - b),
            Num::Mul => Value::Number(a * b),
            Num::Div => Value::Number(a / b),
            Num::Rem => Value::Number(a % b),
            Num::Lt => Value::Bool(a < b),
            Num::Le => Value::Bool(a <= b),
            Num::Gt => Value::Bool(a > b),
            Num::Ge => Value::Bool(a >= b),
        }
    }

    #[inline]
    fn negate(a: f64) -> Value {
        Value::Number(-a)
    }

    #[inline]
    fn num_eq(a: f64, b: f64) -> bool {
        a == b
    }

    #[inline]
    fn str_eq(a: &Value, b: &Value) -> bool {
        a == b
    }

    fn str_cmp(op: Num, a: &Value, b: &Value) -> bool {
        let o = crate::strings::order(
            a.as_str().unwrap_or_default(),
            b.as_str().unwrap_or_default(),
        );
        match op {
            Num::Lt => o.is_lt(),
            Num::Le => o.is_le(),
            Num::Gt => o.is_gt(),
            _ => o.is_ge(),
        }
    }

    #[inline]
    fn concat(a: &Value, b: &Value) -> Option<Value> {
        let (a, b) = (a.as_str()?, b.as_str()?);
        if a.len() + b.len() > MAX_STRING {
            return None;
        }
        Some(stdlib::assembled(|s| {
            s.push_str(a);
            s.push_str(b);
        }))
    }
}

/// One evaluation's environment, effects and bounds: the machine's host.
struct Run<'e, 'a> {
    env: &'e Env<'a>,
    code: &'e [u8],
    allowed_writes: &'e [u32],
    /// Where the last instruction was read up to.
    cursor: Reader<'e>,
    constants: Option<(&'e Constants, usize)>,
    out: Outcome,
    extents: Extents,
    decoded: Decoded,
    steps: u32,
    calls: u32,
    depth: u16,
}

impl Host<Value> for Run<'_, '_> {
    type Extent = Extent;

    #[inline]
    fn fetch(&mut self, pos: usize) -> Result<Option<(Instruction, usize)>, Trap> {
        if pos >= self.code.len() {
            return Ok(None);
        }
        if let Some((pool, base)) = self.constants {
            if let Some(kept) = pool.get(base, pos, self.code.len()) {
                return Ok(Some(kept));
            }
        }
        if let Some(kept) = self.decoded.get(pos) {
            return Ok(Some(kept));
        }
        // Straight-line code reads on from the last instruction; a jump or a
        // callback's next item moves the cursor.
        if self.cursor.position() != pos {
            self.cursor = Reader::new(self.code);
            self.cursor
                .bytes(pos)
                .map_err(|_| Trap::Malformed { pc: pos })?;
        }
        let ins = decode(&mut self.cursor)?;
        let next = self.cursor.position();
        self.decoded.keep(pos, ins, next);
        Ok(Some((ins, next)))
    }

    #[inline]
    fn code_len(&self) -> usize {
        self.code.len()
    }

    fn string(&self, i: u64, pc: usize) -> Result<Value, Trap> {
        self.env
            .strings
            .get(self.env.plan, i as usize)
            .cloned()
            .ok_or(Trap::Malformed { pc })
    }

    fn load_slot(&self, i: u64, pc: usize) -> Result<Value, Trap> {
        let slot = i as usize;
        let env = self.env;
        let row = env.plan.slots.get(slot).ok_or(Trap::Malformed { pc })?;
        match env.plan.owner_region(row) {
            // An owned slot: the value the innermost instance of its region
            // (a row, an arm) holds.
            Some(region) => Frame::row_of(env.frames, region.0)
                .and_then(|r| r.borrow().get(&(slot as u32)).cloned())
                .ok_or(Trap::BadScope { pc, depth: 0 }),
            None => env.slots.get(slot).cloned().ok_or(Trap::Malformed { pc }),
        }
    }

    fn load_derive(&mut self, i: u64, pc: usize) -> Result<Value, Trap> {
        let derive = i as usize;
        let env = self.env;
        if let Some(row) = env.plan.unsettled_body(derive as u32) {
            self.calls += 1;
            if self.calls > 65_536 {
                return Err(Trap::CallLimit { pc });
            }
            if self.depth >= 64 {
                return Err(Trap::CallDepth { pc });
            }
            let frames = env
                .plan
                .owner_frames(env.frames, row.owner, |f| f.region)
                .ok_or(Trap::BadScope { pc, depth: 0 })?;
            let nested_env = Env {
                frames,
                params: &[],
                ..*env
            };
            let code = env.plan.code(row.body);
            let mut nested = Run {
                env: &nested_env,
                code,
                allowed_writes: &[],
                constants: env
                    .strings
                    .constants
                    .as_deref()
                    .map(|pool| (pool, row.body.offset as usize)),
                cursor: Reader::new(code),
                out: Outcome::default(),
                extents: std::mem::take(&mut self.extents),
                decoded: Decoded::default(),
                steps: self.steps,
                calls: self.calls,
                depth: self.depth + 1,
            };
            let value = machine::run(&mut nested, &mut Machine::default());
            self.steps = nested.steps;
            self.calls = nested.calls;
            self.extents = nested.extents;
            self.out.store_dependent |= nested.out.store_dependent;
            return value;
        }
        let value = env
            .derives
            .get(derive)
            .ok_or(Trap::Malformed { pc })?
            .clone()
            .ok_or(Trap::Pending { pc })?;
        self.out.store_dependent |= env
            .store_dependent_derives
            .get(derive)
            .copied()
            .unwrap_or(false);
        Ok(value)
    }

    fn load_resource(&mut self, i: u64, pc: usize) -> Result<Value, Trap> {
        let resource = i as usize;
        let env = self.env;
        let value = env
            .resources
            .get(resource)
            .ok_or(Trap::Malformed { pc })?
            .as_ref()
            .ok_or(Trap::Pending { pc })?
            .read(env.plan);
        self.out.store_dependent |= env
            .store_dependent_resources
            .get(resource)
            .copied()
            .unwrap_or(false);
        Ok(value)
    }

    fn load_param(&self, i: u64, pc: usize) -> Result<Value, Trap> {
        self.env
            .params
            .get(i as usize)
            .cloned()
            .ok_or(Trap::BadParam {
                pc,
                index: i as u16,
            })
    }

    fn load_frame(&self, op: Opcode, depth: u64, pc: usize) -> Result<Value, Trap> {
        let depth = depth as usize;
        let bad = Trap::BadScope {
            pc,
            depth: depth as u16,
        };
        let frame = frame_at(self.env.frames, depth, pc)?;
        match op {
            Opcode::LoadIndex => frame.index.map(|index| Value::Number(index as f64)),
            Opcode::LoadItem => frame.item.clone(),
            _ => frame.bound.clone(),
        }
        .ok_or(bad)
    }

    fn resource_flag(&self, op: Opcode, i: u64, pc: usize) -> Result<bool, Trap> {
        // Known once the resource settled this pass, like its value.
        let i = i as usize;
        let env = self.env;
        if env.resources.get(i).is_none_or(Option::is_none) {
            return Err(Trap::Pending { pc });
        }
        Ok(if op == Opcode::FailedResource {
            env.failed_resources.get(i).is_some_and(Option::is_some)
        } else if op == Opcode::CurrentResource {
            env.current_resources.get(i).copied().unwrap_or(false)
        } else {
            env.pending_resources.get(i).copied().unwrap_or(false)
        })
    }

    // @ref LLP 1109 D3 — `failure(x)`: known once settled, as `failed(x)`;
    // the `Failure` record is `{ code, message, reason }` by position.
    fn resource_failure(&mut self, i: u64, pc: usize) -> Result<Value, Trap> {
        if !self.resource_flag(Opcode::FailedResource, i, pc)? {
            return Ok(Value::Option(None));
        }
        let why = self.env.failed_why.get(i as usize).and_then(Option::as_ref);
        let (code, message, reason) = why.map_or(("error", "it failed", ""), |w| {
            (w.exposed_code(), w.message.as_str(), w.reason.as_str())
        });
        let record = Value::Record(
            [Value::str(code), Value::str(message), Value::str(reason)]
                .into_iter()
                .collect(),
        );
        self.some(record, pc)
    }

    #[inline]
    fn mutation_failed(&self, i: u64) -> bool {
        self.env
            .failed_mutations
            .get(i as usize)
            .is_some_and(Option::is_some)
    }

    fn mutation_failure(&mut self, i: u64, pc: usize) -> Result<Value, Trap> {
        let Some(why) = self
            .env
            .failed_mutations
            .get(i as usize)
            .and_then(Option::as_ref)
        else {
            return Ok(Value::Option(None));
        };
        self.some(
            Value::Record(
                [
                    Value::str(why.exposed_code()),
                    Value::str(&why.message),
                    Value::str(&why.reason),
                ]
                .into_iter()
                .collect(),
            ),
            pc,
        )
    }

    fn pending_mutation(&self, i: u64) -> bool {
        self.env
            .pending_mutations
            .get(i as usize)
            .copied()
            .unwrap_or(false)
    }

    // @ref LLP 1090 D2 — a `Some` adds depth and nothing else: the JS
    // target erases it, and `type-option-option` keeps at most one above
    // each counted value, so walks stay within twice the count.
    fn some(&mut self, v: Value, pc: usize) -> Result<Value, Trap> {
        let inner = self.extents.of(&v, pc)?;
        let e = Extent {
            depth: inner.depth + 1,
            ..inner
        }
        .check(pc)?;
        let v = Value::some(v);
        self.extents.remember(&v, e);
        Ok(v)
    }

    fn list(&mut self, items: Vec<Value>, pc: usize) -> Result<Value, Trap> {
        let e = self.extents.built(&items, pc)?;
        let v = Value::list(items);
        self.extents.remember(&v, e);
        Ok(v)
    }

    fn record_len(&self, ty: u64, _pc: usize) -> Result<usize, Trap> {
        let ty = exact_plan::TypesId(ty as u32);
        Ok(self.env.plan.type_(ty).fields.len as usize)
    }

    fn record(&mut self, _ty: u64, fields: Vec<Value>, pc: usize) -> Result<Value, Trap> {
        let e = self.extents.built(&fields, pc)?;
        let v = Value::record(fields);
        self.extents.remember(&v, e);
        Ok(v)
    }

    fn native_props(&mut self, pairs: Vec<Value>, pc: usize) -> Result<Value, Trap> {
        let json = stdlib::native_props(&pairs).ok_or(Trap::TypeMismatch {
            pc,
            op: Opcode::NativeProps,
        })?;
        if json.len() > MAX_STRING {
            return Err(Trap::StringTooLong { pc });
        }
        Ok(Value::str(&json))
    }

    fn arity(&self, f: u64, pc: usize) -> Result<usize, Trap> {
        let f = Stdlib::from_wire(f as u8).ok_or(Trap::Malformed { pc })?;
        Ok(f.arity())
    }

    fn call(&mut self, f: u64, stack: &[Value], at: usize, pc: usize) -> Result<Value, Trap> {
        let f = Stdlib::from_wire(f as u8).ok_or(Trap::Malformed { pc })?;
        let op = Opcode::Call;
        let call_args = &stack[at..];
        if f == Stdlib::Join {
            if let Some(Value::List(items)) = call_args.first() {
                step(&mut self.steps, items.len(), pc)?;
            }
            return match stdlib::join(call_args, MAX_STRING) {
                Ok(v) => Ok(v),
                Err(stdlib::JoinError::Type) => Err(Trap::TypeMismatch { pc, op }),
                Err(stdlib::JoinError::TooLong) => Err(Trap::StringTooLong { pc }),
            };
        }
        if let Some(v) = list_call(f, call_args, &mut self.steps, &mut self.extents, pc)? {
            return Ok(v);
        }
        let env = self.env;
        stdlib::call(
            f,
            call_args,
            env.now_ms,
            env.plan,
            env.router,
            env.format,
            env.geometry,
        )
        .map_err(|error| match error {
            stdlib::CallError::TypeMismatch
                if f == Stdlib::EncodeRouteSegment
                    && call_args.first().is_some_and(Value::is_str) =>
            {
                Trap::PathParameter { pc }
            }
            stdlib::CallError::TypeMismatch => Trap::TypeMismatch { pc, op },
            stdlib::CallError::StringTooLong => Trap::StringTooLong { pc },
        })
    }

    fn may_write(&self, slot: u64) -> bool {
        self.allowed_writes.contains(&(slot as u32))
    }

    fn store(&mut self, slot: u64, v: Value, pc: usize) -> Result<(), Trap> {
        let slot = slot as u32;
        let plan = self.env.plan;
        let row = plan
            .slots
            .get(slot as usize)
            .ok_or(Trap::Malformed { pc })?;
        match plan.owner_region(row) {
            Some(region) => {
                // An owned slot: written to the instance in force — an action
                // run with none (`act`, a timer) has none to write.
                let row = Frame::row_of(self.env.frames, region.0)
                    .ok_or(Trap::BadScope { pc, depth: 0 })?;
                self.out.row_writes.push((slot, v, row.clone()));
            }
            None => self.out.writes.push((slot, v)),
        }
        Ok(())
    }

    fn command(&mut self, name: u64, args: Vec<Value>) {
        let name = self
            .env
            .plan
            .str(exact_plan::StrId(name as u32))
            .to_string();
        self.out.commands.push((name, args));
    }

    fn mutation_slot(&self, m: u64, _pc: usize) -> Result<u64, Trap> {
        let id = exact_plan::MutationsId(m as u32);
        Ok(u64::from(self.env.plan.mutation(id).slot.0))
    }

    fn send(&mut self, m: u64, source: u64, args: Vec<Value>) {
        let source = self
            .env
            .plan
            .str(exact_plan::StrId(source as u32))
            .to_string();
        self.out.sends.push((m as u32, source, args));
    }

    fn refresh(&mut self, r: u64) {
        self.out.refreshes.push(r as u32);
    }

    /// A callback keeps instructions that miss the shared pool locally.
    #[inline]
    fn steps(&mut self, n: usize, pc: usize) -> Result<(), Trap> {
        self.decoded.start(self.code.len());
        step(&mut self.steps, n, pc)
    }

    #[inline]
    fn extent(&self) -> Extent {
        Extent {
            nodes: 1,
            ..Extent::default()
        }
    }

    #[inline]
    fn kept(&mut self, e: Extent, v: &Value, pc: usize) -> Result<Extent, Trap> {
        e.with(self.extents.of(v, pc)?, pc)
    }

    fn collected(&mut self, out: Vec<Value>, e: Extent) -> Value {
        let v = Value::list(out);
        self.extents.remember(&v, e);
        v
    }
}

#[cfg(test)]
thread_local! {
    /// Values `measure` walked on this thread: sizing, counted.
    static MEASURED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;

impl Trap {
    /// The bound in diagnostic text, after the unchanged trap token.
    pub fn bound(&self) -> &'static str {
        match self {
            Self::IterationLimit { .. } => ": past the 65,536 list steps one evaluation may take",
            Self::CallLimit { .. } => ": past the 65,536 derive calls one evaluation may take",
            Self::CallDepth { .. } => ": past 64 derive call levels",
            Self::StringTooLong { .. } => ": past the 64 MiB a string may hold",
            Self::ValueTooLarge { .. } => ": past the 16,777,216 nodes or 64 MiB a value may hold",
            Self::ValueTooDeep { .. } => ": past 64 levels",
            _ => "",
        }
    }
}
#[cfg(test)]
mod unsettled_tests;
