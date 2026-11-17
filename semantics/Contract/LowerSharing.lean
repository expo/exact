/-
Executable-row sharing: the lowerer's admission predicate and the value
correspondence between an admitted source expression's reader and owner
scopes. The definitions mirror `contract/lower/src/sharing.rs`; the source
semantics remains unbounded, while `Contract.VmRows` states the executor's
separate resource outcomes.
-/
import Contract.ExpandSubstRev
import Contract.FvWeakening
import Contract.LowerStmt
import Contract.VmRowsProof

namespace Contract.LowerSharing

open Contract Expand ExpandSubst Lower Vm VmRows

/-- One lexical scope at a sharing site. `regionDepth` is kept separately,
as it is by Rust's `contract_types::Scope`. -/
structure LexicalScope where
  names : Lower.Scope
  regionDepth : Nat
  deriving Inhabited

/-- The enclosing arm of each arm. `none` is the root. -/
abbrev ArmParents := List (Nat × Option Nat)

def parentOf (parents : ArmParents) (arm : Nat) : Option (Option Nat) :=
  (parents.find? (·.1 == arm)).map (·.2)

/-- `Plan::frame_cut`: reach `owner` by walking outward from `reader`.
The result is the number of removed region frames. -/
def frameCutAux (parents : ArmParents) (owner : Option Nat) : Nat → Option Nat → Option Nat
  | 0, reader => if reader = owner then some 0 else none
  | fuel + 1, reader =>
    if reader = owner then some 0 else do
      let arm ← reader
      let parent ← parentOf parents arm
      return (← frameCutAux parents owner fuel parent) + 1

def frameCut (parents : ArmParents) (reader owner : Option Nat) : Option Nat :=
  frameCutAux parents owner (parents.length + 1) reader

/-- A component instance lies on the reader's parent chain. -/
def instanceAncestor (parents : List (Nat × Option Nat)) (reader candidate : Nat) : Bool :=
  (frameCut parents (some reader) (some candidate)).isSome

def checkedSub (a b : Nat) : Option Nat := if b ≤ a then some (a - b) else none

/-- Rebase a free reference after cutting `cut` inner region frames. This is
`same_reads`'s `rebase` closure. Expression locals are never visible to a
fresh executable-row machine. -/
def rebaseRef (cut : Nat) : Lower.Ref → Option Lower.Ref
  | .item d => (checkedSub d cut).map .item
  | .index d => (checkedSub d cut).map .index
  | .bound d => (checkedSub d cut).map .bound
  | .local _ => none
  | r => some r

def rebaseRead (cut : Nat) : Option (Lower.Ref × STy) → Option Lower.Ref
  | none => none
  | some (r, _) => rebaseRef cut r

/-- Rust `same_reads`: every free value read must resolve to the owner's
reference after the reader's region depths are rebased. `fv` excludes
locally bound names, call heads and record shape names, exactly the names
the Rust operand walk does not compare. -/
def sameReads (reader owner : Lower.Scope) (cut : Nat) (e : Expr) : Bool :=
  (fv e).all fun name =>
    decide (rebaseRead cut (scopeLookup name reader) = (scopeLookup name owner).map (·.1))

/-- One candidate already selected from the source-span bucket. -/
structure Candidate where
  expr : Expr
  instanceId : Nat
  ownerArm : Option Nat
  ownerScope : LexicalScope
  active : Bool := false
  /-- A previously emitted row's maximum call-chain depth. -/
  cachedDepth : Option Nat := none
  deriving Inhabited

/-- The site at which lowering considers replacing the expression. -/
structure Site where
  expr : Expr
  instanceId : Nat
  readerArm : Option Nat
  readerScope : LexicalScope
  activeDepth : Nat := 0
  deriving Inhabited

def maxSharedDepth : Nat := 48

