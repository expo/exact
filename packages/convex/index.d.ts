// @exact/convex's public API (LLP 1157), for the type checker and for reading:
// `index.ts` implements it, and `test/types.ts`, given to the type checker by a
// case in `test/sync.test.ts`, confirms the two agree. The recipe is
// docs/recipes/convex.md.

/** A Convex function's failure: `message` is Convex's, `data` a `ConvexError`'s payload. */
export declare class ConvexFailure extends Error {
  data: unknown;
  constructor(message: string, data?: unknown);
}

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
   *  it did not run), and when the socket closes first (it may have run, and an expired token's words say why). */
  mutation<M extends Reference<'mutation'>>(ref: M, args: ArgsOf<M>): Promise<ReturnOf<M>>;
  /** An action: its result, as soon as Convex has it; the queries may show what it wrote a moment later. It rejects
   *  as a mutation does. */
  action<A extends Reference<'action'>>(ref: A, args: ArgsOf<A>): Promise<ReturnOf<A>>;
  /** The person's token (a JWT the deployment is configured to accept), or `undefined` signed out. Call it in a
   *  mutation's source; in an `async` one it may follow an `await`. From a plain `.then` callback its send throws on the web.
   *  It sends on the socket, so the same turn may not `fetch`: the runner refuses that mutation and sends nothing. Await
   *  the fetch first, then call it. Until Convex confirms the token, no answer computed for another is given, and each
   *  mutation and action is sent behind the token again, so none runs as the person before; a call whose frame never
   *  left may be made again. Convex says nothing when a token expires on a quiet socket: renew it before it expires. */
  setAuth(token: string | undefined): void;
  /** The queries Convex is asked for and the requests in flight, for a test or a log. */
  inspect(): { queries: { id: number; udfPath: string; sent: boolean; answered: boolean }[]; requests: number };
}

/** A client for the deployment at `address` (its `CONVEX_URL`, or the same origin as a socket's, `wss:` or `ws:`), over
 *  `functions`, Convex's own. */
export declare function convex(address: string, functions: ConvexFunctions, options?: ConvexOptions): ConvexClient;
