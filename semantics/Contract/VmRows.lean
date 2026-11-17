/-
Executable derive calls over the expression VM. The caller keeps its machine;
the callee starts fresh and shares the evaluation's calls, list steps and
structural extent accounting. The wrapper leaves `Vm.step` unchanged and adds
the production host's charges at the same instruction boundaries.
-/
import Contract.Vm

namespace Contract.VmRows

open Vm

def maxString : Nat := 1 <<< 26
def maxValueNodes : Nat := 1 <<< 24
def maxValueBytes : Nat := 1 <<< 26
def maxValueDepth : Nat := 64
def maxListSteps : Nat := 1 <<< 16
def maxCalls : Nat := 1 <<< 16
def maxCallDepth : Nat := 64
def maxU32 : Nat := (1 <<< 32) - 1

/-- A value's structural extent. A shared part is counted at every place it
appears, as the runner's cached extent is. -/
structure Extent where
  nodes : Nat := 0
  bytes : Nat := 0
  depth : Nat := 0
  deriving Repr, BEq, Inhabited, DecidableEq

def Extent.check (e : Extent) : Except Trap Extent :=
  if e.depth > maxValueDepth then
    .error (.unsupported "ValueTooDeep: value depth")
  else if e.nodes > maxValueNodes || e.bytes > maxValueBytes then
    .error (.unsupported "ValueTooLarge: value extent")
  else .ok e

def Extent.with (e part : Extent) : Except Trap Extent :=
  Extent.check {
    nodes := e.nodes + part.nodes
    bytes := e.bytes + part.bytes
    depth := e.depth.max (part.depth + 1) }

/-!
The partial extent and string helpers are executable definitions, opaque to the
kernel. The theorems below establish bound predicates and frame and counter
invariants; they do not prove those helpers' computations or cache correctness.
Differential comparisons and `native_decide` examples provide executable
checks, not a general refinement proof.
-/

mutual
  /-- Production's preorder extent walk. Depth is checked on entry to every
  value, before that value's contents can pass another bound. -/
  partial def measureExtent (v : Value) (depth : Nat) (total : Extent) : Except Trap Extent :=
    let total := { total with depth := total.depth.max depth }
    match v with
    | .some inner => do
      let total ← Extent.check total
      measureExtent inner (depth + 1) total
    | .list xs | .record _ xs => do
      let total ← Extent.check { total with nodes := total.nodes + 1 }
      let total ← measureExtentParts xs (depth + 1) total
      Extent.check total
    | .str s => Extent.check {
        total with nodes := total.nodes + 1, bytes := total.bytes + Str.utf8Length s }
    | _ => Extent.check { total with nodes := total.nodes + 1 }

  partial def measureExtentParts (xs : List Value) (depth : Nat) (total : Extent) :
      Except Trap Extent :=
    match xs with
    | [] => .ok total
    | x :: xs => do
      let total ← measureExtent x depth total
      measureExtentParts xs depth total

  /-- The runner's `Extents::of`, without its allocation-identity cache. The
  cache changes only whether this structural result is walked again. -/
  partial def extentOf (v : Value) : Except Trap Extent :=
    measureExtent v 0 {}

  /-- The runner's `Extents::built`, adding parts left to right so the first
  refused part and the refused bound agree with production. -/
  partial def extentParts : List Value → Extent → Except Trap Extent
    | [], e => .ok e
    | x :: xs, e => do
      let part ← extentOf x
      extentParts xs (← e.with part)
end

def builtExtent (xs : List Value) : Except Trap Extent :=
  extentParts xs { nodes := 1 }

abbrev ExtentCache := List (Value × Extent)

def cachedExtent (cache : ExtentCache) (v : Value) : Option Extent :=
  match cache.find? (fun entry => Value.equal entry.1 v == some true) with
  | some entry => some entry.2
  | none => none

def rememberExtent (cache : ExtentCache) (v : Value) (e : Extent) : ExtentCache :=
  if e.nodes >= 64 then (v, e) :: cache else cache

/-- Production keys this cache by pinned allocation identity. Structural
equality is a sound model key because equal values have equal extents. -/
partial def extentOfCached (cache : ExtentCache) (v : Value) :
    Except Trap (Extent × ExtentCache) :=
  match cachedExtent cache v with
  | some e => .ok (e, cache)
  | none => do
    let e ← extentOf v
    pure (e, rememberExtent cache v e)

partial def extentPartsCached (cache : ExtentCache) (xs : List Value) (e : Extent) :
    Except Trap (Extent × ExtentCache) :=
  match xs with
  | [] => .ok (e, cache)
  | x :: xs => do
    let (part, cache) ← extentOfCached cache x
    extentPartsCached cache xs (← e.with part)

def builtExtentCached (cache : ExtentCache) (xs : List Value) :
    Except Trap (Extent × ExtentCache) :=
  extentPartsCached cache xs { nodes := 1 }

def checkStringBytes (n : Nat) : Except Trap Unit :=
  if n > maxString then
    .error (.unsupported "StringTooLong: string bytes")
  else .ok ()

def checkString (s : String) : Except Trap Unit :=
  checkStringBytes (Str.utf8Length s)

theorem extent_nodes_at_bound :
    Extent.check { nodes := maxValueNodes } = .ok { nodes := maxValueNodes } := by
  simp [Extent.check, maxValueNodes, maxValueBytes, maxValueDepth]

theorem extent_nodes_past_bound :
    Extent.check { nodes := maxValueNodes + 1 } =
      .error (.unsupported "ValueTooLarge: value extent") := by
  simp [Extent.check, maxValueNodes, maxValueBytes, maxValueDepth]