/-- The conjunction implemented by `Lowerer::shared_expression` after a
candidate is obtained from the expression's source-span bucket, for the
scope cut computed by `checked_sub`. -/
structure AdmissionAt (armParents instanceParents : ArmParents) (site : Site)
    (candidate : Candidate) (cut : Nat) : Prop where
  depthAvailable : site.activeDepth < maxSharedDepth
  instanceOnChain : instanceAncestor instanceParents site.instanceId candidate.instanceId = true
  inactive : candidate.active = false
  sameExpression : candidate.expr = site.expr
  ownerReachable : (frameCut armParents site.readerArm candidate.ownerArm).isSome = true
  scopeCut : checkedSub site.readerScope.regionDepth candidate.ownerScope.regionDepth = some cut
  sameFreeReads : sameReads site.readerScope.names candidate.ownerScope.names cut site.expr = true
  cachedDepthFits : ∀ d, candidate.cachedDepth = some d →
    site.activeDepth + d ≤ maxSharedDepth

def Admission (armParents instanceParents : ArmParents) (site : Site)
    (candidate : Candidate) : Prop :=
  ∃ cut, AdmissionAt armParents instanceParents site candidate cut

theorem sameReads_name {reader owner : Lower.Scope} {cut : Nat} {e : Expr}
    (h : sameReads reader owner cut e = true) {name : String} (hn : name ∈ fv e) :
    rebaseRead cut (scopeLookup name reader) = (scopeLookup name owner).map (·.1) := by
  simp only [sameReads, List.all_eq_true] at h
  simpa only [decide_eq_true_eq] using h name hn

theorem admission_owner_reachable {armParents instanceParents site candidate}
    (h : Admission armParents instanceParents site candidate) :
    ∃ cut, frameCut armParents site.readerArm candidate.ownerArm = some cut := by
  obtain ⟨_, h⟩ := h
  cases hc : frameCut armParents site.readerArm candidate.ownerArm with
  | none =>
      have ho := h.ownerReachable
      simp [hc] at ho
  | some cut => exact ⟨cut, rfl⟩

theorem admission_transitive_margin {armParents instanceParents site candidate}
    (h : Admission armParents instanceParents site candidate) {d : Nat}
    (hd : candidate.cachedDepth = some d) : site.activeDepth + d ≤ maxSharedDepth :=
  let ⟨_, h⟩ := h
  h.cachedDepthFits d hd

/-- The source locals at the reader and owner sites realize the scope
references the matcher equates. This is the source-level part of the
lowerer's `Ctx` invariant; it mentions only names the expression reads. -/
def LocalsRespectCut (reader owner : Lower.Scope) (cut : Nat) (e : Expr)
    (readerLocals ownerLocals : Locals) : Prop :=
  ∀ name, name ∈ fv e →
    rebaseRead cut (scopeLookup name reader) = (scopeLookup name owner).map (·.1) →
    lookup name readerLocals = lookup name ownerLocals

/-- Every free read has a typed scope reference. Successful compilation at
the reader and owner supplies this property for the corresponding source
expression. -/
def ScopeResolves (scope : Lower.Scope) (e : Expr) : Prop :=
  ∀ name, name ∈ fv e → ∃ r t, scopeLookup name scope = some (r, t)

/-- The VM environments selected for a reader and its owner realize a frame
cut. Region frames retain their identities so `owner_frames_cut` can construct
this relation; the lowering contexts see their mapped `Vm.Frame` values. -/
def FrameCutCtx (reader owner : Vm.Env) (cut : Nat) : Prop :=
  ∃ inner outer : List VmRows.InstanceFrame,
    inner.length = cut ∧
    reader.frames = (inner ++ outer).map (·.frame) ∧
    owner.frames = outer.map (·.frame) ∧
    reader.params = owner.params

theorem FrameCutCtx.read (h : FrameCutCtx reader owner cut) (d : Nat)
    (read : Vm.Frame → Option α) :
    (reader.frames[cut + d]?).bind read = (owner.frames[d]?).bind read := by
  obtain ⟨inner, outer, hlength, hreader, howner, _⟩ := h
  rw [hreader, howner]
  simpa [hlength] using VmRows.frame_read_rebase inner outer d read

