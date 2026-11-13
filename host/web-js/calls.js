// Tool calls on the JS target (LLP 1111 D4, LLP 1133.023 D7), as the runner's `modules/calls.rs`: a door hands
// `{tool, args, door}`, and the tool's action runs as one commit of origin `tool`, after any wake or earlier call
// (waiting up to 5 s), done when the work it owns has settled, else `timeout` 10 s after its dispatch. `fails when`
// and `returns` are read once, at the end.
//
// What a call owns (D7, "The web"): the page cannot tell one request from another, so the call stamps the requests
// its dispatch commit issued, and then those issued in a turn in which one of its own settled (a `then`'s ask, an
// answer's next send); a mutation queued from its dispatch on; a request the server holds among them. A person's
// commit meanwhile is not the call's, but neither is it told apart when it replaces the call's ask, so a call here
// never ends `superseded`.
//
// Each faced tool is offered to the browser's agent through WebMCP where the page has `document.modelContext`
// (https://webmachinelearning.github.io/webmcp/, D8): registered with an input schema from the plan's types and the
// face's words, an output schema of the call's envelope, its annotations (`readOnlyHint` for a `reads` that sends
// nothing, `consequentialHint` for a `destroys` and for a `writes` but an `additive closed` one, `untrustedContentHint`
// for a string `returns`), withdrawn while `available when` is false, and its `execute` a call through the `web` door,
// which the caller's abort cancels.
import { clock, data, After, Clocked, Mutations, Origin, say, time as refreshTime, drive, order } from "./rt.js";
import { Counting, heldTickets } from "./held.js";

const WAIT = 5_000, BOUND = 10_000, KEPT = 20;
/** The commands a call's commit does not run, each moving the person's view (D4a); any other but RUN is refused. */
const SKIPPED = new Set(["focus", "blur", "scrollIntoView", "selectText", "setSelectionRange", "fastSeek", "load", "haptic"]);
const RUN = new Set(["postMessage", "stopSounds", "setSounds", "preventDefault", "stopPropagation"]);

/** `json` against plan type `type` (type_descriptors.rs), strictly, as the runner's `tools.rs` checks it: the value the
 * runtime holds (a record as its fields in order), else an Error naming the first failing path. */
function check(json, type, path) {
  const fail = why => { throw new Error(path ? `${path}: ${why}` : why); };
  const show = () => JSON.stringify(json) ?? "undefined";
  if (typeof type === "string") {
    const ok = type === "n" ? typeof json === "number" && isFinite(json) : type === "b" ? typeof json === "boolean" : type === "s" ? typeof json === "string" : json === null;
    if (!ok) fail(`${{ n: "a finite number", b: "true or false", s: "a string" }[type] ?? "null"}, not ${show()}`);
    return json;
  }
  if (Array.isArray(type)) {
    if (type[0] === "?") return json === null ? null : check(json, type[1], path);
    if (type[0] === "e") { if (!type[1].includes(json)) fail(`one of ${type[1].join(", ")}, not ${show()}`); return json; }
    if (!Array.isArray(json)) fail(`a list, not ${show()}`);
    return json.map((x, i) => check(x, type[1], `${path}[${i}]`));
  }
  if (json === null || typeof json !== "object" || Array.isArray(json)) fail(`an object, not ${show()}`);
  const extra = Object.keys(json).find(k => !Object.hasOwn(type, k));
  if (extra !== undefined) fail(`no field \`${extra}\``);
  return Object.entries(type).map(([f, t]) => {
    const at = path ? `${path}.${f}` : f;
    if (json[f] === undefined) throw new Error(`${at}: missing`);
    return check(json[f], t, at);
  });
}

/** The JSON Schema of plan type `type`, with the face's title and `integer` for a parameter (D8). */
function schema(type, face = {}) {
  const words = { ...(face.title ? { title: face.title } : {}), ...(face.description ? { description: face.description } : {}) };
  if (typeof type === "string") return { type: { n: face.type === "integer" ? "integer" : "number", b: "boolean", s: "string" }[type] ?? "null", ...words };
  if (Array.isArray(type)) {
    if (type[0] === "?") return { anyOf: [schema(type[1]), { type: "null" }], ...words };
    if (type[0] === "e") return { type: "string", enum: type[1], ...words };
    return { type: "array", items: schema(type[1]), ...words };
  }
  return { type: "object", properties: Object.fromEntries(Object.entries(type).map(([f, t]) => [f, schema(t)])), required: Object.keys(type),
    additionalProperties: false, ...words };
}

