// @exact/motion on the web (LLP 1133.013 D2, D11): the Generic Sensor
// classes where the browser has them (Chromium), else `devicemotion` and
// `deviceorientation`, answered in the interface's shapes. The page runtime
// (host/web-js/modules.js) calls it outside the agent; under the agent the
// runner's substitute answers and this is never loaded.
//
// - One acquisition per name for the page (D4), and for `deviceOrientation`
//   one per event type (`absolute` picks `deviceorientationabsolute`): a
//   `Sensor` object, or one listener of an event, at the fastest
//   `frequency` its readers and canvas inputs ask, 60 Hz for a reader that
//   names none, as Apple's; each reader group (a name with one
//   `frequency`, `referenceFrame` and `history`) keeps a ring of its own
//   thinned samples, numbered, and is answered statelessly from the cursor
//   the runner passes as `after` (D5).
// - The device frame is the specifications' (Device Orientation §3.1);
//   `referenceFrame: "screen"` turns x and y by `screen.orientation.angle`
//   (Accelerometer §7.1), per reader, from the one stream.
// - WebKit reports `devicemotion`'s accelerations with the opposite sign to
//   the specification and Chromium (`kGravity` in WebCoreMotionManager.mm);
//   known by `webkitCompassHeading`, which only WebKit ships, they are
//   negated, and nothing else is (D2).
// - `requestMotionPermission` calls both events' `requestPermission` in the
//   turn it is called, before awaiting either, so both stay inside the
//   press (D11): the runtime calls it synchronously in the input event's
//   dispatch (`"gesture": true`).
// - Acquisition stops while the page is hidden and resumes when it is
//   shown; the next answer of each reader reads `interrupted` (D8).
// - A canvas input needs the host's `surfaces.input`, a writer into the
//   surface's input; where the context has none, `startSensorInput` fails
//   `NotSupportedError`.

const G = 9.80665;
const RING = 1024;
const CLASSES = {
  accelerometer: 'Accelerometer', linearAccelerationSensor: 'LinearAccelerationSensor', gravitySensor: 'GravitySensor',
  gyroscope: 'Gyroscope', magnetometer: 'Magnetometer',
  absoluteOrientationSensor: 'AbsoluteOrientationSensor', relativeOrientationSensor: 'RelativeOrientationSensor',
};
const ORIENTATIONS = new Set(['absoluteOrientationSensor', 'relativeOrientationSensor']);
// `startSensorInput`'s sensor names to the sources whose streams feed them.
const INPUTS = {
  accelerometer: 'accelerometer', linearAcceleration: 'linearAccelerationSensor', gravity: 'gravitySensor',
  gyroscope: 'gyroscope', magnetometer: 'magnetometer',
  absoluteOrientation: 'absoluteOrientationSensor', relativeOrientation: 'relativeOrientationSensor',
};

const global = typeof globalThis === 'object' ? globalThis : {};
const webkit = () => typeof global.DeviceOrientationEvent === 'function' && 'webkitCompassHeading' in global.DeviceOrientationEvent.prototype;
const hasEvents = () => typeof global.DeviceMotionEvent === 'function';
const hasClass = name => typeof global[CLASSES[name]] === 'function';
const angle = () => global.screen?.orientation?.angle ?? 0;

