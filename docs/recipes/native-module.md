# Recipe: a native module in Swift

Put an `ExactModule` in `modules/apple/*.swift`; `let exactModule` names its class.
No app.json entry. A native module is for what only the platform knows; this one reads the
device's thermal state (`ProcessInfo.thermalState`), which TypeScript cannot.

A value a launch gives the app (an origin a test sets) is not a native module's: mark it
`{ "name": "API_BASE_URL", "launch": true }` in app.json's `env` and read it in app.ts as
`import.meta.env.API_BASE_URL`, which holds it before the module's first request. A launch
sets it in the process's environment (`SIMCTL_CHILD_API_BASE_URL=…` on a simulator;
`contract vocab env` has the rest); `-API_BASE_URL value` as an argument sets `UserDefaults`
instead. Android and the web read the build's value.

Declare device access in the activating source's `grants`
([permissions](permissions.md)). `device.photos-add purpose.savePhoto` derives
`NSPhotoLibraryAddUsageDescription` from that strings key. Grants derive
entitlements too. Apple `infoPlist` and `entitlements` dictionaries refuse every
key; see [bundle metadata](../reference.md#apple-bundle-metadata).

**Kept answers:** a resource reading `native.available` or calling `native.call`
reads device state; its answer is kept across launches. Keep native access out of
fetch sources.

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
  thermalState: async (_args, _store, _storage, native) =>
    native?.available ? await native.later({}) as Result<'thermalState'> : { state: 'unknown' },
};
export const answer: Answer = (source, args, store, storage, native) =>
  sources[source](args, store, storage, native);
```

`call` runs on main within 100 ms; mark it `throws` to refuse. `later(request, reply)`
answers with one `reply.send` or `fail`, from any thread, and is the one the web implements, in
`modules/web/index.js`, so the source above awaits it.

Drive with `clock +1`, then `clock data`. Rebuild Swift with `bun exact.mjs ios`; `print` reaches agent `logs`.
