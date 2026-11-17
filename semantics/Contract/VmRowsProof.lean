/-
Operational facts for executable rows. Successful runs carry the complete
nested call tree. Resource refusals are stated separately because the source
semantics and bare VM are intentionally unbounded.
-/
import Contract.VmRows

namespace Contract.VmRows

open Vm

/-- A successful bounded execution, including every direct or transitive
executable-row call it takes. The final state retains shared accounting. -/
inductive Runs (rows : List (Option Row)) : State → Value → Vm.Effects → State → Prop
  | done : advanceState rows s = .done v fx final → Runs rows s v fx final
  | next : advanceState rows s = .next nextState →
      Runs rows nextState v fx final → Runs rows s v fx final

/-- Zero or more successful machine transitions. Unlike `Runs`, this can
stop when a callee resumes its caller, before the caller's continuation. -/
inductive Nexts (rows : List (Option Row)) : State → State → Prop
  | refl : Nexts rows s s
  | next : advanceState rows s = .next nextState →
      Nexts rows nextState final → Nexts rows s final

theorem Nexts.trans {rows : List (Option Row)} {a b c : State}
    (hab : Nexts rows a b) (hbc : Nexts rows b c) : Nexts rows a c := by
  induction hab with
  | refl => exact hbc
  | next hstep _ ih => exact .next hstep (ih hbc)

/-- `start` returns `v` to `caller`. The endpoint is the exact state made by
`resume`, so this observes the load's value without running its continuation. -/
def ReturnsTo (rows : List (Option Row)) (start caller : State) (v : Value) : Prop :=
  ∃ callee, Nexts rows start (resume callee v caller.toCaller caller.callers)

/-- A successful report has an operational witness. A trapped report has no
such premise, so this theorem makes no value claim for a resource refusal. -/
theorem runReportFrom_ok_runs : ∀ {fuel s v fx},
    (runReportFrom rows fuel s).outcome = .ok (v, fx) →
    ∃ final, Runs rows s v fx final := by
  intro fuel
  induction fuel with
  | zero =>
      intro s v fx h
      simp [runReportFrom] at h
  | succ fuel ih =>
      intro s v fx h
      cases hs : advanceState rows s with
      | trapped t stopped => simp [runReportFrom, hs] at h
      | done w wfx final =>
          simp only [runReportFrom, hs, Except.ok.injEq] at h
          obtain ⟨rfl, rfl⟩ := h
          exact ⟨final, .done hs⟩
      | next nextState =>
          simp only [runReportFrom, hs] at h
          obtain ⟨final, hr⟩ := ih h
          exact ⟨final, .next hs hr⟩

/-- The exact successful `LoadDerive` transition. The call counter and
active depth checks precede owner-frame selection, as in the runner. -/
theorem loadDerive_step {rows : List (Option Row)} {s : State} {i : Nat} {row : Row}
    {frames : List InstanceFrame}
    (hcallback : s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc) = false)
    (hcode : s.code[s.machine.pc]? = some (.loadDerive i))
    (hrow : rows[i]?.bind id = some row)
    (hcalls : s.calls + 1 ≤ maxCalls)
    (hdepth : s.callers.length < maxCallDepth)
    (howner : ownerFrames s.regions row.owner = some frames) :
  advanceState rows s = .next (enter s row frames) := by
  simp [advanceState, hcallback, hcode, hrow, howner,
    Nat.not_lt.mpr hcalls, Nat.not_le.mpr hdepth]

theorem Nexts.tail {rows : List (Option Row)} {s nextState final : State}
    (hne : s ≠ final) (hstep : advanceState rows s = .next nextState) :
    Nexts rows s final → Nexts rows nextState final := by
  intro hrun
  cases hrun with
  | refl => exact (hne rfl).elim
  | next hnext hrest =>
      rw [hstep] at hnext
      cases hnext
      exact hrest

