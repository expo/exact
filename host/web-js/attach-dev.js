// The tab's half of agent attach (LLP 1012.000 D2, the web row), which the dev loop serves at /__dev/attach.js and
// its page client loads at ⌥⇧A or at a frame of its stream: the pairing code, the person's Allow, the indicator and
// the reads. The page's clock, animations, storage and journal are its own throughout: the reads are agent.js's
// passive ones (`exact.attachReads`), which a development build carries and a release does not.
const TEXT = {
  pairing: 'Agent Attach is on. Give the agent this code: it works once, for 2 minutes.',
  ask: 'An agent asks to read this tab: its views, its state (password fields masked), its log and its layout. They can hold private information.',
  attached: 'Agent Attached',
};
const STYLE = `:host{all:initial}
.box{position:fixed;top:12px;right:12px;z-index:2147483646;max-width:320px;padding:10px 12px;border-radius:10px;
  font:13px/1.35 system-ui,-apple-system,sans-serif;color-scheme:light dark;background:Canvas;color:CanvasText;
  border:1px solid color-mix(in srgb,CanvasText 18%,transparent);box-shadow:0 4px 18px rgb(0 0 0/.18)}
.code{display:block;margin:6px 0;font:600 15px/1.3 ui-monospace,monospace;letter-spacing:.04em;user-select:all}
.row{display:flex;gap:8px;justify-content:flex-end;margin-top:8px}
.attached{display:flex;align-items:center;gap:10px;padding:6px 10px;border-radius:999px}
.attached .row{margin:0}
button{font:inherit}`;

export function install({ fetch, doc }) {
  // `era` counts the ends (Detach, a new stream): work begun before one shows and answers nothing.
  let host = null, conn = null, allowed = null, reads = null, expiry = null, era = 0, before = null, enabling = false;
  const post = (route, body) => fetch(`/__dev/attach/${route}`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ doc: doc(), ...body }) });
  const clear = () => {
    clearTimeout(expiry);
    // Focus the ask took goes back where it was.
    if (host && before?.isConnected && host.contains(document.activeElement)) before.focus();
    host?.remove(); host = null; before = null;
  };
  const show = (kind, build) => {
    clear();
    host = document.createElement('exact-agent-attach');
    const root = host.attachShadow({ mode: 'open' }), box = document.createElement('div');
    root.append(Object.assign(document.createElement('style'), { textContent: STYLE }), box);
    box.className = `box ${kind}`;
    box.setAttribute('role', kind === 'ask' ? 'alertdialog' : 'status');
    box.setAttribute('aria-label', kind === 'attached' ? TEXT.attached : 'Agent Attach');
    build(box);
    document.documentElement.append(host);
  };
  const button = (label, act) => {
    const b = Object.assign(document.createElement('button'), { type: 'button', textContent: label });
    // A person's press only: a script's `click()` on the page is not one.
    b.addEventListener('click', (e) => { if (e.isTrusted) act(); });
    return b;
  };
  const row = (...buttons) => { const r = document.createElement('div'); r.className = 'row'; r.append(...buttons); return r; };
  const text = (t) => document.createTextNode(t);
  const end = () => { era++; conn = allowed = null; clear(); };
  const detach = () => { end(); post('detach', {}).catch(() => {}); };

  async function enable() {
    const mine = era, r = await post('enable', { driven: !!globalThis.exact?.clock?.agent });
    const body = await r.json().catch(() => ({}));
    if (mine !== era) return;
    if (!r.ok) { console.info(`exact: Agent Attach: ${body.error ?? r.status}`); return; }
    show('pairing', (box) => {
      box.append(text(TEXT.pairing), Object.assign(document.createElement('span'), { className: 'code', textContent: body.code }), row(button('Turn Off', detach)));
    });
    expiry = setTimeout(() => { if (!conn) { detach(); console.info('exact: the Agent Attach code expired; ⌥⇧A shows a new one'); } }, body.ms);
    console.info(`exact: Agent Attach on: give the agent the code shown in the tab (bun exact.mjs agent web --attach … reads it from stdin)`);
  }

  function ask(id) {
    conn = id;
    const answer = async (allow) => {
      if (conn !== id) return;
      const mine = era;
      // Reads are answered from the Allow on, as the loop may send one before its answer to the POST arrives; the
      // indicator shows once the loop took the Allow for this connection.
      if (allow) allowed = id; else { conn = null; clear(); }
      const r = await post('answer', { conn: id, allow, title: document.title, location: location.pathname + location.search + location.hash, planDigest: globalThis.exact?.plan ?? null }).catch(() => null);
      if (allow && mine === era && conn === id) { if (r?.ok) attached(); else end(); }
    };
    show('ask', (box) => {
      box.append(text(TEXT.ask), row(button('Don’t Allow', () => answer(false)), button('Allow', () => answer(true))));
      box.addEventListener('keydown', (e) => { if (e.key === 'Escape' && e.isTrusted) answer(false); });
    });
    // Focus on the ask, on neither button, so Escape answers it and Return allows nothing.
    before = document.activeElement;
    const box = host.shadowRoot.querySelector('.box');
    box.tabIndex = -1;
    box.focus();
  }
  const attached = () => show('attached', (box) => { box.append(text(TEXT.attached), row(button('Detach', detach))); });

  async function read(m) {
    if (m.conn !== allowed) return;
    const mine = era;
    let reply;
    try {
      if (!globalThis.exact?.attachReads) throw new Error('unsupported: this page carries no passive reads: reload it on this dev loop');
      reads ??= globalThis.exact.attachReads().catch((e) => { reads = null; throw e; });
      reply = await (await reads)(m.read);
      // The page's own clock as the read found it, and the wall clock's (LLP 1012.000 D3).
      if (m.read.op === 'state' && !reply.error) reply = { ...reply, observation: { attached: true, clock: globalThis.exact.clock.now, wall: Date.now() } };
    } catch (e) { reply = { error: String(e?.message ?? e) }; }
    // The loop refuses a reply past 32 MiB unread: one it would refuse is said in a few bytes instead.
    if (new Blob([JSON.stringify(reply)]).size > (32 << 20) - 4096) reply = { error: 'the reply is over 32 MiB', refused: 'reply-size' };
    if (mine === era && m.conn === allowed) await post('reply', { conn: m.conn, request: m.request, reply }).catch(() => {});
  }

  return {
    /** ⌥⇧A: on, with a code to give the agent; again, off, ending any attachment. */
    toggle: () => {
      if (host || conn) return detach();
      if (enabling) return;
      enabling = true;
      enable().catch((e) => console.info(`exact: Agent Attach: ${e.message}`)).finally(() => { enabling = false; });
    },
    frame(m) {
      if (m.kind === 'attach-ask') ask(m.conn);
      else if (m.kind === 'attach-request') read(m);
      else if (m.kind === 'attach-end' && m.conn === conn) { end(); console.info(`exact: agent attach ended: ${m.why}`); }
    },
    /** The page's stream is a new one: whatever the old one paired or attached has ended. */
    reset: end,
  };
}
