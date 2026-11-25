# auth::arm_with_fixture only blocks CallOrigin::Action, letting Tool, Wake, and Device origins open openAuthSession UI

**Status:** Closed
**Resolution:** Rejected all non-App origins in auth::arm_with_fixture and host/web-js/auth.js with 403 and wired person-required tool termination via Runner::ticket_person_required and Origin.personRequired.
**Systems:** runner (auth sessions, call provenance)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Back doors)

Verification: verified by reading.

At `9c0431fd5`, `exact_runner::auth::arm_with_fixture` (`runner/src/auth.rs:820-829`) rejects `openAuthSession` requests only when `out.request.reach.origin == crate::CallOrigin::Action`:

```rust
let ticket = out.ticket;
if out.request.reach.origin == crate::CallOrigin::Action {
    settle(
        runner,
        ticket,
        403,
        "auth sessions are refused during a background action",
    );
    return Arm::Settled;
}
```

`CallOrigin` (`runner/src/runner/provenance.rs:38-55`) defines `Device < Wake < Tool < Action < App` (least privileged first). `Device`, `Wake`, and `Tool` are all strictly less privileged than `Action`, so `== CallOrigin::Action` evaluates to `false` for all three. No earlier gate in the runner refuses `Request::auth`:
- `Runner::append_request` (`runner/src/runner/commit.rs:1068-1070`) stamps `request.reach = self.reach_of(self.cause_for(target))` and calls `self.ask_refusal(target, &args, &request)`.
- `Runner::ask_refusal` (`runner/src/runner/processes.rs:135-152`) only checks `request.is_process()` (`process_refusal` returns `None` at `:109-111` for non-process requests).
- `Runner::refuse_in_wake` (`runner/src/runner/wakes.rs:783-805`) only filters Contract `self.commands`, not data-source `Request::auth` requests.
- Every host passes `r.request.is_auth()` requests straight into `auth::arm` / `auth::arm_with_fixture`: Apple (`host/apple/src/abi.rs:317` -> `host/apple/src/abi/commands.rs:316`), Painted / Android / Linux / Windows (`host/painted/src/presenter.rs:1010-1017`), and Web wasm (`host/web/src/host/complete.rs:121` -> `host/web/src/auth.rs:55`).

### Trigger
In an app with `auth.session` / `auth.callback` grants, invoke a tool (`Runner::call_tool` -> `calls.rs:242-249`, `CallOrigin::Tool`), raise a background wake (`Runner::raise_wake`, `CallOrigin::Wake`), or dispatch a device event (`CallOrigin::Device`) whose Contract action triggers a resource or mutation that returns `Answer::Later(Request::auth(...))` (from `openAuthSession(...)` in `app.ts` or `exact_data::auth::open_auth_session` in Rust). The host delivers the emitted `RequestOut` to `exact_runner::auth::arm` / `auth::arm_with_fixture`.

### Actual vs. Expected
- **Actual:** `arm_with_fixture` passes the `== CallOrigin::Action` check, passes `check_grants`, pushes the session into `auth.live` / `auth.opened`, and returns `Arm::Present(session)` (launching `ASWebAuthenticationSession`, Android browser auth, or a web popup without user interaction) or `Arm::Held` under the agent. For a `Tool` invocation, `runner.call_person_required()` is never triggered, so the tool call does not end `refused` with `person-required`.
- **Expected (`llp/1111-apps-as-tools-for-agents.rfc.md:308-312` D4a; `llp/1114.001-background-refresh-and-notification-taps.rfc.md:414-416` D4; `runner/src/runner/provenance.rs:38-55`):** `openAuthSession` from any origin other than `CallOrigin::App` (`Tool`, `Wake`, `Device`, or `Action`) must be refused with 403 before opening UI or creating an agent hold, and a tool invocation that attempts it must end `refused` (`person-required`, `uncertain`).

### Verification & Siblings Checked
Verified by reading at `9c0431fd5`.
- **Direct sibling defect found on the JS web target (`host/web-js/auth.js:141-202`):** `openAuthSession` (`start()`) never imports or checks `Origin.now()` (which `host/web-js/calls.js:191-193` sets to `"tool"` and `host/web-js/modules.js:435-438` sets to `"device"` / `"wake"`), so tool, wake, and device calls on the JS web target also bypass origin refusal and never end a tool call with `person-required`.
- Checked properly gated siblings: `runner/src/runner/processes.rs:92-101, 124` (`admit_process` requires `CallOrigin::App` and calls `call_person_required()`), `runner/src/runner/modules/admit.rs:145-155, 245-254` (refuses `origin != CallOrigin::App` and calls `call_person_required()`), and `runner/src/runner/presence_secrets.rs:194-197, 483-489` (checks `origin != CallOrigin::App`).

### Suggested Fix & Regression Test
1. In `runner/src/auth.rs:821`, change `out.request.reach.origin == crate::CallOrigin::Action` to `out.request.reach.origin != crate::CallOrigin::App` (using the origin's description in the 403 message).
2. Because hosts invoke `auth::arm_with_fixture` outside `Runner::as_cause` (so `runner.invocation_id()` is `0` when `calls.rs:658-663` checks it), trigger `call_person_required()` under the ticket's recorded cause (e.g. `runner.as_cause(runner.ticket_cause(ticket), |r| r.call_person_required())` or refuse auth requests of non-`App` origins inside `Runner::append_request` / `ask_refusal` where `cause_for(target)` is installed).
3. Mirror the non-`app` origin refusal and `person-required` tool ending in `host/web-js/auth.js` / `host/web-js/calls.js`.
4. Add unit tests in `runner/src/auth.rs` (and `host/web-js/auth.test.js`) asserting `arm_with_fixture` settles 403 without opening or holding for `CallOrigin::Tool`, `CallOrigin::Wake`, `CallOrigin::Device`, and `CallOrigin::Action`, and an end-to-end tool call test asserting a tool emitting `Request::auth` ends `refused` with reason `person-required` and certainty `uncertain`.

### Siblings checked

Found direct sibling defect in host/web-js/auth.js:141-202 (openAuthSession never checks Origin.now() from host/web-js/calls.js or host/web-js/modules.js). Checked all Rust host dispatchers (host/apple/src/abi/commands.rs:316, host/painted/src/presenter.rs:1017, host/web/src/auth.rs:55) and properly gated runner siblings (processes.rs:92-101, modules/admit.rs:145-155, 245-254, presence_secrets.rs:194-197, 483-489).
