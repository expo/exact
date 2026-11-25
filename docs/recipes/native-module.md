# Answer a native call in Swift or Kotlin

Put an `ExactModule` in `modules/apple/*.swift`; `let exactModule` names its class.
On Android, put `class exact.app.Module : ExactModule()` in `modules/android/Module.kt`.


Declare device [grants](permissions.md) in the activating source for permissions and entitlements.
On Apple, `device.photos-add purpose.savePhoto` derives `NSPhotoLibraryAddUsageDescription` from that strings key. Fetch sources cannot use native.

```swift
import Foundation

final class Thermal: ExactModule {
    override func call(_ request: [String: Any]) -> [String: Any] {
        if context.agent { return ["state": "nominal"] }
        let state = ProcessInfo.processInfo.thermalState
        return ["state": state == .nominal ? "nominal" : "elevated"]
    }
    override func later(_ request: [String: Any], reply: ExactReply) { reply.send(call(request)) }
}
let exactModule: ExactModule.Type = Thermal.self
```

```kotlin
package exact.app

import android.os.PowerManager
import dev.exact.canvas.ExactModule
import org.json.JSONObject

class Module : ExactModule() {
    override fun call(request: JSONObject): JSONObject {
        if (context.agent) return JSONObject().put("state", "nominal")
        val power = context.application.getSystemService(PowerManager::class.java)
        val state = when (power?.currentThermalStatus) {
            null -> "unknown"
            PowerManager.THERMAL_STATUS_NONE -> "nominal"
            else -> "elevated"
        }
        return JSONObject().put("state", state)
    }
}
```

`context.after(1000.0) { … }` follows the session clock, including under the driver.
`context.changed("thermal")` refreshes sources that called `native.watch("thermal")`.

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

`call` runs within 100 ms, on main on Apple and on the calling
thread on Android. Throw to refuse. `later` starts on main. Answer once with
`reply.send` or `reply.fail` from any thread. Kotlin’s default sends `call(request)`.

Rebuild with `bun exact.mjs ios` or `android`.

For `API_BASE_URL`, set `launch: true` in manifest `env` and read `import.meta.env.API_BASE_URL`. Use `SIMCTL_CHILD_API_BASE_URL` on the simulator. Android and web read the build value.