/** `v` (x, y, z) in the screen's frame at `deg` (Accelerometer §7.1). */
function toScreen([x, y, z], deg) {
  const r = deg * Math.PI / 180, c = Math.round(Math.cos(r)), s = Math.round(Math.sin(r));
  return [x * c - y * s, x * s + y * c, z];
}
/** Quaternion `q` ([x, y, z, w]) of the device frame, of the screen's frame at `deg`. */
function quaternionToScreen([x, y, z, w], deg) {
  const h = deg * Math.PI / 360, bz = -Math.sin(h), bw = Math.cos(h);
  // q ⊗ (0, 0, −sin(θ/2), cos(θ/2)): a vector's screen components are its device components turned by θ about z.
  return [x * bw + y * bz, y * bw - x * bz, z * bw + w * bz, w * bw - z * bz];
}
/** Device Orientation's alpha, beta, gamma (degrees, Z-X'-Y'') as a quaternion (Orientation Sensor §6.1). */
function quaternionFromAngles(alpha, beta, gamma) {
  const d = Math.PI / 360, [a, b, g] = [(alpha ?? 0) * d, (beta ?? 0) * d, (gamma ?? 0) * d];
  const [ca, sa, cb, sb, cg, sg] = [Math.cos(a), Math.sin(a), Math.cos(b), Math.sin(b), Math.cos(g), Math.sin(g)];
  return [
    sb * cg * ca - cb * sg * sa,
    cb * sg * ca + sb * cg * sa,
    cb * cg * sa + sb * sg * ca,
    cb * cg * ca - sb * sg * sa,
  ];
}
const finite = v => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const xyz = v => (v && [v.x, v.y, v.z].every(n => typeof n === 'number') ? [v.x, v.y, v.z] : null);

