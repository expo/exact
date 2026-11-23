/-
The program-level operational semantics: statements, actions as
transactions, rendering, timers, and the events a host delivers, over the
settlement of derives and resources (`Contract.Settle`).

A configuration steps on an event to a new configuration. Each step is one
commit or none: an action reads the state as it was when the event arrived,
its writes land together after its body has run (the last write to a slot
wins), and anything that refuses — a trap, a value of the wrong shape, a
settlement that fails — leaves the configuration exactly as it was. A
failure while rendering poisons the configuration: it keeps its state and
refuses every later event (the runner's `poison`).
-/
import Contract.Settle

namespace Contract

/-- What an action body asks for, in statement order. -/
structure Effects where
  writes : List (String × Value) := []
  rowWrites : List (String × Value) := []
  commands : List (String × List Value) := []
  sends : List (String × String × List Value) := []
  refreshes : List String := []
  deriving Inhabited

def isRootState (p : Program) (x : String) : Bool :=
  p.states.any (fun s => s.name == x && s.owner.isNone) ||
    p.mutations.any (fun m => m.name == x && m.owner.isNone)

/-- A row's slot: a state an arm owns, or a child's mutation, whose answer
is its instance's (LLP 1035.005.004 D6). -/
def isRowState (p : Program) (x : String) : Bool :=
  p.states.any (fun s => s.name == x && s.owner.isSome) ||
    p.mutations.any (fun m => m.name == x && m.owner.isSome)

/-- Run a block of statements. A `let` scopes over the statements after it
in its block and the blocks nested there; a branch's block is a scope of
its own. -/
def exec : Nat → Env → Locals → List Stmt → Effects → Result Effects
  | 0, _, _, _, _ => .error outOfFuel
  | _ + 1, _, _, [], fx => .ok fx
  | fuel + 1, env, ls, s :: rest, fx =>
    match s with
    | .letS x e => do
      let v ← eval fuel env false ls e
      exec fuel env ((x, v) :: ls) rest fx
    | .assign t e => do
      let v ← eval fuel env false ls e
      let fx ←
        if isRootState env.prog t then pure { fx with writes := fx.writes ++ [(t, v)] }
        else if isRowState env.prog t then
          if (lookup t env.rows).isSome then pure { fx with rowWrites := fx.rowWrites ++ [(t, v)] }
          else .error (.refused s!"row slot `{t}` written outside its row")
        else .error (.unbound t)
      exec fuel env ls rest fx
    | .command name args => do
      let vs ← evalList fuel env false ls args
      exec fuel env ls rest { fx with commands := fx.commands ++ [(name, vs)] }
    | .send t src args => do
      let vs ← evalList fuel env false ls args
      exec fuel env ls rest { fx with sends := fx.sends ++ [(t, src, vs)] }
    | .refresh t => exec fuel env ls rest { fx with refreshes := fx.refreshes ++ [t] }
    | .ifS c thn els => do
      let fx ← match ← eval fuel env false ls c with
        | .bool true => exec fuel env ls thn fx
        | .bool false => exec fuel env ls els fx
        | _ => .error (.type "`if` on a value that is not a bool")
      exec fuel env ls rest fx
    | .matchS subj x sm nn => do
      let fx ← match ← eval fuel env false ls subj with
        | .some v => exec fuel env ((x, v) :: ls) sm fx
        | .none => exec fuel env ls nn fx
        | _ => .error (.type "`match` on a value that is not an option")
      exec fuel env ls rest fx
    -- A call (LLP 1089 D9): the arguments in the caller's scope, then the
    -- callee's body in a scope of its parameters alone, against the same
    -- starting state and rows, adding to the same effects. A refusal in
    -- it refuses the action. Fuel bounds the nesting: an accepted program
    -- has no call cycle, and a cycle runs out of fuel.
    | .call a args => do
      let vs ← evalList fuel env false ls args
      match env.prog.actions.find? (·.name == a) with
      | .none => .error (.unbound a)
      | .some ad =>
        if ad.params.length != vs.length then .error (.refused "arity") else
        let fx ← exec fuel env ((ad.params.map (·.1)).zip vs).reverse ad.body fx
        exec fuel env ls rest fx

/-! ## A child's requests (LLP 1035.005.004) -/

/-- The name an instance keeps a request's arguments under, beside its
value: absent while the request is idle (D2). No authored name has an `@`. -/
def ownedArgs (x : String) : String := x ++ "@args"

/-- Set `x` in an instance's slots, adding it when absent. -/
def putSlot (s : List (String × Value)) (x : String) (v : Value) : List (String × Value) :=
  if (lookup x s).isSome then s.map (fun (y, w) => if x == y then (y, v) else (y, w)) else s ++ [(x, v)]

/-- Whether `x` is a child's resource, one each instance owns. -/
def isOwned (p : Program) (x : String) : Bool :=
  p.resources.any fun r => r.name == x && r.owner.isSome

/-- An environment in an instance's rows: each request its rows' instances
own reads as a resource, holding its value where its type admits it,
current unless idle. -/
def Env.withOwned (env : Env) : Env :=
  let own := env.rows.filter fun (x, v) =>
    env.prog.resources.any fun r => r.name == x && r.owner.isSome && conforms env.prog v r.ty
  { env with
    resources := own ++ env.resources,
    idle := (own.filter fun (x, _) => (lookup (ownedArgs x) env.rows).isNone).map (·.1) ++
      env.idle.filter fun x => !isOwned env.prog x }

@[simp] theorem Env.withOwned_prog (env : Env) : env.withOwned.prog = env.prog := rfl
@[simp] theorem Env.withOwned_slots (env : Env) : env.withOwned.slots = env.slots := rfl
@[simp] theorem Env.withOwned_derives (env : Env) : env.withOwned.derives = env.derives := rfl
@[simp] theorem Env.withOwned_rows (env : Env) : env.withOwned.rows = env.rows := rfl
@[simp] theorem Env.withOwned_now (env : Env) : env.withOwned.now = env.now := rfl

/-- The requests an arm instance owns, settled in its scope (D2): each
child resource whose owner is the arm, in declaration order, its `when`
and arguments read with the instance's slots and locals. A request keeps
its value while its arguments are those the value answers; one asked anew
asks the oracle, whose every answer is synchronous; a closed `when` keeps
the value (its placeholder before any), idle, with no arguments, so the
render that opens it asks again. Passes repeat while a request changes, so
one whose arguments read another's answer asks on that answer.

Its slots are kept only where each holds a value its slot type admits (a
request's name is no state's, so any), as `armSlots` checks a fresh slot.
An evaluation that fails, as a gate or argument reading a pending root
resource, refuses the render: the semantics models every owned answer as
synchronous (D2), and nothing here waits. -/
def settleOwned (fuel : Nat) (o : Oracle) (env : Env) (ls : Locals) (owner : Nat × Nat)
    (slots : List (String × Value)) : Result (List (String × Value)) :=
  let rs := env.prog.resources.filter (·.owner == Option.some owner)
  let rec go : Nat → List (String × Value) → Result (List (String × Value))
    | 0, _ => .error (.refused "an instance's requests did not settle")
    | n + 1, slots => do
      let mut s := slots
      let mut changed := false
      for r in rs do
        let env' := ({ env with rows := s ++ env.rows } : Env).withOwned
        let isOpen ← match r.gate with
          | .none => pure true
          | .some g => do pure ((← eval fuel env' false ls g) matches .bool true)
        if !isOpen then
          let v := (lookup r.name s).getD (placeholderValue env.prog r)
          if !conforms env.prog v r.ty then throw (.refused s!"resource `{r.name}` of the wrong shape")
          if (lookup r.name s).isNone || (lookup (ownedArgs r.name) s).isSome then changed := true
          s := (putSlot s r.name v).filter (·.1 != ownedArgs r.name)
        else
          let args ← evalList fuel env' false ls r.args
          let kept := match lookup r.name s, lookup (ownedArgs r.name) s with
            | .some _, .some (.list old) => argsEq old args
            | _, _ => false
          if !kept then
            let v ← o.ask r.source args r.name
            if !conforms env.prog v r.ty then throw (.refused s!"resource `{r.name}` of the wrong shape")
            s := putSlot (putSlot s r.name v) (ownedArgs r.name) (.list args)
            changed := true
      if !changed then return s
      go n s
  match go (rs.length + 2) slots with
  | .ok s =>
    if s.all fun (x, v) => conforms env.prog v (slotTy env.prog x) then .ok s
    else .error (.refused "an instance's request in a slot of another type")
  | .error (.unsupported m) => .error (.unsupported m)
  | .error (.refused m) => .error (.refused m)
  | .error _ => .error (.refused "an instance's request did not evaluate")

/-! ## Rendering -/

/-- A row's identity: the `each` it belongs to, its key (a string, a
finite number with `-0` read as `0`, or a bool; anything else fails the
render, as the runner's `key_text` does), and how many
earlier rows of that list had the same key (the runner's `dup`). A row in
a nested `each` is named by its path from the outermost. -/
abbrev RowId := List (Nat × Value × Nat)

/-- The binder values the render chose for one live region instance. -/
structure RowFrame where
  item : Option Value := .none
  index : Option Nat := .none
  bound : Option Value := .none
  deriving Repr, Inhabited

/-- Each live row's slots. The renderer also adds empty marker rows after
an instance to retain its item, index, or match binding. Their tag is
outside the program's region tags, so they cannot name an authored row. -/
abbrev RowStore := List (RowId × List (String × Value))

def RowStore.find (rs : RowStore) (id : RowId) : Option (List (String × Value)) :=
  (rs.find? fun (i, _) => i.length == id.length &&
    (i.zip id).all fun ((t, k, d), (t', k', d')) => t == t' && d == d' && Value.same k k').map (·.2)

def regionTags : List Node → List Nat
  | [] => []
  | .element _ _ _ _ children :: rest => regionTags children ++ regionTags rest
  | .when tag _ thn els :: rest => tag :: (regionTags thn ++ regionTags els ++ regionTags rest)
  | .matchN tag _ _ someNodes noneNodes :: rest =>
    tag :: (regionTags someNodes ++ regionTags noneNodes ++ regionTags rest)
  | .each tag _ _ _ _ body :: rest => tag :: (regionTags body ++ regionTags rest)

/-- A tag no authored region has. It distinguishes render-owned frame
markers from live instance rows without changing the row-store type. -/
def instanceMarkerTag (p : Program) : Nat :=
  (regionTags p.view).foldl max 0 + 1

def frameMarkerIds (p : Program) (id : RowId) (frame : RowFrame) : List RowId :=
  let marker (kind : Nat) (value : Value) : RowId :=
    id ++ [(instanceMarkerTag p, value, kind)]
  (frame.item.map (marker 0)).toList ++
    (frame.index.map (fun i => marker 1 (.num (F64.ofNat i)))).toList ++
    (frame.bound.map (marker 2)).toList

def frameMarkers (p : Program) (id : RowId) (frame : RowFrame) : RowStore :=
  (frameMarkerIds p id frame).map fun marker => (marker, [])

def recordInstance (p : Program) (live : RowStore) (id : RowId)
    (slots : List (String × Value)) (frame : RowFrame) : RowStore :=
  live ++ [(id, slots)] ++ frameMarkers p id frame

/-- A rendered element: what the view shows and what a host can address. -/
structure VNode where
  tag : String
  testId : Option String
  /-- The first positional argument, displayed: a `text`'s text. -/
  text : Option String
  /-- (event, action, curried arguments): the arguments are evaluated when
  the event arrives, in the element's scope (`locals`, `rows`) against the
  configuration then — the same state the element was rendered from, but
  the clock may have moved since (`performanceNow()`). -/
  handlers : List (String × String × List Expr)
  /-- The names bound where the element stands. -/
  locals : Locals
  /-- The form control it is, when not a text field (`select`, or an
  `input` whose literal `type` is `checkbox`, `radio`, `range`, `date`, …): its
  payloads follow HTML's rules, which the semantics leaves out. -/
  control : Option String := .none
  /-- A synthetic group around a virtualized list's data region: the
  observation omits those rows and retains authored fixed children. -/
  windowed : Bool := false
  navigation : TabInfo := {}
  /-- The rows this element stands in, innermost first. -/
  rows : List RowId
  children : List VNode
  deriving Inhabited

structure RenderCx where
  env : Env
  store : RowStore
  rows : List RowId := []
  tabs : TabCx := {}
  /-- What the instances' requests ask (LLP 1035.005.004 D2). -/
  oracle : Oracle := {}
  enterFrame : RowFrame := {}

/-- An element's text: the first positional argument, displayed, of a tag
whose positional is its text (`text`, `tspan`, `option`). -/
def elementText (fuel : Nat) (env : Env) (ls : Locals) (tag : String) (pos : List Expr) :
    Result (Option String) :=
  match pos with
  | e :: _ =>
    if tag == "text" || tag == "tspan" || tag == "option" then do
      match ← unsetRefused (eval fuel env false ls e) with
      | .some v => pure (Option.some (← v.display))
      | .none => pure Option.none
    else pure Option.none
  | [] => pure Option.none

/-- An element's `testId`, displayed. -/
def elementTestId (fuel : Nat) (env : Env) (ls : Locals) (props : List (String × Expr)) :
    Result (Option String) :=
  match lookupField "testId" props with
  | .some e => do
    match ← unsetRefused (eval fuel env false ls e) with
    | .some v => pure (Option.some (← v.display))
    | .none => pure Option.none
  | .none => pure Option.none

/-- The form control an element is, when not a text field. -/
def elementControl (tag : String) (props : List (String × Expr)) : Option String :=
  if tag == "select" then Option.some "select"
  else if tag == "input" then
    match lookupField "type" props with
    | .some (.str t) =>
      if ["checkbox", "radio", "file", "range", "date", "time", "datetime-local"].contains t.toLower
      then Option.some t else Option.none
    | _ => Option.none
  else Option.none

/-- The navigation fact that decides whether an element's children render. -/
def elementNavigation (fuel : Nat) (env : Env) (ls : Locals) (tabs : TabCx)
    (props : List (String × Expr)) : Result TabInfo := do
  let values ← (tabProps props).mapM fun (name, e) => do
    pure (name, ← unsetRefused (eval fuel env false ls e))
  pure (tabs.element props (values.filterMap fun (n, v) => v.map (n, ·)))

def TabInfo.rendersChildren (navigation : TabInfo) : Bool := navigation.opened

/-- Render a view. Returns the elements and the rows that are live after
it (their slots, kept from `store` or initialized now). -/
def render : Nat → RenderCx → Locals → List Node → RowStore → Result (List VNode × RowStore)
  | 0, _, _, _, _ => .error outOfFuel
  | _ + 1, _, _, [], live => .ok ([], live)
  | fuel + 1, cx, ls, n :: rest, live => do
    let (here, live) ← (match n with
      | .element tag pos props hs children => do
        let text ← elementText fuel cx.env ls tag pos
        let testId ← elementTestId fuel cx.env ls props
        let navigation ← elementNavigation fuel cx.env ls cx.tabs props
        let windowed := tag == "list" && ((lookupField "virtualized" props).map tabLiteral matches .some (.bool true))
        -- Only the direct data region is hidden from observation and tab discovery.
        let children := if windowed then children.map (fun child => match child with
          | .each .. => .element "#window" [] [] [] [child]
          | _ => child) else children
        let (kids, live) ← (if navigation.rendersChildren then
          render fuel { cx with tabs := cx.tabs.children props } ls children live
          else pure ([], live))
        pure ([{ tag, testId, text, handlers := hs, locals := ls, rows := cx.rows,
                 control := elementControl tag props,
                 windowed := tag == "#window",
                 navigation, children := kids : VNode }], live)
      | .when tag c thn els => do
        match ← eval fuel cx.env false ls c with
        | .bool true => renderArm fuel { cx with enterFrame := {} } ls tag 0 thn live
        | .bool false => renderArm fuel { cx with enterFrame := {} } ls tag 1 els live
        | _ => .error (.type "`when` on a value that is not a bool")
      | .matchN tag s x sm nn => do
        match ← eval fuel cx.env false ls s with
        | .some v =>
          renderArm fuel { cx with enterFrame := { bound := some v } } ((x, v) :: ls) tag 0 sm live
        | .none => renderArm fuel { cx with enterFrame := {} } ls tag 1 nn live
        | _ => .error (.type "`match` on a value that is not an option")
      | .each tag x ix list key body => do
        let items ← (← eval fuel cx.env false ls list).asList
        renderRows fuel cx ls tag x ix key body items 0 [] live : Result (List VNode × RowStore))
    let (more, live) ← render fuel { cx with tabs := cx.tabs.next } ls rest live
    pure (here ++ more, live)
where
  /-- The slots an arm instance owns: kept from `store`, or initialized in
  the arm's scope now (an initializer may read the item or binding). -/
  armSlots : Nat → RenderCx → Locals → RowId → Nat × Nat → Result (List (String × Value))
    | fuel, cx, ls, id, owner =>
      match cx.store.find id with
      | .some s => .ok s
      | .none => do
        let mut s : List (String × Value) := []
        for st in cx.env.prog.states do
          if st.owner == Option.some owner then
            let env := { cx.env with rows := s ++ cx.env.rows }
            let v ← eval fuel env false ls st.init
            if !conforms cx.env.prog v st.ty then
              throw (unfit (typed cx.env.prog v st.ty)
                s!"slot `{st.name}` initialized with a number that is not finite"
                s!"slot `{st.name}` initialized with a value of the wrong type")
            s := s ++ [(st.name, v)]
        -- A child's mutation holds no answer until the instance sends it
        -- (its slot type, `option`, admits `none`: its name is no state's).
        for m in cx.env.prog.mutations do
          if m.owner == Option.some owner && conforms cx.env.prog Value.none (slotTy cx.env.prog m.name) then
            s := s ++ [(m.name, Value.none)]
        pure s
  /-- A `when` or `match` arm: an instance of its own, named by the arm. -/
  renderArm : Nat → RenderCx → Locals → Nat → Nat → List Node → RowStore →
      Result (List VNode × RowStore)
    | 0, _, _, _, _, _, _ => .error outOfFuel
    | fuel + 1, cx, ls, tag, arm, body, live => do
      let id : RowId := (cx.rows.head?.getD []) ++ [(tag, Value.num (F64.ofNat arm), 0)]
      let slots ← armSlots fuel cx ls id (tag, arm)
      let slots ← settleOwned fuel cx.oracle cx.env ls (tag, arm) slots
      let cx' : RenderCx := { cx with
        env := Env.withOwned { cx.env with rows := slots ++ cx.env.rows }, rows := id :: cx.rows,
        tabs := cx.tabs.enter (.num (F64.ofNat arm)), enterFrame := {} }
      render fuel cx' ls body (recordInstance cx.env.prog live id slots cx.enterFrame)
  /-- The rows of an `each`, in list order. `seen` holds the keys of the
  rows before this one, for `dup`. -/
  renderRows : Nat → RenderCx → Locals → Nat → String → Option String → Expr → List Node →
      List Value → Nat → List Value → RowStore → Result (List VNode × RowStore)
    | 0, _, _, _, _, _, _, _, _, _, _, _ => .error outOfFuel
    | _ + 1, _, _, _, _, _, _, _, [], _, _, live => .ok ([], live)
    | fuel + 1, cx, ls, tag, x, ix, key, body, item :: items, i, seen, live => do
      let ls' := (x, item) :: ls
      let ls' := match ix with
        | .some n => (n, Value.num (F64.ofNat i)) :: ls'
        | .none => ls'
      let k ← rowKey (← eval fuel cx.env false ls' key)
      let dup := (seen.filter (Value.same k ·)).length
      let id : RowId := (cx.rows.head?.getD []) ++ [(tag, k, dup)]
      -- The row's slots: kept, or initialized from their initializers in
      -- the row's own scope (an initializer may read the item).
      let slots ← armSlots fuel cx ls' id (tag, 0)
      let slots ← settleOwned fuel cx.oracle cx.env ls' (tag, 0) slots
      let cx' : RenderCx := { cx with
        env := Env.withOwned { cx.env with rows := slots ++ cx.env.rows },
        rows := id :: cx.rows, tabs := cx.tabs.enter k dup, enterFrame := {} }
      let frame : RowFrame := { item := some item, index := some i }
      let (vs, live) ← render fuel cx' ls' body
        (recordInstance cx.env.prog live id slots frame)
      let (more, live) ← renderRows fuel cx ls tag x ix key body items (i + 1) (seen ++ [k]) live
      pure (vs ++ more, live)

/-- The selection sees the instance tree, without virtualized rows. -/
def tabIndex : List VNode → Tabs.Index
  | [] => []
  | n@{ children, .. } :: rest => n.navigation ::
      ((if n.windowed then [] else tabIndex children) ++ tabIndex rest)

/-- The tab context from which render and validation traverse a configuration. -/
def renderTabContext (o : Oracle) (view : List VNode) : TabCx :=
  { opened := Tabs.visited (tabIndex view), every := o.everyTab }

/-- Render empty waiting panels, then open the ones selection requires.
Each pass retains the preceding pass's initialized slots. The bound only
limits a render that keeps discovering more nested panels. -/
def renderTabs : Nat → RenderCx → Result (List VNode × RowStore)
  | 0, _ => .error outOfFuel
  | n + 1, cx => do
    let (view, live) ← render fuel cx [] cx.env.prog.view []
    let opened := Tabs.toOpen (tabIndex view)
    if opened.isEmpty then pure (view, live) else
      renderTabs n { cx with
        store := live,
        tabs := { cx.tabs with opened := Tabs.visited (tabIndex view) ++ opened } }

/-! ## Configurations and steps -/

/-- A frame task's `k`th virtual frame after `base`: `base + k·1000/60`,
the product first (the runner's `virtual_frame`), so sixty frames are
exactly a second. -/
def virtualFrame (base : F64) (k : Nat) : F64 := base + F64.ofNat k * 1000 / 60

/-- The least virtual frame at or after a deadline, correcting binary64 division. -/
def frameAtOrAfter (deadline : F64) : F64 :=
  let k := F64.toNat (F64.ceil (deadline * 60 / 1000))
  let k := if k > 0 && deadline ≤ virtualFrame 0 (k - 1) then k - 1 else k
  let k := if virtualFrame 0 k < deadline then k + 1 else k
  virtualFrame 0 k

/-- The first nominal beat strictly after an execution time. -/
def waitBeatAfter (deadline interval execution : F64) : F64 :=
  let k := max 1 (F64.toNat (F64.floor ((execution - deadline) / interval)) + 1)
  let beat := fun k => deadline + F64.ofNat k * interval
  let k := if k > 1 && execution < beat (k - 1) then k - 1 else k
  beat (if beat k ≤ execution then k + 1 else k)

/-- A timer. A frame task's is a virtual display (the runner's, while the
host does not present frames, as in a differential run): it is due at
`virtualFrame base k`, and each fire moves to the next frame. -/
structure TaskState where
  next : F64 := F64.posInf
  nominal : F64 := 0
  base : F64 := 0
  k : Nat := 1
  armed : Bool := false
  armedKey : Option Value := .none
  deriving Inhabited, Repr

structure Timer where
  owner : Option (Nat × Nat) := .none
  args : List Expr := []
  instances : List (RowId × TaskState) := []
  selected : Option RowId := .none
  change : Bool := false
  action : String
  waitKind : Nat := 0
  nominal : F64 := 0
  maxMs : Nat := 0
  interval : F64
  once : Bool
  next : F64
  frame : Bool := false
  base : F64 := 0
  k : Nat := 1
  /-- A gated task's gate and key (LLP 1092 D7); `none` for `mount`. -/
  gate : Option Expr := .none
  key : Option Expr := .none
  /-- Whether its gate holds; an idle timer's `next` is infinite (D8). -/
  armed : Bool := true
  /-- The key it was armed with, as `rowKey` reads it. -/
  armedKey : Option Value := .none
  deriving Inhabited

/-- The timer after it fires: spent, at its next virtual frame, or one
interval on. -/
def Timer.tick (tm : Timer) : Timer :=
  if tm.once then { tm with next := F64.posInf }
  else if tm.frame then { tm with k := tm.k + 1, next := virtualFrame tm.base (tm.k + 1) }
  else if tm.waitKind != 0 then
    let nominal := waitBeatAfter tm.nominal tm.interval tm.next
    { tm with nominal, next := if tm.waitKind == 1 then frameAtOrAfter nominal else nominal }
  else { tm with next := tm.next + tm.interval }

def Timer.state (tm : Timer) : TaskState :=
  ⟨tm.next, tm.nominal, tm.base, tm.k, tm.armed, tm.armedKey⟩

def Timer.withState (tm : Timer) (s : TaskState) : Timer :=
  { tm with next := s.next, nominal := s.nominal, base := s.base, k := s.k,
            armed := s.armed, armedKey := s.armedKey }

theorem Timer.tick_fields (tm : Timer) :
    tm.tick.action = tm.action ∧ tm.tick.gate = tm.gate ∧ tm.tick.key = tm.key ∧ tm.tick.owner = tm.owner := by
  unfold Timer.tick
  split <;> (try split) <;> (try split) <;> exact ⟨rfl, rfl, rfl, rfl⟩

/-- Execution time, then idle priority, nominal time and declaration order. -/
def Timer.precedes (tm b : Timer) : Bool :=
  tm.next < b.next || (tm.next == b.next &&
    ((b.waitKind == 4 && tm.waitKind != 4) ||
      ((b.waitKind == 4) == (tm.waitKind == 4) &&
        (if tm.waitKind == 0 then tm.next else tm.nominal) <
        (if b.waitKind == 0 then b.next else b.nominal))))

/-- Select the earliest instance even when the preceding firing's action refuses. -/
def Timer.selectInstance (tm : Timer) (instances : List (RowId × TaskState)) : Timer :=
  let picked := instances.foldl (fun best item => match best with
    | .none => some item
    | .some (_, s) => if (tm.withState item.2).precedes (tm.withState s) then some item else best) none
  let next := tm.withState (picked.map (·.2) |>.getD {})
  { next with instances, selected := picked.map (·.1) }

def Timer.fired (tm : Timer) : Timer :=
  let fired := tm.tick
  if tm.owner.isNone then fired else
    fired.selectInstance (tm.instances.map fun (id, state) =>
      (id, if tm.selected.any (fun selected => (RowStore.find [(id, [])] selected).isSome) then fired.state else state))

theorem Timer.fired_fields (tm : Timer) :
    tm.fired.action = tm.action ∧ tm.fired.gate = tm.gate ∧ tm.fired.key = tm.key ∧ tm.fired.owner = tm.owner := by
  unfold Timer.fired
  split <;> exact tm.tick_fields

/-- What a refused queued send saw: the slots, and the derives and
resources settled against them (LLP 1092 D3). -/
abbrev Basis := List (String × Value) × Settled

/-- Row identities, compared as `RowStore.find` compares them. -/
def RowId.same (i id : RowId) : Bool :=
  i.length == id.length &&
    (i.zip id).all fun ((t, k, d), (t', k', d')) => t == t' && d == d' && Value.same k k'

/-- The runner's row for one instance's sends to one child's mutation
(LLP 1035.005.004 D6): made at the instance's first send, after the
declared mutations' rows, in the order made. It holds the mutation, the
instance's row while the instance lives, and when its armed `then` is due.
A destroyed instance's row is let go, and the next instance to send that
mutation takes it (the runner's `send_row`). -/
structure SendRow where
  mutation : String
  inst : Option RowId
  due : F64 := F64.posInf
  deriving Inhabited

structure Config where
  slots : List (String × Value)
  settled : Settled
  store : RowStore
  view : List VNode
  now : F64
  timers : List Timer
  poisoned : Bool := false
  /-- The commands committed actions issued, oldest first. -/
  commands : List (String × List Value) := []
  /-- The `then`s armed: each mutation answered in a commit that stood,
  whose `then` runs at the next clock advance, due at the commit's time. -/
  armed : List (String × F64) := []
  /-- The sends that wait their turn, oldest first: each a `queue`
  mutation, its source and its send-time arguments (LLP 1092 D2). -/
  queued : List (String × String × List Value) := []
  /-- The `queue` mutations whose head an advance does not ask until a
  commit changes what its refusal saw (LLP 1092 D3). -/
  stalled : List (String × Basis) := []
  /-- Each `queue` mutation's armed `next`, due at the time of the commit
  after which it was free (the runner's `next_due`, LLP 1092 D3). -/
  nexts : List (String × F64) := []
  /-- The rows of instances' sends to children's mutations. -/
  sendRows : List SendRow := []
  deriving Inhabited

/-- A step's outcome, as a host sees it. -/
inductive Outcome where
  | ok
  | refused (why : Err)
  | poisoned (why : Err)
  deriving Inhabited

/-- A host keeps the first refusal while running the work ahead of an input.
Poison and exhaustion of the firing budget end the advance. -/
def Outcome.afterRefusal (later : Outcome) (first : Err) : Outcome :=
  match later with
  | .poisoned _ => later
  | .refused e => if e == .refused "too many timer fires" then later else .refused first
  | .ok => .refused first

def resumeRefused {α} (applyChanges : Bool) (c : α) (e : Err)
    (resume : α → α × Outcome) : α × Outcome :=
  if applyChanges then
    let rest := resume c
    (rest.1, rest.2.afterRefusal e)
  else (c, .refused e)

def setSlot (slots : List (String × Value)) (x : String) (v : Value) : List (String × Value) :=
  slots.map fun (y, w) => if x == y then (y, v) else (y, w)

/-- The slots of the rows in force, innermost first. -/
def rowSlots (store : RowStore) (rows : List RowId) : List (String × Value) :=
  rows.foldr (fun id acc => (store.find id).getD [] ++ acc) []

/-- The region arm that owns a row slot: a state's, or a child's mutation's. -/
def slotOwner (p : Program) (x : String) : Option (Nat × Nat) :=
  match (p.states.find? (·.name == x)).bind (·.owner) with
  | .some o => .some o
  | .none => (p.mutations.find? (·.name == x)).bind (·.owner)

/-- Apply row writes: each goes to the innermost row in force whose `each`
owns the slot. -/
def applyRowWrites (p : Program) (store : RowStore) (rows : List RowId)
    (ws : List (String × Value)) : RowStore :=
  ws.foldl (fun store (x, v) =>
    let owner := (slotOwner p x).map (·.1)
    match rows.find? (fun id => (id.getLast?.map (·.1)) == owner) with
    | .some id => store.map fun (i, s) =>
        if (RowStore.find [(i, s)] id).isSome then (i, setSlot s x v) else (i, s)
    | .none => store) store

/-- An action's `refresh` of a child's resource reaches only the instance
whose action ran (LLP 1035.005.004 D5): that request forgets its
arguments, so the render asks again. An idle one has none to forget. -/
def refreshOwned (p : Program) (store : RowStore) (rows : List RowId) (names : List String) :
    RowStore :=
  names.foldl (fun store x =>
    match (p.resources.find? (·.name == x)).bind (·.owner) with
    | .none => store
    | .some (tag, _) =>
      match rows.find? (fun id => (id.getLast?.map (·.1)) == Option.some tag) with
      | .some id => store.map fun (i, s) =>
          if (RowStore.find [(i, s)] id).isSome then (i, s.filter (·.1 != ownedArgs x)) else (i, s)
      | .none => store) store

/-- Settle and render against new slots. -/
def update (p : Program) (o : Oracle) (slots : List (String × Value)) (store : RowStore)
    (now : F64) (prev : Settled := {}) (force : List String := []) (view : List VNode := []) :
    Result (Settled × Result (List VNode × RowStore)) := do
  let st ← settle p o slots now prev force
  let env : Env := { prog := p, slots, derives := st.derives, resources := st.resources, now, idle := st.idle }
  .ok (st, renderTabs fuel { env, store, tabs := renderTabContext o view, oracle := o })

/-- Whether the router slot, if the program has one, holds a valid router
(`Route.routerOf`). -/
def routerValid (p : Program) (slots : List (String × Value)) : Bool :=
  match p.router with
  | .none => true
  | .some x =>
    match lookup x slots with
    | .some v => (Route.routerOf p.routes v).isSome
    | .none => true

/-- Arm the `then` of each mutation a commit at `now` sent into (the
runner's `arm_then`: every answer here is synchronous, so every send
landed); arming again moves the due time. -/
def armThens (p : Program) (armed : List (String × F64))
    (sends : List (String × String × List Value)) (now : F64) : List (String × F64) :=
  sends.foldl (fun armed (m, _, _) =>
    if (p.mutations.find? (·.name == m)).any (·.andThen.isSome) then
      armed.filter (·.1 != m) ++ [(m, now)]
    else armed) armed

/-! ## A child's sends (LLP 1035.005.004 D6) -/

/-- Whether `m` is a child's mutation, one each instance sends. -/
def isOwnedMutation (p : Program) (m : String) : Bool :=
  p.mutations.any fun md => md.name == m && md.owner.isSome

/-- The instance a send of a child's mutation `m` from `rows` goes to: the
innermost row in force that its owner's region made, as `applyRowWrites`
finds the row of the slot its answer lands in. -/
def sendInstance (p : Program) (rows : List RowId) (m : String) : Option RowId :=
  match (p.mutations.find? (·.name == m)).bind (·.owner) with
  | .some (tag, _) => rows.find? fun id => (id.getLast?.map (·.1)) == Option.some tag
  | .none => .none

/-- The row instance `id` sends `m` on: the one it holds, else the first
let-go row of `m`, else a new one after the rest. -/
def sendRow (rs : List SendRow) (m : String) (id : RowId) : List SendRow × Nat :=
  match rs.findIdx? (fun r => r.mutation == m && r.inst.any (RowId.same · id)) with
  | .some i => (rs, i)
  | .none =>
    match rs.findIdx? (fun r => r.mutation == m && r.inst.isNone) with
    | .some i => (rs.set i { mutation := m, inst := .some id }, i)
    | .none => (rs ++ [{ mutation := m, inst := .some id }], rs.length)

/-- Rows whose instance a render left out of `live` are let go: nothing
of theirs is armed, and their `then` does not run. -/
def letGo (rs : List SendRow) (live : RowStore) : List SendRow :=
  rs.map fun r =>
    match r.inst with
    | .some id => if (live.find id).isSome then r else { r with inst := .none, due := F64.posInf }
    | .none => r

/-- After a commit at `now` that stood and rendered `live`: each send to a
child's mutation (with the instance it went to) takes its row and arms its
`then` (every answer here lands at once); then the rows of instances the
render destroyed are let go. -/
def landSends (p : Program) (rs : List SendRow) (sent : List (String × RowId)) (now : F64)
    (live : RowStore) : List SendRow :=
  let rs := sent.foldl (fun rs (m, id) =>
    let (rs, i) := sendRow rs m id
    if (p.mutations.find? (·.name == m)).any (·.andThen.isSome) then
      rs.mapIdx fun j r => if j == i then { r with due := now } else r
    else rs) rs
  letGo rs live

/-- Row `i`'s `then`, no longer armed. -/
def disarm (rs : List SendRow) (i : Nat) : List SendRow :=
  rs.mapIdx fun j r => if j == i then { r with due := F64.posInf } else r

/-! ## Gated tasks and queued sends (LLP 1092) -/

/-- Two keys the same, as the runner's `key_text` compares them. -/
def keySame : Option Value → Option Value → Bool
  | .none, .none => true
  | .some a, .some b => Value.same a b
  | _, _ => false

/-- Whether a gate holds: no gate (`key=` alone) always does; a value
that is not `true` is false. -/
def gateOn (env : Env) (ls : Locals := []) : Option Expr → Result Bool
  | .none => .ok true
  | .some g => eval fuel env false ls g >>= fun v =>
    .ok (match v with
      | .bool true => true
      | _ => false)

/-- A timer's key as `rowKey` reads it: none when unkeyed; a key that is
no key refuses (the runner's `TaskKey`). -/
def gateKey (env : Env) (ls : Locals := []) : Option Expr → Result (Option Value)
  | .none => .ok .none
  | .some k => eval fuel env false ls k >>= fun v => rowKey v >>= fun kv => .ok (.some kv)

/-- Where instance `id`'s elements were rendered: their locals and rows in
force. -/
def instanceScope (id : RowId) : List VNode → Option (Locals × List RowId)
  | [] => .none
  | n@{ children, .. } :: rest =>
    if n.rows.head?.any (RowId.same · id) then .some (n.locals, n.rows)
    else (instanceScope id children).orElse fun _ => instanceScope id rest

inductive RegionBinding where
  | when
  | matchN (name : String)
  | each (item : String) (index : Option String)

partial def findRegion (tag : Nat) : List Node → Option RegionBinding
  | [] => none
  | .element _ _ _ _ children :: rest =>
    (findRegion tag children).orElse fun _ => findRegion tag rest
  | .when own _ thn els :: rest =>
    if own == tag then some .when else
      (findRegion tag thn).orElse fun _ =>
        (findRegion tag els).orElse fun _ => findRegion tag rest
  | .matchN own _ name someNodes noneNodes :: rest =>
    if own == tag then some (.matchN name) else
      (findRegion tag someNodes).orElse fun _ =>
        (findRegion tag noneNodes).orElse fun _ => findRegion tag rest
  | .each own item index _ _ body :: rest =>
    if own == tag then some (.each item index) else
      (findRegion tag body).orElse fun _ => findRegion tag rest

def markerValue (p : Program) (store : RowStore) (id : RowId) (kind : Nat) : Option Value :=
  (store.find? fun (candidate, _) =>
    candidate.length == id.length + 1 && RowId.same (candidate.take id.length) id &&
      match candidate.getLast? with
      | some (tag, _, markerKind) => tag == instanceMarkerTag p && markerKind == kind
      | none => false).bind fun (candidate, _) => candidate.getLast?.map (·.2.1)

/-- An instance's bindings, including when its view has no elements. -/
def recordedScope (p : Program) (store : RowStore) (id : RowId) : Locals × List RowId := Id.run do
  let mut locals : Locals := []
  let mut rows : List RowId := []
  for n in [0:id.length] do
    let path := id.take (n + 1)
    rows := path :: rows
    let some (tag, _, _) := path.getLast? | continue
    match findRegion tag p.view with
    | some (.matchN name) =>
      if let some value := markerValue p store path 2 then
        locals := (name, value) :: locals
    | some (.each item index) =>
      if let some value := markerValue p store path 0 then
        locals := (item, value) :: locals
      if let some name := index then
        if let some value := markerValue p store path 1 then
          locals := (name, value) :: locals
    | _ => pure ()
  return (locals, rows)

/-- One timer after a commit (D8): a false gate drops it (an infinite
deadline); a gate turned true, or a changed key, arms it from the commit's
time — a frame task's virtual frames start again there (the runner's
`gate_step`); anything else leaves it as it was. -/
def gateTimer (env : Env) (tm : Timer) (ls : Locals := []) : Result Timer :=
  match tm.gate, tm.key with
  | .none, .none => .ok tm
  | g, k =>
    gateOn env ls g >>= fun on =>
    if !on then .ok { tm with armed := false, armedKey := .none, next := F64.posInf } else
    gateKey env ls k >>= fun key =>
    .ok (if tm.armed && keySame key tm.armedKey then tm
      else if tm.frame then
        { tm with armed := true, armedKey := key, base := env.now, k := 1,
                  next := virtualFrame env.now 1 }
      else { tm with armed := true, armedKey := key, nominal := env.now + tm.interval, next := (if tm.waitKind == 1 then frameAtOrAfter (env.now + tm.interval) else env.now + tm.interval) })

theorem gateTimer_fields {env : Env} {tm tm' : Timer} {ls : Locals} (h : gateTimer env tm ls = .ok tm') :
    tm'.action = tm.action ∧ tm'.gate = tm.gate ∧ tm'.key = tm.key ∧ tm'.owner = tm.owner := by
  unfold gateTimer at h
  split at h
  · cases h; exact ⟨rfl, rfl, rfl, rfl⟩
  · simp only [bind, Except.bind] at h
    split at h
    · cases h
    · split at h
      · cases h; exact ⟨rfl, rfl, rfl, rfl⟩
      · split at h
        · cases h
        · cases h
          split
          · exact ⟨rfl, rfl, rfl, rfl⟩
          · split <;> exact ⟨rfl, rfl, rfl, rfl⟩

theorem gateTimer_action {env : Env} {tm tm' : Timer} (h : gateTimer env tm = .ok tm') :
    tm'.action = tm.action :=
  (gateTimer_fields h).1

def eachTags : List Node → List Nat
  | [] => []
  | .element _ _ _ _ children :: rest => eachTags children ++ eachTags rest
  | .when _ _ thn els :: rest => eachTags thn ++ eachTags els ++ eachTags rest
  | .matchN _ _ _ sm nn :: rest => eachTags sm ++ eachTags nn ++ eachTags rest
  | .each tag _ _ _ _ body :: rest => tag :: (eachTags body ++ eachTags rest)

/-- The task states of the live instances of one declaration. A render's
locals supply its props; each instance keeps its phase until its gate or key changes.
Surviving instances retain creation order, with newly mounted instances appended. -/
def taskInstances (env : Env) (store : RowStore) (_view : List VNode) (tm : Timer)
    (owner : Nat × Nat) : Result (List (RowId × TaskState)) := do
  let live := store.filter fun (id, _) => id.getLast?.any fun (tag, key, _) =>
    tag == owner.1 && (Value.same key (.num (F64.ofNat owner.2)) ||
      (eachTags env.prog.view).contains owner.1)
  let kept := tm.instances.filter fun (id, _) => live.any fun (i, _) => RowId.same i id
  let added := live.filter fun (id, _) => !(tm.instances.any fun (i, _) => RowId.same i id)
  (kept.map (·.1) ++ added.map (·.1)).mapM fun id => do
    let scope := recordedScope env.prog store id
    let old := (tm.instances.find? fun (i, _) => RowId.same i id).map (·.2)
    let due := env.now + tm.interval
    let initial : Timer :=
      { tm with
        armed := !(tm.gate.isSome || tm.key.isSome)
        base := env.now
        k := 1
        nominal := due
        next := if tm.frame then virtualFrame env.now 1 else if tm.waitKind == 1 then frameAtOrAfter due else due }
    let current := old.map (tm.withState) |>.getD initial
    let ownedEnv := ({ env with rows := rowSlots store scope.2 } : Env).withOwned
    let g ← gateTimer ownedEnv current scope.1
    pure (id, g.state)

/-- One declared timer, with the earliest live instance selected for the clock.
The declaration's action, gate and key stay fixed. -/
def gateTask (env : Env) (store : RowStore) (view : List VNode) (tm : Timer) : Result Timer :=
  match tm.owner with
  | .none => gateTimer env tm
  | .some owner =>
    match taskInstances env store view tm owner with
    | .error _ => .error (.refused "a child's task gate or key did not evaluate")
    | .ok instances => .ok (tm.selectInstance instances)

theorem gateTask_fields {env store view tm tm'} (h : gateTask env store view tm = .ok tm') :
    tm'.action = tm.action ∧ tm'.gate = tm.gate ∧ tm'.key = tm.key ∧ tm'.owner = tm.owner := by
  unfold gateTask at h
  split at h
  · exact gateTimer_fields h
  · split at h
    · cases h
    · cases h; exact ⟨rfl, rfl, rfl, rfl⟩

/-- D8's step over a commit's settled state, at its time. -/
def gateStep (p : Program) (slots : List (String × Value)) (st : Settled) (now : F64)
    (timers : List Timer) (store : RowStore := []) (view : List VNode := []) : Result (List Timer) :=
  timers.mapM (gateTask { prog := p, slots, derives := st.derives, resources := st.resources, now, idle := st.idle } store view)

theorem mapM_gateTimer_actions {env : Env} {store view} :
    ∀ {ts ts' : List Timer}, ts.mapM (gateTask env store view) = .ok ts' → ts'.map (·.action) = ts.map (·.action)
  | [], ts', h => by cases h; rfl
  | t :: ts, ts', h => by
    simp only [List.mapM_cons, bind, Except.bind] at h
    cases ht : gateTask env store view t with
    | error e => rw [ht] at h; cases h
    | ok t' =>
      rw [ht] at h
      cases hs : ts.mapM (gateTask env store view) with
      | error e => rw [hs] at h; cases h
      | ok ts₂ =>
        rw [hs] at h; cases h
        simp only [List.map_cons, (gateTask_fields ht).1, mapM_gateTimer_actions hs]

theorem mapM_gateTimer_mem {env : Env} {store view} :
    ∀ {ts ts' : List Timer}, ts.mapM (gateTask env store view) = .ok ts' → ∀ tm' ∈ ts', ∃ tm ∈ ts,
      tm'.action = tm.action ∧ tm'.gate = tm.gate ∧ tm'.key = tm.key ∧ tm'.owner = tm.owner
  | [], ts', h, _, hm => by cases h; cases hm
  | t :: ts, ts', h, tm', hm => by
    simp only [List.mapM_cons, bind, Except.bind] at h
    cases ht : gateTask env store view t with
    | error e => rw [ht] at h; cases h
    | ok t' =>
      rw [ht] at h
      cases hs : ts.mapM (gateTask env store view) with
      | error e => rw [hs] at h; cases h
      | ok ts₂ =>
        rw [hs] at h; cases h
        rcases List.mem_cons.mp hm with rfl | hm
        · exact ⟨t, List.mem_cons_self .., gateTask_fields ht⟩
        · obtain ⟨tm, h₁, h₂⟩ := mapM_gateTimer_mem hs tm' hm
          exact ⟨tm, List.mem_cons_of_mem _ h₁, h₂⟩

/-- The gate step keeps every timer, its gate and its key; it moves only
whether it is armed and its deadline. -/
theorem gateStep_mem {p slots st now ts ts' store view} (h : gateStep p slots st now ts store view = .ok ts') :
    ∀ tm' ∈ ts', ∃ tm ∈ ts, tm'.action = tm.action ∧ tm'.gate = tm.gate ∧ tm'.key = tm.key ∧ tm'.owner = tm.owner :=
  mapM_gateTimer_mem h

/-- The gate step keeps every timer's action. -/
theorem gateStep_actions {p slots st now ts ts' store view} (h : gateStep p slots st now ts store view = .ok ts') :
    ts'.map (·.action) = ts.map (·.action) :=
  mapM_gateTimer_actions h

/-- A timer after a step whose timers keep their actions runs one of theirs. -/
theorem action_mem_of_map {ts ts' : List Timer} (h : ts'.map (·.action) = ts.map (·.action))
    {tm} (hm : tm ∈ ts') : ∃ tm₀ ∈ ts, tm₀.action = tm.action := by
  have : tm.action ∈ ts.map (·.action) := h ▸ List.mem_map_of_mem hm
  simpa using this

def sameBinds (a b : List (String × Value)) : Bool :=
  a.length == b.length && (a.zip b).all fun ((x, v), (y, w)) => x == y && Value.same v w

/-- Whether a slot, a derive or a resource differs from what `b` saw. -/
def Config.moved (c : Config) (b : Basis) : Bool :=
  !(sameBinds b.1 c.slots && sameBinds b.2.derives c.settled.derives &&
    sameBinds b.2.resources c.settled.resources)

/-- After a commit that stood: the stalls its change lets go (D3). -/
def Config.unstall (c : Config) : Config :=
  { c with stalled := c.stalled.filter fun (_, b) => !c.moved b }

/-- Whether `m` is a `queue` mutation of `p`. -/
def isQueue (p : Program) (m : String) : Bool :=
  p.mutations.any fun md => md.name == m && md.queue

/-- Which of an action's sends are asked in its commit and which wait
(LLP 1092 D2): a send of a `queue` mutation is asked only when nothing of
it waits, it is not stalled, and no send of it came earlier in the commit;
otherwise it joins its queue with its arguments. Free is the runner's
`idle`: neither its `then` nor its `next` armed, and not stalled. A send of
a child's mutation is neither: its instance's (`runAction`). -/
def splitSends (p : Program) (c : Config) (sends : List (String × String × List Value)) :
    List (String × String × List Value) × List (String × String × List Value) :=
  sends.foldl (fun (acc : List (String × String × List Value) × List (String × String × List Value)) s =>
    if isOwnedMutation p s.1 then acc else
    if isQueue p s.1 && (acc.1.any (·.1 == s.1) || acc.2.any (·.1 == s.1) ||
        c.queued.any (·.1 == s.1) || c.stalled.any (·.1 == s.1) ||
        c.armed.any (·.1 == s.1) || c.nexts.any (·.1 == s.1))
    then (acc.1, acc.2 ++ [s]) else (acc.1 ++ [s], acc.2)) ([], [])

theorem splitSends_go_sub {p : Program} {c : Config} :
    ∀ (l : List (String × String × List Value)) acc s, s ∈ (l.foldl (fun (acc : List (String × String × List Value) × List (String × String × List Value)) s =>
      if isOwnedMutation p s.1 then acc else
      if isQueue p s.1 && (acc.1.any (·.1 == s.1) || acc.2.any (·.1 == s.1) ||
          c.queued.any (·.1 == s.1) || c.stalled.any (·.1 == s.1) ||
          c.armed.any (·.1 == s.1) || c.nexts.any (·.1 == s.1))
      then (acc.1, acc.2 ++ [s]) else (acc.1 ++ [s], acc.2)) acc).1 → s ∈ acc.1 ∨ s ∈ l
  | [], _, _, h => .inl h
  | x :: l, acc, s, h => by
    rw [List.foldl_cons] at h
    rcases splitSends_go_sub l _ s h with h | h
    · split at h
      · exact .inl h
      split at h
      · exact .inl h
      · rcases List.mem_append.mp h with h | h
        · exact .inl h
        · simp only [List.mem_singleton] at h; exact .inr (h ▸ List.mem_cons_self ..)
    · exact .inr (List.mem_cons_of_mem _ h)

/-- A send asked now is one of the action's. -/
theorem splitSends_sub {p : Program} {c : Config} {l : List (String × String × List Value)} {s}
    (h : s ∈ (splitSends p c l).1) : s ∈ l :=
  (splitSends_go_sub l _ s h).resolve_left (by simp)

@[simp] theorem splitSends_nil {p : Program} {c : Config} : splitSends p c [] = ([], []) := rfl

/-- The scan after every commit concludes (the runner's `arm_next`): each
`queue` mutation, in declaration order, with a send waiting, no stall, no
`then` armed and no `next` armed yet, has its `next` due now. -/
def Config.armNexts (p : Program) (c : Config) : Config :=
  { c with nexts := p.mutations.foldl (fun ns md =>
      if md.queue && c.queued.any (·.1 == md.name) && !c.stalled.any (·.1 == md.name) &&
          !c.armed.any (·.1 == md.name) && !ns.any (·.1 == md.name)
      then ns ++ [(md.name, c.now)] else ns) c.nexts }

/-- At most this many sends wait per mutation (the runner's `QUEUE_BOUND`). -/
def queueBound : Nat := 64

/-- The runner consumes `setMode` and the seven-argument row form of
`scrollIntoView` in the commit (LLP 1121 D5; LLP 1070.000 §12–13).
Their presentation and layout effects are outside the semantics.
The four-argument element form of `scrollIntoView` reaches the host. -/
def hostCommands (cs : List (String × List Value)) : List (String × List Value) :=
  cs.filter fun (name, args) =>
    name != "setMode" && !(name == "scrollIntoView" && args.length == 7)

/-- A send of a child's mutation from `rows`, asked now (LLP 1035.005.004
D6): the mutation, the instance it goes to and the answer for the
instance's slot. -/
def sendOwned (p : Program) (o : Oracle) (rows : List RowId) :
    String × String × List Value → Result (String × RowId × Value)
  | (m, src, vs) => do
    if isQueue p m then throw (.unsupported "a child's `queue` mutation")
    let .some id := sendInstance p rows m
      | throw (.refused s!"mutation `{m}` sent outside its instance")
    let v ← o.ask src vs
    let ty := ((p.mutations.find? (·.name == m)).map (·.ty)).getD .unknown
    if !conforms p v ty then throw (.refused s!"mutation `{m}` answered with the wrong shape")
    pure (m, id, Value.some v)

/-- Run an action as one commit. `rows` are the rows in force where the
event arrived (none for a timer). -/
def runAction (p : Program) (o : Oracle) (c : Config) (name : String) (args : List Value)
    (rows : List RowId) : Config × Outcome :=
  let refuse (e : Err) := (c, Outcome.refused e)
  if c.poisoned then refuse (.refused "poisoned") else
  match p.actions.find? (·.name == name) with
  | .none => refuse (.unbound name)
  | .some a =>
    if a.params.length != args.length then refuse (.refused "arity") else
    if !((a.params.zip args).all fun (q, v) => argOk p q v) then
      refuse (.refused "an argument of the wrong type") else
    let env : Env := ({ prog := p, slots := c.slots, derives := c.settled.derives,
                        resources := c.settled.resources, rows := rowSlots c.store rows, now := c.now, idle := c.settled.idle } : Env).withOwned
    let ls : Locals := ((a.params.map (·.1)).zip args).reverse
    match exec fuel env ls a.body {} with
    | .error e => refuse e
    | .ok fx =>
      -- A queue's send waits its turn unless it may be asked now (LLP 1092 D2).
      let split := splitSends p c fx.sends
      let queued := c.queued ++ split.2
      if split.2.any (fun s => (queued.filter (·.1 == s.1)).length > queueBound) then
        refuse (.refused "a queue is full") else
      -- Sends ask their source now; an answer lands in the mutation's slot.
      let answered : Result (List (String × Value)) := split.1.mapM fun (m, src, vs) => do
        let v ← o.ask src vs
        let ty := ((p.mutations.find? (·.name == m)).map (·.ty)).getD .unknown
        if !conforms p v ty then throw (.refused s!"mutation `{m}` answered with the wrong shape")
        pure (m, Value.some v)
      match answered with
      | .error e => refuse e
      | .ok answered =>
      let sent := (fx.sends.filter fun s => isOwnedMutation p s.1).mapM (sendOwned p o rows)
      match sent with
      | .error e => refuse e
      | .ok sent =>
        if !((fx.writes ++ fx.rowWrites).all fun (x, v) => conforms p v (slotTy p x)) then
          refuse (unfit ((fx.writes ++ fx.rowWrites).all fun (x, v) => typed p v (slotTy p x))
            "a write of a number that is not finite" "a write of the wrong type") else
        let slots := (answered ++ fx.writes).foldl (fun s (x, v) => setSlot s x v) c.slots
        -- The router slot must hold a valid router after every commit (the
        -- runner's `router_change`): a forged value is refused.
        if !routerValid p slots then refuse (.refused "an invalid router value") else
        -- An instance's answers land before its action's writes, as the
        -- root's do.
        let rowWrites := sent.map (fun (s : String × RowId × Value) => (s.1, s.2.2)) ++ fx.rowWrites
        let store := refreshOwned p (applyRowWrites p c.store rows rowWrites) rows fx.refreshes
        match update p o slots store c.now c.settled fx.refreshes c.view with
        | .error e => refuse e
        | .ok (st, r) =>
          -- D8's gate step, after settlement and inside the commit: a
          -- refusal there refuses it, before the view is touched.
          match gateStep p slots st c.now c.timers (r.toOption.map (·.2) |>.getD []) (r.toOption.map (·.1) |>.getD []) with
          | .error e => refuse e
          | .ok timers =>
            match r with
            | .ok (view, live) =>
              (({ c with slots, settled := st, store := live, view, timers,
                         commands := c.commands ++ hostCommands fx.commands, queued,
                         armed := armThens p c.armed split.1 c.now,
                         sendRows := landSends p c.sendRows (sent.map fun (s : String × RowId × Value) => (s.1, s.2.1)) c.now live
                       }.unstall).armNexts p, .ok)
            | .error e =>
              ({ c with slots, settled := st, store, poisoned := true,
                        commands := c.commands ++ hostCommands fx.commands, queued := [], stalled := [],
                        nexts := [] }, .poisoned e)

/-- The armed `next` due earliest by `t` (ties by declaration order): the
mutation and its due time (the runner's `due_next`). -/
def dueNext (p : Program) (nexts : List (String × F64)) (t : F64) : Option (String × F64) :=
  p.mutations.foldl (fun best m =>
    if m.queue then
      match nexts.find? (·.1 == m.name) with
      | .some (_, w) =>
        if w ≤ t then
          match best with
          | .none => Option.some (m.name, w)
          | .some (_, b) => if w < b then Option.some (m.name, w) else best
        else best
      | .none => best
    else best) Option.none

/-- A `next` commit (D3): `m`'s oldest waiting send, asked as an action's
send is. Its own ask's refusal drops it (it could never be asked); any
other refusal keeps it and stalls `m` until a commit changes the state.
A landed answer arms `m`'s `then`; every outcome ends with the scan. -/
def nextCommit (p : Program) (o : Oracle) (c : Config) (m : String) : Config × Outcome :=
  if c.poisoned then (c, .refused (.refused "poisoned")) else
  match c.queued.find? (·.1 == m) with
  | .none => (c, .ok)
  | .some (_, src, vs) =>
    let c₀ := { c with queued := c.queued.eraseP (·.1 == m) }
    let ty := ((p.mutations.find? (·.name == m)).map (·.ty)).getD .unknown
    match o.ask src vs with
    | .error e => (c₀.armNexts p, .refused e)
    | .ok v =>
      if !conforms p v ty then
        (c₀.armNexts p, .refused (.refused s!"mutation `{m}` answered with the wrong shape")) else
      let stall (e : Err) : Config × Outcome :=
        ({ c with stalled := c.stalled ++ [(m, (c.slots, c.settled))] }.armNexts p, .refused e)
      let slots := setSlot c.slots m (.some v)
      if !routerValid p slots then stall (.refused "an invalid router value") else
      match update p o slots c.store c.now c.settled [] c.view with
      | .error e => stall e
      | .ok (st, r) =>
        match gateStep p slots st c.now c.timers (r.toOption.map (·.2) |>.getD []) (r.toOption.map (·.1) |>.getD []) with
        | .error e => stall e
        | .ok timers =>
          match r with
          | .ok (view, live) =>
            (({ c₀ with slots, settled := st, store := live, view, timers,
                         armed := armThens p c.armed [(m, src, vs)] c.now,
                         sendRows := letGo c.sendRows live }.unstall).armNexts p, .ok)
          | .error e => ({ c₀ with slots, settled := st, poisoned := true, queued := [], stalled := [],
                                    nexts := [] }, .poisoned e)

/-- A root slot's boot value (LLP 1116 D5): for a persisted state, the
store's value when it is one of the state's type; otherwise `v`, the
initializer's (a value of another shape is ignored, as the runner keeps
the initializer's and journals it). -/
def restore (p : Program) (o : Oracle) (st : StateDecl) (v : Value) : Value :=
  match st.persist, lookup st.name o.stored with
  | true, .some w => if conforms p w st.ty then w else v
  | _, _ => v

/-- The root slots at boot, in declaration order (an initializer reads the
slots before it), then the mutations' (`none`). A late slot holds `()`
until boot settlement is done (the runner's slots start as unit). -/
def initSlots (p : Program) (o : Oracle) : Result (List (String × Value)) := do
  let slots ← p.states.foldlM (fun slots st => do
      if st.owner.isSome then return slots
      if st.late then return slots ++ [(st.name, Value.unit)]
      -- The router slot starts at the launch of `/` (the location the
      -- harness's `Runner::boot` launches), never its initializer.
      if p.router == Option.some st.name then
        match Route.launch p.routes "/" with
        | .ok r => return slots ++ [(st.name, Route.routerValue p.routes r)]
        | .error why => throw (.refused s!"router launch refused: {why}")
      let env : Env := { prog := p, slots, now := 0 }
      let v ← eval fuel env false [] st.init
      if !conforms p v st.ty then
        throw (unfit (typed p v st.ty) s!"slot `{st.name}` initialized with a number that is not finite"
          s!"slot `{st.name}` initialized with the wrong type")
      pure (slots ++ [(st.name, restore p o st v)])) []
  pure (slots ++ p.mutations.map (·.name, Value.none))

/-- A restored value is of the state's type when the initializer's is. -/
theorem restore_conforms {p : Program} {o : Oracle} {st : StateDecl} {v : Value}
    (h : conforms p v st.ty = true) : conforms p (restore p o st v) st.ty = true := by
  unfold restore
  split
  · split
    · assumption
    · exact h
  · exact h

/-- A restored value is the initializer's, or the store's for a persisted
state, of its type. -/
theorem restore_cases (p : Program) (o : Oracle) (st : StateDecl) (v : Value) :
    restore p o st v = v ∨ (st.persist = true ∧ lookup st.name o.stored = .some (restore p o st v) ∧
      conforms p (restore p o st v) st.ty = true) := by
  unfold restore
  cases hp : st.persist <;> cases hl : lookup st.name o.stored
  · exact .inl rfl
  · exact .inl rfl
  · exact .inl rfl
  · rename_i w
    by_cases hc : conforms p w st.ty = true
    · simp [hc]
    · simp only [hc]; exact .inl rfl

/-- The timers boot starts, one per task. -/
def startTimers (p : Program) (slots : List (String × Value)) : Result (List Timer) :=
  p.tasks.mapM fun t => do
    -- A frame task's first virtual frame follows boot (at 0); its `ms`
    -- is a placeholder, never read.
    -- A gated task starts idle; boot's gate step arms it (LLP 1092 D8).
    let gated := t.owner.isSome || t.gate.isSome || t.key.isSome
    if t.kind == .frame then
      return { owner := t.owner, args := t.args, action := t.action, interval := 0, once := false,
               next := if gated then F64.posInf else virtualFrame 0 1, frame := true, base := 0, k := 1,
               gate := t.gate, key := t.key, armed := !gated }
    let ms ← (← eval fuel { prog := p, slots } false [] t.ms).asNum
    pure { owner := t.owner, args := t.args, action := t.action, interval := ms, once := t.kind == .after || t.kind == .change, change := t.kind == .change,
           waitKind := t.waitKind, maxMs := t.maxMs, nominal := ms,
           next := if gated then F64.posInf else if t.waitKind == 1 then frameAtOrAfter ms else ms, gate := t.gate, key := t.key, armed := !gated }

/-- The late slots, in declaration order, against the settled values. -/
def lateSlots (p : Program) (st : Settled) (slots : List (String × Value)) :
    Result (List (String × Value)) :=
  p.states.foldlM (fun slots decl => do
    if !decl.late || decl.owner.isSome then return slots
    let env : Env := { prog := p, slots, derives := st.derives, resources := st.resources, now := 0, idle := st.idle }
    let v ← eval fuel env false [] decl.init
    if !conforms p v decl.ty then
      throw (unfit (typed p v decl.ty) s!"slot `{decl.name}` initialized with a number that is not finite"
        s!"slot `{decl.name}` initialized with the wrong type")
    pure (setSlot slots decl.name v)) slots

/-- The configuration before boot, and after a boot that is refused. -/
def Config.empty : Config := { slots := [], settled := {}, store := [], view := [], now := 0, timers := [] }

/-- Boot: initialize the root slots, settle, initialize the late slots,
render, and start the timers. -/
def boot (p : Program) (o : Oracle) : Config × Outcome :=
  let empty := Config.empty
  match initSlots p o with
  | .error e => (empty, .refused e)
  | .ok slots =>
    match startTimers p slots with
    | .error e => (empty, .refused e)
    | .ok timers =>
      match settle p o slots 0 (lateReady := false) with
      | .error e => (empty, .refused e)
      | .ok st =>
      match lateSlots p st slots with
      | .error e => (empty, .refused e)
      | .ok slots =>
      -- A late resource reads the late slots just made: settle again
      -- (LLP 1035.005.004 D2), as the runner does only when there is one.
      match (if p.resources.any (·.late) then settle p o slots 0 st else Except.ok st) with
      | .error e => (empty, .refused e)
      | .ok st =>
      match gateStep p slots st 0 timers with
      | .error e => (empty, .refused e)
      | .ok timers =>
      let env : Env := { prog := p, slots, derives := st.derives, resources := st.resources, now := 0, idle := st.idle }
      match renderTabs fuel { env, store := [], tabs := renderTabContext o [], oracle := o } with
      | .ok (view, live) =>
        match gateStep p slots st 0 timers live view with
        | .error e => (empty, .refused e)
        | .ok timers => ({ slots, settled := st, store := live, view, now := 0, timers }, .ok)
      -- A view that fails to render at boot fails the boot: there is no
      -- runner to poison.
      | .error e => (empty, .refused e)

/-! ## Events -/

/-- The first element, in preorder, whose `testId` is `id`. -/
def findTestId (id : String) : List VNode → Option VNode
  | [] => .none
  | n@{ children, .. } :: rest =>
    if n.testId == Option.some id then .some n
    else match findTestId id children with
      | .some m => .some m
      | .none => findTestId id rest

/-- Deliver `event` (with its payload, if it has one) to the element
`id`: its handler's curried arguments, then the payload, are the action's
arguments. -/
def dispatch (p : Program) (o : Oracle) (c : Config) (target : String) (event : String)
    (payload : Option Value) : Config × Outcome :=
  if c.poisoned then (c, .refused (.refused "poisoned")) else
  match findTestId target c.view with
  | .none => (c, .refused (.refused s!"no element `{target}`"))
  | .some n =>
    match n.handlers.find? (·.1 == event) with
    | .none => (c, .refused (.refused s!"`{target}` has no {event} handler"))
    | .some (_, a, args) =>
      if payload.isSome && n.control.isSome then
        (c, .refused (.unsupported s!"a payload for a `{n.control.getD ""}` control")) else
      let env : Env := ({ prog := p, slots := c.slots, derives := c.settled.derives,
                          resources := c.settled.resources, rows := rowSlots c.store n.rows,
                          now := c.now, idle := c.settled.idle } : Env).withOwned
      match evalList fuel env false n.locals args with
      | .error e => (c, .refused e)
      | .ok vs => runAction p o c a (vs ++ payload.toList) n.rows

/-- The most timer fires one advance makes. -/
def timerFireLimit : Nat := 4096

/-- The actions the clock may run besides the tasks': mutations' `then`s. -/
def thenActions (p : Program) : List String := p.mutations.filterMap (·.andThen)

/-- The armed `then` due earliest by `t` (ties by declaration order): the
mutation, its due time and its action. -/
def dueThen (p : Program) (armed : List (String × F64)) (t : F64) :
    Option (String × F64 × String) :=
  p.mutations.foldl (fun best m =>
    match m.andThen, armed.find? (·.1 == m.name) with
    | .some a, .some (_, w) =>
      if w ≤ t then
        match best with
        | .none => Option.some (m.name, w, a)
        | .some (_, b, _) => if w < b then Option.some (m.name, w, a) else best
      else best
    | _, _ => best) Option.none

/-- The send rows with their indices, in the order a tie between their
`then`s goes: by the mutation's declaration, then by row (the runner's
`advance`). -/
def sendOrder (p : Program) (rs : List SendRow) : List (SendRow × Nat) :=
  p.mutations.flatMap fun m => rs.zipIdx.filter (·.1.mutation == m.name)

/-- The send row whose armed `then` is due earliest by `t`, ties in
`sendOrder`: the row, its due time and its action. -/
def dueOwned (p : Program) (rs : List SendRow) (t : F64) : Option (Nat × F64 × String) :=
  (sendOrder p rs).foldl (fun best (r, i) =>
    match r.inst, (p.mutations.find? (·.name == r.mutation)).bind (·.andThen) with
    | .some _, .some a =>
      if r.due ≤ t then
        match best with
        | .none => Option.some (i, r.due, a)
        | .some (_, b, _) => if r.due < b then Option.some (i, r.due, a) else best
      else best
    | _, _ => best) Option.none

/-- What the clock runs besides a timer: a `queue` mutation's `next`, a
mutation's `then`, or the `then` of an instance's send row. -/
inductive Clock where
  | next (m : String) (w : F64)
  | andThen (m : String) (w : F64) (a : String)
  | owned (i : Nat) (w : F64) (a : String)

/-- The armed `then` the clock runs by `t`, when it is due no later than
a timer due at `timer`: the earliest, a root mutation's on a tie (the
root's mutations are declared before those lifted from children). -/
def thenPick (p : Program) (c : Config) (t : F64) (timer : Option F64) : Option Clock :=
  match dueThen p c.armed t, dueOwned p c.sendRows t with
  | .some (m, w, a), .some (i, w', a') =>
    if w' < w then (if timer.all (w' ≤ ·) then Option.some (.owned i w' a') else Option.none)
    else if timer.all (w ≤ ·) then Option.some (.andThen m w a) else Option.none
  | .some (m, w, a), .none => if timer.all (w ≤ ·) then Option.some (.andThen m w a) else Option.none
  | .none, .some (i, w, a) => if timer.all (w ≤ ·) then Option.some (.owned i w a) else Option.none
  | .none, .none => Option.none

/-- What the clock runs next by `t`, before a timer due at `timer` (the
runner's `advance`): a `next` due before any armed `then` and no later than
the timer; else a `then` due no later than the timer; else nothing. -/
def clockPick (p : Program) (c : Config) (t : F64) (timer : Option F64) : Option Clock :=
  match dueNext p c.nexts t with
  | .some (m, w) =>
    if timer.all (w ≤ ·) && (dueThen p c.armed t).all (fun d => w < d.2.1) &&
        (dueOwned p c.sendRows t).all (fun d => w < d.2.1)
    then Option.some (.next m w) else thenPick p c t timer
  | .none => thenPick p c t timer

/-- A mutation's `then` arguments: the props a child's lifted action
captures, none for the root's. -/
def thenArgsOf (p : Program) (m : String) : List Expr :=
  ((p.mutations.find? (·.name == m)).map (·.thenArgs)).getD []

/-- A `then` as its own commit: action `a`, its arguments read in the rows
and locals it runs in (none for the root's). An argument that fails to
evaluate refuses the commit: the checker types a child's props in its
use's scope, which the semantics does not carry, as for a request's
arguments (`settleOwned`). -/
def thenCommit (p : Program) (o : Oracle) (c : Config) (a : String) (args : List Expr)
    (rows : List RowId) (ls : Locals) : Config × Outcome :=
  let env : Env := ({ prog := p, slots := c.slots, derives := c.settled.derives,
                      resources := c.settled.resources, rows := rowSlots c.store rows,
                      now := c.now, idle := c.settled.idle } : Env).withOwned
  match evalList fuel env false ls args with
  | .ok vs => runAction p o c a vs rows
  | .error (.unsupported m) => (c, .refused (.unsupported m))
  | .error (.refused m) => (c, .refused (.refused m))
  | .error _ => (c, .refused (.refused "a `then`'s arguments did not evaluate"))

/-- The `then` of send row `i` (LLP 1035.005.004 D6): run in its
instance's rows, its arguments read with the locals the instance's
elements were rendered with (none when it rendered none; its rows are
then the prefixes of its identity, the instances around it). -/
def ownedThen (p : Program) (o : Oracle) (c : Config) (i : Nat) (a : String) : Config × Outcome :=
  match c.sendRows[i]? with
  | .some { mutation, inst := .some id, .. } =>
    let scope := (instanceScope id c.view).getD
      ([], (List.range id.length).reverse.map fun k => id.take (k + 1))
    thenCommit p o c a (thenArgsOf p mutation) scope.2 scope.1
  | _ => (c, .ok)

/-- A task's action reads its captures in its selected instance, as a child's `then` does. -/
def timerCommit (p : Program) (o : Oracle) (c : Config) (tm : Timer) : Config × Outcome :=
  let scope := tm.selected.map (recordedScope p c.store) |>.getD ([], [])
  thenCommit p o c tm.action tm.args scope.2 scope.1

/-- Move the clock to `t`, firing every timer due by then in order of due
time (ties by declaration order), each at its own due time. An armed
`then` runs, as its own commit, before any timer due at or after its
time (the answer landed first). A `queue` mutation's armed `next` runs
before a `then` due at or after its time and before a timer due after it
(LLP 1092 D3; the runner's `advance`). A refusal stops the advance there,
with the clock at the refusing timer's due time. Before an input, the host
applies each change receipt and continues to the same cutoff. -/
def advance (p : Program) (o : Oracle) (c : Config) (t : F64)
    (applyChanges : Bool := false) : Config × Outcome :=
  let due (c : Config) : Option (Timer × Nat) :=
    (c.timers.zipIdx).foldl (fun best (tm, i) =>
      if tm.next ≤ t then
        match best with
        | .none => Option.some (tm, i)
        | .some (b, j) => if tm.precedes b then Option.some (tm, i) else Option.some (b, j)
      else best) Option.none
  let pick (c : Config) : Option Clock := clockPick p c t ((due c).map (·.1.next))
  let finish (c : Config) : Config × Outcome := ({ c with now := if t > c.now then t else c.now }, .ok)
  -- At most `timerFireLimit` fires per advance (the runner's
  -- `TIMER_FIRE_LIMIT`), `then`s and queued sends counted; one more due is
  -- a refusal.
  let rec go : Nat → Config → Config × Outcome
    | 0, c =>
      match pick c, due c with
      | .none, .none => finish c
      | _, _ => (c, .refused (.refused "too many timer fires"))
    | n + 1, c =>
      if c.poisoned then (c, .refused (.refused "poisoned")) else
      match pick c with
      | .some (.next m w) =>
        let c := { c with nexts := c.nexts.filter (·.1 != m), now := if c.now < w then w else c.now }
        match nextCommit p o c m with
        | (c, .ok) => go n c
        | (c, .refused e) => resumeRefused applyChanges c e (go n)
        | (c, out@(.poisoned _)) => (c, out)
      | .some (.andThen m w a) =>
        let c := { c with armed := c.armed.filter (·.1 != m), now := if c.now < w then w else c.now }
        match thenCommit p o c a (thenArgsOf p m) [] [] with
        | (c, .ok) => go n c
        | (c, .refused e) => resumeRefused applyChanges (c.armNexts p) e (go n)
        | (c, out@(.poisoned _)) => (c.armNexts p, out)
      | .some (.owned i w a) =>
        let c := { c with sendRows := disarm c.sendRows i, now := if c.now < w then w else c.now }
        match ownedThen p o c i a with
        | (c, .ok) => go n c
        | (c, .refused e) => resumeRefused applyChanges (c.armNexts p) e (go n)
        | (c, out@(.poisoned _)) => (c.armNexts p, out)
      | .none =>
      match due c with
      | .none => finish c
      | .some (tm, i) =>
        if tm.change && !applyChanges && n + 1 < timerFireLimit then (c, .ok) else
        let timers := c.timers.set i tm.fired
        let c := { c with timers, now := if tm.change && tm.next < c.now then c.now else tm.next }
        match timerCommit p o c tm with
        | (c, .ok) => if tm.change && !applyChanges then (c, .ok) else go n c
        | (c, .refused e) => resumeRefused applyChanges c e (go n)
        | (c, out@(.poisoned _)) => (c, out)
  if t < c.now then (c, .ok) else go timerFireLimit c

end Contract