theorem extent_bytes_at_bound :
    Extent.check { nodes := 1, bytes := maxValueBytes } =
      .ok { nodes := 1, bytes := maxValueBytes } := by
  simp [Extent.check, maxValueNodes, maxValueBytes, maxValueDepth]

theorem extent_bytes_past_bound :
    Extent.check { nodes := 1, bytes := maxValueBytes + 1 } =
      .error (.unsupported "ValueTooLarge: value extent") := by
  simp [Extent.check, maxValueNodes, maxValueBytes, maxValueDepth]

theorem extent_depth_at_bound :
    Extent.check { nodes := 1, depth := maxValueDepth } =
      .ok { nodes := 1, depth := maxValueDepth } := by
  simp [Extent.check, maxValueNodes, maxValueBytes, maxValueDepth]

theorem extent_depth_past_bound :
    Extent.check { nodes := 1, depth := maxValueDepth + 1 } =
      .error (.unsupported "ValueTooDeep: value depth") := by
  simp [Extent.check, maxValueDepth]

theorem string_at_bound {s : String} (h : Str.utf8Length s = maxString) :
    checkString s = .ok () := by
  simp [checkString, checkStringBytes, h]

theorem string_past_bound {s : String} (h : Str.utf8Length s = maxString + 1) :
    checkString s = .error (.unsupported "StringTooLong: string bytes") := by
  simp [checkString, checkStringBytes, h, maxString]

structure InstanceFrame where
  region : Option Nat := none
  arm : Option Nat := none
  frame : Vm.Frame := {}
  slots : List (Nat × Value) := []
  deriving Inhabited

structure Row where
  code : Code
  /-- The owning arm's region; none means the root. -/
  owner : Option Nat := none
  deriving Inhabited

/-- Frame regions, like Vm.Env.frames, are innermost first. -/
def ownerFrames (frames : List InstanceFrame) (owner : Option Nat) :
    Option (List InstanceFrame) :=
  match owner with
  | none => some []
  | some r =>
    let rest := frames.dropWhile (fun f => f.region != some r)
    if rest.isEmpty then none else some rest

/-- Cutting selects the nearest owner and retains every enclosing frame. -/
theorem owner_frames_cut (r : Nat) (arm : Option Nat) (frame : Frame) (slots) (rest)
    (innerFrames : List InstanceFrame)
    (outside : ∀ f ∈ innerFrames, f.region ≠ some r) :
    ownerFrames (innerFrames ++ { region := some r, arm, frame, slots } :: rest) (some r) =
      some ({ region := some r, arm, frame, slots } :: rest) := by
  induction innerFrames with
  | nil => simp [ownerFrames]
  | cons f fs ih =>
    have first := outside f (by simp)
    have tail : ∀ g ∈ fs, g.1 ≠ some r := fun g hg => outside g (by simp [hg])
    simpa [ownerFrames, first] using ih tail

/-- A reader's extra inner frames add exactly their count to an owner's
item, index or match-binding depth. -/
theorem frame_read_rebase (innerFrames owner : List InstanceFrame) (d : Nat)
    (read : Frame → Option α) :
    (((innerFrames ++ owner).map (·.frame))[innerFrames.length + d]?).bind read =
      ((owner.map (·.frame))[d]?).bind read := by
  simp [List.map_append, List.getElem?_append_right]

/-- Owned slots come from the nearest instance of their region, even when
that instance has no value for the slot. A root slot keeps its root value. -/
def slotValue (root : Option Value) (owner : Option Nat) (slot : Nat)
    (frames : List InstanceFrame) : Option Value :=
  match owner with
  | none => root
  | some r => (frames.find? (·.region == some r)).bind fun f =>
      (f.slots.find? (·.1 == slot)).map (·.2)

def slotsAt (env : Vm.Env) (owners : List (Option Nat)) (frames : List InstanceFrame) :=
  env.slots.zipIdx.map fun (v, i) => slotValue v (owners[i]?.join) i frames

theorem root_slot_unchanged (root : Option Value) (slot : Nat) (frames) :
    slotValue root none slot frames = root := rfl

theorem owned_slot_nearest (root : Option Value) (r slot : Nat) (arm : Option Nat)
    (frame : Frame) (slots : List (Nat × Value)) (rest : List InstanceFrame) :
    slotValue root (some r) slot ({ region := some r, arm, frame, slots } :: rest) =
      (slots.find? (·.1 == slot)).map (·.2) := by
  simp [slotValue]

structure Caller where
  code : Code
  env : Vm.Env
  regions : List InstanceFrame
  machine : Machine
  /-- Extents of in-flight callbacks, innermost first, parallel to
  `machine.cbs`. -/
  callbackExtents : List Extent := []
  deriving Inhabited

structure State extends Caller where
  callers : List Caller := []
  calls : Nat := 0
  listSteps : Nat := 0
  extents : ExtentCache := []
  slotOwners : List (Option Nat) := []
  resourceOwners : List (Option (Nat × Nat)) := []
  deriving Inhabited

def enter (s : State) (row : Row) (frames : List InstanceFrame) : State :=
  { code := row.code
    env := { s.env with
      frames := frames.map (·.frame)
      params := [], writable := []
      slots := slotsAt s.env s.slotOwners frames }
    regions := frames
    machine := .init
    callbackExtents := []
    callers := s.toCaller :: s.callers
    calls := s.calls + 1
    listSteps := s.listSteps
    extents := s.extents
    slotOwners := s.slotOwners
    resourceOwners := s.resourceOwners }