theorem FrameCutCtx.params (h : FrameCutCtx reader owner cut) :
    reader.params = owner.params := by
  obtain ⟨_, _, _, _, _, hparams⟩ := h
  exact hparams

/-- Selecting an owner frame constructs the suffix relation used by source
locals, and proves that `ownerFrames` selects that same suffix. -/
theorem frameCutCtx_of_owner (region : Nat) (arm : Option Nat) (frame : Vm.Frame) (slots)
    (rest inner : List VmRows.InstanceFrame)
    (outside : ∀ f ∈ inner, f.region ≠ some region)
    {reader owner : Vm.Env}
    (hreader : reader.frames =
      (inner ++ { region := some region, arm, frame, slots } :: rest).map (·.frame))
    (howner : owner.frames = ({ region := some region, arm, frame, slots } :: rest).map (·.frame))
    (hparams : reader.params = owner.params) :
    VmRows.ownerFrames (inner ++ { region := some region, arm, frame, slots } :: rest) (some region) =
        some ({ region := some region, arm, frame, slots } :: rest) ∧
      FrameCutCtx reader owner inner.length := by
  exact ⟨VmRows.owner_frames_cut region arm frame slots rest inner outside,
    ⟨inner, { region := some region, arm, frame, slots } :: rest, rfl,
      hreader, howner, hparams⟩⟩

theorem checkedSub_eq_some {a b c : Nat} (h : checkedSub a b = some c) :
    a = b + c := by
  unfold checkedSub at h
  split at h
  · simp at h
    omega
  · simp at h

theorem refOk_rebased_lookup_eq {env : Env} {inFn : Bool} {name : String}
    {readerLocals ownerLocals : Locals} {readerEnv ownerEnv : Vm.Env}
    {readerRef ownerRef : Lower.Ref} {readerTy ownerTy : STy} {cut : Nat}
    (hreader : Lower.RefOk env inFn readerLocals readerEnv [] name readerRef readerTy)
    (howner : Lower.RefOk env inFn ownerLocals ownerEnv [] name ownerRef ownerTy)
    (hcut : FrameCutCtx readerEnv ownerEnv cut)
    (href : rebaseRef cut readerRef = some ownerRef) :
    lookup name readerLocals = lookup name ownerLocals := by
  cases readerRef <;> cases ownerRef <;> simp [rebaseRef] at href
  case slot.slot => exact hreader.1.trans howner.1.symm
  case derive.derive => exact hreader.1.trans howner.1.symm
  case resource.resource => exact hreader.1.trans howner.1.symm
  case mutation.mutation => exact hreader.1.trans howner.1.symm
  case param.param readerIndex ownerIndex =>
    obtain ⟨readerValue, hreaderLookup, hreaderParam, _⟩ := hreader
    obtain ⟨ownerValue, hownerLookup, hownerParam, _⟩ := howner
    obtain rfl := href
    have hparam := congrArg (fun params => params[readerIndex]?) (FrameCutCtx.params hcut)
    rw [hreaderParam, hownerParam] at hparam
    cases hparam
    exact hreaderLookup.trans hownerLookup.symm
  case item.item readerDepth ownerDepth =>
    obtain ⟨readerValue, hreaderLookup, hreaderFrame, _⟩ := hreader
    obtain ⟨ownerValue, hownerLookup, hownerFrame, _⟩ := howner
    have hdepth := checkedSub_eq_some href
    have hframe := hcut.read ownerDepth (·.item)
    rw [hdepth] at hreaderFrame
    rw [hreaderFrame, hownerFrame] at hframe
    cases hframe
    exact hreaderLookup.trans hownerLookup.symm
  case index.index readerDepth ownerDepth =>
    obtain ⟨readerIndex, hreaderLookup, hreaderFrame, _⟩ := hreader
    obtain ⟨ownerIndex, hownerLookup, hownerFrame, _⟩ := howner
    have hdepth := checkedSub_eq_some href
    have hframe := hcut.read ownerDepth (·.index)
    rw [hdepth] at hreaderFrame
    rw [hreaderFrame, hownerFrame] at hframe
    cases hframe
    exact hreaderLookup.trans hownerLookup.symm
  case bound.bound readerDepth ownerDepth =>
    obtain ⟨readerValue, hreaderLookup, hreaderFrame, _⟩ := hreader
    obtain ⟨ownerValue, hownerLookup, hownerFrame, _⟩ := howner
    have hdepth := checkedSub_eq_some href
    have hframe := hcut.read ownerDepth (·.bound)
    rw [hdepth] at hreaderFrame
    rw [hreaderFrame, hownerFrame] at hframe
    cases hframe
    exact hreaderLookup.trans hownerLookup.symm