/-- A shared load returns whatever its fresh callee execution returns. Since
the premise is itself `Runs`, it includes arbitrarily nested and transitive
loads rather than assuming the callee contains no calls. -/
theorem shared_load_runs {rows : List (Option Row)} {s : State} {i : Nat} {row : Row}
    {frames : List InstanceFrame} {v : Value} {fx : Vm.Effects} {final : State}
    (hcallback : s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc) = false)
    (hcode : s.code[s.machine.pc]? = some (.loadDerive i))
    (hrow : rows[i]?.bind id = some row)
    (hcalls : s.calls + 1 ≤ maxCalls)
    (hdepth : s.callers.length < maxCallDepth)
    (howner : ownerFrames s.regions row.owner = some frames)
    (hrun : Runs rows (enter s row frames) v fx final) :
    Runs rows s v fx final :=
  .next (loadDerive_step hcallback hcode hrow hcalls hdepth howner) hrun

/-- A successful shared load has exactly the callee's successful outcomes.
This is the operational substitution rule used compositionally for rows
whose bodies call other rows. -/
theorem shared_load_runs_iff {rows : List (Option Row)} {s : State} {i : Nat} {row : Row}
    {frames : List InstanceFrame} {v : Value} {fx : Vm.Effects} {final : State}
    (hcallback : s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc) = false)
    (hcode : s.code[s.machine.pc]? = some (.loadDerive i))
    (hrow : rows[i]?.bind id = some row)
    (hcalls : s.calls + 1 ≤ maxCalls)
    (hdepth : s.callers.length < maxCallDepth)
    (howner : ownerFrames s.regions row.owner = some frames) :
    Runs rows s v fx final ↔ Runs rows (enter s row frames) v fx final := by
  have hstep := loadDerive_step hcallback hcode hrow hcalls hdepth howner
  constructor
  · intro hrun
    cases hrun with
    | done hdone => rw [hstep] at hdone; cases hdone
    | next hnext hrest =>
        rw [hstep] at hnext
        cases hnext
        exact hrest
  · exact shared_load_runs hcallback hcode hrow hcalls hdepth howner

/-- A shared load returns exactly the value returned by its fresh callee.
The caller's continuation is not part of either side. -/
theorem shared_load_returns_iff {rows : List (Option Row)} {s : State} {i : Nat} {row : Row}
    {frames : List InstanceFrame} {v : Value}
    (hcallback : s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc) = false)
    (hcode : s.code[s.machine.pc]? = some (.loadDerive i))
    (hrow : rows[i]?.bind id = some row)
    (hcalls : s.calls + 1 ≤ maxCalls)
    (hdepth : s.callers.length < maxCallDepth)
    (howner : ownerFrames s.regions row.owner = some frames) :
    ReturnsTo rows s s v ↔ ReturnsTo rows (enter s row frames) s v := by
  have hstep := loadDerive_step hcallback hcode hrow hcalls hdepth howner
  constructor
  · rintro ⟨callee, hrun⟩
    have hne : s ≠ resume callee v s.toCaller s.callers := by
      intro heq
      have hpc := congrArg (fun state => state.machine.pc) heq
      simp [resume] at hpc
    exact ⟨callee, hrun.tail hne hstep⟩
  · rintro ⟨callee, hrun⟩
    exact ⟨callee, .next hstep hrun⟩