def resume (s : State) (v : Value) (caller : Caller) (rest : List Caller) : State :=
  { toCaller := { caller with machine := { caller.machine with
      pc := caller.machine.pc + 1
      stack := v :: caller.machine.stack } }
    callers := rest
    calls := s.calls
    listSteps := s.listSteps
    extents := s.extents
    slotOwners := s.slotOwners
    resourceOwners := s.resourceOwners }

structure Accounting where
  calls : Nat := 0
  listSteps : Nat := 0
  callDepth : Nat := 0
  deriving Repr, BEq, Inhabited, DecidableEq

def State.accounting (s : State) : Accounting :=
  { calls := s.calls, listSteps := s.listSteps, callDepth := s.callers.length }

/-- A refused nested row returns the shared counters to the outermost caller,
as production's nested `Run` does before propagating its trap. -/
def unwindCallers : List Caller → State → State
  | [], s => { s with callers := [] }
  | c :: cs, s => unwindCallers cs {
      toCaller := c
      callers := cs
      calls := s.calls
      listSteps := s.listSteps
      extents := s.extents
      slotOwners := s.slotOwners
      resourceOwners := s.resourceOwners }

def unwindTrap (s : State) : State := unwindCallers s.callers s

theorem unwind_trap_counters (s : State) :
    (unwindTrap s).calls = s.calls ∧ (unwindTrap s).listSteps = s.listSteps := by
  unfold unwindTrap
  induction s.callers generalizing s with
  | nil => simp [unwindCallers]
  | cons c cs ih =>
    simp only [unwindCallers]
    let next : State := {
      toCaller := c
      callers := cs
      calls := s.calls
      listSteps := s.listSteps
      extents := s.extents
      slotOwners := s.slotOwners
      resourceOwners := s.resourceOwners }
    simpa [next] using ih next

theorem unwind_trap_depth (s : State) : (unwindTrap s).callers = [] := by
  unfold unwindTrap
  induction s.callers generalizing s with
  | nil => simp [unwindCallers]
  | cons c cs ih =>
    simp only [unwindCallers]
    let next : State := {
      toCaller := c
      callers := cs
      calls := s.calls
      listSteps := s.listSteps
      extents := s.extents
      slotOwners := s.slotOwners
      resourceOwners := s.resourceOwners }
    exact ih next

def resourceRead : Instr → Option Nat
  | .loadResource r | .pendingResource r | .failedResource r
  | .failureResource r | .currentResource r => some r
  | _ => none

/-- Production resolves an owned request through its arm's frame retained by
the row call. A root resource has no owner and is always in scope. -/
def resourceInScope (s : State) (resource : Nat) : Bool :=
  match s.resourceOwners[resource]? with
  | some (some (region, arm)) =>
    s.regions.any fun frame => frame.region == some region && frame.arm == some arm
  | _ => true

inductive Charge where
  | ok (s : State)
  | trapped (t : Trap) (s : State)

def Charge.state : Charge → State
  | .ok s | .trapped _ s => s

def takeSteps (s : State) (n : Nat) : Charge :=
  if maxU32.min (s.listSteps + maxU32.min n) > maxListSteps then
    .trapped (.unsupported "IterationLimit: list steps")
      { s with listSteps := maxU32.min (s.listSteps + maxU32.min n) }
  else .ok { s with listSteps := maxU32.min (s.listSteps + maxU32.min n) }

theorem take_steps_exact (s : State) (n : Nat) :
    (takeSteps s n).state.listSteps = maxU32.min (s.listSteps + maxU32.min n) := by
  simp only [takeSteps]
  split <;> rfl

