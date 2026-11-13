// Resources and mutations (LLP 1016): a resource's value, the arguments it settled with and its one ticket in flight;
// a mutation's slot and its newest-winning, queued or parallel tickets; each request's reply lands in its own commit.
import { sendRequest } from './request-send.js';
import {
  conforms,
  eq,
  equal,
  unchangedArguments,
  failureCode,
  failureReason,
  loneSurrogate,
  Refusal,
  Failed,
  checkArguments,
  finiteArguments,
} from './shape.js';
import * as Ov from './overlay.js';
import {
  sig,
  memo,
  write,
  onEnd,
  commit,
  recommit,
  say,
  W,
  R,
  data,
  Store,
  inflight,
  Inspections,
  DEVELOPMENT_STORE,
  Out,
  Landed,
  Sends,
  Writes,
  Refused,
  Restoring,
  Sched,
  Origin,
} from './rt.js';
import { checkpoint, READER, keptAnswer } from './mount.js';
/** Resource instances, including list members; list aggregates have no entry. */
export const Resources = [];
export let Ticket = 0;
export const ticket = () => Ticket,
  nextTicket = () => ++Ticket;
/** What its resource reads (a watched topic, ts-data.js `changed`, LLP 1016.002 D4; the store, `lateReaders`) changed
 * while ticket `t` was in flight (`t.again`): its reply landed or failed, so the resource is asked again. */
const again = t => {
    if (t.again) {
      t.again = false;
      say(`${t.r.name}: asked again, what it reads changed while ticket ${t.id} was in flight`);
      R(t.r);
    }
  },
  openEnded = t => !!(t.stream || t.req?.stream || t.req?.longPoll || t.call?.held > 0),
  reads = r => r.store || !!r.ticket?.call?.seen?.read,
  // A store write outside a commit asks every store reader again (one whose answer has read the store so far, landed
  // or not), a failed one too, as the runner's commit after a module turn does (LLP 1146 D2): one in flight once its
  // reply lands, marked before that commit's checkpoint so a refusal keeps the mark, and one whose request may never
  // land (a stream, a long poll, a held fetch) now.
  lateReaders = () => {
    for (const r of Resources) if (reads(r) && r.ticket && !openEnded(r.ticket)) r.ticket.again = true;
  },
  lateIdle = () => {
    for (const r of Resources) if (reads(r) && (!r.ticket || openEnded(r.ticket))) R(r);
  };
export { lateReaders, lateIdle };
const sameReq = (a, b) =>
  a &&
  b &&
  a.storage === b.storage &&
  a.method === b.method &&
  a.url === b.url &&
  a.body === b.body &&
  JSON.stringify(a.headers) === JSON.stringify(b.headers) &&
  a.http === b.http &&
  a.cache === b.cache &&
  JSON.stringify(a.longPoll) === JSON.stringify(b.longPoll);
const send = (t, land) => sendRequest(t, land, Out, data, inflight, say);

function ask(source, args, name, mutation, forced) {
  checkArguments(name, args);
  if (mutation) mutation.requests++;
  if (['startActivity', 'updateActivity', 'endActivity'].includes(source))
    throw new Refusal(`${source} is not linked into this artifact`);
  let a;
  try {
    a = data.reserved?.[source]
      ? (v => (v?.then ? v : { v }))(data.reserved[source](source, args, name, forced))
      : data.answer(source, args, Store, name, !!mutation, forced);
  } catch (e) {
    // what a resource's source sent before it threw goes if the commit stands (ts-data.js)
    Out.push(...(data.thrown?.() ?? []));
    throw e;
  }
  // (a reaped build's lazy chunk fails here: the page loads again, generation-reload.js)
  return a && a.then
    ? (a.then(undefined, e => globalThis.exact?.generationGone?.(e)), { promise: a, call: a.call })
    : a;
}
/** The reply to ticket `t` while its target `held()` it: a commit in which `f` takes the source's answer (a TypeScript
 * promise's value, or the parse of the outcome). Any refusal spends the reply: the ticket goes in a commit of its own,
 * as the runner's `release_failed` (admission.rs), so a derive's refusal cannot leave it pending forever. */