/** WebMCP's `outputSchema` (D8): LLP 1111 D4's envelope, which `execute` resolves with for every outcome, its `value`
 * typed from `returns` (`type`, or null for none). An outcome other than `ok` is data in the result, not a rejection:
 * the caller of a rejected `execute` learns only "OperationError" (WebMCP §3, "tool execution error"). */
function output(type) {
  const words = description => ({ type: "string", description });
  const properties = {
    outcome: { ...words("ok: it ran and its work settled. failed: the app reports that it failed. refused: it did not run, or it stopped because it needs a person. timeout: its work had not settled 10 s after it started."),
      enum: ["ok", "failed", "superseded", "timeout", "interrupted", "refused"] },
    certainty: { ...words("none: the action did not run. applied: it ran and its work settled. uncertain: it ran, and its effect may or may not have landed; read before calling again."),
      enum: ["none", "applied", "uncertain"] },
    reason: words("Why it was refused or failed, as one word (unavailable, arguments, busy, cancelled, …); empty when there is none."),
    stale: { type: "boolean", description: "Something else changed what it reports while it ran." },
    skipped: { type: "array", items: { type: "string" }, description: "The commands it did not run because a call has no person: focus, scrolling, haptics." },
    value: type ? { anyOf: [schema(type), { type: "null" }], description: "What the tool returns; null when it was refused." } : { type: "null" },
    message: words("A sentence for the agent, empty when there is nothing to add."),
  };
  return { type: "object", properties, required: Object.keys(properties), additionalProperties: false };
}

/** `table`: tool name → [action, its parameters as a record type, consequence, [additive, idempotent, closed, sends],
 * description, fails, returns, available (each a thunk or null), face (an object or null), the type `returns` has (or
 * null)]. */
