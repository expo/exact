// A stand-in for a Convex deployment, for an app's tests (LLP 1157 §6): the
// server half of the sync protocol over functions given as plain JavaScript
// on in-memory state, so `bun exact.mjs test` runs with no Convex backend,
// from empty data, on this machine. It answers what `convex@1.49.0`'s
// `src/browser/sync/protocol.ts` declares and a backend was recorded sending
// (test/sync.test.ts, test/identity.test.ts): a `Transition` per query-set or
// identity change, a `MutationResponse` and then a `Transition` to every
// session whose results the mutation changed. Convex's HTTP API answers too
// (`POST /api/query`, `/api/mutation`, `/api/action`), so a test can write as
// another client would. As Convex does, it answers a mutation a client has
// sent before (its session id and request id) with its first answer, for as
// long as that client lives: a `Connect` whose `connectionCount` is 0 is a
// new one. It checks no token's signature and runs no Convex
// code: the app's real functions are proven against Convex itself.
//
// An app's server.ts:
//   import { serve } from '@exact/convex/test/server.ts';
//   serve({ port: Number(process.env.PORT), queries: { 'board:get': (args, { identity }) => … }, mutations: { … } });

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
/** Who is asking: the `sub` of the token the client sent, taken as it is (no signature is checked), or `null` signed out. */
export type Caller = { identity: string | null };
type Handler = (args: any, caller: Caller) => Json | void;
export interface Functions {
  /** The port to listen on, on 127.0.0.1; 0 for any free one, which `serve` returns. */
  port: number;
  /** By path (`module:function`): a query's value for its arguments, read from the caller's own state. */
  queries: Record<string, Handler>;
  /** A mutation changes that state and returns its value; one that throws is refused with the error's message. */
  mutations?: Record<string, Handler>;
  actions?: Record<string, Handler>;
  /** Any other HTTP request (an HTTP action's route, an auth endpoint the app calls): its response, or `undefined`
   *  for none. A response without CORS headers is given permissive ones, as the web's page is another origin. */
  http?: (request: Request) => Response | undefined | Promise<Response | undefined>;
}

type Version = { querySet: number; identity: number; ts: string };
type Session = Caller & { client: string; version: Version; queries: Map<number, { udfPath: string; args: Json; last: string }>; send: (message: object) => void; close: (code?: number, reason?: string) => void; idle: ReturnType<typeof setInterval> | undefined };
type Outcome = { ok: true; value: Json } | { ok: false; message: string };

/** A timestamp as the wire has it: a u64, little-endian, in base64. */
export const wireTs = (n: number) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(n), true); return btoa(String.fromCharCode(...b)); };

/** A JWT's `sub`, unchecked; `undefined` when the token is not a JWT. */
export function subject(token: string): string | null | undefined {
  try {
    const payload = JSON.parse(atob(token.split('.')[1].replace(/-/g, '+').replace(/_/g, '/')));
    return typeof payload.sub === 'string' ? payload.sub : null;
  } catch { return undefined; }
}

const CORS = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Methods': 'GET, POST, PUT, PATCH, DELETE, OPTIONS', 'Access-Control-Allow-Headers': 'Content-Type, Authorization, Convex-Client', 'Access-Control-Max-Age': '600' };
/** `response` with permissive CORS headers, unless it has its own. A new response: a redirect's or a fetched
 *  response's headers are immutable. */
const withCors = (response: Response) => {
  if (response.headers.has('Access-Control-Allow-Origin')) return response;
  const headers = new Headers(response.headers);
  for (const [name, value] of Object.entries(CORS)) headers.set(name, value);
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
};
const canonical = (path: string) => path.replace(/\.js:/, ':');

