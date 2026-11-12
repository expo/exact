# A module's NaN in a scalar answer or an event's arguments aborts the app instead of failing the call

**Status:** Closed
**Resolution:** Every site in the module seam that encodes a value a module or package supplies asks `JSONSerialization.isValidJSONObject` first (`exactJSON` in the seam), so a NaN, an infinity or a `Date` fails the call (`the reply was not JSON`; a `TypeError` for a handler's arguments), is not sent as an event or activation, or answers a service query nil, and the app keeps running.
**Systems:** host/apple/modules/ExactNativeModule.swift, ExactPackage.swift, ExactExtensions.swift, ExactServices.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`JSONSerialization.data(withJSONObject:)` does not throw for an object it cannot write (a NaN or infinite number, a `Date`, a `Data`): it raises `NSInvalidArgumentException`, which `try?` does not catch. The module seam knows this in five places, which ask `JSONSerialization.isValidJSONObject` first (`host/apple/modules/ExactNativeModule.swift:508`, `:1047`; `ExactExtensions.swift:433`, `:460`, `:472`). The others encode a value the app's module or a package supplies with `try?` alone:

- `ExactReply.send` (`ExactNativeModule.swift:210-218`): a scalar answer skips the check (`guard scalar || JSONSerialization.isValidJSONObject(sent)`), and a NaN is an `NSNumber`, so scalar.
- `ExactPackageEvents.emit(_:_:)` (`ExactPackage.swift:47-51`): the event's `args`.
- A package's `invoke` arguments (`ExactExtensions.swift:334`, from `:386-389`), `activation`'s `args` (`:491`), and a module's `service` request (`ExactServices.swift:18`).

## The failure

A package source that answers a number:

```swift
reply(["exact:answer": done / total])   // total is 0, both are Double
```

Observed: the process ends with `*** Terminating app due to uncaught exception 'NSInvalidArgumentException', reason: 'Invalid number value (NaN) in JSON write'`. The same value as an object's field, `reply(["level": done / total])`, fails the one call with "the reply was not JSON", which is what `ExactReply.send` is written to do. A view's `events.emit("level", [Double.nan])` ends the process the same way.

Expected: each of these refuses the value (the call fails, the event is not sent) and the app keeps running.

## Verification

Run on macOS: the `ExactReply` class as it stands in `ExactNativeModule.swift`, and `ExactPackage.swift` whole, compiled into a command-line program that calls `send` and `emit`, each case in a process of its own:

```
send(["exact:answer": 1.5])               reply: status 200 1.5                        exit 0
send(["level": Double.nan])               reply: status 500 the reply was not JSON     exit 0
send(["exact:answer": Double.nan])        … 'Invalid number value (NaN) in JSON write'        exit 134
send(["exact:answer": Double.infinity])   … 'Invalid number value (infinite) in JSON write'   exit 134
emit("level", [0.5])                      event: kind 8 {"args":[0.5],"exact:event":"level"}
emit("level", [Double.nan])               … 'Invalid number value (NaN) in JSON write'        exit 134
```

Not run: a module inside an app, or the `invoke`, `activation` and `service` sites, which are read.

## Direction

One helper in the seam that answers `Data?` for any value (`isValidJSONObject([value])`, then the encode with `.fragmentsAllowed`), used at every site above; `ExactReply.send` drops its `scalar ||` shortcut in favour of it.

## Related

No issue or QUEUE entry names `isValidJSONObject` or `ExactReply`. No package in `packages/` answers a bare number today. `videoarm` and `webarm` encode only values they built themselves.
