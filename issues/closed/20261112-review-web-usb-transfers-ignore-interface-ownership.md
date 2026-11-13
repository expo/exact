# Web USB transfers can use an interface claimed by another connection

**Status:** Closed
**Resolution:** Web USB transfers, endpoint and interface control requests and `clearHalt` are checked against the calling connection's claims before the device is asked, and claims, releases and closes on one device run one at a time.
**Systems:** packages/usb/web/index.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, two USB connections share one browser `USBDevice`. Exact
records interface ownership per connection and rejects a second connection's
`claimInterface`, but the transfer methods at
`packages/usb/web/index.js:102-106` call the shared device directly. They do
not check `c.claimed`. Once connection A claims an interface, connection B
can issue class/vendor requests to that interface or consume its endpoint
data without owning the claim. Separate consumers can corrupt one another's
protocol or firmware transfer.

The browser only sees the shared `USBDevice` and its claimed interface, so it
cannot enforce Exact's connection ownership. The agent substitute does
reject an interface-recipient control transfer without that connection's
claim (`host/web-js/devices.js:44-51`); the real implementation therefore
disagrees with drives that appear to qualify this boundary.

## Reproduction

Ran the unmodified module under Bun with a stand-in `USBDevice` that rejects
interface transfers unless its browser-level interface is claimed:

```js
const d = (await impl.getDevices())[0];
const a = await impl.open({ device: d.id });
const b = await impl.open({ device: d.id });
await impl.claimInterface({ ...a, interfaceNumber: 0 });
console.log(impl.usbConnection(b).claimed); // []
console.log(await impl.controlTransferOut({
  ...b,
  setup: { recipient: 'interface', index: 0,
    requestType: 'class', request: 1, value: 0 },
  data: 'Qg=='
})); // { status: 'ok', bytesWritten: 1 }
```

The device received byte 66 from B. This tests the package's ownership
boundary, not hardware or browser conformance. No physical device was used.

## Direction and acceptance

Check interface-recipient control requests against the calling connection's
claims. Resolve endpoint transfers and endpoint-recipient requests to their
interface and check that ownership too. Serialize claim/release/close state
changes around asynchronous browser operations so checking ownership cannot
be overtaken by a close or a competing claim. Cover two connections to one
device: the owner succeeds, the nonowner is refused before device I/O, and
ownership can be transferred only after release.

## Duplicate check

Checked open and closed filesystem issues and the GitHub issue export on
2026-11-12 for USB, interface claims, transfers and connection ownership. The
existing USB reports concern native numeric conversions and Linux device
metadata; none covers this ownership bypass.
