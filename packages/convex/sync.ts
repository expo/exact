// The client half of Convex's sync protocol (LLP 1157 D3), with no I/O: it is
// told what happened (the socket opened, a frame arrived, the app asked) and
// returns the frames to send. It reads no clock, no randomness and no global,
// and imports nothing, so a data module can hold it (LLP 1027.000). The
// protocol is the one `convex@1.49.0` speaks (its `src/browser/sync/`: `protocol.ts`,
// `local_state.ts`, `remote_query_set.ts`, `request_manager.ts`), and each
// rule below names the file it follows.

/** Convex's value encoding, from the app's own `convex/values`: this module imports no package, so an app has one copy of `convex`. */
export interface Codec { convexToJson(value: any): any; jsonToConvex(json: any): any }
type Value = unknown;

/** The npm client version whose protocol this speaks; it is the socket URL's `/api/<version>/sync`. */
export const PROTOCOL = '1.49.0';

export type Frame = { type: string; [field: string]: unknown };
type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type Version = { querySet: number; identity: number; ts: string };
/** A query's or a request's outcome: Convex's value, or its error's words and `ConvexError` data. `again`: the
 *  handle of a mutation the connection ended before Convex answered (`Sync.again`). */
export type Outcome = { ok: true; value: Value } | { ok: false; message: string; data?: Value; again?: number };

/** A Convex function's failure: `message` is the server's, `data` a `ConvexError`'s payload. `again`: set on a
 *  mutation the connection ended before Convex answered, which `again(handle)` sends once more as the same request. */
export class ConvexFailure extends Error {
  data: Value | undefined;
  again: number | undefined;
  constructor(message: string, data?: Value, again?: number) { super(message); this.name = 'ConvexFailure'; this.data = data; this.again = again; }
}

type Query = {
  id: number; udfPath: string; args: Json;
  /** The server's opaque cursor state for a paginated query, sent back on a reconnect (`local_state.ts`, `restart`). */
  journal: string | null | undefined;
  outcome: Outcome | undefined;
  /** The token `outcome` was computed for (`undefined`: signed out). It is given only while that is the token asked for. */
  who: string | undefined;
  /** A result for it has arrived on this connection, so each `Transition` makes it its end's identity's. */
  current: boolean;
  /** The generation of asks that last asked for it (see `ask`). */
  asked: number;
  /** The server has been told of it on this connection. */
  sent: boolean;
  /** Settles when an outcome arrives or the socket ends; one promise, however many asks wait. */
  wait?: { promise: Promise<void>; settle: () => void };
};
type Request = {
  kind: 'Mutation' | 'Action';
  /** The identity version of the last `Authenticate` sent ahead of it on its connection. */
  behind: number;
  /** How many times the app had signed out when it was sent (`Sync.era`). */
  era: number;
  /** What was sent: a mutation left unanswered is sent again as this frame, under the same request id. */
  frame: Frame;
  /** A mutation the server ran, held until a `Transition` reaches `ts`, so its caller reads its own write. */
  done?: { ts: string; value: Value };
  settle: (outcome: Outcome) => void;
};

const ZERO_TS = 'AAAAAAAAAAA=';
const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
/** A timestamp's eight bytes, most significant first: the wire's is a base64 little-endian u64 (`protocol.ts`, `u64ToLong`). */
function tsBytes(ts: string): number[] {
  const bytes: number[] = [];
  let bits = 0, held = 0;
  for (const c of ts) {
    const v = B64.indexOf(c);
    if (v < 0) continue;
    held = (held << 6) | v; bits += 6;
    if (bits >= 8) { bits -= 8; bytes.push((held >> bits) & 0xff); }
  }
  return bytes.slice(0, 8).reverse();
}
/** Whether timestamp `a` is at or before `b`. */
export function tsAtOrBefore(a: string, b: string): boolean {
  const x = tsBytes(a), y = tsBytes(b);
  for (let i = 0; i < 8; i++) if (x[i] !== y[i]) return x[i] < y[i];
  return true;
}

