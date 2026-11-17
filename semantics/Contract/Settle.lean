/-
Values at run time and settlement: the checks the runner makes at every
boundary (`conforms`, `typed`), the data seam a source's answer crosses
(`Oracle`), and the settlement of derives and resources against the
slots. `Contract.Runtime` builds the steps on them.
-/
import Contract.Tabs

namespace Contract
/-! ## Types at run time -/

mutual
/-- Whether a value has a type: what the runner checks at every boundary
(a slot written, a derive settled, a source's answer, an action's
argument). A source's arguments are checked for finiteness too. Infinities
and NaN live only inside an evaluation. A record is of the shape it names
(the runner's records carry no name; here the name selects field indices, so a
record of another name is not of this shape). -/
def conforms (p : Program) : Value → Ty → Bool
  | _, .unknown => true
  | .num f, .number => Number.isFinite f
  | .bool _, .bool => true
  | .str _, .string => true
  | .unit, .unit => true
  | .none, .option _ => true
  | .some v, .option t => conforms p v t
  | .list xs, .list t => conformsAll p xs t
  | .record s' vs, .record s =>
    match p.shapes.find? (·.name == s) with
    | .some sh => s' == s && vs.length == sh.fields.length && conformsFields p vs sh.fields
    | .none => false
  | _, _ => false
/-- Every item of a list has the type. -/
def conformsAll (p : Program) : List Value → Ty → Bool
  | [], _ => true
  | v :: vs, t => conforms p v t && conformsAll p vs t
/-- Each value has its field's type, pairwise (as far as both go). -/
def conformsFields (p : Program) : List Value → List Field → Bool
  | v :: vs, f :: fs => conforms p v f.ty && conformsFields p vs fs
  | _, _ => true
end

mutual
/-- Whether a value has a type, its numbers any (infinities and NaN
included): `conforms` without finiteness. -/
def typed (p : Program) : Value → Ty → Bool
  | _, .unknown => true
  | .num _, .number => true
  | .bool _, .bool => true
  | .str _, .string => true
  | .unit, .unit => true
  | .none, .option _ => true
  | .some v, .option t => typed p v t
  | .list xs, .list t => typedAll p xs t
  | .record s' vs, .record s =>
    match p.shapes.find? (·.name == s) with
    | .some sh => s' == s && vs.length == sh.fields.length && typedFields p vs sh.fields
    | .none => false
  | _, _ => false
def typedAll (p : Program) : List Value → Ty → Bool
  | [], _ => true
  | v :: vs, t => typed p v t && typedAll p vs t
def typedFields (p : Program) : List Value → List Field → Bool
  | v :: vs, f :: fs => typed p v f.ty && typedFields p vs fs
  | _, _ => true
end

/-- Why a value fails a boundary's check (`conforms`): when it is of the
type (`typed`), a number in it is not finite, which is refused (`finite`);
otherwise it is of another type (`wrong`), the type error a checked program
never makes. -/
def unfit (isTyped : Bool) (finite wrong : String) : Err :=
  if isTyped then .refused finite else .type wrong

/-- A hidden parameter: one the expansion adds to a child's lifted action
for a prop or inject it captures (`@capture:…`; `@` begins no authored
name). Its argument is the compiler's, not the host's. -/
def hiddenParam (x : String) : Bool := x.startsWith "@"

/-- An action's argument check: an authored parameter's argument crosses
the boundary (`conforms`, its numbers finite); a hidden parameter's has
its type, its numbers any, as the prop it captures would be read in place. -/
def argOk (p : Program) : String × Ty → Value → Bool
  | (x, t), v => if hiddenParam x then typed p v t else conforms p v t

/-- A root slot's declared type: a state's, or `option<T>` for a mutation
`T` (what a write to it is checked against). -/
def slotTy (p : Program) (x : String) : Ty :=
  match p.states.find? (·.name == x) with
  | .some s => s.ty
  | .none => ((p.mutations.find? (·.name == x)).map (fun m => Ty.option m.ty)).getD .unknown

/-! ## The data seam -/

/-- Bit-for-bit sameness (NaN is itself; records by fields). -/
partial def Value.same : Value → Value → Bool
  | .num a, .num b => a.toBits == b.toBits
  | .bool a, .bool b => a == b
  | .str a, .str b => a == b
  | .unit, .unit => true
  | .none, .none => true
  | .some a, .some b => Value.same a b
  | .list xs, .list ys => xs.length == ys.length && (xs.zip ys).all fun (a, b) => Value.same a b
  | .record _ xs, .record _ ys => xs.length == ys.length && (xs.zip ys).all fun (a, b) => Value.same a b
  | _, _ => false