export function implementation({ changed, now = () => performance.now(), foreground = () => true, log = () => {}, surfaces } = {}) {
  // The page's time origin to the session clock: a sample's DOMHighResTimeStamp, on the session's (D7).
  const offset = () => now() - performance.now();
  const streams = new Map(); // streamKey(name, args) → { name, absolute, stop, frequency, groups: Map, latest }
  const inputs = new Map(); // `${surface} ${sensor}` → { writer, name, period, next, dropped }
  let decided = null, paused = false;

  /** The platform's answer for one sample of `name`: `{values, at, fields}` in the device frame. */
  const reading = (name, values, at, extra = {}) => ({ name, values, at, extra });

  // A group's thinned ring: a sample nearest each point of its grid of 1 / frequency (D4).
  const group = (stream, args) => {
    const key = JSON.stringify([args.frequency ?? null, args.referenceFrame ?? 'device', args.history ?? 0]);
    let g = stream.groups.get(key);
    if (!g) stream.groups.set(key, g = { key, args, period: args.frequency > 0 ? 1000 / args.frequency : 0, next: -Infinity, ring: [], sequence: 0, pauses: [], waiting: [] });
    return g;
  };
  const take = (stream, sample) => {
    stream.latest = sample;
    for (const g of stream.groups.values()) {
      if (g.period && sample.at < g.next - g.period / 2) continue;
      g.next = Math.max(g.next + g.period, sample.at + g.period / 2);
      g.sequence += 1;
      g.ring.push({ sequence: g.sequence, sample });
      if (g.ring.length > Math.max(1, Math.min(RING, g.args.history ?? 0))) g.ring.shift();
      for (const w of g.waiting.splice(0)) w();
    }
    // No reader is asked again for a sample: the runtime polls each at its `interval`, and the samples between
    // are the next answer's batch.
    for (const input of inputs.values()) if (input.name === stream.name) feed(input, sample);
  };

  /** The fields a sample answers in `args.referenceFrame`. */
  const fields = (name, sample, args) => {
    const screen = args.referenceFrame === 'screen' ? angle() : 0, at = sample.at + offset();
    if (name === 'deviceMotion' || name === 'deviceOrientation') return { ...sample.extra, timeStamp: at };
    if (ORIENTATIONS.has(name)) return { quaternion: screen ? quaternionToScreen(sample.values, screen) : sample.values, timestamp: at, apple: null };
    const [x, y, z] = screen ? toScreen(sample.values, screen) : sample.values;
    return { x, y, z, timestamp: at };
  };

  // The platform stream `s`, started at `frequency`: a Generic Sensor object, else an event.
  const start = (s, frequency) => {
    const name = s.name;
    if (CLASSES[name] && hasClass(name)) {
      const Class = global[CLASSES[name]], options = frequency ? { frequency } : {};
      const sensor = new Class(options);
      sensor.addEventListener('reading', () => {
        const values = ORIENTATIONS.has(name) ? [...sensor.quaternion] : [sensor.x, sensor.y, sensor.z];
        if (values.every(Number.isFinite)) take(s, reading(name, values, sensor.timestamp));
      });
      sensor.addEventListener('error', e => fail(s, e.error?.name === 'NotAllowedError' ? 'NotAllowedError' : e.error?.name === 'NotReadableError' ? 'NotReadableError' : 'NotSupportedError'));
      sensor.start();
      return () => sensor.stop();
    }
    const motion = ['accelerometer', 'linearAccelerationSensor', 'gravitySensor', 'gyroscope', 'deviceMotion'].includes(name);
    const type = name === 'absoluteOrientationSensor' || s.absolute ? 'deviceorientationabsolute' : 'deviceorientation';
    const listener = motion ? e => {
      const sign = webkit() ? -1 : 1, with_ = xyz(e.accelerationIncludingGravity), without = xyz(e.acceleration);
      const at = e.timeStamp, scale = v => v && v.map(n => n * sign);
      const r = e.rotationRate, rad = Math.PI / 180;
      let values = null;
      if (name === 'accelerometer') values = scale(with_);
      if (name === 'linearAccelerationSensor') values = scale(without);
      if (name === 'gravitySensor') values = with_ && without ? scale(with_.map((n, i) => n - without[i])) : null;
      if (name === 'gyroscope') values = r && [r.beta, r.gamma, r.alpha].every(n => typeof n === 'number') ? [r.beta * rad, r.gamma * rad, r.alpha * rad] : null;
      if (name === 'deviceMotion') {
        const v3 = v => (v ? { x: v[0], y: v[1], z: v[2] } : null);
        const extra = { acceleration: v3(scale(without)), accelerationIncludingGravity: v3(scale(with_)),
          rotationRate: r ? { alpha: finite(r.alpha), beta: finite(r.beta), gamma: finite(r.gamma) } : null, interval: e.interval ?? 0 };
        return take(s, reading(name, [0, 0, 0], at, extra));
      }
      if (values?.every(Number.isFinite)) take(s, reading(name, values, at));
    } : e => {
      if (name === 'deviceOrientation') {
        const extra = { alpha: finite(e.alpha), beta: finite(e.beta), gamma: finite(e.gamma), absolute: !!e.absolute };
        return take(s, reading(name, [0, 0, 0], e.timeStamp, extra));
      }
      if ([e.alpha, e.beta, e.gamma].every(n => typeof n === 'number')) take(s, reading(name, quaternionFromAngles(e.alpha, e.beta, e.gamma), e.timeStamp));
    };
    addEventListener(motion ? 'devicemotion' : type, listener);
    return () => removeEventListener(motion ? 'devicemotion' : type, listener);
  };
  const supported = name => {
    if (CLASSES[name] && hasClass(name)) return true;
    if (name === 'magnetometer') return false;
    if (name === 'absoluteOrientationSensor' || name === 'deviceOrientation') return 'ondeviceorientationabsolute' in global || name === 'deviceOrientation';
    return hasEvents();
  };
  const fail = (s, code) => {
    s.error = code;
    for (const g of s.groups.values()) for (const w of g.waiting.splice(0)) w();
    changed(`motion.${s.name}`);
  };
  const streamKey = (name, args) => (name === 'deviceOrientation' && args.absolute ? 'deviceOrientation absolute' : name);
  // The stream of `name` that `args` reads, running at the fastest frequency of its groups and inputs; with
  // `reads` false (a canvas input), no reader group is made.
  const stream = (name, args, reads = true) => {
    const key = streamKey(name, args);
    let s = streams.get(key);
    if (!s) streams.set(key, s = { name, groups: new Map(), frequency: null, stop: null, latest: null, error: null, absolute: key !== name });
    const g = reads ? group(s, args) : null;
    restart(s);
    // A reader that moved to the other `deviceOrientation` event left its old stream with no group (`releaseArgs`).
    for (const [k, o] of streams) if (o !== s && o.name === name && !o.groups.size) { o.stop?.(); streams.delete(k); }
    return [s, g];
  };
  const rate = s => {
    const fs = [...s.groups.values()].map(g => (g.args.frequency > 0 ? g.args.frequency : 60))
      .concat([...inputs.values()].filter(i => i.name === s.name).map(i => 1000 / i.period));
    return fs.length ? Math.max(...fs) : null;
  };
  const restart = s => {
    const f = rate(s);
    if (s.stop && f === s.frequency) return;
    s.stop?.();
    s.frequency = f;
    s.stop = paused ? null : start(s, f);
  };

  // `MotionSensors`: each sensor by its `startSensorInput` name, and the two events.
  const sensorsOf = () => ({ ...Object.fromEntries(Object.entries(INPUTS).map(([input, name]) => [input, !!supported(name)])),
    deviceMotion: hasEvents(), deviceOrientation: typeof global.DeviceOrientationEvent === 'function' });
  const status = async () => {
    const sensors = sensorsOf();
    const available = hasEvents() || Object.values(sensors).some(Boolean);
    if (!available) return { available: false, state: 'unknown', sensors, apple: null, android: null };
    let state = 'granted';
    if (webkit() && typeof global.DeviceMotionEvent.requestPermission === 'function') state = decided ?? 'prompt';
    else if (global.navigator?.permissions) {
      try {
        const states = await Promise.all(['accelerometer', 'gyroscope'].map(name => navigator.permissions.query({ name }).then(p => p.state)));
        state = states.includes('denied') ? 'denied' : states.includes('prompt') ? 'prompt' : 'granted';
      } catch { /* a browser whose Permissions API names no sensor: the events ask nothing */ }
    }
    return { available, state, sensors, apple: null, android: null };
  };

  /** A reader's answer from its group's ring after `after` (D5). */
  const answer = async (name, args) => {
    if (!supported(name)) return { error: 'NotSupportedError' };
    const known = await status();
    if (known.state === 'denied') return { error: 'NotAllowedError' };
    // WebKit delivers no event until the person allowed it in a press (`requestMotionPermission`).
    if (known.state === 'prompt' && webkit()) return { error: 'NotAllowedError' };
    const [s, g] = stream(name, args);
    if (!s.latest && !s.error) {
      if (!foreground()) return { error: 'NotReadableError' };
      await new Promise(w => { g.waiting.push(w); setTimeout(w, 2000); });
    }
    if (s.error) return { error: s.error };
    if (!s.latest) return { error: 'NotReadableError' };
    const after = typeof args.after === 'number' ? args.after : null;
    const newest = g.ring.at(-1);
    const out = { ...fields(name, newest?.sample ?? s.latest, args), sequence: newest?.sequence ?? 0, samples: [], dropped: 0, interrupted: false };
    if (after != null) {
      const fresh = g.ring.filter(r => r.sequence > after), history = Math.min(RING, args.history ?? 0);
      const kept = fresh.slice(Math.max(0, fresh.length - history));
      const strip = ({ sequence, samples, dropped, interrupted, ...rest }) => rest;
      out.samples = history ? kept.map(r => strip(fields(name, r.sample, args))) : [];
      out.dropped = history ? Math.max(0, out.sequence - after - out.samples.length) : 0;
      out.interrupted = g.pauses.some(p => p >= after);
    }
    return out;
  };

  const feed = (input, sample) => {
    if (input.writer.closed) return;
    const values = ORIENTATIONS.has(input.name) ? sample.values : (input.screen ? toScreen(sample.values, angle()) : sample.values);
    if (input.period && sample.at < input.next - input.period / 2) return;
    input.next = Math.max(input.next + input.period, sample.at + input.period / 2);
    input.writer.push({ t: 'sensor', sensor: input.sensor, values: [...values, 0, 0, 0, 0].slice(0, 4), dropped: input.dropped, at: sample.at });
    input.dropped = 0;
  };

  const visibility = () => {
    const hidden = !foreground();
    if (hidden === paused) return;
    paused = hidden;
    for (const s of streams.values()) {
      if (paused) { s.stop?.(); s.stop = null; for (const g of s.groups.values()) g.pauses.push(g.sequence); }
      else restart(s);
    }
  };

  const sources = {};
  for (const name of [...Object.keys(CLASSES), 'deviceMotion', 'deviceOrientation']) sources[name] = args => answer(name, args ?? {});
  return {
    ...sources,
    motionStatus: status,
    motionStatusSync: () => ({ available: hasEvents() || Object.keys(CLASSES).some(hasClass), state: webkit() ? decided ?? 'prompt' : 'granted', sensors: sensorsOf(), apple: null, android: null }),
    /** Both events' `requestPermission`, called before either is awaited (D11). */
    requestMotionPermission(args) {
      const asks = [];
      if (typeof global.DeviceMotionEvent?.requestPermission === 'function') asks.push(global.DeviceMotionEvent.requestPermission());
      const orientation = global.DeviceOrientationEvent?.requestPermission;
      if (typeof orientation === 'function') asks.push(args?.absolute && orientation.length > 0 ? orientation.call(global.DeviceOrientationEvent, true) : orientation.call(global.DeviceOrientationEvent));
      if (!asks.length) return status();
      return Promise.allSettled(asks).then(results => {
        const said = results.map(r => (r.status === 'fulfilled' ? r.value : 'prompt'));
        decided = said.includes('denied') ? 'denied' : said.every(s => s === 'granted') ? 'granted' : decided;
        changed('motion.motionStatus');
        // Failed readers keep their last value until their source changes.
        for (const name of Object.keys(sources)) changed(`motion.${name}`);
        return status();
      });
    },
    startSensorInput(args) {
      const name = INPUTS[args.sensor], writer = surfaces?.input?.(args.surface);
      if (!name || !writer || !supported(name)) return { error: 'NotSupportedError' };
      const key = `${args.surface} ${args.sensor}`;
      inputs.get(key)?.writer.close?.();
      const frequency = args.frequency > 0 ? args.frequency : 60;
      inputs.set(key, { writer, name, sensor: args.sensor, period: 1000 / frequency, next: -Infinity, dropped: 0, screen: args.referenceFrame === 'screen' });
      stream(name, {}, false);
      log(`sensor input ${key}: ${frequency} Hz`);
      return { surface: args.surface, sensor: args.sensor };
    },
    stopSensorInput(args) {
      const key = `${args.surface} ${args.sensor}`, input = inputs.get(key);
      if (input) {
        inputs.delete(key);
        input.writer.push({ t: 'sensorEnded', sensor: args.sensor, at: performance.now() });
        input.writer.close?.();
        const s = streams.get(input.name);
        if (s) { if (!s.groups.size && ![...inputs.values()].some(i => i.name === s.name)) { s.stop?.(); streams.delete(s.name); } else restart(s); }
      }
      return { surface: args.surface, sensor: args.sensor };
    },
    foreground: visibility,
    /** A reader group gone (LLP 1133.009 D10 item 12). */
    releaseArgs(name, args) {
      const s = streams.get(streamKey(name, args ?? {}));
      if (!s) return;
      s.groups.delete(group(s, args ?? {}).key);
      restart(s);
    },
    /** The last reader of `name` gone: its streams stop unless a canvas input still reads one. */
    release(name) {
      for (const [key, s] of streams) {
        if (s.name !== name) continue;
        s.groups.clear();
        if ([...inputs.values()].some(i => i.name === name)) { restart(s); continue; }
        s.stop?.();
        streams.delete(key);
      }
    },
  };
}

// For the conformance suite (test/web.mjs): the pure conversions.
export const conversions = { toScreen, quaternionToScreen, quaternionFromAngles };