theorem FrameCutCtx.length (h : FrameCutCtx reader owner cut) :
    reader.frames.length = cut + owner.frames.length := by
  obtain ⟨inner, outer, hlength, hreader, howner, _⟩ := h
  rw [hreader, howner]
  simp [hlength]

/-- Valid lowering contexts and the selected frame suffix derive the local
value equality used by the source-site theorem. No free-read value equality
is assumed. -/
theorem localsRespectCut_of_ctx {reader owner : Lower.Scope} {cut : Nat} {e : Expr}
    {env : Env} {inFn : Bool} {readerLocals ownerLocals : Locals}
    {readerEnv ownerEnv : Vm.Env} {p : Program}
    (hreader : Lower.Ctx env inFn readerLocals readerEnv [] p reader 0)
    (howner : Lower.Ctx env inFn ownerLocals ownerEnv [] p owner 0)
    (hcut : FrameCutCtx readerEnv ownerEnv cut)
    (hreaderResolves : ScopeResolves reader e)
    (hownerResolves : ScopeResolves owner e) :
    LocalsRespectCut reader owner cut e readerLocals ownerLocals := by
  intro name hname href
  obtain ⟨readerRef, readerTy, hreaderRef⟩ := hreaderResolves name hname
  obtain ⟨ownerRef, ownerTy, hownerRef⟩ := hownerResolves name hname
  rw [hreaderRef, hownerRef] at href
  exact refOk_rebased_lookup_eq
    (hreader.agree name readerRef readerTy hreaderRef)
    (howner.agree name ownerRef ownerTy hownerRef) hcut href

theorem sameReads_fvOn {reader owner : Lower.Scope} {cut : Nat} {e : Expr}
    {readerLocals ownerLocals : Locals} (hs : sameReads reader owner cut e = true)
    (hr : LocalsRespectCut reader owner cut e readerLocals ownerLocals) :
    FvOn (fv e) readerLocals ownerLocals := by
  intro name hn
  exact hr name hn (sameReads_name hs hn)

/-- **Admitted sharing preserves the source value.** The expression at
the read site and the candidate body at its owner evaluate alike in the
same semantic environment. The scopes may contain different locals; the
matcher and `LocalsRespectCut` say their free reads denote the same values. -/
theorem admitted_site_value_iff {armParents instanceParents site candidate cut}
    {env : Env} {inFn : Bool} {readerLocals ownerLocals : Locals} {v : Value}
    (ha : AdmissionAt armParents instanceParents site candidate cut)
    (hsmall : Small site.expr = true)
    (hr : LocalsRespectCut site.readerScope.names candidate.ownerScope.names cut site.expr
      readerLocals ownerLocals) :
    EvalR env inFn readerLocals site.expr v ↔
      EvalR env inFn ownerLocals candidate.expr v := by
  rw [ha.sameExpression]
  exact wkFv_iff hsmall (sameReads_fvOn ha.sameFreeReads hr)