theorem take_steps_never_decreases (s s' : State) (n : Nat) (hs : s.listSteps ≤ maxU32)
    (h : takeSteps s n = .ok s' ∨ ∃ t, takeSteps s n = .trapped t s') :
    s.listSteps ≤ s'.listSteps := by
  have hv := take_steps_exact s n
  rcases h with h | ⟨t, h⟩ <;> rw [h] at hv <;> simp [Charge.state] at hv
  all_goals
    rw [hv]
    apply (Nat.le_min).2
    exact ⟨hs, by omega⟩

def charBytes (c : Char) : Nat :=
  let n := c.toNat
  if n ≤ 0x7f then 1 else if n ≤ 0x7ff then 2 else if n ≤ 0xffff then 3 else 4

inductive ReplacementPart where
  | char (c : Char)
  | before
  | matched
  | after

partial def replacementParts : List Char → List ReplacementPart
  | [] => []
  | '$' :: c :: rest =>
    if c == '$' then .char '$' :: replacementParts rest
    else if c == '&' then .matched :: replacementParts rest
    else if c.toNat == 96 then .before :: replacementParts rest
    else if c.toNat == 39 then .after :: replacementParts rest
    else .char '$' :: replacementParts (c :: rest)
  | c :: rest => .char c :: replacementParts rest

structure ReplacementSize where
  literalBytes : Nat := 0
  beforeCount : Nat := 0
  matchCount : Nat := 0
  afterCount : Nat := 0

def replacementSize (parts : List ReplacementPart) : ReplacementSize :=
  parts.foldl (fun size part => match part with
    | .char c => { size with literalBytes := size.literalBytes + charBytes c }
    | .before => { size with beforeCount := size.beforeCount + 1 }
    | .matched => { size with matchCount := size.matchCount + 1 }
    | .after => { size with afterCount := size.afterCount + 1 }) {}

def ReplacementSize.bytes (size : ReplacementSize) (find before after : Nat) : Nat :=
  size.literalBytes + size.beforeCount * before + size.matchCount * find +
    size.afterCount * after

partial def replaceNonemptyBytes (find : List Char) (findBytes totalBytes : Nat)
    (size : ReplacementSize) : List Char → Nat → Nat → Except Trap Unit
  | [], _, bytes => checkStringBytes bytes
  | source, before, bytes =>
    if Str.isPrefix find source then
      let added := size.bytes findBytes before (totalBytes - before - findBytes)
      match checkStringBytes (bytes + added) with
      | .error t => .error t
      | .ok _ => replaceNonemptyBytes find findBytes totalBytes size
          (source.drop find.length) (before + findBytes) (bytes + added)
    else match source with
      | [] => checkStringBytes bytes
      | c :: rest =>
        let n := charBytes c
        match checkStringBytes (bytes + n) with
        | .error t => .error t
        | .ok _ => replaceNonemptyBytes find findBytes totalBytes size rest
            (before + n) (bytes + n)

structure Utf8Counter where
  bytes : Nat := 0
  pendingHigh : Bool := false

def Utf8Counter.add (c : Utf8Counter) (n : Nat) : Except Trap Utf8Counter := do
  checkStringBytes (c.bytes + n)
  pure { c with bytes := c.bytes + n }

def Utf8Counter.settle (c : Utf8Counter) : Except Trap Utf8Counter :=
  if c.pendingHigh then do
    let c ← c.add 3
    pure { c with pendingHigh := false }
  else .ok c

def Utf8Counter.char (c : Utf8Counter) (v : Char) : Except Trap Utf8Counter := do
  let c ← c.settle
  c.add (charBytes v)

def Utf8Counter.unit (c : Utf8Counter) (u : Nat) : Except Trap Utf8Counter :=
  if Str.isHigh u then do
    let c ← c.settle
    pure { c with pendingHigh := true }
  else if Str.isLow u then
    if c.pendingHigh then do
      let c ← c.add 4
      pure { c with pendingHigh := false }
    else c.add 3
  else do
    let c ← c.settle
    c.add (charBytes (Char.ofNat u))

partial def feedUnits : Utf8Counter → List Nat → Except Trap Utf8Counter
  | c, [] => .ok c
  | c, u :: us => do feedUnits (← c.unit u) us

partial def feedReplacement (parts : List ReplacementPart) (beforeRev rest : List Nat) :
    Utf8Counter → Except Trap Utf8Counter
  | c => match parts with
    | [] => .ok c
    | part :: parts => do
      let c ← match part with
        | .char v => c.char v
        | .before => feedUnits c beforeRev.reverse
        | .matched => .ok c
        | .after => feedUnits c rest
      feedReplacement parts beforeRev rest c

partial def replaceEmptyBytes (parts : List ReplacementPart) :
    List Nat → List Nat → Utf8Counter → Except Trap Unit
  | beforeRev, rest, c => do
    let c ← feedReplacement parts beforeRev rest c
    match rest with
    | [] => let _ ← c.settle; pure ()
    | u :: us => replaceEmptyBytes parts (u :: beforeRev) us (← c.unit u)

def checkReplaceAll (source find replacement : String) : Except Trap Unit :=
  let parts := replacementParts replacement.toList
  if find.isEmpty then
    replaceEmptyBytes parts [] (Str.utf16Units source) {}
  else
    replaceNonemptyBytes find.toList (Str.utf8Length find) (Str.utf8Length source)
      (replacementSize parts) source.toList 0 0

partial def nonoverlapMatches (needle : List Char) : List Char → Nat
  | [] => 0
  | source@(_ :: rest) =>
    if Str.isPrefix needle source then 1 + nonoverlapMatches needle (source.drop needle.length)
    else nonoverlapMatches needle rest

def splitStats (s sep : String) : Nat × Nat :=
  if sep.isEmpty then
    s.foldl (fun (n, bytes) c =>
      if c.toNat > 0xffff then (n + 2, bytes + 6) else (n + 1, bytes + charBytes c)) (0, 0)
  else
    let matchCount := nonoverlapMatches sep.toList s.toList
    (matchCount + 1, Str.utf8Length s - matchCount * Str.utf8Length sep)

partial def flatLength : List Value → Option Nat
  | [] => some 0
  | .list ys :: rest => (flatLength rest).map (ys.length + ·)
  | _ => none

partial def fillTextBytes (value : String → Option String) :
    Nat → List Char → Nat → Except Trap Unit
  | 0, _, bytes => checkStringBytes bytes
  | _ + 1, [], bytes => checkStringBytes bytes
  | fuel + 1, '\\' :: c :: rest, bytes =>
    let bytes := bytes + charBytes c
    match checkStringBytes bytes with
    | .error t => .error t
    | .ok _ => fillTextBytes value fuel rest bytes
  | fuel + 1, '{' :: rest, bytes =>
    let inner := rest.takeWhile (· != '}')
    let after := (rest.dropWhile (· != '}')).drop 1
    let raw := '{' :: inner ++ ['}']
    let e := ((inner.dropWhile Char.isWhitespace).reverse.dropWhile Char.isWhitespace).reverse
    let name := match e with | '$' :: r => r | r => r
    let text := (value (String.ofList name)).getD (String.ofList raw)
    let bytes := bytes + Str.utf8Length text
    match checkStringBytes bytes with
    | .error t => .error t
    | .ok _ => fillTextBytes value fuel after bytes
  | fuel + 1, c :: rest, bytes =>
    let bytes := bytes + charBytes c
    match checkStringBytes bytes with
    | .error t => .error t
    | .ok _ => fillTextBytes value fuel rest bytes

def checkText (env : Contract.Env) (args : List Value) : Option (Except Trap Unit) :=
  match args with
  | [.str loc, .str key, .list pairs] =>
    (Format.localized env.prog.strings loc key).map fun template =>
      let values := pairStrs [.list pairs]
      fillTextBytes (Format.pairValue · values) (template.length + 1) template.toList 0
  | _ => none

def uriKeeps (b : UInt8) : Bool :=
  let c := b.toNat
  (0x41 ≤ c && c ≤ 0x5a) || (0x61 ≤ c && c ≤ 0x7a) || (0x30 ≤ c && c ≤ 0x39) ||
    "-_.!~*'()".toList.any (·.toNat == c)

def encodedBytes (s : String) : Nat :=
  s.toUTF8.toList.foldl (fun n b => n + if uriKeeps b then 1 else 3) 0

def listPosition (same : Value → Value → Bool) (xs : List Value) (x : Value) : Nat :=
  (xs.findIdx? (same · x)).getD xs.length

def chargeSomeValue (s : State) (v : Value) : Charge :=
  match extentOfCached s.extents v with
  | .error t => .trapped t s
  | .ok (inner, extents) =>
    match Extent.check { inner with depth := inner.depth + 1 } with
    | .error t => .trapped t s
    | .ok e => .ok { s with extents := rememberExtent extents (.some v) e }

/-- List work charged by `Run::call`, after the production argument checks
that precede it. `none` means this roster call takes no list steps. -/
def rosterSteps (f : String) (args : List Value) : Option Nat :=
  match f, args with
  | "join", .list xs :: _ => some xs.length
  | "range", [.num n] =>
    if n.isFinite && n.floor == n && F64.le 0 n then some n.toNat else none
  | "split", [.str s, .str sep] => some (splitStats s sep).1
  | "concat", [.list xs, .list ys] => some (xs.length + ys.length)
  | "flat", [.list xss] => (flatLength xss).map fun n => xss.length + n
  | "slice", [.list xs, .num a, .num b] =>
    let first := Str.clampIndex a xs.length
    some ((xs.drop first).take (Str.clampIndex b xs.length - first)).length
  | "includes", [.list xs, x] => some ((listPosition Value.sameValueZero xs x).min xs.length +
      if xs.any (Value.sameValueZero · x) then 1 else 0)
  | "indexOf", [.list xs, x] => some ((listPosition Value.strictEq xs x).min xs.length +
      if xs.any (Value.strictEq · x) then 1 else 0)
  | "sum", [.list xs] => some xs.length
  | _, _ => none

def rosterBuildsList (f : String) : Bool :=
  ["sortBy", "range", "split", "concat", "flat", "slice"].contains f

partial def extentFlatCached (cache : ExtentCache) (xss : List Value) (e : Extent) :
    Option (Except Trap (Extent × ExtentCache)) :=
  match xss with
  | [] => some (.ok (e, cache))
  | .list ys :: rest => match extentPartsCached cache ys e with
    | .error t => some (.error t)
    | .ok (e, cache) => extentFlatCached cache rest e
  | _ => none

partial def extentSliceCached (cache : ExtentCache) (xs : List Value) (skip count : Nat)
    (e : Extent) : Except Trap (Extent × ExtentCache) :=
  match xs with
  | [] => .ok (e, cache)
  | x :: xs =>
    if skip > 0 then extentSliceCached cache xs (skip - 1) count e
    else if count == 0 then .ok (e, cache)
    else do
      let (part, cache) ← extentOfCached cache x
      extentSliceCached cache xs 0 (count - 1) (← e.with part)

def extentCharge (s : State) (checked : Except Trap (Extent × ExtentCache)) : Charge :=
  match checked with
  | .error t => .trapped t s
  | .ok (_, extents) => .ok { s with extents }

def precheckListResult (s : State) (f : String) (args : List Value) : Charge :=
  match f, args with
  | "range", [.num n] =>
    if n.isFinite && n.floor == n && F64.le 0 n then
      match Extent.check { nodes := n.toNat + 1, depth := if n.toNat == 0 then 0 else 1 } with
      | .error t => .trapped t s
      | .ok _ => .ok s
    else .ok s
  | "split", [.str text, .str separator] =>
    let stats := splitStats text separator
    match Extent.check {
      nodes := stats.1 + 1, bytes := stats.2, depth := if stats.1 == 0 then 0 else 1 } with
    | .error t => .trapped t s
    | .ok _ => .ok s
  | "concat", [.list xs, .list ys] =>
    extentCharge s do
      let (e, cache) ← extentPartsCached s.extents xs { nodes := 1 }
      extentPartsCached cache ys e
  | "flat", [.list xss] =>
    match flatLength xss with
    | none => .ok s
    | some _ => match extentFlatCached s.extents xss { nodes := 1 } with
      | some checked => extentCharge s checked
      | none => .ok s
  | "slice", [.list xs, .num a, .num b] =>
    let first := Str.clampIndex a xs.length
    let count := Str.clampIndex b xs.length - first
    extentCharge s (extentSliceCached s.extents xs first count { nodes := 1 })
  | "sortBy", [.list xs, .list ks, .bool desc] =>
    match sortByKeys xs ks desc with
    | .ok (.list ys) => extentCharge s (builtExtentCached s.extents ys)
    | _ => .ok s
  | _, _ => .ok s

def precheckStringResult (s : State) (f : String) (args : List Value) : Charge :=
  let checked : Option (Except Trap Unit) := match f, args with
    | "t", _ => checkText (Vm.callEnv s.env) args
    | "replaceAll", [.str source, .str find, .str replacement] =>
      some (checkReplaceAll source find replacement)
    | "encodeURIComponent", [.str text] => some (checkStringBytes (encodedBytes text))
    | "encodeRouteSegment", [.str text] =>
      if text == "" || text == "." || text == ".." then none
      else some (checkStringBytes (encodedBytes text))
    | _, _ => none
  match checked with
  | some (.error t) => .trapped t s
  | _ => .ok s

def joinItemBytes : Value → Option Nat
  | .str text => some (Str.utf8Length text)
  | v@(.num _) | v@(.bool _) =>
    match v.display with
    | .ok text => some (Str.utf8Length text)
    | .error _ => none
  | _ => none

partial def checkJoinItems (separatorBytes : Nat) : List Value → Bool → Nat → Except Trap Unit
  | [], _, _ => .ok ()
  | item :: rest, afterFirst, bytes =>
    let bytes := bytes + if afterFirst then separatorBytes else 0
    match joinItemBytes item with
    | none => .ok ()
    | some itemBytes =>
      let bytes := bytes + itemBytes
      if bytes > maxString then .error (.unsupported "StringTooLong: string bytes")
      else checkJoinItems separatorBytes rest true bytes

/-- Production exempts a one-string list; otherwise it checks the accumulated
output after each printable item and before inspecting the next item. -/
def checkJoin (items : List Value) (separator : String) : Except Trap Unit :=
  match items with
  | [.str _] => .ok ()
  | _ => checkJoinItems (Str.utf8Length separator) items false 0

def chargeRosterResult (s : State) (f : String) (args : List Value) : Charge :=
  let stepped := match rosterSteps f args with
    | some n => takeSteps s n
    | none => .ok s
  match stepped with
  | .trapped t s => .trapped t s
  | .ok s =>
    if f == "join" then
      match args with
      | [.list items, .str separator] =>
        match checkJoin items separator with
        | .ok _ => .ok s
        | .error t => .trapped t s
      | _ => .ok s
    else
      match precheckListResult s f args with
      | .trapped t s => .trapped t s
      | .ok s => match precheckStringResult s f args with
        | .trapped t s => .trapped t s
        | .ok s =>
          match Contract.stdlib (Vm.callEnv s.env) f args with
          | .error _ => .ok s
          | .ok v =>
            if rosterBuildsList f then
              match v with
              | .list xs => match builtExtentCached s.extents xs with
                | .ok (_, extents) => .ok { s with extents }
                | .error t => .trapped t s
              | _ => .ok s
            else .ok s

def chargeInstruction (s : State) (i : Instr) : Charge :=
  match i with
  | .some =>
    match s.machine.stack with
    | v :: _ => chargeSomeValue s v
    | _ => .ok s
  | .list n | .record _ n =>
    match Vm.popN n s.machine.stack with
    | some (xs, _) => match builtExtentCached s.extents xs with
      | .ok (_, extents) => .ok { s with extents }
      | .error t => .trapped t s
    | none => .ok s
  | .concat =>
    match s.machine.stack with
    | .str b :: .str a :: _ => match checkStringBytes (Str.utf8Length a + Str.utf8Length b) with
      | .ok _ => .ok s
      | .error t => .trapped t s
    | _ => .ok s
  | .call f n =>
    match Vm.popN n s.machine.stack with
    | some (args, _) => chargeRosterResult s f args
    | none => .ok s
  | .map off | .filter off =>
    let stop := s.machine.pc + 1 + off
    if stop > s.code.length then .ok s else
    match s.machine.stack with
    | .list (_ :: _) :: _ => takeSteps s 1
    | _ => .ok s
  | .failureResource r =>
    match s.env.resources[r]? with
    | some (some _) =>
      match Vm.failureValue s.env r with
      | .some v => chargeSomeValue s v
      | _ => .ok s
    | _ => .ok s
  | .failureMutation k =>
    match Vm.mutationFailureValue s.env k with
    | .some v => chargeSomeValue s v
    | _ => .ok s
  | _ => .ok s

def replaceHead (x : α) : List α → List α
  | [] => [x]
  | _ :: xs => x :: xs

/-- At a callback end production measures a kept value before it charges the
next callback run. A refusal keeps every charge made before it. -/
def chargeCallbackEnd (s : State) : Charge :=
  match s.machine.cbs, s.machine.stack with
  | c :: _, [v] =>
    if s.machine.locals.length != c.base + 2 then .ok s else
    let decision := if c.filter then
        match v with
        | .bool true => some (some c.cur)
        | .bool false => some none
        | _ => none
      else some (some v)
    match decision with
    | none => .ok s
    | some kept =>
      let e := s.callbackExtents.head?.getD { nodes := 1 }
      let charged : Except Trap (Extent × ExtentCache) := match kept with
        | none => Except.ok (e, s.extents)
        | some v => do
          let (part, extents) ← extentOfCached s.extents v
          pure (← e.with part, extents)
      match charged with
      | .error t => .trapped t s
      | .ok (e, extents) =>
        let s := { s with callbackExtents := replaceHead e s.callbackExtents }
        let s := { s with extents := extents }
        if c.rest.isEmpty then .ok s else takeSteps s 1
  | _, _ => .ok s

inductive AdvanceResult where
  | next (s : State)
  | done (v : Value) (fx : Vm.Effects) (s : State)
  | trapped (t : Trap) (s : State)

/-- Calls, callback work, roster work and structural bounds have one state
across the entire evaluation. -/
def advanceState (rows : List (Option Row)) (s : State) : AdvanceResult :=
  let callbackEnd := s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc)
  if !callbackEnd then
    match s.code[s.machine.pc]? with
    | some (.loadDerive i) =>
      match rows[i]?.bind id with
      | some row =>
        let called := { s with calls := s.calls + 1 }
        if called.calls > maxCalls then
          .trapped (.unsupported "CallLimit: executable derive calls") called
        else if s.callers.length >= maxCallDepth then
          .trapped (.unsupported "CallDepth: executable derive calls") called
        else match ownerFrames s.regions row.owner with
          | some fs => .next (enter s row fs)
          | none => .trapped Trap.badScope called
      | none => stepCharged rows s callbackEnd
    | _ => stepCharged rows s callbackEnd
  else stepCharged rows s callbackEnd