function reply(t, name, source, held, f, next, gone) {
  t.held = held;
  // what the answer's module calls read (ts-data.js `__exact_module`); not `call.held`, held.js's count of its held
  // fetches
  if (t.call) t.call.wanted = held;
  return o => {
    if (
      commit(
        () => {
          if (!held()) return say(`dropped reply for ${name}: ticket ${t.id} is no longer held`);
          let p;
          try {
            if (o.error !== undefined) throw new Failed(o.error, o.code, o.reason);
            p = o.v !== undefined ? { v: o.v } : data.parse(source, t.args, o, Store);
          } catch (e) {
            throw e instanceof Failed ? e : new Failed(String(e?.message ?? e), failureCode(e), failureReason(e));
          }
          // a socket frame's replies from its mapper (a Rust parse's, a TypeScript stream's), sent once this commit
          // stands (LLP 1016.000 D7)
          Out.push(...(p.effects ?? []));
          f(p, o);
          Out.push(...(o.effects ?? []));
        },
        `${o.more ? 'message' : 'reply'} ${name}; wall ${t.elapsed} ms`,
      ) !== false ||
      !held()
    )
      return;
    // why, on the line a test failure quotes (sb4 farm wave 6, E3: a mutation's refused reply named nowhere); a
    // development page's console too (LLP 1142 D4)
    const failed = `request ${t.id} (${name}) failed and is no longer pending: ${next}: ${Refused?.message ?? Refused}`;
    say(failed);
    if (DEVELOPMENT_STORE) console.error(`exact: ${failed}`);
    gone(Refused.message, Refused.code, Refused.reason);
    recommit(() => {
      if (t.r) again(t);
    }, 'a failed request');
  };
}
// runner lines.rs
const revalidated = (name, same) =>
  `${name} answered: ${same ? 'equal to its build-time answer' : 'replaces its build-time answer'}`;
/** A resource: its value, the arguments it settled with, one ticket in flight. A bake's answer is a first frame:
 * `baked` asks at launch, as a native runner at `data_ready` (LLP 1048.003 D6; feed F24); a document's `kept` was asked
 * for its page, and is asked again when the page lists it as `open`, or when it read the store and the device's store
 * holds the app's own entries; one that read the store is a store reader's (LLP 1048.000 D6). `carried`: settled, not
 * a bake to ask again — a dev reload's (checkpoint.js), an `else` row's (emit.rs); `kept`: the page's answer it
 * starts from, a list element's as each.js finds it. */
