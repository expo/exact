# A session destroyed while main serves an owner callback marks its runtime destroyed and never frees it

**Status:** Closed
**Resolution:** `Runtime.destroy` sends `exact_destroy` through `Owner.syncOrLater`, so a destroy from a served callback frees the runtime after the owner's current job instead of being refused.
**Systems:** host/apple/Sources/ExactCore/Bridge.swift (`Runtime.destroy`), host/apple/Sources/ExactCore/Owner.swift, host/apple/Sources/ExactKit/Session.swift (`ExactSession.destroy`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`:

```swift
package func destroy() {
    guard !destroyed else { return }
    destroyed = true
    let rt = rt
    on { exact_destroy(rt) }
}
```

(`host/apple/Sources/ExactCore/Bridge.swift:143-148`.) `on` is
`Owner.shared.sync(body, busy: ())` (`:196`). While main runs a callback the owner
waits on, `sync` refuses every runtime call and returns its `busy` value
(`host/apple/Sources/ExactCore/Owner.swift:101-104`, LLP 1072 T5). For `destroy`
that value is `()`: `exact_destroy` does not run, and `destroyed` is already true,
so `deinit` and any later `destroy()` return at the guard. The library keeps the
runtime, with its kernel, runner and plan, for the life of the process. LLP 1072
§8.4 makes `exact_destroy` on the owner a step of every destroy.

App code runs inside such a callback when the app's native module answers a
synchronous `native.call`: the owner hands the call to main through
`Owner.callMain` (`host/apple/Sources/ExactKit/NativeModule.swift:392-403`). A
module whose `call` ends the session reaches the defect: an embedder that answers
the app's "close" by calling `ExactSession.destroy()` (`Session.swift:1470`, which
calls `runtime.destroy()` at `:1496`), or any handler that lets the last reference
to a session go.

## The failure

```swift
let session = ExactApp.shared.makeSession(label: "x"), rt = session.runtime.rt
Owner.shared.sync { Owner.shared.callMain { session.destroy() } }  // as a native.call's handler runs
Owner.shared.sync { exact_in(rt, 1) != nil }   // true: the library still has it
session.destroy()                              // returns at the guard
Owner.shared.sync { exact_in(rt, 1) != nil }   // still true
```

`destroy()` called outside a callback leaves `exact_in` answering null, as it
should.

## Verification

Two temporary XCTest classes in `host/apple/Tests/ExactKitTests`, run on macOS
with `bun host/apple/build.mjs --test --only <class>`. For `Runtime.destroy()`:
`destroy from a served callback: destroyed true, still in the library true`,
`after asking again: still in the library true`, and for the control
`destroy from main: destroyed true, still in the library false`. For
`ExactSession.destroy()` the same three results, with the log line
`a runtime call from inside a callback the owner is waiting on was refused (LLP 1072 T5)`.
The served callback was made as `OwnerTests` makes one
(`Owner.shared.sync { Owner.shared.callMain { … } }`); no native module was built
to call `destroy` from its own `call`.

## Direction

Send the destroy through `Owner.syncOrLater` (`Owner.swift:170-180`), which runs
it after the owner's current job when main cannot wait for it.

## Related

`20261110-destroy-with-hatch-frame-ticket-keeps-display-link.md` and
`20261110-session-registries-keyed-by-released-address.md` are other leftovers of
a session's end; neither is the runtime's own free.
