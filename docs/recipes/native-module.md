# Recipe: a native module in Swift

Put an `ExactModule` in `modules/apple/*.swift`; `let exactModule` names its class.
No app.json entry. A native module is for what only the platform knows; this one reads the
device's thermal state (`ProcessInfo.thermalState`), which TypeScript cannot.

A value a launch gives the app (an origin a test or a benchmark sets, `API_BASE_URL`) is not
a native module's: mark it `{ "name": "API_BASE_URL", "launch": true }` in app.json's `env`
and read it in app.ts as `import.meta.env.API_BASE_URL`, which holds it before the module's
first request (`contract vocab env`). A launch sets it in the process's environment:
`SIMCTL_CHILD_API_BASE_URL=… xcrun simctl launch <udid> <bundle-id>` on a simulator, an
XCUITest's `app.launchEnvironment`, `xcrun devicectl device process launch
--environment-variables '{"API_BASE_URL": "…"}'` on a phone, the environment of a macOS,
Linux or Windows process. Android and the web read the build's value.

**Kept answers:** a resource that reads `native.available` or calls `native.call`
reads device state, so its answer can be kept across launches and shown before it is
asked again.

```swift
import Foundation

final class Thermal: ExactModule {
    override func call(_ request: [String: Any]) -> [String: Any] {
        let states: [ProcessInfo.ThermalState: String] = [
            .nominal: "nominal", .fair: "fair", .serious: "serious", .critical: "critical",
        ]
        return ["state": states[ProcessInfo.processInfo.thermalState] ?? "unknown"]
    }
    override func later(_ request: [String: Any], reply: ExactReply) {
        reply.send(call(request))
    }
}
let exactModule: ExactModule.Type = Thermal.self
```

```contract
shape Thermal
  state: string

component App
  resource thermal = thermalState() as shape Thermal else empty()
  action check
    refresh thermal
  view
    column
      if pending(thermal)
        progress aria-label="Reading"
      if failed(thermal)
        text "Couldn't read the thermal state."
      text thermal.state testId="thermal"
      button press=check testId="check"
        text "Check again"
```

```ts
import type { Answer, Result, Sources } from './app.contract.d.ts';

export const grants = '';
const sources: Sources = {
  thermalState: (_args, _store, _storage, native) =>
    native?.available ? native.call({}) as Result<'thermalState'> : { state: 'unknown' },
};
export const answer: Answer = (source, args, store, storage, native) =>
  sources[source](args, store, storage, native);
```

`call` runs on main within 100 ms; mark it `throws` to refuse. `later(request)` awaits one
`ExactReply.send` or `fail` from any thread. The web implements only `later`, in
`modules/web/index.js`.

Web drives use that module when present, otherwise the fallback; iOS calls Swift.
Use `clock +1`, then `clock data`.
Swift edits need `bun exact.mjs ios`. `print` reaches agent `logs`.
