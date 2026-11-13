# A closed web serial handle can write to a later connection on the same port

**Status:** Open
**Systems:** packages/serial/web/index.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, `connection()` checks that a handle exists but not that it is
open (`packages/serial/web/index.js:66-70`). `close()` keeps the connection in
the map and marks it closed. Opening the same `SerialPort` again creates a
different connection pointing at the same browser object.

An outstanding callback holding the first handle can therefore call `write`
or `setSignals` after the second open and control the second connection.
`write` obtains the port's *current* writer at line 147. Its ordering queue
belongs to the old connection, so these writes also bypass the new
connection's queue. The real-module path in `host/web-js/modules.js:852`
forwards to the implementation without the agent substitute's open-state
check.

## Reproduction

Ran the unmodified module under Bun with a stand-in `SerialPort` whose
`open()` creates a `WritableStream`, `close()` clears it, and write sink
records bytes:

```js
const p = (await impl.getPorts())[0];
const first = await impl.open({ port: p.id, baudRate: 9600 });
await impl.close(first);
const second = await impl.open({ port: p.id, baudRate: 9600 });
console.log(impl.serialConnection(first).open); // false
console.log(await impl.write({ ...first, data: 'QQ==' })); // null: success
// The second connection's sink received byte 65.
await impl.close(second);
```

The probe imported `packages/serial/web/index.js` directly. No hardware or
browser drive was used. The transition under test is open → close → reopen
→ use an old handle; browser enforcement cannot distinguish these two
package handles because they share one `SerialPort`.

## Direction and acceptance

Reject closed or unplugged handles for operations that require a live
connection, and recheck queued writes before obtaining the writer. Keep any
explicitly allowed final buffered-input read separate from this guard. Test
that a closed handle cannot write, change signals, or close a later open of
the same port, including a write queued before close.

## Duplicate check

Checked open and closed filesystem issues and the GitHub issue export on
2026-11-12 for serial, closed handles, reopen and ownership. The existing
serial reports cover blocked native writes, descriptor cleanup, enumeration,
DTR and large web reads; none covers this stale handle transition.
