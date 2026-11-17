/-
Evaluation weakening over the names an expression actually reads. Unlike
`Expand.freeNames`, `ExpandSubst.fv` excludes static call heads and record
shape names. This is the weakening relation used by executable-row sharing.
-/
import Contract.ExpandSubst

namespace Contract.ExpandSubst

open Contract Expand

/-- Two local scopes agree on the value reads in `ns`. -/
def FvOn (ns : List String) (L L' : Locals) : Prop :=
  ∀ y ∈ ns, lookup y L = lookup y L'

theorem FvOn.sub {ns ms : List String} {L L' : Locals} (h : FvOn ns L L')
    (hm : ∀ y ∈ ms, y ∈ ns) : FvOn ms L L' :=
  fun y hy => h y (hm y hy)

theorem FvOn.cons {L L' : Locals} {x : String} {w : Value} {e : Expr}
    (h : FvOn ((fv e).filter (· != x)) L L') :
    FvOn (fv e) ((x, w) :: L) ((x, w) :: L') := by
  intro y hy
  by_cases hyx : y = x
  · subst hyx; simp [lookup]
  · rw [lookup_cons_ne hyx, lookup_cons_ne hyx]
    exact h y (by simpa [hyx] using hy)

theorem FvOn.params {ps : List String} {L L' : Locals} {x : Value} {i : Nat} {e : Expr}
    (hl : ps.length ≤ 2) (h : FvOn ((fv e).filter (fun y => !ps.contains y)) L L') :
    FvOn (fv e) (bindParams ps x i L) (bindParams ps x i L') := by
  intro y hy
  by_cases hp : y ∈ ps
  · exact lookup_bindParams_mem hl hp
  · rw [lookup_bindParams_not hp, lookup_bindParams_not hp]
    exact h y (by simpa [List.contains_eq_mem, hp] using hy)

mutual
theorem wkFvE {env : Env} {inFn L L' e v} (h : EvalR env inFn L e v) (hs : Small e = true)
    (ha : FvOn (fv e) L L') : EvalR env inFn L' e v :=
  match h with
  | .num => .num
  | .str => .str
  | .bool => .bool
  | .none => .none
  | .list h => .list (wkFvL h (by simpa [Small] using hs) (by simpa [fv] using ha))
  | .some h => .some (wkFvE h (by simpa [Small] using hs) (by simpa [fv] using ha))
  | .template h => .template (wkFvD h (by simpa [Small] using hs) (by simpa [fv] using ha))
  | .local hl => by
    rename_i x
    exact .local ((ha x (by simp [fv])) ▸ hl)
  | .global hl hi hg => by
    rename_i x
    exact .global ((ha x (by simp [fv])) ▸ hl) hi hg
  | .member h hi hv =>
    .member (wkFvE h (by simpa [Small] using hs) (by simpa [fv] using ha)) hi hv
  | .fn hfd hargs hb => by
    simp only [Small] at hs
    exact .fn hfd (wkFvL hargs hs (by simpa [fv] using ha)) hb
  | .map hfd hl hm => by
    simp only [Small, SmallList, Bool.and_eq_true, decide_eq_true_eq] at hs
    exact .map hfd
      (wkFvE hl hs.1 (ha.sub fun y hy => by simp [fv, fvList, hy]))
      (wkFvM hm hs.2.1.1 hs.2.1.2 (ha.sub fun y hy => by
        simp only [fv, fvList, List.mem_append, List.not_mem_nil, or_false]
        exact .inr hy))
  | .filter hfd hl hm => by
    simp only [Small, SmallList, Bool.and_eq_true, decide_eq_true_eq] at hs
    exact .filter hfd
      (wkFvE hl hs.1 (ha.sub fun y hy => by simp [fv, fvList, hy]))
      (wkFvF hm hs.2.1.1 hs.2.1.2 (ha.sub fun y hy => by
        simp only [fv, fvList, List.mem_append, List.not_mem_nil, or_false]
        exact .inr hy))
  | .sortBy hfd hl hm hd hv => by
    simp only [Small, SmallList, Bool.and_eq_true, decide_eq_true_eq] at hs
    exact .sortBy hfd
      (wkFvE hl hs.1 (ha.sub fun y hy => by simp [fv, fvList, hy] <;> grind))
      (wkFvM hm hs.2.1.1 hs.2.1.2
        (ha.sub fun y hy => by simp [fv, fvList] <;> grind)) hd hv
  | .pendingSettled hfd hr hv => .pendingSettled hfd hr hv
  | .pendingOther hfd hr => .pendingOther hfd hr
  | .failedSettled hfd hr hv => .failedSettled hfd hr hv
  | .failedOther hfd hr => .failedOther hfd hr
  | .failureSettled hfd hr hv => .failureSettled hfd hr hv
  | .failureOther hfd hr => .failureOther hfd hr
  | .currentSettled hfd hr hv => .currentSettled hfd hr hv
  | .currentOther hfd hr => .currentOther hfd hr
  | .stdlib hfd hargs hv => by
    simp only [Small] at hs
    exact .stdlib hfd (wkFvL hargs hs (by simpa [fv] using ha)) hv
  | .record hd h => by
    simp only [Small, Bool.true_and] at hs
    exact .record hd (wkFvR h hs (by simpa [fv, fvOpt] using ha))
  | .recordBase hb hd h => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .recordBase (wkFvE hb hs.1 (ha.sub fun y hy => by simp [fv, fvOpt, hy])) hd
      (wkFvR h hs.2 (ha.sub fun y hy => by simp [fv, fvOpt, hy]))
  | .neg h => .neg (wkFvE h (by simpa [Small] using hs) (by simpa [fv] using ha))
  | .not h => .not (wkFvE h (by simpa [Small] using hs) (by simpa [fv] using ha))
  | .andFalse h => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .andFalse (wkFvE h hs.1 (ha.sub fun y hy => by simp [fv, hy]))
  | .andTrue h1 h2 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .andTrue (wkFvE h1 hs.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.2 (ha.sub fun y hy => by simp [fv, hy]))
  | .orTrue h => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .orTrue (wkFvE h hs.1 (ha.sub fun y hy => by simp [fv, hy]))
  | .orFalse h1 h2 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .orFalse (wkFvE h1 hs.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.2 (ha.sub fun y hy => by simp [fv, hy]))
  | .binop h1 h2 h3 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .binop (wkFvE h1 hs.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.2 (ha.sub fun y hy => by simp [fv, hy])) h3
  | .ternaryTrue h1 h2 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .ternaryTrue (wkFvE h1 hs.1.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.1.2 (ha.sub fun y hy => by simp [fv, hy]))
  | .ternaryFalse h1 h2 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .ternaryFalse (wkFvE h1 hs.1.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.2 (ha.sub fun y hy => by simp [fv, hy]))
  | .matchSome h1 h2 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .matchSome (wkFvE h1 hs.1.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.1.2 (FvOn.cons (ha.sub fun y hy => by simp [fv, hy])))
  | .matchNone h1 h2 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .matchNone (wkFvE h1 hs.1.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.2 (ha.sub fun y hy => by simp [fv, hy]))
  | .letE h1 h2 => by
    simp only [Small, Bool.and_eq_true] at hs
    exact .letE (wkFvE h1 hs.1 (ha.sub fun y hy => by simp [fv, hy]))
      (wkFvE h2 hs.2 (FvOn.cons (ha.sub fun y hy => by simp [fv, hy])))
  | .typed h => .typed (wkFvE h (by simpa [Small] using hs) (by simpa [fv] using ha))

theorem wkFvL {env : Env} {inFn L L' es vs} (h : ListR env inFn L es vs)
    (hs : SmallList es = true) (ha : FvOn (fvList es) L L') : ListR env inFn L' es vs :=
  match h with
  | .nil => .nil
  | .cons h1 h2 => by
    simp only [SmallList, Bool.and_eq_true] at hs
    exact .cons (wkFvE h1 hs.1 (ha.sub fun y hy => by simp [fvList, hy]))
      (wkFvL h2 hs.2 (ha.sub fun y hy => by simp [fvList, hy]))

theorem wkFvD {env : Env} {inFn L L' es ss} (h : DisplaysR env inFn L es ss)
    (hs : SmallList es = true) (ha : FvOn (fvList es) L L') : DisplaysR env inFn L' es ss :=
  match h with
  | .nil => .nil
  | .cons h1 hd h2 => by
    simp only [SmallList, Bool.and_eq_true] at hs
    exact .cons (wkFvE h1 hs.1 (ha.sub fun y hy => by simp [fvList, hy])) hd
      (wkFvD h2 hs.2 (ha.sub fun y hy => by simp [fvList, hy]))

theorem wkFvM {env : Env} {inFn L L' ps body xs i ys} (h : MapR env inFn L ps body xs i ys)
    (hl : ps.length ≤ 2) (hs : Small body = true)
    (ha : FvOn ((fv body).filter (fun y => !ps.contains y)) L L') :
    MapR env inFn L' ps body xs i ys :=
  match h with
  | .nil => .nil
  | .cons h1 h2 => .cons (wkFvE h1 hs (FvOn.params hl ha)) (wkFvM h2 hl hs ha)

theorem wkFvF {env : Env} {inFn L L' ps body xs i ys} (h : FilterR env inFn L ps body xs i ys)
    (hl : ps.length ≤ 2) (hs : Small body = true)
    (ha : FvOn ((fv body).filter (fun y => !ps.contains y)) L L') :
    FilterR env inFn L' ps body xs i ys :=
  match h with
  | .nil => .nil
  | .keep h1 h2 => .keep (wkFvE h1 hs (FvOn.params hl ha)) (wkFvF h2 hl hs ha)
  | .drop h1 h2 => .drop (wkFvE h1 hs (FvOn.params hl ha)) (wkFvF h2 hl hs ha)

theorem wkFvR {env : Env} {inFn L L' w b fs i vs} (h : FieldsR env inFn L w b fs i vs)
    (hs : SmallFields w = true) (ha : FvOn (fvFields w) L L') :
    FieldsR env inFn L' w b fs i vs :=
  match h with
  | .nil => .nil
  | .written he h1 h2 => by
    obtain ⟨k, hk⟩ := lookupField_mem he
    exact .written he (wkFvE h1 (smallField hs he) (ha.sub (fields_fv hk)))
      (wkFvR h2 hs ha)
  | .base he hb h2 => .base he hb (wkFvR h2 hs ha)
end

/-- Evaluation is invariant under changes to locals outside its actual
free value reads. -/
theorem wkFv_iff {env : Env} {inFn L L' e v} (hs : Small e = true)
    (ha : FvOn (fv e) L L') :
    EvalR env inFn L e v ↔ EvalR env inFn L' e v :=
  ⟨fun h => wkFvE h hs ha, fun h => wkFvE h hs fun y hy => (ha y hy).symm⟩

end Contract.ExpandSubst