/-- The data sources: a function of (source, arguments). The semantics is
parametric in it; a differential run supplies the answers the runner's
source gave, as a table. -/
structure Oracle where
  answers : List (String × List Value × Value) := []
  /-- Host facts, by resource: what a resource whose source the host
  answers itself (`exactViewport`, `exactPage`, …) holds. -/
  facts : List (String × Value) := []
  /-- The device's store at launch (LLP 1116 D5): each persisted state's
  kept value, by the state's name, as the host read it before boot — the
  device's, like every answer here. A fresh install's is empty. -/
  stored : List (String × Value) := []
  /-- The sources a native module answers (LLP 1133.002 D13): the runner
  answers them from the module's substitute, whose state (a permission, a
  preferred value) is its own, so a call to one with no answer and no fact
  is outside the semantics, not a refusal. -/
  modules : List String := []
  /-- The sources a program declares (LLP 1162.001): the runner makes their
  HTTP request itself and never asks the oracle, so a call to one with no
  answer and no fact is outside the semantics, not a refusal. -/
  fetches : List String := []
  /-- The host built every tab panel at boot and builds each as it is made
  (LLP 1075.003 §9.14: a page rendered ahead of its runtime, which the
  runtime adopts whole; `Runner::build_every_tab`). -/
  everyTab : Bool := false
  deriving Inhabited

mutual
/-- Source arguments carry only finite numbers, including in containers (LLP 1027 D2). -/
private def finiteValue : Value → Bool
  | .num f => Number.isFinite f
  | .some v => finiteValue v
  | .list vs | .record _ vs => finiteValues vs
  | _ => true
private def finiteValues : List Value → Bool
  | [] => true
  | v :: vs => finiteValue v && finiteValues vs
end

/-- The differential oracle's numeric keys (`difftest/src/oracle.rs`)
use canonical bits; signed zeros remain distinct. -/
private partial def oracleSame : Value → Value → Bool
  | .num a, .num b => Number.canonicalBits a == Number.canonicalBits b
  | .some a, .some b => oracleSame a b
  | .list xs, .list ys => xs.length == ys.length && (xs.zip ys).all fun (a, b) => oracleSame a b
  | .record _ xs, .record _ ys => xs.length == ys.length && (xs.zip ys).all fun (a, b) => oracleSame a b
  | a, b => Value.same a b

def Oracle.ask (o : Oracle) (src : String) (args : List Value) (resource : String := "") :
    Result Value :=
  if !finiteValues args then .error (.refused "a source argument contains a non-finite number") else
  match o.answers.find? (fun (s, as, _) =>
      s == src && as.length == args.length && (as.zip args).all fun (a, b) => oracleSame a b) with
  | .some (_, _, v) => .ok v
  | .none =>
    match lookup resource o.facts with
    | .some v => .ok v
    | .none =>
      if src = "exactTransport" ∨ src = "exactMIDI" then
        .error (.unsupported s!"`{src}`: the runner answers it from its transport or its MIDI ports, which change as they play (LLP 1125)")
      else if o.modules.contains src then
        .error (.unsupported s!"`{src}`: a native module's, which the runner answers from the module's substitute (LLP 1133.002 D13)")
      else if o.fetches.contains src then
        .error (.unsupported s!"`{src}`: a declared source's, whose HTTP request the runner makes itself (LLP 1162.001)")
      else .error (.refused s!"the oracle has no answer for {src}")

/-! ## Settlement -/

structure Settled where
  derives : List (String × Value) := []
  resources : List (String × Value) := []
  /-- The arguments each resource's value answers. An idle resource has
  none: its value answers nothing now. -/
  args : List (String × List Value) := []
  /-- The resources whose `when` was false (LLP 1161.002 D2). -/
  idle : List String := []
  deriving Inhabited

/-- An unrefined type's zero: `0`, `""`, `false`, `()`, `none`, `[]`, and
a record of its fields' zeros. The embedding carries an enum's first
member in `ResourceDecl.placeholder` (LLP 1054.000.002). -/
def zeroOf (shapes : List Shape) : Nat → Ty → Value
  | _, .number => .num 0
  | _, .bool => .bool false
  | _, .string => .str ""
  | _, .unit => .unit
  | _, .option _ => .none
  | _, .list _ => .list []
  | _, .unknown => .unit
  | 0, .record s => .record s []
  | n + 1, .record s =>
    match shapes.find? (·.name == s) with
    | .some sh => .record s (sh.fields.map fun f => zeroOf shapes n f.ty)
    | .none => .record s []