/-- A call-free literal row discharges the base case of a row-graph proof:
the callee returns its compiled literal without a semantic premise about the
callee. -/
theorem literal_row_returns {rows : List (Option Row)} {s : State} {i : Nat}
    {row : Row} {frames : List InstanceFrame} {text : String}
    (hcallback : s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc) = false)
    (hcode : s.code[s.machine.pc]? = some (.loadDerive i))
    (hrow : rows[i]?.bind id = some row)
    (hliteral : row.code = [.str text, .ret])
    (hcalls : s.calls + 1 ≤ maxCalls)
    (hdepth : s.callers.length < maxCallDepth)
    (howner : ownerFrames s.regions row.owner = some frames) :
    ReturnsTo rows s s (.str text) := by
  rcases row with ⟨rowCode, rowOwner⟩
  simp only at hliteral
  subst rowCode
  have hload := loadDerive_step hcallback hcode hrow hcalls hdepth howner
  let entered := enter s { code := [.str text, .ret], owner := rowOwner } frames
  let pushed : State := { entered with machine := {
    entered.machine with pc := 1, stack := [.str text] } }
  have hpushed : advanceState rows entered = .next pushed := by
    simp [advanceState, advanceState.stepCharged, resourceRead, entered, pushed, enter,
      Machine.init, chargeInstruction, Vm.step, Vm.exec]
  have hreturned : advanceState rows pushed =
      .next (resume pushed (.str text) s.toCaller s.callers) := by
    simp [advanceState, advanceState.stepCharged, resourceRead, pushed, entered, enter,
      Machine.init, chargeInstruction, Vm.step, Vm.exec]
  have hpushed' : advanceState rows
      (enter s { code := [.str text, .ret], owner := rowOwner } frames) = .next pushed := by
    simpa [entered] using hpushed
  exact ⟨pushed, .next hload (.next hpushed' (.next hreturned .refl))⟩

/-- A finite load-only graph whose dependencies have smaller row indices.
It is the fragment for which the emitted-row induction can be completed
without a model of Rust's recursive expression interception. -/
inductive LiteralGraph (rows : List (Option Row)) (text : String) : Nat → Nat → Prop
  | literal (i : Nat)
      (hrow : rows[i]?.bind id = some { code := [.str text, .ret] }) :
      LiteralGraph rows text i 1
  | alias (i j need : Nat) (hlt : j < i)
      (hrow : rows[i]?.bind id = some { code := [.loadDerive j, .ret] })
      (child : LiteralGraph rows text j need) :
      LiteralGraph rows text i (need + 1)

/-- A whole acyclic literal graph returns its leaf value when the entry state
has room for every call and active frame in the chain. No callee semantic
result is a premise: the proof is induction on the smaller-index graph. -/
theorem LiteralGraph.returns {rows : List (Option Row)} {text : String} {i need : Nat}
    (graph : LiteralGraph rows text i need) {s : State}
    (hcallback : s.machine.cbs.head?.any (fun c => c.stop == s.machine.pc) = false)
    (hcode : s.code[s.machine.pc]? = some (.loadDerive i))
    (hcalls : s.calls + need ≤ maxCalls)
    (hdepth : s.callers.length + need ≤ maxCallDepth) :
    ReturnsTo rows s s (.str text) := by
  induction graph generalizing s with
  | literal i hrow =>
      apply literal_row_returns hcallback hcode hrow rfl
      · omega
      · omega
      · exact root_frames _
  | alias i j childNeed hlt hrow child ih =>
      let row : Row := { code := [.loadDerive j, .ret] }
      have hload : advanceState rows s = .next (enter s row []) := by
        apply loadDerive_step hcallback hcode
        · simpa [row] using hrow
        · omega
        · omega
        · exact root_frames _
      let entered := enter s row []
      have hchild : ReturnsTo rows entered entered (.str text) := by
        apply ih
        · simp [entered, row, enter, Machine.init]
        · simp [entered, row, enter, Machine.init]
        · simp [entered, enter]
          omega
        · simp [entered, enter]
          omega
      obtain ⟨callee, htrace⟩ := hchild
      let back := resume callee (.str text) entered.toCaller entered.callers
      have hback : back = resume callee (.str text) entered.toCaller entered.callers := rfl
      have hret : advanceState rows back =
          .next (resume back (.str text) s.toCaller s.callers) := by
        simp [advanceState, advanceState.stepCharged, resourceRead, back, entered, row, enter,
          resume, Machine.init, chargeInstruction, Vm.step, Vm.exec]
      refine ⟨back, .next hload ?_⟩
      exact (hback ▸ htrace).trans (.next hret .refl)

def nestedLiteralRows : List (Option Row) :=
  [some { code := [.str "shared", .ret] },
   some { code := [.loadDerive 0, .ret] }]

theorem nestedLiteralGraph : LiteralGraph nestedLiteralRows "shared" 1 2 := by
  exact @LiteralGraph.alias nestedLiteralRows "shared" 1 0 1 (by decide) rfl
    (@LiteralGraph.literal nestedLiteralRows "shared" 0 rfl)

/-- The graph theorem applies independently at both reads of the shared row. -/
example :
    let first : State := {
      code := [.loadDerive 1, .ret], env := {}, regions := [], machine := .init }
    let second : State := {
      code := [.unit, .loadDerive 1, .ret], env := {}, regions := [],
      machine := { Machine.init with pc := 1, stack := [.unit] } }
    ReturnsTo nestedLiteralRows first first (.str "shared") ∧
      ReturnsTo nestedLiteralRows second second (.str "shared") := by
  dsimp
  constructor
  · apply nestedLiteralGraph.returns <;> decide
  · apply nestedLiteralGraph.returns <;> decide

/-- An error outcome and a value outcome are disjoint. In particular,
`CallLimit`, `CallDepth`, list-work and extent refusals imply no source-value
claim in the bounded model. -/
theorem refused_report_has_no_value {rows : List (Option Row)} {fuel : Nat} {s : State}
    {trap : Trap} (h : (runReportFrom rows fuel s).outcome = .error trap) :
    ¬ ∃ v fx, (runReportFrom rows fuel s).outcome = .ok (v, fx) := by
  rintro ⟨v, fx, hv⟩
  rw [h] at hv
  cases hv

/-- With room for only the entry call, the nested call is refused. The graph
value theorem is therefore inapplicable, and the refusal has no value outcome. -/
example :
    let state : State := {
      code := [.loadDerive 1, .ret], env := {}, regions := [], machine := .init,
      calls := maxCalls - 1 }
    let report := runReportFrom nestedLiteralRows 20 state
    report.outcome = .error (.unsupported "CallLimit: executable derive calls") ∧
      ¬ ∃ v fx, report.outcome = .ok (v, fx) := by
  dsimp
  have hrefused :
      (runReportFrom nestedLiteralRows 20
        { code := [.loadDerive 1, .ret], env := {}, regions := [],
          machine := .init, calls := maxCalls - 1 }).outcome =
        .error (.unsupported "CallLimit: executable derive calls") := by
    rfl
  exact ⟨hrefused, refused_report_has_no_value hrefused⟩

/-- A concrete transitive call tree and its inlined body return the same
nontrivial value. -/
example :
    (match run
        [some { code := [.loadDerive 1, .some, .ret] },
         some { code := [.str "shared", .ret] }]
        [.loadDerive 0, .ret] {} [] 100 with
      | .ok (.some (.str "shared"), _) => true
      | _ => false) = true ∧
    (match Vm.runFrom [.str "shared", .some, .ret] {} 100 .init with
      | .ok (.some (.str "shared"), _) => true
      | _ => false) = true := by
  native_decide

/-- A finite row chain in production order: row zero is the inlined leaf and
every later row loads its immediate smaller-index dependency. -/
def orderedUnitRows : Nat → List (Option Row)
  | 0 => []
  | n + 1 => orderedUnitRows n ++ [some { code :=
      if n = 0 then [.unit, .ret] else [.loadDerive (n - 1), .ret] }]

/-- The same fully inlined body succeeds across the runtime depth boundary.
The 64-call shared chain returns; attempting the leaf of the 65-call chain
is refused because 64 callers are already active. -/
example :
    (match Vm.runFrom [.unit, .ret] {} 10 .init with
      | .ok (.unit, _) => true
      | _ => false) = true ∧
    (match run (orderedUnitRows 64) [.loadDerive 63, .ret] {} [] 1000 with
      | .ok (.unit, _) => true
      | _ => false) = true ∧
    (match run (orderedUnitRows 65) [.loadDerive 64, .ret] {} [] 1000 with
      | .error (.unsupported "CallDepth: executable derive calls") => true
      | _ => false) = true := by
  native_decide

/-- A reader may also exhaust the shared call counter before a shallow row,
although the corresponding inlined body needs no executable-row call. -/
example :
    (match Vm.runFrom [.unit, .ret] {} 10 .init with
      | .ok (.unit, _) => true
      | _ => false) = true ∧
    (match runFrom [some { code := [.unit, .ret] }] 100
        { code := [.loadDerive 0, .ret], env := {}, regions := [],
          machine := .init, calls := maxCalls } with
      | .error (.unsupported "CallLimit: executable derive calls") => true
      | _ => false) = true := by
  native_decide

/-- Lowering's 48-call margin is not a runtime refusal: both sides return,
with the attempted calls charged exactly. -/
example :
    let at48 := runReport (orderedUnitRows 48) [.loadDerive 47, .ret] {}
    let at49 := runReport (orderedUnitRows 49) [.loadDerive 48, .ret] {}
    (match at48.outcome with | .ok (.unit, _) => true | _ => false) = true ∧
      at48.accounting.calls = 48 ∧
      (match at49.outcome with | .ok (.unit, _) => true | _ => false) = true ∧
      at49.accounting.calls = 49 := by
  native_decide

end Contract.VmRows
