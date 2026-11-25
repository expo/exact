# Mixed-app data-code module calls bypass the calling executor's narrowed request.grants scope

**Status:** Closed
**Resolution:** Enforced the calling executor's intersected request.grants across module_call_linked, module_effect, downstream admission helpers, forwarded effect authority, and LinkedPresence::presence_call.
**Systems:** runner (module admission, mixed-executor capability isolation)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Back doors)

Verification: verified by reading.

At `9c0431fd5`, data-code module calls routed through `Runner::module_call_linked` (`runner/src/runner/modules.rs:1085-1153`) ignore the calling executor's narrowed `native.grants` scope across all general grant admission checks:

- In a mixed JavaScript + Rust app, `Mixed::construct` (`data/src/mixed.rs:185-206`) sets `Mixed::grants` (returned by `self.data.grants()`) to the union of `javascript.grants()` and `rust.grants()`, while `Mixed::route_answer` (`data/src/mixed.rs:330-358`) and `Mixed::route_delivery` (`data/src/mixed.rs:561-573`) stamp each outgoing `Request` with `request.grants = Some(executor_grants)`.
- In `Runner::module_call_linked` (`runner/src/runner/modules.rs:1118-1153`):
  - Line 1118 passes `native.grants.as_deref()` only to `self.scoped_input_refusal` (`runner/src/runner/modules/admit.rs:478-495`), which returns `None` unless `sub["origin"] == "input" && cause.input_started() && !self.page.hidden`.
  - Line 1124 calls `self.effect_call(&row, args, cause)` (`runner/src/runner/modules/declared.rs:369-381` -> `module_effect` at `:184-220`) without `native.grants` (and line 219 forwards via `bound_request` without even stamping `bound::with_authority`).
  - Line 1134 calls `self.admitted(&row, &args, cause, send, timeout)` (`runner/src/runner/modules/admit.rs:126-180`) without `native.grants`.
  - Only after admission succeeds do lines 1147-1153 read `native.grants` again to stamp `bound::with_authority(&body, &scope)` on forwarded sends.
- Every downstream grant check invoked by `module_effect` and `admitted` reads `self.data.grants()` (the full JS + Rust union) instead of the effective scoped grants:
  - `admit` (`runner/src/runner/modules/admit.rs:504` for `nameGrants`, `:580` via `self.covers` at `runner/src/runner/links.rs:95-97` for `paths`, `:602` via `fetch_refusal` and `:607` via `self.covers` for `admits`),
  - `value_grants` (`runner/src/runner/modules/ceremony.rs:142`) and `methods_admitted` (`runner/src/runner/modules/sheet.rs:203-220`),
  - `identity_admitted` (`runner/src/runner/modules/members.rs:224-273`),
  - `permission_of` (`runner/src/runner/modules/admit.rs:324`),
  - `owned_call` (`runner/src/runner/modules/pictures.rs:173`, called from `admit.rs:191, 537`), and
  - `declares` (`runner/src/runner/modules/permits.rs:463`).

### Trigger
In a mixed JS + Rust app where the Rust executor declares a module capability (e.g., `fs.read app:/secret`, `net.fetch https://private.api`, `device.camera`, `passkeys`, `payment`, or `auth.google`) that the JS executor does not declare, JS data code emits a native module call (`fetch("exact-native:", { method: "POST", body: JSON.stringify({ extension, source|send, args }) })` per `js/bake/src/__exact_binding.mjs:397`, or Rust data code returns `Answer::Later(Request::native(...))`). `Mixed::route_answer` sets `request.grants = Some(js_grants)`. The host dispatches the native request through `Runner::native_work(&r.request)` (`host/apple/src/abi.rs:323`, `host/painted/src/presenter.rs:1026`, `host/render/src/requests.rs:54`) -> `Runner::module_call` -> `Runner::module_call_linked`.