/-- The existing lowering theorem, transported from the row owner's scope
to an admitted read site. This is the compiler-correctness statement used
for both direct and transitively called executable rows. -/
theorem admitted_expr_correct {armParents instanceParents site candidate cut}
    {fuel p depth c t env inFn readerLocals ownerLocals venv}
    (hc : Lower.compile fuel p depth candidate.ownerScope.names 0 candidate.expr = .ok (c, t))
    (hx : Lower.Ctx env inFn ownerLocals venv [] p candidate.ownerScope.names 0)
    (ha : AdmissionAt armParents instanceParents site candidate cut)
    (hsmall : Small site.expr = true)
    (hr : LocalsRespectCut site.readerScope.names candidate.ownerScope.names cut site.expr
      readerLocals ownerLocals) :
    (∀ v, EvalR env inFn readerLocals site.expr v →
      ∃ k, Vm.runFrom (c ++ [.ret]) venv k .init = .ok (v, {})) ∧
    (∀ v fx k, Vm.runFrom (c ++ [.ret]) venv k .init = .ok (v, fx) →
      EvalR env inFn readerLocals site.expr v ∧ fx = {}) := by
  have hcorr := Lower.expr_correct hc hx
  exact ⟨fun v h => hcorr.1 v ((admitted_site_value_iff (v := v) ha hsmall hr).mp h),
    fun v fx k h =>
    let ⟨he, hfx⟩ := hcorr.2 v fx k h
    ⟨(admitted_site_value_iff (v := v) ha hsmall hr).mpr he, hfx⟩⟩

/-- **A successful shared load is the substitution it replaces.** The
callee premise is the compositional induction hypothesis for its row body;
it may itself use shared loads. Resource checks are explicit premises, so a
refused call has no value conclusion. -/
theorem admitted_shared_load_value_iff
    {armParents instanceParents site candidate cut env inFn readerLocals ownerLocals}
    {rows : List (Option VmRows.Row)} {state : VmRows.State} {i : Nat} {row : VmRows.Row}
    {frames : List VmRows.InstanceFrame} {v : Value}
    (ha : AdmissionAt armParents instanceParents site candidate cut)
    (hsmall : Small site.expr = true)
    (hr : LocalsRespectCut site.readerScope.names candidate.ownerScope.names cut site.expr
      readerLocals ownerLocals)
    (hcallback : state.machine.cbs.head?.any (fun c => c.stop == state.machine.pc) = false)
    (hcode : state.code[state.machine.pc]? = some (.loadDerive i))
    (hrow : rows[i]?.bind id = some row)
    (hcalls : state.calls + 1 ≤ VmRows.maxCalls)
    (hdepth : state.callers.length < VmRows.maxCallDepth)
    (howner : VmRows.ownerFrames state.regions row.owner = some frames)
    (hcallee : ∀ w, VmRows.ReturnsTo rows (VmRows.enter state row frames) state w ↔
      EvalR env inFn ownerLocals candidate.expr w) :
    VmRows.ReturnsTo rows state state v ↔
      EvalR env inFn readerLocals site.expr v := by
  have hload := VmRows.shared_load_returns_iff
    (v := v) hcallback hcode hrow hcalls hdepth howner
  have hvalue := admitted_site_value_iff (env := env) (inFn := inFn) (v := v) ha hsmall hr
  constructor
  · intro hrun
    exact hvalue.mpr ((hcallee v).mp (hload.mp hrun))
  · intro heval
    exact hload.mpr ((hcallee v).mpr (hvalue.mp heval))