export function res(
  name,
  source,
  args,
  initial,
  initialArgs,
  type,
  ph,
  carried = false,
  kept = keptAnswer(name, source, type),
) {
  const ver = sig(0),
    pend = sig(false),
    fail = sig(null),
    // `current(x)` (LLP 1161.001), its own signal: an answer equal to the placeholder changes no value
    cur = sig(false);
  if (kept) [initialArgs, initial] = kept;
  // `marks`: the uploads it asked, `context`: how many of its last arguments are its `with` context (uploads.js)
  const r = {
    name,
    source,
    type,
    value: initial,
    settled: initialArgs,
    pending: pend,
    baked: kept
      ? READER.has(source) || checkpoint().revalidate || kept[2] || (kept[3] && Store.map.size > 0)
      : !carried && initialArgs !== undefined,
    ticket: null,
    forced: false,
    rev: false,
    store: !!kept?.[3],
    origin: 0,
    marks: [],
    context: args.context ?? 0,
  };
  Inspections.track(r, !kept && !carried, () => pend.n.v);
  const flag = (s, v, undo) => {
      if (!eq(s.n.v, v)) {
        undo?.push([s.n, s.n.v]);
        write(s.n, v);
      }
    },
    release = () => {
      if (r.ticket) {
        say(`forget ticket ${r.ticket.id} (${name})`);
        r.ticket = null;
      }
      flag(pend, false);
    },
    fails = (args, error, code, reason) => {
      r.failed = args;
      r.error = error;
      r.code = code;
      r.reason = reason;
      flag(fail, failure());
      release();
      say(`resource ${name} failed: ${error}`);
      Inspections.fail(r);
      return r.value;
    },
    // `fail` holds the message too: a `failure(x)` reader is asked again when only it changes
    failure = () => (r.failed ? [r.failed, r.code ?? 'error', r.error ?? 'it failed', r.reason ?? ''] : null);
  const hold = () => {
    if (r.value !== undefined) return;
    // a placeholder until an answer lands: what time-to-interactive waits for (marks.js)
    r.placeholder = true;
    const v = typeof ph === 'function' ? ph() : ph;
    if (v === undefined) throw new Failed(`${name} answers later and has nothing to show; give it an \`else\``);
    r.value = v;
    Inspections.placeholder(r);
  };
  // `id`: the call arguments, `with` aside (`current`, LLP 1161.001); `asleep`: a closed `when`, the value kept and no
  // answer, a request let go but an upload's (LLP 1161.002 D2)
  const id = a => a?.slice(0, Math.max(0, a.length - r.context)),
    asleep = () => {
      r.asleep = r.slept = true;
      r.forced = r.rev = r.reconciling = false;
      if (!r.ticket?.upload) release();
      if (r.failed) {
        r.failed = null;
        r.error = r.code = r.reason = undefined;
      }
      flag(fail, failure());
      hold();
      return r.value;
    };
  // `origin`: when it was asked (overlay.js)
  const take = (v, a, origin, generation) => {
    if (type && !conforms(v, type, [0], r.checked))
      throw new Failed(
        loneSurrogate(v)
          ? `${name}: result string contains a lone surrogate`
          : `${name}: the answer does not conform to its shape`,
        'shape',
      );
    r.checked = v;
    r.value = v;
    r.settled = a;
    r.origin = origin;
    r.placeholder = false;
    // a fresh answer: current again after idleness (LLP 1161.001)
    r.slept = false;
    Inspections.take(r, generation);
  };
  const land = t =>
    reply(
      t,
      name,
      source,
      () => r.ticket === t,
      // an upload keeps how it ended (uploads.js)
      ...data.uploads.ends(
        r,
        t,
        (p, o) => {
          if (t.call?.seen?.read || p.store) r.store = t.read = true;
          if (p.req) {
            // A request its reply asks for is the same answer's, asked by the same origin.
            const req = { ...p.req, forced: t.forced, origin: t.req?.origin };
            if (o.more) {
              const n = {
                id: ++Ticket,
                name,
                owner: r,
                args: t.args,
                forced: t.forced,
                req,
                r,
                origin: t.origin,
                generation: t.generation,
              };
              r.ticket = n;
              send(n, land(n));
            } else {
              t.req = req;
              t.id = ++Ticket;
              send(t, land(t));
            }
            return;
          }
          if (t.baked) say(revalidated(name, eq(p.v, r.value)));
          t.baked = false;
          take(p.v, t.args, t.origin, t.generation);
          r.failed = null;
          r.error = r.code = r.reason = undefined;
          if (!o.more) r.ticket = null;
          again(t);
          // an upload is pending until its reply
          W(pend, !!(o.more && t.upload));
          W(fail, null);
          W(ver, ver.n.v + 1);
        },
        'it keeps its last value',
        (error, code, reason) => {
          if (t.call?.seen?.read || t.read || data.threwRead?.()) r.store = true;
          r.ticket = null;
          r.failed = t.args;
          r.error = error;
          r.code = code;
          r.reason = reason;
          write(pend.n, false);
          write(fail.n, failure());
          Inspections.fail(r);
        },
      ),
    );
  const m = memo(() => {
    const shown = Ov.shown(
      r,
      (() => {
        ver();
        // `when` (LLP 1161.002): closed, nothing is asked; waking asks again, forced
        if (args.gate && !args.gate()) return asleep();
        if (r.asleep) {
          r.asleep = false;
          r.forced = true;
        }
        // `asked`: the overlay's arguments while a placeholder stands; a refused commit put `failed` back, and `fail`
        // follows it there too
        const a = (r.asked = args());
        if (Restoring) return (flag(fail, failure()), r.value);
        // `reconciling`: asked after a write ended, a refusal fails it (overlay.js); `mk`: an upload asked at these
        // arguments, not asked again (LLP 1016.000 D10), and none at non-finite ones (settlement.rs `valid_args`)
        const forced = r.forced,
          waited = r.launch,
          rev = r.rev,
          reconciling = r.reconciling,
          mk = finiteArguments(a) ? data.uploads.at(r, a) : null;
        // `launch`: true while the launch's own ask waits for the source, false once a refresh was asked meanwhile
        r.forced = r.rev = r.reconciling = false;
        if (forced) r.launch = undefined;
        // A failure keeps the value for its arguments, including NaN; `refresh` or new ones ask again (settlement.rs),
        // but not an upload's. `fail` follows.
        if (r.failed && ((forced && !mk?.failed) || !unchangedArguments(a, r.failed))) {
          r.failed = null;
          r.error = r.code = r.reason = undefined;
        }
        flag(fail, failure());
        if (r.failed) return r.value;
        try {
          checkArguments(name, a);
        } catch (e) {
          if (!reconciling) throw e;
          return fails(a, e.message, failureCode(e), failureReason(e));
        }
        // an upload answers as it ended, or goes on
        if (mk) return data.uploads.answer(r, mk, a, { take, fails, release });
        const baked = r.baked;
        r.baked = false;
        if (!forced && !rev) {
          // Reuse follows the request in flight; the displayed value may answer older arguments (LLP 1016 D5).
          // `equal`: `-0` is `0`, NaN asks again; a bake is asked once anyway (LLP 1048.003 D6).
          if (!r.ticket && r.settled !== undefined && equal(a, r.settled) && !baked) return r.value;
          if (r.ticket && equal(a, r.ticket.args)) return r.value;
        }
        let ans;
        Inspections.begin(r);
        // the launch's own ask is sent as an ordinary one (LLP 1162.000 D1; `Runner::launch` in the runner)
        const refresh = forced && waited !== true;
        try {
          ans = ask(source, a, name, false, refresh);
          // what its source sent on a socket goes once this commit stands (LLP 1016.000 D7, amended 2026-10-18)
          Out.push(...(ans?.effects ?? []));
        } catch (e) {
          // a failed ask watched what it read, the shown answer's topics kept
          if (data.threwRead?.()) r.store = true;
          for (const t of data.threwWatches?.() ?? []) (r.watches ??= new Set()).add(t);
          if (e instanceof Refusal && !(reconciling && e instanceof Failed)) throw e;
          if (e.refuse && !reconciling) throw new Failed(`resource ${name}: ${e.message}`);
          if (globalThis.__exactRender) hold();
          // its newest ask failed: an older one in flight is let go
          return fails(a, String(e?.message ?? e), failureCode(e), failureReason(e));
        }
        // a fresh answer's topics replace the last's (LLP 1016.002 D2), a later turn's joining them
        if (ans && ans.store) r.store = true;
        r.watches = ans?.watches;
        if (ans && 'v' in ans) {
          if (baked) say(revalidated(name, eq(ans.v, r.value)));
          try {
            take(ans.v, a, Ov.tick());
          } catch (e) {
            if (!reconciling) throw e;
            return fails(a, String(e?.message ?? e), 'shape');
          }
          if (r.ticket) {
            say(`forget ticket ${r.ticket.id} (${name})`);
            r.ticket = null;
          }
          if (!r.ticket) flag(pend, false);
          return r.value;
        }
        // a Rust request, or an upload (LLP 1016.000 D10)
        if (
          ans &&
          !forced &&
          !rev &&
          r.ticket &&
          !equal(a, r.ticket.args) &&
          ((ans.req && r.ticket.req && sameReq(ans.req, r.ticket.req)) ||
            (ans.upload && ans.upload === r.ticket.upload))
        ) {
          say(`keep request ${r.ticket.id} (${name}): the same request for newer arguments`);
          r.ticket.args = a;
          r.ticket.generation = r.inspection?.request;
          return data.uploads.kept(r, a); // what it shows meanwhile; an upload's mark covers these arguments too
        }
        if (ans && (ans.req || ans.promise || ans.stream)) {
          hold();
          const t = {
            id: ++Ticket,
            name,
            owner: r,
            args: a,
            forced: refresh,
            // The origin of the commit that asked it (LLP 1133.036 D6), which the host sends it as.
            req: ans.req && { ...ans.req, forced: refresh, origin: Origin.now() },
            promise: ans.promise,
            call: ans.call,
            stream: ans.stream,
            upload: ans.upload,
            baked,
            r,
            origin: Ov.tick(),
            generation: r.inspection?.request,
          };
          if (t.call) t.call.ticket = t;
          if (baked) say(`${name} shows its build-time answer until its source answers`);
          if (r.ticket) say(`forget ticket ${r.ticket.id} (${name})`);
          r.ticket = t;
          flag(pend, true);
          send(t, land(t));
          return r.value;
        }
        // Not ready (Rust loads after first paint): the bake's answer stands, not pending, asked at `ready` as at
        // data_ready (review B3); another compiled value stands, stale, forced then (LLP 1027 D4). That ask's refusal
        // fails it, not the commit; a commit refused otherwise is followed by one failing it.
        const shown = baked && eq(a, r.settled);
        r.launch = !forced && waited !== false;
        if (!shown) {
          hold();
          flag(pend, true);
        }
        if (!r.waiting) {
          r.waiting = true;
          data.ready(() => {
            r.waiting = false;
            r.forced = r.reconciling = true;
            r.baked ||= shown;
            recommit(() => {
              r.ov = null;
              W(ver, ver.n.v + 1);
              W(pend, false);
            }, `data ready ${name}`);
          });
        }
        return r.value;
      })(),
      r.settled ?? r.asked,
      r.origin,
    );
    flag(
      cur,
      !r.placeholder &&
        !r.slept &&
        r.settled !== undefined &&
        r.asked !== undefined &&
        equal(id(r.settled), id(r.asked)),
    );
    return shown;
  }, type);
  Object.assign(r, {
    save: () => [
      r.value,
      r.settled,
      r.ticket,
      r.ticket?.args,
      r.store,
      r.failed,
      r.baked,
      r.error,
      r.ticket?.again,
      r.code,
      r.origin,
      r.forced,
      r.reconciling,
      r.rev,
      r.reason,
      r.inspection && { ...r.inspection },
      r.ticket?.generation,
      r.marks,
      r.placeholder,
      pend.n.v,
      r.asleep,
      r.slept,
      r.watches,
      r.launch,
    ],
    restore: x => {
      [r.value, r.settled, r.ticket] = x;
      if (r.ticket) {
        r.ticket.args = x[3];
        r.ticket.again = x[8];
        r.ticket.generation = x[16];
      }
      r.store = x[4];
      r.failed = x[5];
      r.baked = x[6];
      r.error = x[7];
      r.code = x[9];
      r.origin = x[10];
      r.forced = x[11];
      r.reconciling = x[12];
      r.rev = x[13];
      r.reason = x[14];
      r.inspection = x[15];
      r.marks = x[17];
      r.placeholder = x[18];
      r.asleep = x[20];
      r.slept = x[21];
      r.watches = x[22];
      r.launch = x[23];
      write(pend.n, x[19]);
      // and what it shows is read again, from what was put back
      write(ver.n, ver.n.v + 1);
    },
    force: undo => {
      r.forced = true;
      r.launch = false;
      flag(ver, ver.n.v + 1, undo);
    },
    touch: undo => flag(ver, ver.n.v + 1, undo), // its writes changed: what it shows is laid over again
    reconcile: undo => {
      r.forced = r.reconciling = true;
      r.launch = false;
      flag(ver, ver.n.v + 1, undo);
    },
    revise: undo => {
      r.rev = true;
      flag(ver, ver.n.v + 1, undo);
    },
    // owed by a refused commit (`commit`)
    giveUp: why => {
      r.baked = r.forced = r.reconciling = r.launch = false;
      fails(r.asked ?? [], why, 'error');
    },
  });
  // its region ended: an open stream closes (`Open`)
  onEnd(() => {
    r.gone = true;
  });
  Resources.push(r);
  m.p = () => (m(), pend());
  m.c = () => (m(), cur());
  m.f = () => (m(), fail() != null);
  // `failure(x)`: `none`, or the `Failure` record `[code, message, reason]`
  m.e = () => {
    m();
    const f = fail();
    return f ? [f[3] || f[1], f[2], f[3]] : null;
  };
  m.n.resource = r;
  m.r = r;
  return m;
}
/** A mutation (LLP 1016): its slot, resources, and newest-winning ticket — or (`mode` 1), a `queue` (LLP 1092), one
 * ticket and the sends waiting their turn (`wait`, schedule.js), each asked at its `next`; or (`mode` 2), `parallel`
 * (LLP 1161.000), a ticket per send in flight (`tickets`), numbered in send order (`send`), the slot and failure
 * following the latest to conclude (`concluded`, runner parallel.rs). */