/** `messages:list` for `messages:list`, `messages.js:list` and a bare module's `default` (`udf_path_utils.ts`). */
export function canonicalPath(udfPath: string): string {
  const pieces = udfPath.split(':');
  const name = pieces.length === 1 ? 'default' : pieces[pieces.length - 1];
  let module = pieces.length === 1 ? pieces[0] : pieces.slice(0, -1).join(':');
  if (module.endsWith('.js')) module = module.slice(0, -3);
  return `${module}:${name}`;
}

/** A frame this client cannot follow. `fatal`: a new connection would meet it again. */
export class ProtocolError extends Error {
  fatal: boolean;
  constructor(message: string, fatal = false) { super(message); this.name = 'ProtocolError'; this.fatal = fatal; }
}

export class Sync {
  /** `connecting` until the socket opens, `open` until its first `Transition`, `ready` from then; `closed` once it has
   *  ended, or `failed` when it ended on a frame this client cannot follow, which a new connection would meet again. */
  status: 'connecting' | 'open' | 'ready' | 'closed' | 'failed' = 'connecting';
  /** Rises with each `Transition` applied: an answer a query gave may have changed. */
  rev = 0;
  /** Why the socket last ended; "" while it is open. */
  error = '';
  /** The server's words when it refused the token; "" otherwise. */
  authError = '';

  private sessionId = '';
  private connectionCount = 0;
  private lastCloseReason: string | null = 'InitialConnect';
  private maxObservedTs: string | undefined;
  /** What the server's next `Transition` must start from (`remote_query_set.ts`). */
  private version: Version = { querySet: 0, identity: 0, ts: ZERO_TS };
  private querySetVersion = 0;
  /** The token the app asked for last (`undefined`: signed out). */
  private token: string | undefined;
  /** The token of each identity version this connection's frames have asked for, from 0 (signed out): an
   *  `Authenticate` carries the last index and adds one. The module is not told when the runner refuses a
   *  commit and sends none of its frames, so one that never left is counted here too, until Convex shows
   *  it never arrived (`transition`). Meanwhile the next `Authenticate` names a version Convex is not at,
   *  and Convex closes the socket without running what follows it (recorded). */
  private identities: (string | undefined)[] = [undefined];
  /** For each of `identities`, the query-set version when it was asked for. */
  private identitiesAt: number[] = [0];
  /** Convex refused a token on this connection (`AuthError`): from which identity version it refused what
   *  followed, `Infinity` when the token expired under requests already sent, `undefined` when none was. */
  private refusedFrom: number | undefined;
  private queries = new Map<string, Query>();
  private byId = new Map<number, Query>();
  private nextQueryId = 0;
  private requests = new Map<number, Request>();
  private nextRequestId = 0;
  /** The mutations a connection ended before Convex answered, by request id, for `again`. Convex answers a
   *  request id a session has sent before with its first answer (`request_manager.ts`, `restart`, sends each
   *  again on that ground), so one sent again writes once. */
  private kept = new Map<number, Frame>();
  /** How many times the app has signed out. A mutation sent before a sign-out is not kept: sent again, it
   *  would run as whoever signs in next. */
  private era = 0;
  /** The newest `rev` an ask has carried, and the generation of asks before it. */
  private generation = -1;
  private chunks: string[] = [];
  private chunkId = '';
  /** The pings since this connection opened, while no `Transition` has arrived. */
  private silent = 0;

  constructor(private codec: Codec) {}

  private connected() { return this.status === 'open' || this.status === 'ready'; }

  /** A socket is being opened: `sessionId` names this client for as long as the module lives, so it is kept from the first. */
  connecting(sessionId: string) {
    if (!this.sessionId) this.sessionId = sessionId;
    this.status = 'connecting';
    this.error = '';
  }

