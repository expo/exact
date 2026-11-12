# A cycle among a module's permissions (`requires`, `grantedBy`) compiles, and reading a state in it overflows the stack

**Status:** Closed
**Resolution:** The compiler refuses a cycle among an interface's permissions through `grantedBy` and one target's `requires`, naming each step, and the runner's permission state stops at a name already on its path, so a cycle across interfaces reads a state rather than overflowing the stack.
**Systems:** runner/src/runner/modules/permits.rs, contract/cli/src/declaration.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A permission's state is the least of its own and of those it `requires` on this target
(LLP 1133.030 D12 item 1), and a permission another's grant includes (`grantedBy`) reads
`granted` while that one does (LLP 1133.019 D5). `permission_state`
(`runner/src/runner/modules/permits.rs:90-96`) calls itself for each required name, and
`own_permission_state` (`permits.rs:163-168`) calls it for the `grantedBy` name. Each
guards only against a permission naming itself, so a cycle through either word, or
through both, recurses without end.

Nothing refuses such a declaration. The compiler copies an interface's permission list
into the plan (`contract/cli/src/declaration.rs:645-660`) and reads neither word. A module
is anyone's to write (LLP 1133.001).

The process aborts where the runner decides the permission itself: a module whose
substitute answers, and one bound under the agent. The agent's `state` asks every
permission's state too (`runner/src/runner/modules/agent.rs:52-56`). A call forwarded to a
bound implementation does not compute it. A cycle through `grantedBy` alone aborts at boot
on any target. One through `requires` aborts only on the target it names: the app boots
on the author's Mac and aborts on Android.

## The failure

The cycle that reads naturally for location: "always" requires "when in use" on Android,
as `@exact/geofencing` ships it (`"requires": { "android": ["geolocation"] }`), and "when
in use" is granted by "always". One line added to the package's second permission:

```json
{
  "name": "geolocation",
  "grantedBy": "geolocation-background",
  "denied": "PERMISSION_DENIED",
  "grants": ["device.geolocation"]
}
```

```
use geofencingStatus, GeofencingStatus from "@exact/geofencing"

component App
  resource status = geofencingStatus() as shape GeofencingStatus
  view
    text status.state
```

Compile, boot, then `runner.set_module_target("android")` and `apply_announced()`, as
Android's host does (`host/linux/src/canvas/requests.rs:132`).

Observed: `thread … has overflowed its stack`, `fatal runtime error: stack overflow,
aborting`, SIGABRT. `contract build app.contract` exits 0.
Expected: the compiler refuses the declaration and names the permissions in the cycle.

## Verification

A temporary test copied the package into an app, edited `exact.json`, compiled with
`contract::compile_path_all` and booted `Runner::boot` on a Mac, one process per case:

| Declaration | Target | Result |
| --- | --- | --- |
| as shipped | macOS, Android | `status.state` is `prompt` |
| `geolocation` is `grantedBy` `geolocation-background` (above) | macOS, iOS | `prompt` |
| the same | Android | stack overflow |
| `geolocation` also `requires` `geolocation-background` on Android | macOS, iOS | `prompt` |
| the same | Android | stack overflow |
| each `requires` the other on macOS | macOS | stack overflow in `Runner::boot` |
| each is `grantedBy` the other | macOS | stack overflow in `Runner::boot` |

The two Android cases abort the same way with the module bound under the agent
(`bind_extensions(&[], &["geofencing"])`). Bound as in production
(`bind_extensions(&["geofencing"], &[])`), the read is forwarded and nothing aborts.
`contract build` printed its summary and exited 0 for the `grantedBy` declaration above
and for the last two. Not run: the agent's `state` on a cyclic declaration, and an Android
device.

## Direction

Refuse a cycle among `requires` (per target) and `grantedBy` where the declaration is
read, naming the permissions, as `permission_of` refuses a name that asks an undeclared
permission (`declaration.rs:1179-1183`). A required permission may be declared by another
interface (LLP 1133.026 D19 item 2), which one declaration's check cannot see, so
`permission_state` should also stop at a name already on its path.

## Related

`issues/20261110-module-option-min-above-max-panics-runner.md` is another declaration the
compiler accepts and the runner cannot survive; it is a different field and a different
failure.