/-- A resource's value before its first answer (LLP 1054.000.002). -/
def placeholderValue (p : Program) (r : ResourceDecl) : Value :=
  r.placeholder.getD (zeroOf p.shapes (p.shapes.length + 1) r.ty)

/-- What an idle resource holds (LLP 1161.002 D2, D5): its previous
value, else its placeholder. -/
def idleValue (p : Program) (prev : Settled) (r : ResourceDecl) : Value :=
  (lookup r.name prev.resources).getD (placeholderValue p r)

/-- Whether a resource's `when` holds (LLP 1161.002 D1): with none it
always does; a value other than `true` closes it, as the runner reads it. -/
def gateOpen (env : Env) (r : ResourceDecl) : Result Bool :=
  match r.gate with
  | .none => .ok true
  | .some g => do
    match ← eval fuel env false [] g with
    | .bool true => pure true
    | _ => pure false

/-- The runner's `==` on values (`exact_plan::Value`'s `PartialEq`):
numbers by IEEE `==` (`-0 = 0`, `NaN ≠ NaN`), the rest structurally. Two
argument lists equal under it ask one question. -/
partial def Value.rustEq : Value → Value → Bool
  | .num a, .num b => a == b
  | .bool a, .bool b => a == b
  | .str a, .str b => a == b
  | .unit, .unit => true
  | .none, .none => true
  | .some a, .some b => Value.rustEq a b
  | .list xs, .list ys => xs.length == ys.length && (xs.zip ys).all fun (a, b) => Value.rustEq a b
  | .record _ xs, .record _ ys =>
    xs.length == ys.length && (xs.zip ys).all fun (a, b) => Value.rustEq a b
  | _, _ => false

def argsEq (xs ys : List Value) : Bool :=
  xs.length == ys.length && (xs.zip ys).all fun (a, b) => Value.rustEq a b

/-- A row key as the runner identifies it (`key_text`): a string, a bool,
or a finite number with `-0` read as `0`. -/
def rowKey : Value → Result Value
  | .str t => .ok (.str t)
  | .bool b => .ok (.bool b)
  | .num f =>
    if Number.isFinite f then .ok (.num (if f == 0 then 0 else f))
    else .error (.refused "a row key that is not finite")
  | _ => .error (.refused "a row key that is not a string, number or bool")

/-! ### A resource over a list (LLP 1017.000 P4c, amended 2026-11-04) -/