  /** The socket opened: what the npm client sends then, in its order (`client.ts`, `onOpen`). The server
   *  starts every connection at version 0, so the token and the whole query set go again, and the next
   *  `Transition` is expected from 0. */
  opened(): Frame[] {
    this.status = 'open';
    this.version = { querySet: 0, identity: 0, ts: ZERO_TS };
    this.chunks = [];
    this.silent = 0;
    const frames: Frame[] = [{
      type: 'Connect', sessionId: this.sessionId, connectionCount: this.connectionCount, lastCloseReason: this.lastCloseReason,
      ...(this.maxObservedTs ? { maxObservedTimestamp: this.maxObservedTs } : {}),
      // The npm client's monotonic clock, for the server's skew estimate; a data module has none.
      clientTs: 0,
    }];
    this.connectionCount += 1;
    this.refusedFrom = undefined;
    this.identities = [undefined];
    this.identitiesAt = [0];
    this.querySetVersion = 0;
    if (this.token !== undefined) frames.push(this.authenticate());
    const adds = [...this.queries.values()].map(q => (q.sent = true, q.current = false, this.add(q)));
    frames.push({ type: 'ModifyQuerySet', baseVersion: 0, newVersion: 1, modifications: adds });
    this.querySetVersion = 1;
    return frames;
  }

  /** The socket ended, with why; `fatal` when a new connection would end the same way. A query with no
   *  answer yet fails, and is asked again when one arrives. A request in flight ends here too: an answer
   *  cannot wait across the gap (natively one that awaits with nothing outstanding is refused), so, unlike
   *  the npm client (`request_manager.ts`, `restart`), this sends no mutation again on the next connection:
   *  the app does, with `again`. One the server acknowledged succeeded; one sent behind an `Authenticate`
   *  Convex refused did not run; any other may have run. */
  closed(reason: string, fatal = false) {
    // The close that follows a fatal end is that end's, not a new one: `failed` and its reason stand.
    if (this.status === 'failed') return;
    this.status = fatal ? 'failed' : 'closed';
    this.error = reason;
    this.lastCloseReason = reason;
    for (const q of this.queries.values()) { q.sent = false; q.current = false; this.wake(q); }
    for (const [id, request] of this.requests)
      request.settle(request.done ? { ok: true, value: request.done.value } : this.left(id, request, reason));
    this.requests.clear();
  }

  /** The failure of a request Convex did not answer. A mutation is kept and its failure carries `again`, unless
   *  the app has signed out since it was sent. An action is not: Convex has no first answer of one to give again. */
  private left(id: number, request: Request, reason: string): Outcome {
    const message = this.unanswered(request, reason);
    if (request.kind !== 'Mutation' || request.era !== this.era) return { ok: false, message };
    // (The oldest goes when a hundred are kept.)
    if (this.kept.size >= 100) this.kept.delete(this.kept.keys().next().value!);
    this.kept.set(id, request.frame);
    return { ok: false, message, again: id };
  }

  /** Why a request Convex did not answer ended. Convex handles a connection's frames in order and closes it
   *  at an `Authenticate` it refuses, so a request sent behind that one never ran. A token that expired is
   *  refused at whichever request meets it first, and one sent before that may be running (an action with
   *  effects outside Convex), so it may have run, and the words say why the connection ended. */
  private unanswered(request: Request, reason: string): string {
    const kind = request.kind.toLowerCase();
    if (this.refusedFrom === undefined) return `The connection to Convex closed before it answered (${reason}); the ${kind} may have run.`;
    if (request.behind >= this.refusedFrom) return this.authError;
    return `Convex refused the token before it answered (${this.authError}); the ${kind} may have run.`;
  }