where
  stepCharged (_rows : List (Option Row)) (s : State) (callbackEnd : Bool) : AdvanceResult :=
    let charged := if callbackEnd then chargeCallbackEnd s else
      match s.code[s.machine.pc]? with
      | some i => match resourceRead i with
        | some resource =>
          if resourceInScope s resource then chargeInstruction s i
          else .trapped .badScope s
        | none => chargeInstruction s i
      | none => .ok s
    match charged with
    | .trapped t s => .trapped t s
    | .ok s =>
      match Vm.step s.code s.env s.machine with
      | .error t => .trapped t s
      | .ok (.run m) =>
        let callbackExtents := if callbackEnd then
            if s.machine.cbs.head?.any (·.rest.isEmpty) then s.callbackExtents.drop 1
            else s.callbackExtents
          else match s.code[s.machine.pc]?, s.machine.stack with
            | some (.map _), .list (_ :: _) :: _ => { nodes := 1 } :: s.callbackExtents
            | some (.filter _), .list (_ :: _) :: _ => { nodes := 1 } :: s.callbackExtents
            | _, _ => s.callbackExtents
        .next { s with machine := m, callbackExtents }
      | .ok (.done v fx) =>
        match s.callers with
        | [] => .done v fx s
        | c :: cs => .next (resume s v c cs)