/-- Whether `h`, an element's request as settlement holds it
(`.list [key, .list args, answer]`), is keyed `k`. -/
def heldKeyed (k : Value) : Value → Bool
  | .list [k', _, _] => Value.same k k'
  | _ => false

/-- A held request's answer. -/
def heldAnswer : Value → Value
  | .list [_, _, v] => v
  | v => v

/-- A held request's answer when it was asked at `args`. -/
def heldAt (args : List Value) : Value → Option Value
  | .list [_, .list old, v] => if argsEq old args then .some v else .none
  | _ => .none

/-- An element's key before `rowKey` reads it: its `key=`, read with the
element bound to `e.item`, or the element itself. -/
def eachKey (env : Env) (e : ResourceEach) (x : Value) : Result Value :=
  match e.key with
  | .some k => eval fuel env false [(e.item, x)] k
  | .none => .ok x

/-- An element's answer: the one held for it, or the oracle's. -/
def eachAnswer (o : Oracle) (r : ResourceDecl) (args : List Value) : Option Value → Result Value
  | .some v => .ok v
  | .none => o.ask r.source args r.name

/-- The elements' requests, in the list's order, as the runner's members
(`runner/src/runner/each.rs`): each element's key (`rowKey`), then, for a
key not yet seen this pass, its arguments, read with the element bound to
`e.item`, and an answer: the one an earlier settlement held for that key
at equal arguments, unless forced, or the oracle's. An element whose key
repeats an earlier one's reads that one's answer. -/
def settleEachGo (p : Program) (o : Oracle) (env : Env) (r : ResourceDecl) (e : ResourceEach)
    (prev : List Value) (forced : Bool) :
    List Value → List Value → List Value → Result (List Value × List Value)
  | [], out, held => .ok (out, held)
  | x :: xs, out, held => do
    let k ← rowKey (← eachKey env e x)
    match held.find? (heldKeyed k) with
    | .some h => settleEachGo p o env r e prev forced xs (out ++ [heldAnswer h]) held
    | .none =>
      let args ← evalList fuel env false [(e.item, x)] r.args
      let v ← eachAnswer o r args
        (if forced then Option.none else (prev.find? (heldKeyed k)).bind (heldAt args))
      settleEachGo p o env r e prev forced xs (out ++ [v]) (held ++ [.list [k, .list args, v]])

/-- A resource over a list: its answers in the list's order, and its
elements' requests, which the next settlement reads as `prev`. -/
def settleEach (p : Program) (o : Oracle) (env : Env) (r : ResourceDecl) (e : ResourceEach)
    (prev : List Value) (forced : Bool) : Result (List Value × List Value) := do
  match ← eval fuel env false [] e.list with
  | .list xs => settleEachGo p o env r e prev forced xs [] []
  | _ => .error (.refused s!"resource `{r.name}`: its `for` reads no list")

/-- Settle every derive and resource against the slots: evaluate each
whose inputs have settled, until all have. A derive or resource that reads
one not yet settled waits (`pending`); a pass with no progress is a cycle.
The result does not depend on the order of evaluation (the dependency
graph is acyclic, and each value is a function of the values it reads).

A resource whose arguments equal (`argsEq`) those its previous value
answered keeps that value and asks nothing, unless the commit refreshes it
(`force`): its source is asked only when the question changes.

A resource whose `when` is false is idle (LLP 1161.002 D2): its arguments
are not read and nothing is asked, even when the commit refreshes it; it
keeps its previous value, or its placeholder before its first answer,
with no arguments, so the settlement that opens the gate asks again (D3).

A child's resource (LLP 1035.005.004 D1) is asked in the render, by each
instance (`settleOwned`): here it is idle, so the root holds its
placeholder, which no root name reads. A late one, a child's used outside
every region, is idle until `lateReady`: at boot, until the late slots it
reads are initialized (D2). -/
def settle (p : Program) (o : Oracle) (slots : List (String × Value)) (now : F64)
    (prev : Settled := {}) (force : List String := []) (lateReady : Bool := true) : Result Settled :=
  let rec pass : Nat → Settled → Result Settled
    | 0, _ => .error (.refused "settlement did not converge")
    | n + 1, st => do
      let mut st := st
      let mut progress := false
      let mut waiting := false
      for d in p.derives do
        if (lookup d.name st.derives).isSome then continue
        let env : Env := { prog := p, slots, derives := st.derives, resources := st.resources, now, idle := st.idle }
        match eval fuel env false [] d.body with
        | .ok v =>
          if !conforms p v d.ty then
            throw (unfit (typed p v d.ty) s!"derive `{d.name}` holds a number that is not finite"
              s!"derive `{d.name}` of the wrong type")
          st := { st with derives := st.derives ++ [(d.name, v)] }
          progress := true
        | .error .pending => waiting := true
        | .error e => throw e
      for r in p.resources do
        if (lookup r.name st.resources).isSome then continue
        let env : Env := { prog := p, slots, derives := st.derives, resources := st.resources, now, idle := st.idle }
        match r.each with
        | .some e =>
          let held := ((prev.args.find? (·.1 == r.name)).map (·.2)).getD []
          match settleEach p o env r e held (force.contains r.name) with
          | .ok (vs, held) =>
            if !conforms p (.list vs) r.ty then throw (.refused s!"resource `{r.name}` of the wrong shape")
            st := { st with resources := st.resources ++ [(r.name, .list vs)], args := st.args ++ [(r.name, held)] }
            progress := true
          | .error .pending => waiting := true
          | .error err => throw err
        | .none =>
        match (if r.owner.isSome || (r.late && !lateReady) then Except.ok false else gateOpen env r) with
        | .error .pending => waiting := true
        | .error e => throw e
        | .ok false =>
          if !conforms p (idleValue p prev r) r.ty then
            throw (.refused s!"resource `{r.name}` of the wrong shape")
          st := { st with resources := st.resources ++ [(r.name, idleValue p prev r)], idle := st.idle ++ [r.name] }
          progress := true
        | .ok true =>
        match evalList fuel env false [] r.args with
        | .ok args =>
          let kept : Option (Value × List Value) :=
            match lookup r.name prev.resources, prev.args.find? (·.1 == r.name) with
            | .some v, .some (_, old) =>
              if !force.contains r.name && argsEq old args then Option.some (v, old) else Option.none
            | _, _ => Option.none
          let (v, held) ← match kept with
            | .some k => pure k
            | .none => do
              let v ← o.ask r.source args r.name
              if !conforms p v r.ty then throw (.refused s!"resource `{r.name}` of the wrong shape")
              pure (v, args)
          st := { st with resources := st.resources ++ [(r.name, v)], args := st.args ++ [(r.name, held)] }
          progress := true
        | .error .pending => waiting := true
        | .error e => throw e
      if !waiting then return st
      if !progress then throw (.refused "a cycle among derives and resources")
      pass n st
  pass (p.derives.length + p.resources.length + 2) {}

end Contract
