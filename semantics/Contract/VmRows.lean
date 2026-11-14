/-
Executable derive calls over the expression VM. The caller keeps its machine;
the callee starts fresh and shares the call and callback-work counters.
Roster-call allocation and extent bounds remain outside this model, as in Vm.
-/
import Contract.Vm

namespace Contract.VmRows

open Vm

structure InstanceFrame where
  region : Option Nat := none
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
theorem owner_frames_cut (r : Nat) (frame : Frame) (slots) (rest)
    (innerFrames : List InstanceFrame)
    (outside : ∀ f ∈ innerFrames, f.region ≠ some r) :
    ownerFrames (innerFrames ++ { region := some r, frame, slots } :: rest) (some r) =
      some ({ region := some r, frame, slots } :: rest) := by
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

theorem owned_slot_nearest (root : Option Value) (r slot : Nat) (frame : Frame)
    (slots : List (Nat × Value)) (rest : List InstanceFrame) :
    slotValue root (some r) slot ({ region := some r, frame, slots } :: rest) =
      (slots.find? (·.1 == slot)).map (·.2) := by
  simp [slotValue]

structure Caller where
  code : Code
  env : Vm.Env
  regions : List InstanceFrame
  machine : Machine
  deriving Inhabited

structure State extends Caller where
  callers : List Caller := []
  calls : Nat := 0
  listSteps : Nat := 0
  slotOwners : List (Option Nat) := []
  deriving Inhabited

def enter (s : State) (row : Row) (frames : List InstanceFrame) : State :=
  { code := row.code
    env := { s.env with
      frames := frames.map (·.frame)
      params := [], writable := []
      slots := slotsAt s.env s.slotOwners frames }
    regions := frames
    machine := .init
    callers := s.toCaller :: s.callers
    calls := s.calls + 1
    listSteps := s.listSteps
    slotOwners := s.slotOwners }

def resume (s : State) (v : Value) (caller : Caller) (rest : List Caller) : State :=
  { toCaller := { caller with machine := { caller.machine with
      pc := caller.machine.pc + 1
      stack := v :: caller.machine.stack } }
    callers := rest
    calls := s.calls
    listSteps := s.listSteps
    slotOwners := s.slotOwners }

inductive Result where
  | next (s : State)
  | done (v : Value) (fx : Vm.Effects)

/-- Calls and callback work have one budget across the entire evaluation. -/
def advance (rows : List (Option Row)) (s : State) : Except Trap Result := do
  let callbackEnd := s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc)
  if !callbackEnd then
    match s.code[s.machine.pc]? with
    | some (.loadDerive i) =>
      match rows[i]?.bind id with
      | some row =>
        if s.calls + 1 > 65536 then
          throw (.unsupported "CallLimit: executable derive calls")
        if s.callers.length >= 64 then
          throw (.unsupported "CallDepth: executable derive calls")
        let frames ← match ownerFrames s.regions row.owner with
          | some fs => pure fs
          | none => throw Trap.badScope
        return .next (enter s row frames)
      | none => pure ()
    | _ => pure ()
  let work := if callbackEnd then
      s.machine.cbs.head?.map (fun c => if c.rest.isEmpty then 0 else 1) |>.getD 0
    else match s.code[s.machine.pc]?, s.machine.stack with
      | some (.map _), .list (_ :: _) :: _ => 1
      | some (.filter _), .list (_ :: _) :: _ => 1
      | _, _ => 0
  let steps := s.listSteps + work
  if steps > 65536 then throw (.unsupported "IterationLimit: list steps")
  let s := { s with listSteps := steps }
  match ← Vm.step s.code s.env s.machine with
  | .run m => return .next { s with machine := m }
  | .done v fx =>
    match s.callers with
    | [] => return .done v fx
    | c :: cs => return .next (resume s v c cs)

def runFrom (rows : List (Option Row)) : Nat → State → Except Trap (Value × Vm.Effects)
  | 0, _ => .error .outOfFuel
  | n + 1, s => do
    match ← advance rows s with
    | .done v fx => pure (v, fx)
    | .next s => runFrom rows n s

def run (rows : List (Option Row)) (code : Code) (env : Vm.Env)
    (regions : List (Option Nat) := []) (fuel : Nat := 1 <<< 22)
    (stores : List (List (Nat × Value)) := []) (slotOwners : List (Option Nat) := []) :
    Except Trap (Value × Vm.Effects) :=
  let frames := (regions.zip env.frames).zipIdx.map fun ((region, frame), i) =>
    { region, frame, slots := stores[i]?.getD [] : InstanceFrame }
  let env := { env with slots := slotsAt env slotOwners frames }
  runFrom rows fuel { code, env, regions := frames, machine := .init, slotOwners }

theorem enter_fresh (s : State) (r : Row) (fs) :
    (enter s r fs).machine = Machine.init ∧
    (enter s r fs).env.params = [] ∧ (enter s r fs).env.writable = [] := by
  exact ⟨rfl, rfl, rfl⟩

theorem enter_shared (s : State) (r : Row) (fs) :
    (enter s r fs).calls = s.calls + 1 ∧
    (enter s r fs).listSteps = s.listSteps := by
  exact ⟨rfl, rfl⟩

theorem resume_preserves (s : State) (v : Value) (c : Caller) (cs) :
    (resume s v c cs).machine.locals = c.machine.locals ∧
    (resume s v c cs).machine.cbs = c.machine.cbs ∧
    (resume s v c cs).calls = s.calls ∧
    (resume s v c cs).listSteps = s.listSteps := by
  exact ⟨rfl, rfl, rfl, rfl⟩

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

end Contract.VmRows