/-- A successful bounded shared load has an inlined bare-VM run with the
same value. The existential fuel is the "enough budget" in this direction;
resource refusals do not satisfy `hshared` and make no value claim. -/
theorem successful_shared_load_has_inlined_run
    {armParents instanceParents site candidate cut env inFn readerLocals ownerLocals}
    {fuel p depth c t venv} {rows : List (Option VmRows.Row)} {state : VmRows.State}
    {i : Nat} {row : VmRows.Row} {frames : List VmRows.InstanceFrame} {v : Value}
    (hc : Lower.compile fuel p depth candidate.ownerScope.names 0 candidate.expr = .ok (c, t))
    (hx : Lower.Ctx env inFn ownerLocals venv [] p candidate.ownerScope.names 0)
    (ha : AdmissionAt armParents instanceParents site candidate cut)
    (hsmall : Small site.expr = true)
    (hr : LocalsRespectCut site.readerScope.names candidate.ownerScope.names cut site.expr
      readerLocals ownerLocals)
    (hcallback : state.machine.cbs.head?.any (fun cb => cb.stop == state.machine.pc) = false)
    (hcode : state.code[state.machine.pc]? = some (.loadDerive i))
    (hrow : rows[i]?.bind id = some row)
    (hcalls : state.calls + 1 ≤ VmRows.maxCalls)
    (hdepth : state.callers.length < VmRows.maxCallDepth)
    (howner : VmRows.ownerFrames state.regions row.owner = some frames)
    (hcallee : ∀ w, VmRows.ReturnsTo rows (VmRows.enter state row frames) state w ↔
      EvalR env inFn ownerLocals candidate.expr w)
    (hshared : VmRows.ReturnsTo rows state state v) :
    ∃ k, Vm.runFrom (c ++ [.ret]) venv k .init = .ok (v, {}) := by
  have heval := (admitted_shared_load_value_iff ha hsmall hr hcallback hcode hrow hcalls
    hdepth howner hcallee).mp hshared
  exact (admitted_expr_correct hc hx ha hsmall hr).1 v heval

/-! The examples exercise the clauses that most easily drift: binder hiding,
owner ancestry, region-depth rebasing and the cached transitive-call margin. -/

def cutProgram : Program := {}
def cutSemEnv : Env := { prog := cutProgram, slots := [] }

def cutOwnerFrame : Vm.Frame := {
  item := some (.str "owner item"), index := some 7, bound := some (.str "owner match") }

def cutInnerFrame : Vm.Frame := {
  item := some (.str "inner item"), index := some 99, bound := some (.str "inner match") }

def cutReaderEnv : Vm.Env := { frames := [cutInnerFrame, cutOwnerFrame] }
def cutOwnerEnv : Vm.Env := { frames := [cutOwnerFrame] }

def cutReaderScope : Lower.Scope :=
  [("item", .item 1, .string), ("index", .index 1, .number), ("match", .bound 1, .string)]

def cutOwnerScope : Lower.Scope :=
  [("item", .item 0, .string), ("index", .index 0, .number), ("match", .bound 0, .string)]

def cutReaderLocals : Locals :=
  [("shadow", .str "reader shadow"), ("item", .str "owner item"),
   ("index", .num (F64.ofNat 7)), ("match", .str "owner match")]

def cutOwnerLocals : Locals :=
  [("shadow", .str "owner shadow"), ("item", .str "owner item"),
   ("index", .num (F64.ofNat 7)), ("match", .str "owner match")]

def cutExpr : Expr :=
  .letE "shadow" (.var "item")
    (.list [.var "shadow", .var "index", .var "match"])

theorem cutReaderQuiet : Lower.Quiet cutSemEnv cutReaderEnv := by
  refine ⟨rfl, rfl, rfl, ?_, ?_, ?_⟩
  all_goals intro i <;> simp [cutReaderEnv]

theorem cutOwnerQuiet : Lower.Quiet cutSemEnv cutOwnerEnv := by
  refine ⟨rfl, rfl, rfl, ?_, ?_, ?_⟩
  all_goals intro i <;> simp [cutOwnerEnv]

