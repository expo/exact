# Virtualized list reading a component-owned resource crashes unconditionally at mount on the web JS target

**Status:** Closed
**Resolution:** Read component-owned resources and mutations from the enclosing instance in Em::row_inputs instead of root stubs.
**Systems:** host/web-js compiler (list_options.rs) and JS runtime (list.js, each.js)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Dropped plumbing / Copy-paste drift)

Verification: verified by running.

At `9c0431fd5`, `Em::row_inputs` in `host/web-js/src/list_options.rs:86-96` unconditionally emits root references `r_{i}()`, `r_{i}.p()`, and `r_{i}.e()` for `Input::Resource(i)`, `Input::PendingResource(i)`, and `Input::FailedResource(i)` without checking `self.plan.resources[i].owner`. For any component-owned resource (`owner.is_some()`), `host/web-js/src/emit.rs:419-426` binds `const r_{i} = owned(name)` at root scope (`host/web-js/each.js:104-109`), which is a throwing stub with no `.p` or `.e` methods; the live resource lives on the owning instance object (`{rows}.r{i}`, created by `host/web-js/src/resources.rs:41-54`).

### Reachable trigger
Compile for the default web JS target any Contract app where a child component mounted in an `if`, `match`, route stack entry, or tab panel (admitted by LLP 1035.005.004 D1 and `contract/types/src/tasks.rs:131-191`) declares a `resource` and renders a `list virtualized=true` whose `for` row body reads that resource or `pending(r)` / `current(r)` / `failed(r)` / `failure(r)`:

```contract
shape Feed
  prefix: string

component App
  state show = true
  view
    column
      if show
        Screen()

component Screen
  resource feed = getFeed() as shape Feed else empty(prefix="")
  state ids = [1, 2]
  view
    list virtualized=true height=200
      for x in ids key=x
        text `${feed.prefix}:${pending(feed)}:${failed(feed)}:${x}`
```

`host/web-js/src/regions.rs:400` calls `self.row_inputs(r, scope)` with `scope.rows` pointing to the enclosing `Screen` instance (`$a1`), and `runner/src/instance/deps.rs:665-677` reports `Input::Resource(0)`, `Input::PendingResource(0)`, and `Input::FailedResource(0)`.

### Actual vs. expected result
- **Actual:** `exact-web-js` emits `const r_0=owned("feed#...");` at root scope, `$a1.r0=own(r_0,...);` in the arm, `$a1["r0"]()` in the row body, and `inputs:()=>[r_0(),r_0.p(),r_0.e()]` in `$vl(...)`'s options. When the virtualized list mounts (`host/web-js/list.js:1210`: `inputs = o.inputs?.() ?? []`), `r_0()` immediately throws `Refusal('resource "feed#..." is an instance\'s, read on its instance')`, and `r_0.p()` / `r_0.e()` throw `TypeError: r_0.p is not a function` / `r_0.e is not a function`.
- **Expected:** Matching `Input::PendingMutation` at `host/web-js/src/list_options.rs:88-95` and `Translator::resource` at `host/web-js/src/code.rs:402-409` (LLP 1035.005.004 D1/D6), owned resources must be read from the enclosing instance in `scope.rows` (`{rows}.r{i}()`, `{rows}.r{i}.p()`, `{rows}.r{i}.e()`).

### Verification and sibling sites checked
Verified by running the checkout's prebuilt `exact-web-js` (`EXACT_ROOT=<local-path> exact-web-js js ... --dump`) on both single-use (`if show`) and two-use (`if a` / `if b`) Contract repros containing an owned `resource feed` and owned `mutation save`. The emitted `app.js` literally produced:
`const r_0=owned("feed#..."); ... $a1.r0=own(r_0,...); $a1.m0=ownMut(m_0,...); ... inputs:()=>[r_0(),r_0.p(),[$a1.m0.p(),$a1.m0.e()],r_0.e()]`.

Sibling sites checked:
- `host/web-js/src/list_options.rs:76-84` (`Input::Slot`): checks `owner.is_none()` and `scope.row_slots.contains(&i)` (safe).
- `host/web-js/src/list_options.rs:88-95` (`Input::PendingMutation`): checks `owner.is_none()` and uses `{rows}.m{i}` when owned (safe).
- `host/web-js/src/code.rs:402-420` (`Translator::resource` and `Translator::mutation`): both check `owner.is_none()` and emit `scope.row_member(rows, format!("r{i}"))` / `m{i}` (safe).
- `host/web-js/src/mutations.rs:22-25` (`refreshes`): checks `owner.is_some()` and uses `{rows}.r{r}` (safe).
- Native runner (`runner/src/runner/{dated,settlement,kept,kept_wait}.rs`): evaluates `Input::Resource`/`PendingResource`/`FailedResource` against runner state rather than root JS stubs.

### Suggested fix and regression test
1. In `host/web-js/src/list_options.rs:86-96`, check `self.plan.resources[i].owner.is_none()` for `Input::Resource`, `Input::PendingResource`, and `Input::FailedResource`, and when `owner.is_some()`, emit `{rows}.r{i}()`, `{rows}.r{i}.p()`, and `{rows}.r{i}.e()` from `&scope.rows` (or `None => continue`, matching `Input::PendingMutation` at lines 88-95; consider `scope.row_member` if shared-template capture can apply).
2. Add a regression test in `host/web-js/src/emit_tests.rs` beside `list_measurements_read_parent_body_inputs_and_outer_frames` (`emit_tests.rs:81`) compiling the `Screen` repro above and asserting `inputs:()=>[...]` reads `$a1.r0()` / `$a1.r0.p()` / `$a1.r0.e()` (never `r_0()`), plus a runtime mount test verifying the virtualized list renders and updates when the owned resource settles.

### Siblings checked

host/web-js/src/list_options.rs:76-84,88-95 (Slot and PendingMutation handle owner.is_some()); host/web-js/src/code.rs:402-420 (Translator::resource/mutation handle owner.is_some()); host/web-js/src/mutations.rs:22-25 (refreshes handles owner.is_some()); runner/src/runner/{dated,settlement,kept,kept_wait}.rs