  /** A frame from the server, taken into the state, and the frames to send in answer. One this client cannot
   *  follow throws a `ProtocolError`: the caller ends the socket with its words. */
  frame(text: string): Frame[] {
    const message = JSON.parse(text) as Frame;
    if (this.chunks.length && message.type !== 'TransitionChunk') throw new ProtocolError(`Convex sent ${String(message.type)} inside a chunked Transition`);
    switch (message.type) {
      case 'Ping':
        // Convex answers a connection's first frames at once and pings a quiet one every 15 s. A second ping
        // with no `Transition` yet is taken as frames that never left (a refused commit takes them back,
        // unsaid): the module cannot tell that from a first answer Convex is half a minute late with.
        if (this.status === 'open' && (this.silent += 1) >= 2) throw new ProtocolError('Convex sent no results on this connection; connecting again');
        return [];
      case 'TransitionChunk': {
        // A large `Transition` in parts, in order (`web_socket_manager.ts`, `assembleTransition`).
        const { chunk, partNumber, totalParts, transitionId } = message as unknown as { chunk: string; partNumber: number; totalParts: number; transitionId: string };
        if (partNumber !== this.chunks.length || (partNumber > 0 && transitionId !== this.chunkId)) throw new ProtocolError(`Convex sent TransitionChunk ${partNumber} of ${transitionId}, expected ${this.chunks.length}${this.chunks.length ? ` of ${this.chunkId}` : ''}`);
        this.chunkId = transitionId;
        this.chunks.push(chunk);
        if (this.chunks.length < totalParts) return [];
        const whole = this.chunks.join('');
        this.chunks = [];
        return this.transition(JSON.parse(whole));
      }
      case 'Transition': return this.transition(message as never);
      case 'MutationResponse':
      case 'ActionResponse': this.response(message as never); return [];
      case 'AuthError': {
        // The server closes the socket after it. The token is dropped, so the next connection is not refused
        // for it too; the app gets a fresh one and calls `setAuth`.
        this.authError = String(message.error ?? 'Convex refused the token');
        // `authUpdateAttempted`: Convex refused the `Authenticate` from `baseVersion`, so it ran nothing after it.
        this.refusedFrom = message.authUpdateAttempted === true && typeof message.baseVersion === 'number' ? message.baseVersion + 1 : Infinity;
        this.token = undefined;
        return [];
      }
      case 'FatalError': throw new ProtocolError(`Convex: ${String(message.error)}`, true);
      default: throw new ProtocolError(`Convex sent ${String(message.type)}, which protocol ${PROTOCOL} does not have`, true);
    }
  }

  private transition(t: { startVersion: Version; endVersion: Version; modifications: Array<Record<string, unknown>> }): Frame[] {
    const { startVersion: start, endVersion: end } = t, at = this.version;
    if (start.querySet !== at.querySet || start.identity !== at.identity || start.ts !== at.ts)
      throw new ProtocolError(`Convex sent a Transition from ${start.querySet}:${start.identity}:${start.ts}, expected ${at.querySet}:${at.identity}:${at.ts}`);
    if (end.identity >= this.identities.length) throw new ProtocolError(`Convex sent identity version ${end.identity}, which this client never asked for`);
    for (const m of t.modifications) {
      const q = this.byId.get(m.queryId as number);
      // (A query removed here while its result was on the way.)
      if (!q) continue;
      if (m.type === 'QueryUpdated') q.outcome = { ok: true, value: this.codec.jsonToConvex(m.value as never) };
      else if (m.type === 'QueryFailed') q.outcome = { ok: false, message: String(m.errorMessage), ...(m.errorData === undefined ? {} : { data: this.codec.jsonToConvex(m.errorData as never) }) };
      else continue;
      if (m.journal !== undefined) q.journal = m.journal as string | null;
      q.current = true;
    }
    this.version = end;
    this.status = 'ready';
    this.observe(end.ts);
    // Convex's results at `end` are its identity's: it names each result the identity changed (LLP 1157
    // §1), so one it leaves unnamed is that person's too. One from an earlier connection is not, until this
    // connection delivers it: a connection's first `Transition` moves only the identity (recorded).
    const who = this.identities[end.identity];
    for (const q of this.queries.values()) if (q.current) q.who = who;
    this.release();
    for (const [id, request] of this.requests) {
      if (!request.done || !tsAtOrBefore(request.done.ts, end.ts)) continue;
      this.requests.delete(id);
      request.settle({ ok: true, value: request.done.value });
    }
    this.rev += 1;
    // Convex has taken a query set asked for after the last `Authenticate`, and is at an identity before it:
    // the frames past that identity never left. Its token is asked for again, from where Convex is.
    const last = this.identities.length - 1;
    if (end.identity === last || end.querySet <= this.identitiesAt[last]) return [];
    this.identities.length = this.identitiesAt.length = end.identity + 1;
    return this.unconfirmed() ? [this.authenticate()] : [];
  }