theorem cutReaderCtx :
    Lower.Ctx cutSemEnv false cutReaderLocals cutReaderEnv [] cutProgram cutReaderScope 0 := by
  refine ⟨rfl, ?_, rfl, cutReaderQuiet⟩
  intro name ref ty href
  simp only [cutReaderScope, scopeLookup] at href
  split at href
  · simp at href
    obtain ⟨rfl, rfl⟩ := href
    rename_i hname
    have : name = "item" := by simpa using hname
    subst name
    exact ⟨.str "owner item", by simp [cutReaderLocals, lookup],
      by simp [cutReaderEnv, cutInnerFrame, cutOwnerFrame], by simp [Lower.VTy]⟩
  · split at href
    · simp at href
      obtain ⟨rfl, rfl⟩ := href
      rename_i _ hname
      have : name = "index" := by simpa using hname
      subst name
      exact ⟨7, by simp [cutReaderLocals, lookup],
        by simp [cutReaderEnv, cutInnerFrame, cutOwnerFrame], by simp [Lower.VTy]⟩
    · split at href
      · simp at href
        obtain ⟨rfl, rfl⟩ := href
        rename_i _ _ hname
        have : name = "match" := by simpa using hname
        subst name
        exact ⟨.str "owner match", by simp [cutReaderLocals, lookup],
          by simp [cutReaderEnv, cutInnerFrame, cutOwnerFrame], by simp [Lower.VTy]⟩
      · simp at href

theorem cutOwnerCtx :
    Lower.Ctx cutSemEnv false cutOwnerLocals cutOwnerEnv [] cutProgram cutOwnerScope 0 := by
  refine ⟨rfl, ?_, rfl, cutOwnerQuiet⟩
  intro name ref ty href
  simp only [cutOwnerScope, scopeLookup] at href
  split at href
  · simp at href
    obtain ⟨rfl, rfl⟩ := href
    rename_i hname
    have : name = "item" := by simpa using hname
    subst name
    exact ⟨.str "owner item", by simp [cutOwnerLocals, lookup],
      by simp [cutOwnerEnv, cutOwnerFrame], by simp [Lower.VTy]⟩
  · split at href
    · simp at href
      obtain ⟨rfl, rfl⟩ := href
      rename_i _ hname
      have : name = "index" := by simpa using hname
      subst name
      exact ⟨7, by simp [cutOwnerLocals, lookup],
        by simp [cutOwnerEnv, cutOwnerFrame], by simp [Lower.VTy]⟩
    · split at href
      · simp at href
        obtain ⟨rfl, rfl⟩ := href
        rename_i _ _ hname
        have : name = "match" := by simpa using hname
        subst name
        exact ⟨.str "owner match", by simp [cutOwnerLocals, lookup],
          by simp [cutOwnerEnv, cutOwnerFrame], by simp [Lower.VTy]⟩
      · simp at href

theorem cutFrames : FrameCutCtx cutReaderEnv cutOwnerEnv 1 := by
  exact ⟨[{ region := some 2, arm := none, frame := cutInnerFrame }],
    [{ region := some 1, arm := none, frame := cutOwnerFrame }], rfl, rfl, rfl, rfl⟩

example : LocalsRespectCut cutReaderScope cutOwnerScope 1 cutExpr
    cutReaderLocals cutOwnerLocals := by
  apply localsRespectCut_of_ctx cutReaderCtx cutOwnerCtx cutFrames
  · intro name hname
    simp [cutExpr, fv, fvList] at hname
    rcases hname with rfl | rfl | rfl
    · exact ⟨.item 1, .string, rfl⟩
    · exact ⟨.index 1, .number, by simp [cutReaderScope, scopeLookup]⟩
    · exact ⟨.bound 1, .string, by simp [cutReaderScope, scopeLookup]⟩
  · intro name hname
    simp [cutExpr, fv, fvList] at hname
    rcases hname with rfl | rfl | rfl
    · exact ⟨.item 0, .string, rfl⟩
    · exact ⟨.index 0, .number, by simp [cutOwnerScope, scopeLookup]⟩
    · exact ⟨.bound 0, .string, by simp [cutOwnerScope, scopeLookup]⟩

/-- The same frames cannot realize the wrong cut. -/
example : ¬ FrameCutCtx cutReaderEnv cutOwnerEnv 0 := by
  intro h
  have := h.length
  simp [cutReaderEnv, cutOwnerEnv] at this

