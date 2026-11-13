// @exact/geolocation on the web (LLP 1133.002 D15): the browser's
// `navigator.geolocation` and `navigator.permissions`, answered in the
// interface's shapes. The page runtime (host/web-js/modules.js) calls it
// outside the agent; under the agent the runner's substitute answers and
// this is never loaded.
//
// - An option the call omits stays the browser's own (D2); one it writes is
//   passed as written.
// - `accuracy` is `unknown`: the web says nothing of approximate location (D8).
// - One `watchPosition` registration serves every reader with the same
//   options, and each set of options has its own, so the browser holds each
//   reader to its own accuracy, timeout and cached-fix age (D2). Each new fix
//   announces `geolocation.watchPosition`, so readers are asked again;
//   `releaseArgs` clears a registration when no reader asks with its options,
//   and `release` every one when the last reader goes (D5).
// - `requestPreciseLocation` is `NotSupportedError` on the web (D10).
// - The state is live: `PermissionStatus`'s change, and, since WebKit sends
//   none, a look again when the page returns to the foreground (where the
//   person comes back from the browser's settings).

const CODES = ['', 'PERMISSION_DENIED', 'POSITION_UNAVAILABLE', 'TIMEOUT'];

/** The browser's position as the interface's `GeolocationPosition`. */
function position(p) {
  const c = p.coords, n = v => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  return {
    coords: {
      latitude: c.latitude, longitude: c.longitude, accuracy: c.accuracy,
      altitude: n(c.altitude), altitudeAccuracy: n(c.altitudeAccuracy), heading: n(c.heading), speed: n(c.speed),
    },
    timestamp: p.timestamp, apple: null, android: null,
  };
}

const failure = e => ({ error: CODES[e?.code] || 'POSITION_UNAVAILABLE' });

/** The written options, and only those (D2). */
function options(args) {
  const out = {};
  for (const k of ['enableHighAccuracy', 'timeout', 'maximumAge']) if (args?.[k] != null) out[k] = args[k];
  return out;
}

export function implementation({ changed }) {
  const geo = typeof navigator === 'object' ? navigator.geolocation : undefined;
  // Queried afresh each time: WebKit's `PermissionStatus` keeps the state it was made with.
  const query = () => navigator.permissions.query({ name: 'geolocation' });
  let permission = null, said = null;
  // By options, as `options` writes them: the browser's watch id, its last answer, and the reads awaiting one.
  const watches = new Map();
  const clear = key => { const w = watches.get(key); if (w) { geo.clearWatch(w.id); watches.delete(key); } };
  const again = () => query().then(p => { if (p.state !== said) changed('geolocation.geolocationStatus'); }, () => {});
  if (typeof addEventListener === 'function') {
    addEventListener('focus', again);
    addEventListener('visibilitychange', again);
  }
  return {
    async geolocationStatus() {
      if (!geo) return { available: false, state: 'unknown', accuracy: 'unknown', apple: null, android: null };
      let state = 'unknown';
      try {
        permission = await query();
        permission.onchange = again;
        state = said = permission.state;
      } catch { /* a browser without the Permissions API's geolocation: unknown */ }
      return { available: true, state, accuracy: 'unknown', apple: null, android: null };
    },
    getCurrentPosition(args) {
      if (!geo) return Promise.resolve({ error: 'NotSupportedError' });
      return new Promise(answer => geo.getCurrentPosition(p => answer(position(p)), e => answer(failure(e)), options(args)));
    },
    watchPosition(args) {
      if (!geo) return Promise.resolve({ error: 'NotSupportedError' });
      const asked = options(args), key = JSON.stringify(asked);
      let w = watches.get(key);
      if (!w) {
        w = { id: null, last: null, waiting: [] };
        const deliver = v => { w.last = v; for (const answer of w.waiting.splice(0)) answer(v); changed('geolocation.watchPosition'); };
        watches.set(key, w);
        w.id = geo.watchPosition(p => deliver(position(p)), e => deliver(failure(e)), asked);
      }
      return w.last ? Promise.resolve(w.last) : new Promise(answer => w.waiting.push(answer));
    },
    async requestPreciseLocation() {
      return { error: 'NotSupportedError' };
    },
    /** No reader asks `source` with `args` any longer: its registration goes. */
    releaseArgs(source, args) {
      if (source === 'watchPosition') clear(JSON.stringify(options(args)));
    },
    /** The last reader of `source` is gone: stop what it started. */
    release(source) {
      if (source === 'watchPosition') for (const key of [...watches.keys()]) clear(key);
    },
  };
}