inductive Result where
  | next (s : State)
  | done (v : Value) (fx : Vm.Effects)

/-- Compatibility projection used by the existing lowering checker. -/
def advance (rows : List (Option Row)) (s : State) : Except Trap Result :=
  match advanceState rows s with
  | .next s => .ok (.next s)
  | .done v fx _ => .ok (.done v fx)
  | .trapped t _ => .error t

def runFrom (rows : List (Option Row)) : Nat → State → Except Trap (Value × Vm.Effects)
  | 0, _ => .error .outOfFuel
  | n + 1, s => do
    match ← advance rows s with
    | .done v fx => pure (v, fx)
    | .next s => runFrom rows n s

/-- A run's outcome and the shared counters after either return or refusal. -/
structure Report where
  outcome : Except Trap (Value × Vm.Effects)
  accounting : Accounting
  deriving Inhabited

def runReportFrom (rows : List (Option Row)) : Nat → State → Report
  | 0, s => { outcome := .error .outOfFuel, accounting := s.accounting }
  | n + 1, s =>
    match advanceState rows s with
    | .trapped t s' => { outcome := .error t, accounting := (unwindTrap s').accounting }
    | .done v fx s' => { outcome := .ok (v, fx), accounting := s'.accounting }
    | .next s' => runReportFrom rows n s'

