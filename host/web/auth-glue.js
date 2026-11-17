// `openAuthSession` on the web (LLP 1069.006 D4): a popup to the
// authorization URL, opened inside the press's own call stack, and the
// app's static callback page (`/.exact/auth/callback`, `auth-callback.js`)
// returning on two channels — `opener.postMessage` and a same-origin
// `BroadcastChannel`, the one that survives a provider's
// Cross-Origin-Opener-Policy. The runner rules (grants, one session,
// supersession) and checks the callback (`exact_runner::auth::accept`); this
// only opens, listens and reports. COOP can leave `closed` true throughout
// sign-in; even returning focus to the app does not prove cancellation.
// Wait for a callback, supersession or the bound. A severed state-less logout
// return carries the identity seeded
// in its own popup; no tab chooses another tab's request. An
// unaddressed logout ends at its bound. No redirect fallback: a blocked popup
// is 428.
// Loaded after first paint when the app grants `auth.session`.
const BOUND_MS = 10 * 60 * 1000; // no callback within this: cancelled (D4 item 4)
const POLL_MS = 250;
let live = null; // this tab's one armed session
const LOGOUT_IDENTITY = 'exact.auth.return';
const randomHex = length => Array.from(crypto.getRandomValues(new Uint8Array(length)), b => b.toString(16).padStart(2, '0')).join('');
const popupName = () => 'exact-auth-' + randomHex(12);
const reservations = () => ((globalThis.exact ??= {}).authReservations ??= new Map());

// Reserve the browsing context synchronously at the press. The later auth
// request must carry this id before it may adopt the window.
export function prepare() {
  const active = navigator.userActivation ? navigator.userActivation.isActive : true;
  if (!active) return null;
  const id = popupName(), popup = window.open('about:blank', id, 'popup,width=520,height=680');
  if (!popup) return null;
  const reservation = { window: popup, timer: null };
  reservations().set(id, reservation);
  reservation.timer = setTimeout(() => cancelPrepared(id), BOUND_MS);
  reservation.timer?.unref?.();
  return id;
}

export function cancelPrepared(id) {
  const reserved = reservations().get(id);
  if (!reserved) return;
  reservations().delete(id);
  clearTimeout(reserved.timer);
  try { reserved.window.close(); } catch {}
}

function takePrepared(id) {
  const reserved = reservations().get(id);
  if (!reserved || reserved.window.closed) { cancelPrepared(id); return null; }
  reservations().delete(id);
  clearTimeout(reserved.timer);
  return reserved.window;
}

const stateLessLogout = url => {
  try {
    const query = new URL(url).searchParams;
    return query.getAll('state').length === 0 && !query.has('code') && !query.has('error');
  } catch { return false; }
};

const logoutReturn = (url, state) => {
  try {
    const query = new URL(url).searchParams, states = query.getAll('state');
    return !query.has('code') && !query.has('error')
      && (states.length === 0 || states.length === 1 && states[0] === state);
  } catch { return false; }
};

function finish(env, report, session = live) {
  if (!session || live !== session) return;
  const { ticket, popup, channel, onMessage, poll, bound } = session;
  live = null;
  clearInterval(poll); clearTimeout(bound);
  removeEventListener('message', onMessage);
  channel?.close();
  if (report) { env.call({ op: 'done', ticket, ...report }); env.deliver(ticket); }
  else { try { popup?.close(); } catch {} } // superseded: the runner let go
}

/** Arm an `{"op":"auth","ticket"}` batch op, synchronously. `env`: `call`
 * (the runner's word, `exact_auth`), `deliver(ticket)` (fulfil what the
 * runner settled), `active(ticket)`, `agent`, `log`. */