/-- The selected owner is the suffix used by `cutFrames`. -/
example :
    VmRows.ownerFrames
        [{ region := some 2, arm := none, frame := cutInnerFrame },
         { region := some 1, arm := none, frame := cutOwnerFrame }] (some 1) =
        some [{ region := some 1, arm := none, frame := cutOwnerFrame }] ∧
      FrameCutCtx cutReaderEnv cutOwnerEnv 1 := by
  apply frameCutCtx_of_owner 1 none cutOwnerFrame [] []
      [{ region := some 2, arm := none, frame := cutInnerFrame }]
  · intro frame hframe
    simp at hframe
    obtain rfl := hframe
    decide
  · rfl
  · rfl
  · rfl

def exampleReader : Lower.Scope :=
  [("local", .local 0, .number), ("item", .item 1, .string), ("root", .slot 0, .number)]

def exampleOwner : Lower.Scope :=
  [("local", .slot 1, .number), ("item", .item 0, .string), ("root", .slot 0, .number)]

example : sameReads exampleReader exampleOwner 1
    (.letE "local" (.var "root") (.binary .add (.var "local") (.var "item"))) = true := by
  native_decide

example : sameReads exampleReader exampleOwner 1 (.var "local") = false := by
  native_decide

example : frameCut [(3, some 2), (2, some 1), (1, none)] (some 3) (some 1) = some 2 := by
  native_decide

example : frameCut [(3, some 2), (2, some 1), (1, none)] (some 1) (some 3) = none := by
  native_decide

/-- Arm reachability and lexical rebasing use independent cuts. `for` keys
are compiled after their item scope is pushed but before their row arm exists,
so the first admitted shape occurs in production. Slot reads are unchanged by
either cut. -/
example :
    let names : Lower.Scope := [("slot", .slot 0, .number)]
    let candidate : Candidate := {
      expr := .var "slot", instanceId := 1, ownerArm := some 1,
      ownerScope := { names, regionDepth := 0 } }
    let keySite : Site := {
      expr := .var "slot", instanceId := 2, readerArm := some 1,
      readerScope := { names, regionDepth := 1 } }
    let nestedSite : Site := {
      expr := .var "slot", instanceId := 2, readerArm := some 3,
      readerScope := { names, regionDepth := 0 } }
    Admission [(3, some 1), (1, none)] [(2, some 1), (1, none)] keySite candidate ∧
      Admission [(3, some 1), (1, none)] [(2, some 1), (1, none)] nestedSite candidate := by
  constructor
  · refine ⟨1, {
      depthAvailable := by decide
      instanceOnChain := by native_decide
      inactive := by decide
      sameExpression := rfl
      ownerReachable := by native_decide
      scopeCut := by decide
      sameFreeReads := by native_decide
      cachedDepthFits := ?_ }⟩
    intro d hd
    simp at hd
  · refine ⟨0, {
      depthAvailable := by decide
      instanceOnChain := by native_decide
      inactive := by decide
      sameExpression := rfl
      ownerReachable := by native_decide
      scopeCut := by decide
      sameFreeReads := by native_decide
      cachedDepthFits := ?_ }⟩
    intro d hd
    simp at hd

example :
    let site : Site := {
      expr := .var "item", instanceId := 2, readerArm := some 3,
      readerScope := { names := exampleReader, regionDepth := 2 }, activeDepth := 1 }
    let candidate : Candidate := {
      expr := .var "item", instanceId := 1, ownerArm := some 1,
      ownerScope := { names := exampleOwner, regionDepth := 1 }, cachedDepth := some 47 }
    Admission [(3, some 1), (1, none)] [(2, some 1), (1, some 0), (0, none)] site candidate := by
  refine ⟨1, {
    depthAvailable := by decide
    instanceOnChain := by native_decide
    inactive := by decide
    sameExpression := rfl
    ownerReachable := by native_decide
    scopeCut := by decide
    sameFreeReads := by native_decide
    cachedDepthFits := ?_ }⟩
  intro d hd
  injection hd with hd
  subst d
  decide

end Contract.LowerSharing
