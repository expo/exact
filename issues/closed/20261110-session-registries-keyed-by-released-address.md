# A session made where a destroyed one was allocated is not asked before leaving the app, and its launch route is journaled as a navigation

**Status:** Closed
**Resolution:** Each session owns its leave and share-inbox observers and navigation history, so address reuse cannot suppress registration or inherit another session's routes.
**Systems:** host/apple/Sources/ExactKit/Leave.swift, NavigationMarks.swift, ShareReceive.swift
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Three process-wide sets remember a session by `ObjectIdentifier` and never forget it (commit `dd6f5dcd`):

- `ExactSession.leaving` (`Leave.swift:20`): `observeLeaving()` returns at line 25 when the identifier is already in the set, so the `exact.leave` observer is added once per identifier.
- `NavigationMarks.launched` and `seen` (`NavigationMarks.swift:20-21`): `routerChanged` treats a session's first route as its launch only when `launched.insert(key).inserted` (line 93), and judges cold or warm from `seen[key]` (line 91).
- `ExactSession.claiming` (`ShareReceive.swift:85`): the same guard at line 131, before the share inbox's foreground observer and its ping are installed.

An identifier "is only valid for comparisons during the lifetime of the instance" ([`ObjectIdentifier`](https://developer.apple.com/documentation/swift/objectidentifier)). `destroy()` removes none of these keys, and the next session is usually allocated where the released one was: of 12 sessions booted in a window one after another, each destroyed and released before the next, 11 had the first one's identifier. That session then:

- adds no `exact.leave` observer, so its `ExactLeaveDelegate` is never asked, and a package's `leave(_:how:)` reads the unanswered ask as allowed (`host/apple/modules/ExactPackage.swift:260`, `ask["allow"] as? Bool ?? true`);
- has its launch route journaled as `navigation` in place of `navigation.launch`, warm when the released session had shown that entry;
- in an app with a `share_target`, installs no foreground observer and is not the ping's session, so it claims the inbox only at its own activation (read, not run).

ExactMac makes a session for each window and destroys it when the window closes (`ExactMac/main.swift:243`, `:510`), so closing one window and opening another does this. Its adapter does not adopt `ExactLeaveDelegate`, so there the navigation marks and the share inbox are what go wrong; the unasked delegate needs an embedder that adopts it and replaces a session. The sample hosts and ExactIOS make their sessions once.

## The failure

```swift
let a = ExactApp.shared.makeSession(delegate: refuses, label: "a")   // refuses: shouldLeaveFor → false
let view = ExactView(session: a); a.observeLeaving()
leaveAsk(view)["allow"]            // false: the control
a.destroy()                        // and released
let b = …                          // a later session with a's ObjectIdentifier
let viewB = ExactView(session: b); b.observeLeaving()
leaveAsk(viewB)["allow"]           // expected false
```

`leaveAsk` posts `exact.leave` with `["url", "how", "view"]` as `ExactPackageContext.leave` does. Observed: `("1") is not equal to ("2") - the new session's delegate was not asked` and `("nil") is not equal to ("Optional(false)")`. For `NavigationMarks.shared.routerChanged(b, payload: ["top": 1, "pattern": "/b"])`, `recent.last` is `"change /b warm program"` where `"launch /b"` was expected.

## Verification

Run as temporary Mac XCTests (`bun host/apple/build.mjs --test --only <Class>`): two tests, the three failures above, both controls passing. Those sessions were not booted; `observeLeaving` and `routerChanged` were called as `activateData` and `apply` call them. The count of 12 used booted sessions in windows.

## Direction

Remove the session's key from each set in `destroy()`, or hold the observer tokens and the marks on the session so they go with it.

## Related

`issues/closed/20261014-macos-a-reinserted-autofocus-node-is.md` fixed the same mechanism for a view's identifier in `autofocusProcessed`. `Formas.bySession` (`Forma.swift:327`) checks the object's identity and is not affected.