  private response(r: { type: string; requestId: number; success: boolean; result: Json; ts?: string; errorData?: Json }) {
    const request = this.requests.get(r.requestId);
    if (!request || request.done) return;
    if (!r.success) {
      this.requests.delete(r.requestId);
      request.settle({ ok: false, message: String(r.result), ...(r.errorData === undefined ? {} : { data: this.codec.jsonToConvex(r.errorData as never) }) });
      return;
    }
    const value = this.codec.jsonToConvex(r.result as never);
    if (r.type === 'ActionResponse') { this.requests.delete(r.requestId); request.settle({ ok: true, value }); return; }
    // Held until the queries show it (`request_manager.ts`, `onResponse`), or settled now when they already do.
    this.observe(r.ts!);
    if (tsAtOrBefore(r.ts!, this.version.ts)) { this.requests.delete(r.requestId); request.settle({ ok: true, value }); }
    else request.done = { ts: r.ts!, value };
  }

  private observe(ts: string) {
    if (this.maxObservedTs === undefined || tsAtOrBefore(this.maxObservedTs, ts)) this.maxObservedTs = ts;
  }

  private add(q: Query): Frame {
    return { type: 'Add', queryId: q.id, udfPath: q.udfPath, args: [q.args], ...(q.journal === undefined ? {} : { journal: q.journal }) };
  }

  private wake(q: Query) {
    const wait = q.wait;
    q.wait = undefined;
    wait?.settle();
  }

  /** An outcome is given only when it was computed for the token the app asked for last. */
  private given(q: Query): Outcome | undefined {
    return q.outcome !== undefined && q.who === this.token ? q.outcome : undefined;
  }

  /** Wakes the asks whose query now has an outcome to give. */
  private release() {
    for (const q of this.queries.values()) if (q.wait && this.given(q)) this.wake(q);
  }

  /** Whether Convex has not yet confirmed the token the app asked for on this connection. */
  private unconfirmed(): boolean {
    return this.version.identity !== this.identities.length - 1 || this.identities[this.identities.length - 1] !== this.token;
  }

  /** `Authenticate` with the token asked for, from the last version this connection asked for. */
  private authenticate(): Frame {
    const baseVersion = this.identities.length - 1;
    this.identities.push(this.token);
    this.identitiesAt.push(this.querySetVersion);
    return this.token === undefined ? { type: 'Authenticate', tokenType: 'None', baseVersion } : { type: 'Authenticate', tokenType: 'User', value: this.token, baseVersion };
  }

  /** The app asks for a query's answer at `rev`. `read` gives its outcome, or nothing when there is none
   *  yet: the caller then waits on `wait`, which settles when one arrives or the socket ends. `frames`
   *  tell the server of a query new to it, and remove the ones nobody asks for any more.
   *
   *  A query resource takes `rev` as an argument, so every live one is asked again at each `rev`
   *  (LLP 1157 D4). The asks at one `rev` are a generation; when the first ask of a newer one arrives,
   *  a query the generation before it did not ask for has no resource left, and is removed. */
  ask(rev: number, udfPath: string, args: Record<string, Value>): { read: () => Outcome | undefined; wait: Promise<void>; frames: Frame[] } {
    const mods: Frame[] = [];
    if (rev > this.generation) {
      for (const [token, q] of this.queries) {
        if (q.asked >= this.generation) continue;
        this.queries.delete(token);
        this.byId.delete(q.id);
        this.wake(q);
        if (q.sent && this.connected()) mods.push({ type: 'Remove', queryId: q.id });
      }
      this.generation = rev;
    }
    const path = canonicalPath(udfPath), json = this.codec.convexToJson(args as never) as Json;
    const token = JSON.stringify({ udfPath: path, args: json });
    let q = this.queries.get(token);
    if (!q) {
      q = { id: this.nextQueryId++, udfPath: path, args: json, journal: undefined, outcome: undefined, who: undefined, current: false, asked: this.generation, sent: false };
      this.queries.set(token, q);
      this.byId.set(q.id, q);
    }
    q.asked = this.generation;
    if (!q.sent && this.connected()) { q.sent = true; mods.push(this.add(q)); }
    const frames: Frame[] = [];
    if (mods.length) {
      frames.push({ type: 'ModifyQuerySet', baseVersion: this.querySetVersion, newVersion: this.querySetVersion + 1, modifications: mods });
      this.querySetVersion += 1;
    }
    const waiting = q, read = () => this.given(waiting);
    if (read() === undefined && !q.wait) {
      let settle = () => {};
      q.wait = { promise: new Promise<void>(resolve => { settle = resolve; }), settle };
    }
    return { read, frames, wait: q.wait?.promise ?? Promise.resolve() };
  }

