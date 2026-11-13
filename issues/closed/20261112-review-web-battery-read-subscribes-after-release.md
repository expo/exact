# Web battery read subscribes after its last reader leaves

**Status:** Closed
**Resolution:** Each release of a device source is counted, and a battery read that a release overtook while it waited on `navigator.getBattery()` answers without subscribing.
**Systems:** @exact/device, web module lifecycle
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, removing a battery resource while its first browser read is
pending leaves battery listeners installed after the last reader is gone.
Later battery changes still notify Exact, despite there being no resource
that needs the acquisition.

`packages/device/web/index.js:45` awaits `navigator.getBattery()` before
creating its subscription at line 50. `release()` at lines 63–65 removes
only an existing subscription and does not invalidate that pending read.
When the promise resolves, it installs both listeners. The web runtime
adds the source to its live set before invoking it
(`host/web-js/modules.js:570`), and removes that entry and calls `release`
when its last reader leaves (lines 868–877); it has no live entry left to
release the late subscription again.

Reproduced with the actual package implementation on pinned Bun 1.4.2,
using a controlled `navigator.getBattery()` promise and a battery manager
that records event listeners. The sequence is:

```js
const pending = device.getBattery();
device.release('getBattery');
resolveBattery(manager);
await pending;
manager.level = 0.7;
listeners.get('levelchange')?.();
```

The resulting output is:

```json
{"listenersAfterRelease":["levelchange","chargingchange"],"notifications":["device.getBattery"]}
```

The runnable probe is
`/tmp/exact-review-20261112/device-release-probe.mjs`. It verifies the
package's async ordering without using a real battery or browser. The
leak is bounded to the source's two listeners; it still retains its
callback and performs unnecessary work until another release or page
teardown.

Invalidate pending reads when `release` ends the subscription generation.
A completed read may provide its answer, but must install listeners only
if its generation still has a reader. Validate release before completion,
normal completion followed by release, and release followed by a new read
before the old promise resolves.

Checked all open and closed filesystem issues and the 275-issue GitHub
snapshot for battery, `getBattery`, pending device subscriptions and
release leaks. No matching issue was found.
