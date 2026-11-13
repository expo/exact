// @exact/convex (LLP 1157): Convex for an Exact data module. One stream
// resource owns the socket; each Convex query is a resource that takes the
// stream's `rev` and answers from what the socket has delivered.

import { ConvexFailure, PROTOCOL, ProtocolError, Sync, type Frame, type Outcome } from './sync.ts';

export { ConvexFailure };

/** What `live()` answers; `ConvexLive` in Contract. */
export interface ConvexLive {
  /** "connecting" until the socket opens, "open" until the first results arrive, "ready" from then; "closed" once it
   *  has ended (refresh it to connect again), or "failed" when it ended on something a new connection would meet too. */
  status: string;
  /** Rises when a query's answer may have changed. Every query resource takes it as an argument. */
  rev: number;
  /** Why the socket ended; "" otherwise. */
  error: string;
  /** Convex's words when it refused the token; "" otherwise. */
  authError: string;
}

type Socket = { send(data: string): void; close(code?: number, reason?: string): void };
type StreamEvent = { readonly type: string; readonly data: string | ArrayBuffer; readonly kind?: string; readonly message?: string; readonly code?: number; readonly reason?: string };

const MAX_MESSAGE_BYTES = 8 * 1024 * 1024, IDLE_TIMEOUT_MS = 60000;

/** Convex's own pure functions, which the app imports from its `convex` and hands over: `getFunctionName` from
 *  `convex/server`, `convexToJson` and `jsonToConvex` from `convex/values`. The module imports no package itself,
 *  so the app has one copy of `convex` and the wire's encoding is Convex's. */
export interface ConvexFunctions { getFunctionName(reference: any): string; convexToJson(value: any): any; jsonToConvex(json: any): any }

// A generated function reference (`api.messages.list`), as `convex/server` types one.
type Reference<Kind extends string> = { _type: Kind; _args: any; _returnType: any };
type ArgsOf<R extends Reference<string>> = R['_args'];
type ReturnOf<R extends Reference<string>> = R['_returnType'];

export interface ConvexOptions {
  /** The largest frame Convex may send, in bytes: 8 MiB when absent. The first frame of a connection carries every
   *  query's result, so it is the sum of them. A larger one ends the socket as `failed`. Natively an open stream
   *  counts twice its ceiling against the 68 MiB that independent reads share. */
  maxMessageBytes?: number;
  /** How long the socket may carry nothing before the host ends it, in milliseconds of real time: 60000 when absent,
   *  four of the frames Convex sends every 15 s. The socket then ends as `closed`, to be opened again. */
  idleTimeoutMs?: number;
}

export interface ConvexClient {
  /** The `live` source's answer: the socket, as a stream. One root resource asks it; refreshing that resource connects again. */
  live(): Promise<ConvexLive>;
  /** A query's answer at `rev` (`live.rev`): now, when the socket has delivered one, so a changed `rev` shows every
   *  query's new result in one commit; else once it arrives. `shape` maps Convex's value to the resource's shape, in
   *  the same turn. A failed query throws `ConvexFailure`, and so does one asked while the socket is closed. */
  query<Q extends Reference<'query'>, T = ReturnOf<Q>>(rev: number, ref: Q, args: ArgsOf<Q>, shape?: (value: ReturnOf<Q>) => T): T | Promise<T>;
  /** A mutation: its result once the queries show its write. It rejects with `ConvexFailure` when Convex refuses it,
   *  when the socket is not open (nothing is sent), when Convex refused the token it was sent behind (its words;
   *  it did not run), and when the socket closes first (it may have run, and an expired token's words say why). A
   *  failure of the last two kinds carries `again`, unless the app has signed out since the mutation was sent. */
  mutation<M extends Reference<'mutation'>>(ref: M, args: ArgsOf<M>): Promise<ReturnOf<M>>;
  /** An action: its result, as soon as Convex has it; the queries may show what it wrote a moment later. It rejects
   *  as a mutation does, with no `again`. */
  action<A extends Reference<'action'>>(ref: A, args: ArgsOf<A>): Promise<ReturnOf<A>>;
  /** The person's token (a JWT the deployment is configured to accept), or `undefined` signed out. Call it in a
   *  mutation's source; in an `async` one it may follow an `await`. From a plain `.then` callback its send throws on the web.
   *  It sends on the socket, so a source's synchronous part may not also `fetch`: the runner refuses that mutation and
   *  sends nothing. Await the fetch first, then call it. Until Convex confirms the token, no answer computed for another is given, and each
   *  mutation and action is sent behind the token again, so none runs as the person before; a call whose frame never
   *  left may be made again. Convex says nothing when a token expires on a quiet socket: renew it before it expires. */
  setAuth(token: string | undefined): void;
  /** A mutation Convex did not answer before its connection ended, sent once more: `handle` is its failure's `again`.
   *  Convex answers a request it has seen with its first answer, so the write happens once; one it never saw runs
   *  now, as the person signed in now. Call it in a mutation's source once `live.status` is "open" or "ready"
   *  again. A handle is good once: when the connection ends before Convex answers again, that failure carries the
   *  next, and "it did not run" then speaks of that sending only. The module keeps the last hundred unanswered
   *  mutations, and signing out (`setAuth(undefined)`) forgets every handle. */
  again<T = unknown>(handle: number): Promise<T>;
  /** The queries Convex is asked for and the requests in flight, for a test or a log. */
  inspect(): { queries: { id: number; udfPath: string; sent: boolean; answered: boolean }[]; requests: number };
}