### Actual vs. Expected
- **Actual:** `module_call_linked` evaluates `admit`, `value_grants`, `methods_admitted`, `identity_admitted`, `owned_call`, `permission_of`, and `declares` against `self.data.grants()` (the full union). In production (`Route::Real`, `admit.rs:185`), the call returns `Decision::Forward` directly to the host's native module implementation; under the agent (`Route::Agent` / `Route::Substitute`), it likewise proceeds as granted. The unprivileged executor exercises module capabilities, filesystem paths, and fetch URLs granted solely to the other executor.
- **Expected (LLP 1133.002 D14; LLP 1133.009 D10 item 4 cited at `admit.rs:551`; `modules.rs:1144-1146`; `data/src/mixed.rs:330-355`):** When `native.grants` is `Some(scope)`, every admission check for that call must evaluate against the intersection of `self.data.grants()` and `scope` (matching `auth::check_grants` at `runner/src/auth.rs:839-845` and `grants::admitting_request` at `runner/src/grants.rs:236`), refusing any module capability not granted to the calling executor.

### Verification & Siblings Checked
Verified by reading at `9c0431fd5`. Note that `picture_decision` (`runner/src/runner/modules/pictures.rs:106-149`) is invoked for view `img` raster admission (`admit_picture_linked`), not from `module_call_linked`, whereas `owned_call` (`pictures.rs:164-176`) is on the `module_call_linked` path.
- **Direct sibling defect found in `Runner::native_work` (`runner/src/runner/commit.rs:1248`):** `LinkedPresence::presence_call` (`runner/src/runner/presence_secrets.rs:467-495`) receives only `&request.body` (never `request.grants`) and checks `r.store.presence_of(&name)` against the app's union store, allowing an unprivileged executor in a mixed app to call `{"presence": "unlock"|"protect"|"unprotect", "name": "..."}` on a presence secret granted only to the other executor.
- **Sibling in `declared.rs:219`:** `module_effect` forwards effect sends via `self.bound_request` without stamping `bound::with_authority(&body, &scope)` from `native.grants`.
- Checked properly scoped siblings: `runner/src/auth.rs:843`, `runner/src/grants.rs:236`, `data/host/src/lib.rs:151`, `host/executor/src/*`, `host/apple/src/canvas_capture.rs:44`, `host/painted/src/host_canvas.rs:98`, and `host/web/src/batch.rs:705, 738, 750`.

### Suggested Fix & Regression Test
Compute the effective grant string once at the top of `module_call_linked` (intersecting `self.data.grants()` with `native.grants.as_deref()` when `Some`, and removing the special-case `scoped_input_refusal` bypass or folding it into scoped `admit`), thread that `&str` (or a scoped helper on `Runner`) through `effect_call`/`module_effect`, `admitted`, `admit`, `value_grants`, `methods_admitted`, `identity_admitted`, `owned_call`, `permission_of`, `declares`, and a scoped `covers_in(grants, capability, path)`, and stamp `bound::with_authority` on forwarded effect sends in `module_effect`. Also pass `request.grants.as_deref()` into `presence_call` (`commit.rs:1248` -> `presence_secrets.rs:467`) and verify the secret name is granted within that scope. Add regression tests in `runner/src/runner/modules/substitute_tests.rs` (and `kept_presence_tests.rs`) invoking `Runner::native_work` with a narrowed `request.grants` across `nameGrants`, `paths` (`fs.read`/`fs.write`), `admits` (`net.fetch`), `value_grants`, `effect_call`, and `presence_call`, asserting all are refused when absent from `request.grants` even when present in `self.data.grants()`.

### Siblings checked

Found sibling bypass in LinkedPresence::presence_call (runner/src/runner/commit.rs:1248 -> runner/src/runner/presence_secrets.rs:467-495, ignores request.grants) and missing with_authority on forwarded effects in declared.rs:219. Verified picture_decision (pictures.rs:113) is view-img admission rather than module_call_linked. Verified auth.rs:843, grants.rs:236, data/host/src/lib.rs:151, host/executor/src/*, host/apple/src/canvas_capture.rs:44, host/painted/src/host_canvas.rs:98, and host/web/src/batch.rs:705,738,750 do intersect or forward request.grants.