export function serve({ port, queries, mutations = {}, actions = {}, http }: Functions) {
  const sessions = new Set<Session>();
  /** What each client's mutations were answered, by its session id and request id. */
  const answered = new Map<string, Map<number, string>>();
  let clock = 1;
  const call = (handler: Handler | undefined, udfPath: string, args: Json, caller: Caller): Outcome => {
    try {
      if (!handler) throw new Error(`Could not find public function for '${udfPath}'.`);
      return { ok: true, value: (handler(args, { identity: caller.identity }) ?? null) as Json };
    } catch (e) { return { ok: false, message: String((e as Error).message ?? e) }; }
  };
  /** A session's `Transition` to `end`: each of `ids`' results, and every other query's that changed. */
  const transition = (s: Session, end: Partial<Version>, ids: number[] = [], removed: number[] = [], always = true) => {
    const modifications: object[] = removed.map(queryId => ({ type: 'QueryRemoved', queryId }));
    for (const [queryId, q] of s.queries) {
      const outcome = call(queries[q.udfPath], q.udfPath, q.args, s);
      const result = outcome.ok ? { type: 'QueryUpdated', value: outcome.value } : { type: 'QueryFailed', errorMessage: outcome.message };
      const text = JSON.stringify(result);
      if (text === q.last && !ids.includes(queryId)) continue;
      q.last = text;
      modifications.push({ ...result, queryId, logLines: [], journal: null });
    }
    if (!modifications.length && !always) return;
    const startVersion = s.version;
    s.version = { ...s.version, ...end };
    s.send({ type: 'Transition', startVersion, endVersion: s.version, modifications });
  };
  /** After a write, each session's queries that it changed, at the write's timestamp. */
  const wrote = (ts: string, writer?: Session) => { for (const other of sessions) transition(other, { ts }, [], [], other === writer); };

  /** Convex's HTTP API: `{ path, args }` in, `{ status, value }` or `{ status, errorMessage }` out. */
  async function api(kind: 'query' | 'mutation' | 'action', request: Request): Promise<Response> {
    const body = await request.json().catch(() => ({})) as { path?: string; args?: Json };
    const token = /^Bearer (.+)$/.exec(request.headers.get('Authorization') ?? '')?.[1];
    const identity = token === undefined ? null : subject(token);
    if (identity === undefined) return Response.json({ code: 'InvalidAuthHeader', message: 'Could not parse JWT payload.' }, { status: 401 });
    const path = canonical(String(body.path ?? '')), table = kind === 'query' ? queries : kind === 'mutation' ? mutations : actions;
    const outcome = call(table[path], path, body.args ?? {}, { identity });
    if (!outcome.ok) return Response.json({ status: 'error', errorMessage: outcome.message, logLines: [] });
    if (kind !== 'query') wrote(wireTs(++clock));
    return Response.json({ status: 'success', value: outcome.value, logLines: [] });
  }

  const server = Bun.serve<Session, never>({
    hostname: '127.0.0.1', port,
    async fetch(request, bun) {
      const { pathname } = new URL(request.url);
      if (pathname.endsWith('/sync') && bun.upgrade(request, { data: { identity: null, client: '', version: { querySet: 0, identity: 0, ts: wireTs(0) }, queries: new Map(), send: () => {}, close: () => {}, idle: undefined } })) return undefined;
      const kind = /^\/api\/(query|mutation|action)$/.exec(pathname)?.[1] as 'query' | 'mutation' | 'action' | undefined;
      if (kind && request.method === 'POST') return withCors(await api(kind, request));
      const own = await http?.(request);
      if (own) return withCors(own);
      if (request.method === 'OPTIONS') return withCors(new Response(null, { status: 204 }));
      return withCors(new Response('A Convex stand-in for tests (@exact/convex/test/server.ts).', { status: pathname === '/' ? 200 : 404 }));
    },
    websocket: {
      open(ws) {
        const s = ws.data;
        s.send = message => { ws.send(JSON.stringify(message)); };
        s.close = (code, reason) => ws.close(code, reason);
        // Convex pings a connection that has been quiet for 15 seconds.
        s.idle = setInterval(() => s.send({ type: 'Ping' }), 15000);
        sessions.add(s);
      },
      close(ws) { clearInterval(ws.data.idle); sessions.delete(ws.data); },
      message(ws, text) {
        const s = ws.data, m = JSON.parse(String(text));
        if (m.type === 'ModifyQuerySet') {
          if (m.baseVersion !== s.version.querySet) {
            s.send({ type: 'FatalError', error: `Base version ${m.baseVersion} passed up doesn't match the current version ${s.version.querySet}` });
            return s.close(1011, 'InternalServerError');
          }
          const added: number[] = [], removed: number[] = [];
          for (const mod of m.modifications) {
            if (mod.type === 'Add') { s.queries.set(mod.queryId, { udfPath: canonical(mod.udfPath), args: mod.args[0] ?? {}, last: '' }); added.push(mod.queryId); }
            else if (s.queries.delete(mod.queryId)) removed.push(mod.queryId);
          }
          transition(s, { querySet: m.newVersion, ts: wireTs(clock) }, added, removed);
        } else if (m.type === 'Connect') {
          s.client = String(m.sessionId);
          if (m.connectionCount === 0) answered.delete(s.client);
        } else if (m.type === 'Mutation' || m.type === 'Action') {
          const kind = m.type === 'Mutation' ? 'MutationResponse' : 'ActionResponse';
          const seen = m.type === 'Mutation' ? answered.get(s.client) ?? answered.set(s.client, new Map()).get(s.client)! : undefined;
          // The answer as it was sent: a handler may change the value it returned.
          const first = seen?.get(m.requestId);
          if (first !== undefined) return s.send(JSON.parse(first));
          const outcome = call((m.type === 'Mutation' ? mutations : actions)[canonical(m.udfPath)], m.udfPath, m.args[0] ?? {}, s);
          const ts = outcome.ok ? wireTs(++clock) : undefined;
          const response = outcome.ok
            ? { type: kind, requestId: m.requestId, success: true, result: outcome.value, logLines: [], ...(m.type === 'Mutation' ? { ts } : {}) }
            : { type: kind, requestId: m.requestId, success: false, result: outcome.message, logLines: [] };
          seen?.set(m.requestId, JSON.stringify(response));
          s.send(response);
          if (ts !== undefined) wrote(ts, s);
        } else if (m.type === 'Authenticate') {
          // Convex closes the socket on an identity version it is not at (recorded), and refuses a token it cannot read.
          if (m.baseVersion !== s.version.identity) return s.close(1011, 'InternalServerError');
          const identity = m.tokenType === 'User' ? subject(String(m.value)) : null;
          if (identity === undefined) {
            s.send({ type: 'AuthError', error: 'Could not parse JWT payload.', baseVersion: m.baseVersion, authUpdateAttempted: true });
            return s.close();
          }
          s.identity = identity;
          transition(s, { identity: m.baseVersion + 1, ts: wireTs(clock) });
        }
      },
    },
  });
  return { stop: () => server.stop(true), port: server.port };
}