  /** A mutation or an action, while the socket is open: the frames to send and its outcome. A mutation's
   *  outcome waits for the `Transition` that shows its write.
   *
   *  Until Convex confirms the token asked for, the request goes behind that token's `Authenticate` again:
   *  the earlier one may not have left (a refused commit sends none of its frames), and if it did not, this
   *  one names a version Convex is not at, so Convex closes the socket instead of running the request as
   *  the person before. If it did, this one moves the identity to the same person. */
  request(kind: 'Mutation' | 'Action', udfPath: string, args: Record<string, Value>, deliver: (frames: Frame[]) => void = () => {}): { frames: Frame[]; outcome: Promise<Outcome> } {
    const message: Frame = { type: kind, requestId: this.nextRequestId, udfPath: canonicalPath(udfPath), args: [this.codec.convexToJson(args as never)] };
    const sent = this.dispatch(kind, this.nextRequestId, message, deliver);
    this.nextRequestId += 1;
    return sent;
  }

  /** The mutation `handle` names (a failure's `again`), sent once more under its own request id, while the
   *  socket is open: Convex answers with what it answered first, or runs it now if it never did, as the
   *  person asked for now. Nothing when no kept mutation has that handle: each is sent again once. */
  again(handle: number, deliver: (frames: Frame[]) => void = () => {}): { frames: Frame[]; outcome: Promise<Outcome> } | undefined {
    const frame = this.kept.get(handle);
    if (!frame) return undefined;
    const sent = this.dispatch('Mutation', handle, frame, deliver);
    this.kept.delete(handle);
    return sent;
  }

  private dispatch(kind: 'Mutation' | 'Action', requestId: number, message: Frame, deliver: (frames: Frame[]) => void): { frames: Frame[]; outcome: Promise<Outcome> } {
    const guarded = this.unconfirmed(), frames = guarded ? [this.authenticate(), message] : [message];
    try { deliver(frames); }
    catch (e) {
      // A turn that may not send: nothing is in flight.
      if (guarded) { this.identities.pop(); this.identitiesAt.pop(); }
      throw e;
    }
    const behind = this.identities.length - 1;
    const outcome = new Promise<Outcome>(settle => this.requests.set(requestId, { kind, behind, era: this.era, frame: message, settle }));
    return { frames, outcome };
  }

  /** The person's token changed (`undefined`: signed out). The answers held are kept, since Convex's next
   *  `Transition` names only the ones that changed, and each is given again only once it is known to be
   *  this token's (`given`). `deliver` sends the frame. A token Convex has not confirmed is sent again
   *  when asked for again, so a call whose frame never left can be repeated. Signing out forgets the
   *  mutations kept for `again`, also when Convex's refusal had already dropped the token. */
  setAuth(token: string | undefined, deliver: (frames: Frame[]) => void = () => {}): Frame[] {
    if (token === this.token && !(this.connected() && this.unconfirmed())) { if (token === undefined) this.signedOut(); return []; }
    const was = this.token;
    this.token = token;
    let frames: Frame[] = [];
    try {
      frames = this.connected() ? [this.authenticate()] : [];
      // A turn that may not send throws here, and nothing has changed.
      deliver(frames);
    } catch (e) {
      if (frames.length) { this.identities.pop(); this.identitiesAt.pop(); }
      this.token = was;
      throw e;
    }
    this.authError = '';
    if (token === undefined) this.signedOut();
    this.release();
    return frames;
  }

  private signedOut() { this.kept.clear(); this.era += 1; }

  /** For `state` and tests: the queries the server is asked for, and the requests in flight. */
  inspect() {
    return { queries: [...this.queries.values()].map(q => ({ id: q.id, udfPath: q.udfPath, sent: q.sent, answered: q.outcome !== undefined })), requests: this.requests.size };
  }
}