def run (rows : List (Option Row)) (code : Code) (env : Vm.Env)
    (regions : List (Option Nat) := []) (fuel : Nat := 1 <<< 22)
    (stores : List (List (Nat × Value)) := []) (slotOwners : List (Option Nat) := [])
    (resourceOwners : List (Option (Nat × Nat)) := []) (arms : List (Option Nat) := []) :
    Except Trap (Value × Vm.Effects) :=
  let frames := (regions.zip env.frames).zipIdx.map fun ((region, frame), i) =>
    { region, arm := arms[i]?.join, frame, slots := stores[i]?.getD [] : InstanceFrame }
  let env := { env with slots := slotsAt env slotOwners frames }
  runFrom rows fuel { code, env, regions := frames, machine := .init, slotOwners, resourceOwners }

def runReport (rows : List (Option Row)) (code : Code) (env : Vm.Env)
    (regions : List (Option Nat) := []) (fuel : Nat := 1 <<< 22)
    (stores : List (List (Nat × Value)) := []) (slotOwners : List (Option Nat) := [])
    (resourceOwners : List (Option (Nat × Nat)) := []) (arms : List (Option Nat) := []) : Report :=
  let frames := (regions.zip env.frames).zipIdx.map fun ((region, frame), i) =>
    { region, arm := arms[i]?.join, frame, slots := stores[i]?.getD [] : InstanceFrame }
  let env := { env with slots := slotsAt env slotOwners frames }
  runReportFrom rows fuel { code, env, regions := frames, machine := .init, slotOwners, resourceOwners }

theorem enter_fresh (s : State) (r : Row) (fs) :
    (enter s r fs).machine = Machine.init ∧
    (enter s r fs).env.params = [] ∧ (enter s r fs).env.writable = [] := by
  exact ⟨rfl, rfl, rfl⟩

theorem enter_shared (s : State) (r : Row) (fs) :
    (enter s r fs).calls = s.calls + 1 ∧
    (enter s r fs).listSteps = s.listSteps := by
  exact ⟨rfl, rfl⟩

theorem enter_accounting (s : State) (r : Row) (fs) :
    (enter s r fs).accounting =
      { calls := s.calls + 1, listSteps := s.listSteps, callDepth := s.callers.length + 1 } := by
  rfl

theorem resume_preserves (s : State) (v : Value) (c : Caller) (cs) :
    (resume s v c cs).machine.locals = c.machine.locals ∧
    (resume s v c cs).machine.cbs = c.machine.cbs ∧
    (resume s v c cs).calls = s.calls ∧
    (resume s v c cs).listSteps = s.listSteps := by
  exact ⟨rfl, rfl, rfl, rfl⟩