/** A client for the deployment at `address` (its `CONVEX_URL`, or the same origin as a socket's, `wss:` or `ws:`), over
 *  `functions`, Convex's own. */
export function convex(address: string, functions: ConvexFunctions, options: ConvexOptions = {}): ConvexClient {
  const sync = new Sync(functions);
  const { getFunctionName } = functions;
  const url = `${address.replace(/^http/, 'ws').replace(/\/+$/, '')}/api/${PROTOCOL}/sync`;
  let socket: Socket | undefined, epoch = 0;
  const send = (frames: Frame[]) => { for (const frame of frames) socket?.send(JSON.stringify(frame)); };
  const snapshot = (): ConvexLive => ({ status: sync.status, rev: sync.rev, error: sync.error, authError: sync.authError });
  const settle = <T>(outcome: Outcome, shape?: (value: any) => T): T => {
    if (!outcome.ok) throw new ConvexFailure(outcome.message, outcome.data, outcome.again);
    return shape ? shape(outcome.value) : outcome.value as T;
  };

  function live(): Promise<ConvexLive> {
    const mine = ++epoch;
    const id = new Uint8Array(16);
    crypto.getRandomValues(id);
    sync.connecting([...id].map(b => b.toString(16).padStart(2, '0')).join('').replace(/^(.{8})(.{4})(.{4})(.{4})(.{12})$/, '$1-$2-$3-$4-$5'));
    socket = undefined;
    return fetch(url, {
      exactIndependentHttp: { maxResponseBytes: options.maxMessageBytes ?? MAX_MESSAGE_BYTES },
      // A socket that carries nothing ends as a Timeout, which is `closed` below (LLP 1016.000 D11).
      exactIdleTimeout: options.idleTimeoutMs ?? IDLE_TIMEOUT_MS,
      exactStream(event: StreamEvent, given?: Socket): ConvexLive {
        if (mine !== epoch) return snapshot();
        if (event.type === 'open') { socket = given; send(sync.opened()); }
        else if (event.type === 'message') {
          try { if (typeof event.data === 'string') send(sync.frame(event.data)); }
          catch (e) {
            // A frame the client cannot follow ends the socket; the snapshot says why, where a throw would keep the last one.
            socket = undefined;
            sync.closed((e as Error).message, e instanceof ProtocolError && e.fatal);
            given?.close(1000);
          }
        } else {
          socket = undefined;
          // A refusal (a frame over the ceiling, an origin outside the grants) would meet a new connection too.
          const refused = event.type === 'error' && event.kind === 'Refused';
          const over = refused && /ceiling/.test(event.message ?? '') ? ': raise maxMessageBytes in convex()\'s options, or ask for less' : '';
          sync.closed(event.type === 'close' ? `the socket closed (${event.code ?? 1006})${event.reason ? `: ${event.reason}` : ''}` : `${event.message ?? 'the socket failed'}${over}`, refused);
        }
        return snapshot();
      },
    } as RequestInit) as unknown as Promise<ConvexLive>;
  }

  function query<Q extends Reference<'query'>, T = ReturnOf<Q>>(rev: number, ref: Q, args: ArgsOf<Q>, shape?: (value: ReturnOf<Q>) => T): T | Promise<T> {
    const { read, wait, frames } = sync.ask(rev, getFunctionName(ref), args ?? {});
    send(frames);
    const now = read();
    if (now) return settle(now, shape);
    if (sync.status === 'closed' || sync.status === 'failed') throw new ConvexFailure(`Not connected to Convex${sync.error ? `: ${sync.error}` : ''}`);
    return wait.then(() => {
      const then = read();
      if (then) return settle(then, shape);
      throw new ConvexFailure(`Not connected to Convex${sync.error ? `: ${sync.error}` : ''}`);
    });
  }

  function request<T>(kind: 'Mutation' | 'Action', name: string, args: Record<string, unknown>): Promise<T> {
    if (sync.status !== 'open' && sync.status !== 'ready') return Promise.reject(new ConvexFailure(`Not connected to Convex: the ${kind.toLowerCase()} was not sent.`));
    return sync.request(kind, name, args, send).outcome.then(o => settle<T>(o));
  }

  function again<T>(handle: number): Promise<T> {
    // The handle stays good: the mutation is still kept.
    if (sync.status !== 'open' && sync.status !== 'ready') return Promise.reject(new ConvexFailure('Not connected to Convex: the mutation was not sent again.', undefined, handle));
    const sent = sync.again(handle, send);
    if (!sent) return Promise.reject(new ConvexFailure('Nothing to send again: no unanswered mutation has that handle.'));
    return sent.outcome.then(o => settle<T>(o));
  }

  return {
    live, query, again,
    mutation: <M extends Reference<'mutation'>>(ref: M, args: ArgsOf<M>) => request<ReturnOf<M>>('Mutation', getFunctionName(ref), args ?? {}),
    action: <A extends Reference<'action'>>(ref: A, args: ArgsOf<A>) => request<ReturnOf<A>>('Action', getFunctionName(ref), args ?? {}),
    setAuth(token: string | undefined) { sync.setAuth(token, send); },
    inspect: () => sync.inspect(),
  };
}