export const Mutations = [];
// The runner's QUEUE_BOUND: a parallel mutation's 65th send in flight refuses its action.
const BOUND = 64;
export function mut(name, slot, refreshes, type, mode) {
  const pend = sig(false),
    fail = sig(null);
  const m = {
    name,
    requests: 0,
    ticket: null,
    then: null,
    due: Infinity,
    next: Infinity,
    queue: mode === 1,
    parallel: mode === 2,
    tickets: [],
    sends: 0,
    concluded: 0,
    pend,
    refreshes: refreshes.map(x => x.r),
  };
  // an assignment or a newer send forgets nothing (LLP 1092 D4, LLP 1161.000 D4); `out`: `t` is no longer in flight,
  // whether `m` is still pending
  const keeps = () => !!(m.wait || m.parallel),
    out = t => {
      m.tickets = m.tickets.filter(x => x !== t);
      return m.tickets.length > 0 || !!m.wait?.length;
    };
  // a send's reply landed or it failed without one: whether its outcome reaches the slot and the failure, as no later
  // send has concluded (D4)
  const concludes = t => {
    if (!m.parallel) return true;
    if (m.concluded > t.send) return false;
    m.concluded = t.send;
    return true;
  };
  // a write began or ended (overlay.js)
  const shown = (re, undo) => {
    for (const r of m.refreshes) r.touch(undo);
    for (const r of re ?? []) r.reconcile(undo);
  };
  Mutations.push(m);
  slot.n.m = m;
  // An answer now lands before the action's own writes, as the runner's do: an assignment in the same action wins.
  const landWrite = (v, undo) => {
    if (!Writes.some(w => w[0] === slot.n)) {
      slot.n.landing = 1;
      try {
        undo.push([slot.n, slot.n.v]);
        write(slot.n, v);
      } finally {
        slot.n.landing = 0;
      }
    }
    Landed.push(m);
  };
  // A reply the source cannot take ends it unsent: its slot as it was, its `then` unarmed (`reply`).
  const land = t =>
    reply(
      t,
      name,
      t.source,
      () => (m.parallel ? m.tickets.includes(t) : m.ticket === t),
      (p, o) => {
        if (p.req) {
          const req = { ...p.req, origin: t.req?.origin };
          if (o.more) {
            const n = { id: ++Ticket, source: t.source, args: t.args, req, write: t.write, send: t.send };
            if (m.parallel) m.tickets = [...m.tickets.filter(x => x !== t), n];
            else m.ticket = n;
            send(n, land(n));
          } else {
            t.req = req;
            send(t, land(t));
          }
          return;
        }
        if (type && !conforms(p.v, type, [0], m.checked))
          throw new Failed(`${name}: the answer does not conform to its shape`, 'shape');
        m.checked = p.v;
        if (m.parallel) {
          if (!o.more) W(pend, out(t));
        } else {
          if (!o.more) m.ticket = null;
          W(pend, !!m.wait?.length);
        }
        Ov.land(m, p.v, t.write);
        // a reply concluding after a later send's leaves the slot (LLP 1161.000 D4)
        if (concludes(t)) {
          W(fail, null);
          slot.n.landing = 1;
          W(slot, p.v);
        }
        Landed.push(m);
        for (const r of refreshes) R(r.r);
        queueMicrotask(() => {
          slot.n.landing = 0;
        });
      },
      'it ends unsent',
      (error, code, reason) => {
        if (concludes(t)) write(fail.n, [reason || code || 'error', error, reason || '']);
        if (m.parallel) write(pend.n, out(t));
        else {
          m.ticket = null;
          write(pend.n, !!m.wait?.length);
        }
        m.ended(t.write);
      },
    );
  Object.assign(m, {
    // A queue's or a parallel mutation's assignment forgets nothing (LLP 1092 D4, LLP 1161.000 D4).
    forget(undo) {
      undo?.push([fail.n, fail.n.v]);
      write(fail.n, null);
      if (m.ticket && !keeps()) {
        say(`forget ticket ${m.ticket.id} (${name})`);
        m.ticket = null;
        undo.push([pend.n, pend.n.v]);
        write(pend.n, false);
      }
      if (!keeps()) shown(Ov.endPending(m), undo);
    },
    // its send in flight ended without landing: whether a write showed (the caller's commit publishes it)
    ended(write) {
      const re = Ov.end(m, write);
      if (re) shown(re);
      return !!re;
    },
    send(source, args, undo, own) {
      // the 65th in flight refuses (LLP 1161.000 D2); `at`, its place in send order
      if (m.parallel && m.tickets.length >= BOUND) throw new Refusal(`QueueFull { mutation: ${JSON.stringify(name)} }`);
      const at = m.parallel ? ++m.sends : 0;
      // its write shows from here (a queue's next was accepted as it began to wait)
      if (!own) shown(Ov.accept(m, source, args), undo);
      // A queue's send waits unless the mutation is free; `own` is a `next`'s, whose ask refusing drops it
      // (LLP 1092 D3).
      if (m.wait && !own && Sched.hold(m, source, args, undo)) return;
      undo.push([fail.n, fail.n.v]);
      write(fail.n, null);
      let a;
      try {
        a = ask(source, args, name, m);
        if (!a || !('v' in a || a.req || a.promise || a.stream)) throw new Refusal(`${name}: its source is not ready`);
        if ('v' in a && type && !conforms(a.v, type, [0], m.checked))
          throw new Refusal(`${name}: the answer does not conform to its shape`);
      } catch (e) {
        if (e instanceof Refusal || e?.refuse || e?.kind === 'UnknownSource') {
          if (own) Sched.own = e;
          throw e;
        }
        m.forget(undo);
        m.ended(Ov.asked(m));
        undo.push([pend.n, pend.n.v]);
        write(pend.n, m.tickets.length > 0 || !!m.wait?.length);
        concludes({ send: at });
        const reason = failureReason(e);
        undo.push([fail.n, fail.n.v]);
        write(fail.n, [reason || failureCode(e), String(e?.message ?? e), reason]);
        say(`send ${name} failed: ${e?.message ?? e}; it ends unsent`);
        return;
      }
      Out.push(...(a.effects ?? []));
      if ('v' in a) {
        m.checked = a.v;
        Ov.land(m, a.v);
        // answered at once, it replaces one still in flight (newest wins): the older reply is not wanted
        m.forget(undo);
        concludes({ send: at });
        landWrite(a.v, undo);
        // a queue: pending while one waits
        if (m.wait && pend.n.v !== !!m.wait.length) {
          undo.push([pend.n, pend.n.v]);
          write(pend.n, !!m.wait.length);
        }
      } else {
        const t = {
          id: ++Ticket,
          name: m.name,
          owner: m,
          source,
          args,
          req: a.req && { ...a.req, origin: Origin.now() },
          promise: a.promise,
          call: a.call,
          stream: a.stream,
          write: Ov.asked(m),
          send: at,
        };
        if (t.call) t.call.ticket = t;
        if (m.parallel) m.tickets = [...m.tickets, t];
        else m.ticket = t;
        undo.push([pend.n, pend.n.v]);
        write(pend.n, true);
        send(t, land(t));
      }
      // answered at once, it has landed: what it changes is forced (commit.rs `landed_now`)
      if ('v' in a) for (const r of refreshes) R(r.r);
    },
  });
  m.p = () => pend();
  m.f = () => fail() != null;
  m.e = () => fail();
  return m;
}
export function M(m, source, args, own) {
  Sends.push([m, source, args, own]);
}
