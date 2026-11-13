// @exact/serial on the web (LLP 1133.037 D7–D9): Web Serial (https://serial.spec.whatwg.org/), the browser's chooser
// and its remembered ports, in the interface's shapes. The page runtime (host/web-js/modules.js) calls it outside the
// agent.
//
// - A port is a record with an id this page gives its `SerialPort` object; `getPorts()` is the browser's list.
// - One reader per connection drains `port.readable` into the bytes not yet delivered, and stops reading while they
//   reach the retained bound (D8: the larger of `bufferSize` and two seconds of the line, at most 16 MiB), so the
//   stream's backpressure holds the rest. Each `serialInput` answer is every byte since the last, with D9's non-fatal
//   errors at their offsets; the reader takes the port's next `readable` after each, as Web Serial says.
// - The web has no attached list (D3): `attachedPorts` is refused before it reaches here.

const MOST = 16 * 1024 * 1024;

const b64 = bytes => { let s = ''; for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000)); return btoa(s); };
const unb64 = text => Uint8Array.from(atob(text), c => c.charCodeAt(0));
const failure = (e, fallback = 'NetworkError') => ({ error: typeof e?.name === 'string' && e.name !== 'Error' ? e.name : fallback, message: String(e?.message ?? e) });

export function implementation({ changed }) {
  const serial = typeof navigator === 'object' ? navigator.serial : undefined;
  const ids = new Map(), ports = new Map(), connections = new Map();
  let next = 0, nextConnection = 0;
  const idOf = port => { if (!ids.has(port)) { const id = `port-${++next}`; ids.set(port, id); ports.set(id, port); } return ids.get(port); };
  const record = port => {
    const info = port.getInfo?.() ?? {};
    return { id: idOf(port), usbVendorId: info.usbVendorId ?? null, usbProductId: info.usbProductId ?? null, name: '', connected: port.connected !== false, granted: true };
  };
  const announce = () => { changed('serial.getPorts'); changed('serial.serialConnection'); };
  if (serial?.addEventListener) {
    serial.addEventListener('connect', announce);
    serial.addEventListener('disconnect', e => {
      for (const c of connections.values()) if (c.port === e.target) { c.unplugged = true; c.wake?.(); }
      announce();
    });
  }

  /** The connection's read loop: reads while the undelivered bytes are under the bound. */
  async function pump(c) {
    while (c.open && c.port.readable && !c.done) {
      const reader = c.port.readable.getReader();
      c.reader = reader;
      try {
        for (;;) {
          while (c.pending.length >= c.bound && c.open) await new Promise(r => { c.wake = r; });
          if (!c.open) break;
          const { value, done } = await reader.read();
          if (done) break;
          // One byte at a time: a spread passes each as an argument, which a read near `bufferSize`'s 16 MiB exceeds.
          if (value?.length) { for (const byte of value) c.pending.push(byte); changed('serial.serialInput'); }
        }
      } catch (e) {
        // A non-fatal error ends this readable; the port gives a new one (Web Serial's "readable" steps).
        const fatal = e?.name === 'NetworkError' || !c.port.readable;
        c.errors.push({ at: c.position + c.pending.length, code: fatal ? 'NetworkError' : e?.name ?? 'UnknownError' });
        if (fatal) c.done = true;
        changed('serial.serialInput');
      } finally {
        try { reader.releaseLock(); } catch { /* already released */ }
        c.reader = null;
      }
      if (!c.open) break;
    }
    c.done = true;
    changed('serial.serialInput');
  }

  const connection = args => {
    const c = connections.get(args?.connection);
    if (!c) return { error: { error: 'InvalidStateError', message: 'no such connection' } };
    return { c };
  };
  // A closed or unplugged handle controls nothing: its `SerialPort` may since have been opened again as another
  // connection, which would take its writes and signals.
  const live = args => {
    const r = connection(args);
    if (r.error || (r.c.open && !r.c.unplugged)) return r;
    return { error: { error: 'InvalidStateError', message: 'the connection is closed' } };
  };

  return {
    serialStatus() {
      return { available: !!serial, attachedList: false, state: serial ? 'granted' : 'denied' };
    },
    async requestPort(args) {
      if (!serial) return { error: 'NotSupportedError' };
      try {
        const options = {};
        if (args?.filters) options.filters = args.filters.map(f => Object.fromEntries(Object.entries(f).filter(([, v]) => v != null)));
        if (args?.allowedBluetoothServiceClassIds) options.allowedBluetoothServiceClassIds = args.allowedBluetoothServiceClassIds;
        const port = await serial.requestPort(options);
        changed('serial.getPorts');
        return record(port);
      } catch (e) {
        return failure(e, 'NotFoundError');
      }
    },
    async getPorts() {
      if (!serial) return [];
      return (await serial.getPorts()).map(record);
    },
    attachedPorts() { return { error: 'NotSupportedError' }; },
    async open(args) {
      const port = ports.get(args?.port);
      if (!port) return { error: 'NotFoundError' };
      const options = { baudRate: args.baudRate, dataBits: args.dataBits ?? 8, stopBits: args.stopBits ?? 1, parity: args.parity ?? 'none', bufferSize: args.bufferSize ?? 255, flowControl: args.flowControl ?? 'none' };
      try {
        await port.open(options);
      } catch (e) {
        return failure(e);
      }
      const handle = `serial-${++nextConnection}`;
      const bound = Math.min(MOST, Math.max(options.bufferSize, Math.floor(options.baudRate / 10 * 2)));
      const c = { port, options, open: true, done: false, unplugged: false, pending: [], errors: [], position: 0, bound, reader: null, wake: null, writer: Promise.resolve() };
      connections.set(handle, c);
      pump(c);
      announce();
      return { connection: handle, port: record(port), options };
    },
    serialConnection(args) {
      const c = connections.get(args?.connection);
      if (!c) return { open: false, connected: false, options: null };
      const connected = c.port.connected !== false && !c.unplugged;
      return { open: c.open && connected, connected, options: c.options };
    },
    async setSignals(args) {
      const { c, error } = live(args);
      if (error) return error;
      const signals = {};
      for (const k of ['dataTerminalReady', 'requestToSend', 'break']) if (args[k] != null) signals[k] = args[k];
      try { await c.port.setSignals(signals); return null; } catch (e) { return failure(e); }
    },
    async getSignals(args) {
      const { c, error } = live(args);
      if (error) return error;
      try { return await c.port.getSignals(); } catch (e) { return failure(e); }
    },
    serialInput(args) {
      const { c, error } = connection(args);
      if (error) return error;
      if (c.ended) return { error: 'InvalidStateError', message: 'the stream has ended' };
      const bytes = Uint8Array.from(c.pending.splice(0));
      const errors = c.errors.splice(0);
      c.position += bytes.length;
      c.wake?.();
      const done = c.done && c.pending.length === 0;
      if (done) c.ended = true;
      return { data: b64(bytes), errors, done, position: c.position };
    },
    async write(args) {
      const { c, error } = live(args);
      if (error) return error;
      // Writes go out in order: each waits for the one before it. One still waiting when the connection closes is
      // aborted, as closing a port aborts its writable's queued chunks (Web Serial's close()).
      const bytes = unb64(args.data ?? '');
      const done = c.writer.then(async () => {
        if (!c.open) throw new DOMException('the connection closed', 'AbortError');
        const writer = c.port.writable.getWriter();
        try { await writer.write(bytes); } finally { writer.releaseLock(); }
      });
      c.writer = done.catch(() => {});
      try { await done; return null; } catch (e) { return failure(e); }
    },
    async close(args) {
      const { c, error } = connection(args);
      if (error) return error;
      // An unplugged connection still closes; a closed one is done, and its port may be another connection's.
      if (!c.open) return { error: 'InvalidStateError', message: 'the connection is closed' };
      c.open = false;
      c.wake?.();
      try { await c.reader?.cancel(); } catch { /* the port went away */ }
      await c.writer;
      try { await c.port.close(); } catch { /* already closed */ }
      announce();
      return null;
    },
    async forget(args) {
      const port = ports.get(args?.port);
      if (!port) return null;
      for (const [handle, c] of connections) if (c.port === port) { c.open = false; c.wake?.(); connections.delete(handle); }
      try { await port.forget?.(); } catch (e) { return failure(e); }
      ports.delete(args.port);
      ids.delete(port);
      announce();
      return null;
    },
    release() {},
  };
}