theorem resume_accounting (s : State) (v : Value) (c : Caller) (cs) :
    (resume s v c cs).accounting =
      { calls := s.calls, listSteps := s.listSteps, callDepth := cs.length } := by
  rfl

theorem enter_slot_scope (s : State) (r : Row) (fs) :
    (enter s r fs).env.slots = slotsAt s.env s.slotOwners fs ∧
    (enter s r fs).slotOwners = s.slotOwners := by
  exact ⟨rfl, rfl⟩

theorem resume_restores_environment (s : State) (v : Value) (c : Caller) (cs) :
    (resume s v c cs).env = c.env ∧ (resume s v c cs).regions = c.regions := by
  exact ⟨rfl, rfl⟩

theorem root_frames (fs) : ownerFrames fs none = some [] := rfl

/-- The row cannot see either a caller local or an intervening arm. -/
example :
    (match run [some { code := [.loadItem 0, .ret], owner := some 1 }]
      [.str "caller local", .bindLocal, .loadDerive 0, .dropLocal, .ret]
      { frames := [{ item := some (.str "inner") }, { item := some (.str "owner") }] }
      [some 2, some 1] 100 with
      | .ok (.str "owner", _) => true
      | _ => false) = true := by native_decide

/-- A callee's cut hides an inner slot and retains the owner's slot. -/
example :
    (match run [some { code := [.loadSlot 0, .ret], owner := some 1 }]
      [.loadDerive 0, .loadSlot 1, .list 2, .ret]
      { slots := [none, none], owned := [true, true], frames := [{}, {}] }
      [some 2, some 1] 100
      [[(1, .str "inner")], [(0, .str "owner")]] [some 1, some 2] with
      | .ok (.list [.str "owner", .str "inner"], _) => true
      | _ => false) = true := by native_decide

/-- An executable root row cannot borrow a caller's owned slot. -/
example :
    (run [some { code := [.loadSlot 0, .ret] }]
      [.loadDerive 0, .ret] { slots := [none], owned := [true], frames := [{}] }
      [some 1] 100 [[(0, .str "caller")]] [some 1]).isOk = false := by native_decide

/-- A missing slot in the nearest same-region instance never falls back
to the value of an enclosing instance of that region. -/
example :
    slotValue none (some 1) 0
      [{ region := some 1 }, { region := some 1, slots := [(0, .str "outer")] }] = none := by
  native_decide

/-- A fresh local stack makes a malformed row's local read fail. -/
example :
    (run [some { code := [.loadLocal 0, .ret] }]
      [.str "caller local", .bindLocal, .loadDerive 0, .ret] {} [] 100).isOk = false := by
  native_decide

/-- Depth is shared even when every row contains only another call. -/
example :
    (match run [some { code := [.loadDerive 0, .ret] }]
      [.loadDerive 0, .ret] {} [] 1000 with
      | .error (.unsupported "CallDepth: executable derive calls") => true
      | _ => false) = true := by native_decide


/-- Sibling calls consume one counter; returning does not replenish it. -/
example :
    (match runFrom [some { code := [.unit, .ret] }] 100
      { code := [.loadDerive 0, .pop, .loadDerive 0, .ret]
        env := {}, regions := [], machine := .init, calls := 65535 } with
      | .error (.unsupported "CallLimit: executable derive calls") => true
      | _ => false) = true := by native_decide

/-- Work already spent by a caller is charged before a callee's callback. -/
example :
    (match runFrom [some { code := [.unit, .list 1, .map 1, .loadLocal 0, .ret] }] 100
      { code := [.loadDerive 0, .ret]
        env := {}, regions := [], machine := .init, listSteps := 65536 } with
      | .error (.unsupported "IterationLimit: list steps") => true
      | _ => false) = true := by native_decide

/-- A roster list walk spends the same evaluation's list-step budget. -/
example :
    (match runFrom [] 100
      { code := [.num 0x3ff0000000000000, .list 1, .call "sum" 1, .ret]
        env := {}, regions := [], machine := .init, listSteps := 65536 } with
      | .error (.unsupported "IterationLimit: list steps") => true
      | _ => false) = true := by native_decide

/-- The last admitted roster step succeeds and is retained after return. -/
example :
    (let report := runReportFrom [] 100
      { code := [.num 0x3ff0000000000000, .list 1, .call "sum" 1, .ret]
        env := {}, regions := [], machine := .init, listSteps := 65535 }
    report.outcome.isOk && report.accounting == { listSteps := 65536 }) = true := by
  native_decide

/-- The first refused roster step is consumed, as it is by `u32::step`. -/
example :
    (let report := runReportFrom [] 100
      { code := [.num 0x3ff0000000000000, .list 1, .call "sum" 1, .ret]
        env := {}, regions := [], machine := .init, listSteps := 65536 }
    (match report.outcome with
      | .error (.unsupported "IterationLimit: list steps") => true
      | _ => false) && report.accounting == { listSteps := 65537 }) = true := by
  native_decide

def chainRowsFrom : Nat → Nat → List (Option Row)
  | 0, _ => []
  | 1, _ => [some { code := [.unit, .ret] }]
  | n + 2, i => some { code := [.loadDerive (i + 1), .ret] } :: chainRowsFrom (n + 1) (i + 1)

/-- Both sides of lowering's 48-call sharing margin remain below the
runtime depth bound; the compiler decides which links are calls. -/
example :
    ((runReport (chainRowsFrom 48 0) [.loadDerive 0, .ret] {}).accounting.calls,
     (runReport (chainRowsFrom 49 0) [.loadDerive 0, .ret] {}).accounting.calls) = (48, 49) := by
  native_decide

end Contract.VmRows