export function arm(op, env) {
  // A window the provider's button opened at the press waits for this request (LLP 1133.014 D5): navigating a
  // window the page holds needs no activation.
  const waiting = globalThis.exact?.authWaiting;
  const available = waiting && !waiting.claimed && !waiting.window.closed ? waiting : null;
  const reserved = [...(globalThis.exact?.authReservations?.values() ?? [])].some(entry => !entry.window.closed);
  const activated = navigator.userActivation ? navigator.userActivation.isActive : true;
  const fixture = env.agent ? new URLSearchParams(location.search ?? '').get('authFixture') : null;
  const reply = env.call({ op: 'arm', ticket: op.ticket, agent: env.agent, origin: location.origin,
    popup: activated || !!available || reserved, ...(fixture ? { fixture } : {}) });
  if (reply.error) { env.log(`auth ${op.ticket}: ${reply.error}`); return; }
  if (reply.settled) { env.deliver(op.ticket); return; }
  if (!reply.present) return; // held for the agent (D7)
  const wanted = reply.present.prepared ?? null;
  let adopted = wanted ? takePrepared(wanted) : null;
  if (!wanted && !reply.present.logout && available) {
    available.claimed = true;
    globalThis.exact.authWaiting = null;
    adopted = available.window;
  }
  if (!adopted && !activated) {
    env.call({ op: 'done', ticket: op.ticket, status: 428, message: 'popup blocked' });
    env.deliver(op.ticket);
    return;
  }
  // A new session supersedes this window's live one (the runner already let
  // its ticket go, or refused this one with 409).
  if (live) finish(env, null, live);
  const name = popupName();
  let popup = adopted ?? window.open(reply.present.logout ? 'about:blank' : reply.present.url,
    name, 'popup,width=520,height=680');
  if (!popup) {
    env.call({ op: 'done', ticket: op.ticket, status: 428, message: 'popup blocked' });
    env.deliver(op.ticket);
    return;
  }
  let identity = null;
  if (reply.present.logout) {
    const candidate = randomHex(16);
    // The callback removes this popup-local address. It carries no request,
    // state, URL or token, and no other tab reads it.
    try { popup.sessionStorage.setItem(LOGOUT_IDENTITY, candidate); identity = candidate; } catch {}
  }
  if (adopted || reply.present.logout) {
    try { popup.location.href = reply.present.url; }
    catch { try { popup.close(); } catch {} popup = null; }
  }
  if (!popup) {
    env.call({ op: 'done', ticket: op.ticket, status: 428, message: 'popup blocked' });
    env.deliver(op.ticket);
    return;
  }
  const want = new URL(reply.present.callback);
  const matches = url => {
    try { const u = new URL(url); return u.origin === want.origin && u.pathname === want.pathname; } catch { return false; }
  };
  const armed = performance.now();
  const session = { ticket: op.ticket, popup, channel: null, onMessage: null, poll: null, bound: null,
    identity };
  live = session;
  // Every same-origin tab hears the channel: a callback there is this tab's only when its one `state` is this
  // session's. One from this tab's own popup is, whatever it carries, and the runner checks it.
  const ours = url => {
    try {
      const states = new URL(url).searchParams.getAll('state');
      return states.length === 1 && states[0] === reply.present.state
        && (!reply.present.logout || logoutReturn(url, reply.present.state));
    } catch { return false; }
  };
  // A runner that let go is silent. The bound is elapsed time, which a system
  // clock change cannot move; its timer firing is itself sufficient. A request
  // still held reports 499 once, whichever message, poll or timer observes it.
  const ended = (timerExpired = false) => {
    if (live !== session) return true;
    if (!env.active(op.ticket)) { finish(env, null, session); return true; }
    if (timerExpired || performance.now() - armed >= BOUND_MS) {
      finish(env, { status: 499, message: 'cancelled' }, session);
      return true;
    }
    return false;
  };
  const accept = (url, via) => {
    if (live !== session || !matches(url)) return;
    if (ended()) return;
    env.log(`auth ${op.ticket}: a callback arrived (${via})`);
    finish(env, { url }, session);
  };
  const onMessage = event => {
    // The opener path: the app's own origin, from the popup it owns.
    if (event.origin !== location.origin || event.data?.type !== 'exact-auth') return;
    if (event.source !== popup) return;
    const url = String(event.data.url);
    if (!matches(url) || ended()) return;
    if (reply.present.logout) {
      if (!logoutReturn(url, reply.present.state)) return;
    }
    accept(url, 'opener');
  };
  let channel = null;
  try {
    channel = new BroadcastChannel('exact-auth');
    channel.onmessage = event => {
      if (event.data?.type !== 'exact-auth') return;
      const url = String(event.data.url);
      if (!matches(url) || ended()) return;
      if (ours(url)) return accept(url, 'channel');
      // A state-less return is addressed, never selected: only the tab that
      // seeded this popup identity can take its severed broadcast.
      if (reply.present.logout && identity && event.data.identity === identity && stateLessLogout(url)) {
        accept(url, 'channel');
      }
    };
  } catch {}
  addEventListener('message', onMessage);
  const poll = setInterval(() => {
    if (ended()) return;
  }, POLL_MS);
  const bound = setTimeout(() => ended(true), BOUND_MS);
  Object.assign(session, { channel, onMessage, poll, bound });
}

globalThis.exact ??= {};
globalThis.exact.authHost = { arm, prepare, cancelPrepared };