export function tools(table) {
  const waiting = [], ended = [];
  let current = null, ids = 0, poll = 0, wake = null;
  const answer = t => !t.stream && !t.req?.stream;
  const time = () => (globalThis.exact?.epochAtZero ?? 0) + clock.now;
  const wakeRunning = () => { const w = globalThis.exact?.extensions?.wake?.(); return !!w && w.outcome == null; };

  // ---- what the call owns: the tickets its dispatch commit issued, then those issued in a turn in which one of its
  // own settled, and the mutations queued from its dispatch on
  let seen = new Set();
  const look = () => {
    const now = new Set([...Counting, ...heldTickets()]), own = current?.own;
    if (own) {
      const settled = [...own.tickets].some(t => !now.has(t));
      if (own.adopting || settled) for (const t of now) if (!seen.has(t)) own.tickets.add(t);
      for (const t of [...own.tickets]) if (!now.has(t)) own.tickets.delete(t);
    }
    seen = now;
  };
  const owns = () => { const own = current?.own; return !!own && ([...own.tickets].some(answer) || Mutations.some(m => !own.mutations.has(m) && (m.due < Infinity || m.wait?.length))); };

  const finish = (c, outcome, certainty, reason = "", value = null, message = "") => {
    say(`tool ${c.name} ${outcome} ${certainty}`);
    if (!message && certainty === "uncertain") message = "It may or may not have taken effect: read before calling again.";
    ended.push({ id: c.id, tool: c.name, door: c.door, origin: "tool", cold: c.cold, outcome, certainty, reason, stale: false, skipped: c.skipped,
      cancelled: c.cancelled, value: outcome === "refused" ? null : value, message, at: time() });
    while (ended.length > KEPT) ended.shift();
    c.done?.(ended.at(-1));
  };
  const end = (...ending) => { const c = current; current = null; finish(c, ...ending); };
  const refuse = (reason, message) => { say(`tool ${current.name}: refused, ${reason} (${message})`); end("refused", "none", reason, null, message); };
  const faceRefused = (face, door, digest) => {
    if (door === "agent") return null;
    if (!face) return `it has no face, so the ${door} door does not offer it`;
    if (face.doors && !face.doors.includes(door)) return `its face does not name the ${door} door`;
    if (digest != null && digest !== face.digest) return `face: the door's digest ${digest} is not the plan's ${face.digest}`;
    return null;
  };
  // Background storage: the module's storage out while no answer is (the runner's `background.rs` ticket, which this
  // page cannot tell from an answer's own storage while one is out).
  const storage = () => { const b = data.background?.(); return b && Counting.size === 0 ? b.queued + b.inFlight : 0; };
  const dispatch = () => {
    const [action, params, , , , , , available, face] = table[current.name];
    if (available && available() === false) return refuse("unavailable", "it is not available now");
    const why = faceRefused(face, current.door, current.digest);
    if (why) return refuse("unavailable", why);
    let values;
    try { values = check(current.args, params, ""); } catch (e) { return refuse("arguments", e.message); }
    if (storage() > 0) return refuse("busy", "the app's storage is still writing");
    current.dispatched = clock.now;
    say(`tool:system:${current.door} ${current.name} dispatched`);
    look();
    // Each look until the dispatch commit returns takes what it issued.
    current.own = { tickets: new Set(), mutations: new Set(Mutations), adopting: true };
    // A refused commit puts everything back and answers false (rt.js `commit`); one whose tree update a view
    // transition defers has already landed its writes and answers true.
    let landed;
    try { landed = action()(...values); } catch { landed = false; }
    if (landed === false) { end("failed", "uncertain", "refused-commit"); return; }
    look();
    current.own.adopting = false;
  };
  const complete = () => {
    const [, , , , , fails, returns] = table[current.name];
    if (storage() > 0) return end("failed", "uncertain", "background");
    const value = returns ? returns() : null;
    if (fails && fails() === true) end("failed", "uncertain", "", value);
    else end("ok", "applied", "", value);
  };
  function step() {
    refreshTime();
    if (wake) { const i = clock.timers.indexOf(wake); if (i >= 0) clock.timers.splice(i, 1); wake = null; }
    look();
    for (;;) {
      // A waiting call's 5 s runs out whatever holds it: a wake or another call.
      while (waiting.length && clock.now >= waiting[0].arrived + WAIT) {
        const c = waiting.shift();
        say(`tool ${c.name}: refused, busy (a wake or another call is running)`);
        finish(c, "refused", "none", "busy", null, "a wake or another call is running");
      }
      if (current?.dispatched != null) {
        if (clock.now >= current.dispatched + BOUND) { end("timeout", "uncertain"); continue; }
        if (owns()) return again();
        complete();
        continue;
      }
      if (!waiting.length) { clearTimeout(poll); poll = 0; drive(); return; }
      if (wakeRunning()) return again();
      current = waiting.shift();
      dispatch();
    }
  }
  function again() {
    const due = Math.min(current?.dispatched != null ? current.dispatched + BOUND : Infinity, waiting.length ? waiting[0].arrived + WAIT : Infinity);
    wake = { due, once: true, data: true, i: order(), action: () => { wake = null; step(); return true; } };
    clock.timers.push(wake); drive();
    if (!poll) poll = setTimeout(() => { poll = 0; step(); }, 20);
  }
  After.push(() => { if (current || waiting.length) queueMicrotask(step); else look(); });
  Clocked.push(() => { if (current || waiting.length) queueMicrotask(step); });

  // ---- a call's origin and its commands (D4a), beside a wake's (modules.js)
  Origin.call = () => current?.dispatched != null ? "tool" : null;
  const outerNow = Origin.now, outerRefused = Origin.refused;
  Origin.now = () => { const o = outerNow(); return o === "app" ? Origin.call() ?? o : o; };
  Origin.callRefuses = name => {
    if (RUN.has(name)) return false;
    if (SKIPPED.has(name)) { current?.skipped.push(name); say(`${name}: skipped in a tool call`); } else say(`${name}: refused: in a tool call`);
    return true;
  };
  Origin.refused = (name, from) => from === "tool" ? Origin.callRefuses(name) : !!outerRefused?.(name, from);

  const call = (req, done) => {
    refreshTime();
    if (!Object.hasOwn(table, req.tool)) {
      const names = Object.keys(table);
      return { error: names.length ? `no tool \`${req.tool}\`; its tools: ${names.join(", ")}` : `no tool \`${req.tool}\`: this app declares none` };
    }
    let args = req.args ?? {};
    if (typeof args === "string") try { args = JSON.parse(args); } catch { args = null; }
    const c = { id: ++ids, name: req.tool, args, door: req.door ?? "agent", digest: req.digest ?? null, cold: !!req.cold, arrived: clock.now,
      dispatched: null, skipped: [], cancelled: false, done };
    say(`tool:system:${c.door} ${c.name}: arrived`);
    waiting.push(c);
    queueMicrotask(step);
    return { call: c.id, tool: c.name, state: "waiting" };
  };
  // The caller cancelled call `id`: one still waiting is dropped, `refused` `none` (`cancelled`); one dispatched runs to
  // its end, since its commit has landed and what it started is the app's to finish (the runner's `cancel_call`).
  const cancel = id => {
    const i = waiting.findIndex(c => c.id === id);
    if (i >= 0) {
      const [c] = waiting.splice(i, 1);
      c.cancelled = true;
      say(`tool:system:${c.door} ${c.name}: cancelled before it ran`);
      finish(c, "refused", "none", "cancelled", null, "Its caller cancelled it before it ran.");
      return { call: id, state: "ended" };
    }
    if (current?.id === id) {
      if (!current.cancelled) say(`tool:system:${current.door} ${current.name}: its caller cancelled; it runs to its end`);
      current.cancelled = true;
      return { call: id, state: "running" };
    }
    return ended.some(e => e.id === id) ? { call: id, state: "ended" } : { error: `no call ${id}` };
  };
  const open = (c, state) => ({ id: c.id, tool: c.name, door: c.door, origin: "tool", state });
  (globalThis.exact ??= {}).calls = {
    /** `state.intents`: the tools and the calls, never their arguments. */
    state: () => ({
      tools: Object.entries(table).map(([name, [, , consequence, [additive, idempotent, closed], description, , , available, face]]) =>
        ({ name, consequence, additive, idempotent, closed, description, available: !(available && available() === false), face })),
      calls: [...ended, ...(current ? [open(current, "running")] : []), ...waiting.map(c => open(c, "waiting"))],
    }),
    /** The agent's `call`: queued, its id answered. */
    call: req => call(req),
    /** The caller's cancellation (`cancel` above). */
    cancel,
    /** The agent's poll: the ended envelope, else the call's state. */
    called: id => ended.find(e => e.id === id) ?? (current?.id === id ? { call: id, state: "running" } : waiting.some(c => c.id === id)
      ? { call: id, state: "waiting" } : { error: `no call ${id}` }),
  };

  // ---- WebMCP (D8)
  const context = globalThis.document?.modelContext;
  if (context?.registerTool) {
    // `turnedDown`: tools the browser refused, offered again only after they have been unavailable, so a
    // refusal that stands (a name another script holds) is not retried at every commit.
    const offered = new Map(), turnedDown = new Set();
    const offer = () => {
      for (const [name, [, params, consequence, [additive, , closed, sends], description, , , available, face, returns]] of Object.entries(table)) {
        if (!face || (face.doors && !face.doors.includes("web"))) continue;
        const on = !(available && available() === false);
        if (!on) turnedDown.delete(name);
        else if (turnedDown.has(name)) continue;
        if (on === offered.has(name)) continue;
        if (!on) { offered.get(name).abort(); offered.delete(name); say(`webmcp ${name}: withdrawn`); continue; }
        const input = schema(params);
        for (const [p, words] of Object.entries(face.parameters ?? {})) if (input.properties[p]) input.properties[p] = schema(params[p], words);
        const signal = new AbortController();
        offered.set(name, signal);
        // A refusal (Chrome's InvalidStateError for a bad or duplicate name, an empty description) leaves the tool
        // unoffered until it has been unavailable; a rejection with the signal's reason is its own withdrawal (§4.2).
        const refused = e => {
          if (signal.signal.aborted && e === signal.signal.reason) return;
          say(`webmcp ${name}: refused: ${e?.name}: ${e?.message}`);
          if (offered.get(name) === signal) { offered.delete(name); turnedDown.add(name); }
        };
        // Only a registration still held is journalled as registered: one withdrawn before the browser answered is
        // not offered, whatever its promise says.
        const registered = () => { if (offered.get(name) === signal && !signal.signal.aborted) say(`webmcp ${name}: registered`); };
        Promise.resolve(context.registerTool({
          name, title: face.title ?? name, description, inputSchema: input, outputSchema: output(returns),
          // `readOnlyHint`: a `reads` tool whose reach sends nothing, since what a source does with a send is the module's
          // (LLP 1111 D3). `consequentialHint`: "significant, real-world, or non-reversible" (§4.2.1), so every write but
          // one that only adds to the app's own data. `untrustedContentHint`: a string `returns` may carry text the author
          // did not write, such as a note or a review (§6.4.4); a number or a bool cannot.
          annotations: { readOnlyHint: consequence === "reads" && !sends, consequentialHint: consequence === "destroys" || (consequence === "writes" && !(additive && closed)),
            untrustedContentHint: returns === "s" },
          // The caller's abort (§4.2.2) cancels the call: dropped while it waits, run to its end once dispatched.
          execute: (input, options) => new Promise(resolve => {
            const r = call({ tool: name, args: input ?? {}, door: "web", digest: face.digest }, resolve);
            if (r.error) return resolve({ outcome: "refused", certainty: "none", reason: "unavailable", stale: false, skipped: [], value: null, message: r.error });
            const abort = options?.signal;
            if (abort?.aborted) cancel(r.call);
            else abort?.addEventListener("abort", () => cancel(r.call), { once: true });
          }).then(({ outcome, certainty, reason, stale, skipped, value, message }) => ({ outcome, certainty, reason, stale, skipped, value, message })),
        }, { signal: signal.signal })).then(registered, refused);
      }
    };
    After.push(offer);
    queueMicrotask(offer);
  }
}
