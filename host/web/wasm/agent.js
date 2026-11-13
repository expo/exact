import { views, ask, shownValue, host, INHERITED_CSS, now, root, iframeLoading, waiting, httpHelpers, loadGpuIfNeeded, heldNamed, loadAfterPaint, stageLoaded, loadStage, agentMode, gpuInPlay, register, seek, postSurface, deferFulfill, documentsGlue, picker, t0, log, readOut, presence, forgettable, writeIn, pageFacts, pageChanged, foldChanged, followOffset, pieces, moduleReady, SETTLE_DEADLINE_MS, mediaAttached, GLIDE_BOUND_MS, collections, settleCandidate } from "../glue.js";
import { inertAncestor, routeFlags, navigation, environment, preferFold, foldEnv, typedControl, settleValue } from "../navigation.js";
import { applyBatch, sendBatch } from "./commit.js";
function nodeDetail(id, plan = false) {
  const el = views.get(id);
  if (!el || !el.isConnected) {
    return { error: `stale node #${id}` };
  }
  const node = ask({
    op: "node",
    id,
    ...plan ? { plan: true } : {}
  });
  if (node.error) {
    return node;
  }
  Object.assign(node, globalThis.exact.heldOf?.(el, ask));
  // what is held on it, and its rows with their tokens (LLP 1122 D8)
  if (node.type === "TextInput" && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) {
    node.props = {
      ...node.props,
      value: shownValue(el)
    };
  }
  delete node.frame;
  delete node.absolute;
  delete node.content;
  const r2 = (x) => Math.round(x * 100) / 100;
  const rect = (r) => ({
    x: r2(r.x),
    y: r2(r.y),
    w: r2(r.width),
    h: r2(r.height)
  });
  const idOf = (e) => {
    for (const [i, v] of views) {
      if (v === e) {
        return i;
      }
    }
    return null;
  };
  const r = exact.gpu?.placementHidden(el) ? new DOMRect() : host.viewBox(el);
  node.space = {
    viewport: rect(r),
    local: {
      w: r2(el.clientWidth),
      h: r2(el.clientHeight)
    },
    capture: { scale: devicePixelRatio }
  };
  const scroll = [], clip = [];
  let clipped = r.width === 0 || r.height === 0;
  for (let a = el.parentElement; a; a = a.parentElement) {
    const aid = idOf(a);
    if (aid == null) {
      continue;
    }
    const cs = getComputedStyle(a);
    if (a.dataset.scroll === "true") {
      scroll.unshift({
        id: aid,
        sx: r2(a.scrollLeft),
        sy: r2(a.scrollTop),
        ...host.scrollRange(a)
      });
    }
    const clips = (cs.overflowX !== "visible" || cs.overflowY !== "visible" ? ["overflow"] : []).concat(cs.clipPath !== "none" ? ["clip-path"] : []);
    for (const kind of clips) {
      clip.unshift({
        id: aid,
        kind
      });
      const c = a.getBoundingClientRect();
      if (r.right <= c.left || r.left >= c.right || r.bottom <= c.top || r.top >= c.bottom) {
        clipped = true;
      }
    }
  }
  const page = host.scrollRange(document.scrollingElement ?? document.documentElement);
  if (scrollX || scrollY || page.rx || page.ry) {
    scroll.unshift({
      viewport: true,
      sx: r2(scrollX),
      sy: r2(scrollY),
      ...page
    });
  }
  // the page's entry while it scrolls or has a range
  node.scroll = scroll;
  node.clip = clip;
  node.visible = {
    hidden: el.checkVisibility ? !el.checkVisibility({ visibilityProperty: true }) : false,
    inert: !!inertAncestor(el),
    inViewport: r.right > 0 && r.bottom > 0 && r.left < innerWidth && r.top < innerHeight,
    clipped
  };
  node.native = { element: el.localName };
  if (el.hasAttribute("data-symbol-source")) {
    const source = el.dataset.symbolSource, name = source.slice(source.startsWith("symbol:sf/") ? 10 : 7), found = !!el.dataset.symbolPath || el.hasAttribute("data-symbol-file");
    node.native.symbol = {
      source,
      name,
      found,
      ...!found ? { reason: source === "symbol:sf/" || source === "symbol:" ? "empty" : source.startsWith("symbol:sf/") ? "platform" : "role" } : {}
    };
  }
  const cs = getComputedStyle(el);
  node.browser = Object.fromEntries(Object.entries(INHERITED_CSS).map(([row, prop]) => [row, cs.getPropertyValue(prop)]));
  const pressed = globalThis.exact?.pressShown?.(el);
  if (pressed) {
    node.press_shown = pressed;
  }
  // what a pressed paint shows (input-glue.js)
  const flow = host.textflow?.facts(id);
  if (flow) {
    node.flow = flow;
    node.flow_shapes = flow.shapes;
  }
  if (el.getClientRects().length > 1) {
    node.column_fragments = [...el.getClientRects()].map(rect);
  }
  // a box a multi-column flow fragments, one rect per column (LLP 1093 D12)
  node.observed = {
    clock: now(),
    wall: Date.now()
  };
  return node;
}
function tree(request) {
  const flag = routeFlags(root), flags = new Map();
  const activity = node => {
    const el = views.get(node.id), up = flags.get(node.parent);
    const f = el?.isConnected ? Math.max(flag(el), up ?? 0) : up ?? (node.hidden ? 2 : node.inactive ? 1 : 0);
    flags.set(node.id, f);
    return f;
  };
  // Read logical ancestors for targeted reads: flowed runs can be detached from the DOM.
  // The browser knows which split lists remain live beside their detail (LLP 1153 D8).
  if (request.target != null) {
    const nodes = ask({ op: "tree" }).nodes ?? [];
    for (const node of nodes) activity(node);
    if (typeof request.target === "string") {
      const matches = nodes.filter(node => node.props?.testId === request.target);
      const target = matches.find(node => !flags.get(node.id)) ?? matches[0];
      if (target) {
        request = { ...request, target: target.id };
      }
    }
  }
  const reply = ask(request);
  // Detached runs inherit their logical parent's flags.
  for (const node of reply.nodes ?? []) {
    const el = views.get(node.id), f = activity(node);
    delete node.inactive;
    delete node.hidden;
    if (f) {
      node.inactive = true;
    }
    if (f > 1) {
      node.hidden = true;
    }
    if (node.type === "TextInput" && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) {
      node.props = {
        ...node.props,
        value: shownValue(el)
      };
    }
    // The projection opt-out is a written style, not a prop or a browser default.
    if (el?.dataset?.exactSaid?.split(' ').includes('appearance')) {
      node.style = { appearance: { value: getComputedStyle(el).appearance, source: 'authored' } };
    }
    node.focused = el === document.activeElement;
    if (el?.exactNative) {
      node.module = el.exactNative.status();
    }
    if (!(el instanceof HTMLIFrameElement)) {
      continue;
    }
    node.url = el.getAttribute("src") ?? "";
    node.loading = iframeLoading.get(el) !== false;
    if (el.$partition != null) {
      node.partition = el.$partition;
    }
    const nav = el.exactNavigation?.last;
    if (nav) {
      node.navigation = {
        url: nav.url,
        title: nav.title,
        loading: nav.loading,
        controller: nav.controller,
        canGoBack: nav.canGoBack,
        canGoForward: nav.canGoForward
      };
    }
    // LLP 1020.002 §10
    const guest = host.guestOutline(el);
    if (guest !== null) {
      node.guest = guest;
    }
  }
  return reply;
}
export async function waitForInflight(deadline, includeRequests = false) {
  while (waiting().length || includeRequests && host.wasm.exact_requests_pending()) {
    const remaining = deadline - performance.now();
    if (remaining <= 0) {
      return false;
    }
    if (waiting().length) {
      let timer;
      const helpers = await Promise.race([httpHelpers(), new Promise((resolve) => {
        timer = setTimeout(() => resolve(null), remaining);
      })]);
      clearTimeout(timer);
      if (!helpers || !await helpers.waitForInflight(waiting, deadline)) {
        return false;
      }
    } else {
      await new Promise((resolve) => setTimeout(resolve, Math.min(50, remaining)));
    }
  }
  return true;
}
async function settleGpu() {
  loadGpuIfNeeded();
  await host.gpuLoading;
  return await globalThis.exact.gpu?.settled() ?? [];
}
function gpuPendingReply(request, pending) {
  const names = pending.map((item) => item.name ?? "GPU work");
  return request.op === "clock" ? heldNamed({
    clock: host.agentClock,
    settled: false,
    reason: "gpu",
    pending: names
  }) : {
    error: `GPU is not settled: ${names.join(", ")}`,
    pending: names
  };
}
export function agentInputReady() {
  if (host.viewBox) {
    return Promise.resolve();
  }
  return host.agentInputLoading ??= loadAfterPaint("./agent-input.js", "agentInput").then((helpers) => {
    ({guestOutline: host.guestOutline, guestTap: host.guestTap, guestType: host.guestType, reveal: host.reveal, backButton: host.backButton, viewBox: host.viewBox, layoutBoxes: host.layoutBoxes, scrollRange: host.scrollRange, selectAll: host.selectAll, typeControl: host.typeControl, glidesSettled: host.glidesSettled, scrollsSeen: host.scrollsSeen} = helpers);
    helpers.watchGlides?.();
    // a scroller gliding on its own holds `clock settle` (agent-input.js)
  });
}
// Synchronous once inspection and the agent's input helpers are ready (LLP 1043.000 D7/D8).
export function agent(request) {
  if (!stageLoaded("inspection")) {
    return loadStage("inspection").then(() => agent(request));
  }
  if (!host.viewBox) {
    return agentInputReady().then(() => agent(request));
  }
  return agentMode && gpuInPlay() ? settleGpu().then((pending) => pending.length ? gpuPendingReply(request, pending) : agentNow(request)) : agentNow(request);
}
function agentNow(request) {
  const r = agentReply(request), decorate = globalThis.exact.gpu?.decorate;
  return decorate ? decorate(request, r) : r;
}
function agentReply(request) {
  try {
    if (!host.wasm) {
      return { error: "not booted" };
    }
    if (agentMode && (request.op === "picture" || request.op === "check" || request.op === "layout" && request.picture)) {
      register(host.agentClock);
      host.followOnSeek = false;
      try {
        seek(host.agentClock);
      } finally {
        host.followOnSeek = true;
      }
    }
    // a picture is the clock's (LLP 1116.003 D8): an animation begun since the last commit (a spinner, LLP 1069.001) is put on it first
    if (request.op === "type" && request.message !== undefined) {
      const gpu = globalThis.exact.gpu, name = request.id !== undefined ? gpu?.surfaceOf(request.id) : request.surface ?? gpu?.liveSurface(), refused = typeof name === "string" && postSurface(name, String(request.message), now());
      return typeof name !== "string" ? { error: name?.error ?? (request.id !== undefined ? `view ${request.id} has no world` : "post: no world is live; name its surface (post <surface> <text>) to hold the message until one is") } : refused ? { error: `post refused: ${refused}` } : tagged({
        posted: name,
        delivery: "posted",
        ...gpu?.waiting(name) ?? true ? { held: true } : {}
      });
    }
    // the driver's `post` (LLP 1139.004 D1)
    if (request.entity !== undefined || request.world === true || request.contact !== undefined) {
      return globalThis.exact.gpu?.handle(request, ask, tagged) ?? { error: `view ${request.id} has no world` };
    }
    if ((request.op === "tap" || request.op === "type") && request.ticket !== undefined) {
      // a held device request, by ticket (LLP 1069.007 D4)
      const { files, ...held } = request, r = ask(held);
      if (r.capability === "auth") {
        deferFulfill(host.incarnation, r.ticket, 9, 0, "", new Uint8Array());
      }
      // a picker's answer is delivered once the runner took it (LLP 1069.002 D9); an auth answer, the runner settled (LLP 1069.006 D7)
      const picked = {
        "open-file": "showOpenFilePicker",
        "open-directory": "showDirectoryPicker",
        "save-file": "showSaveFilePicker"
      }[r.capability];
      if (picked && r.node != null) {
        return documentsGlue().then((m) => m.answer(r, picked, files)).then(() => tagged(r));
      }
      // LLP 1069.010 D2
      if (r.capability === "export" && r.node != null) {
        return picker().then((m) => m.answerSave(r, held.text)).then((out) => tagged({
          ...r,
          ...out
        }));
      }
      // LLP 1069.010 D3: the bytes go back to the driver
      return r.capability === "pick" && r.node != null ? picker().then((m) => m.answer(r.node, r.answered === "cancel" ? null : files ?? [])).then(() => tagged(r)) : tagged(r);
    }
    if (request.op === "perf" && request.frames) {
      return request.live > 0 ? loadAfterPaint("./frames.js", "liveFrames").then((live) => live({
        ms: request.live,
        late: request.late,
        origin: () => t0,
        log,
        clock: () => host.agentClock,
        advance: (to) => applyBatch(JSON.parse(readOut(host.wasm.exact_advance(to, 0)))),
        gpu: globalThis.exact.gpu
      })).then(tagged) : { virtual: true };
    }
    // the agent's clock presents no frame, but lends it to the wall for a live window (LLP 1079 D4)
    switch (request.op) {
      case "state": {
        const st = ask(request);
        if (st.error) {
          return st;
        }
        st.presence = presence.live?.observation() ?? [];
        for (const [c, t] of forgettable) {
          const s = c.retrying && st.streams?.find((s) => s.ticket === t);
          if (s) {
            s.retrying = {
              attempt: c.retrying.attempt,
              inMs: Math.max(0, Math.round(c.retrying.at - performance.now())),
              error: c.retrying.error
            };
          }
        }
        /* a long poll's backoff (LLP 1016.000 D8), which only the page sees */ const faults = globalThis.__exactFaults?.entries;
        if (faults?.length) {
          st.faults = faults;
        }
        // LLP 1103 D3 (faults.js keeps the table there)
        const r2 = (x) => Math.round(x * 100) / 100;
        const idOf = (e) => {
          for (const [i, v] of views) {
            if (v === e) {
              return i;
            }
          }
          return null;
        };
        const active = document.activeElement && document.activeElement !== document.body ? document.activeElement : null;
        const editor = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement || active?.exactMarkup ? idOf(active) : null;
        st.media = [...views].filter(([, el]) => el instanceof HTMLMediaElement).map(([id, el]) => ({
          id,
          state: {
            currentTime: el.currentTime,
            duration: Number.isFinite(el.duration) ? el.duration : null,
            paused: el.paused,
            muted: el.muted,
            volume: el.volume,
            playbackRate: el.playbackRate,
            readyState: el.readyState,
            videoWidth: el.videoWidth,
            videoHeight: el.videoHeight,
            src: el.currentSrc,
            error: el.error ? {
              code: el.error.code,
              message: el.error.message
            } : null,
            renderer: el.constructor.name
          }
        }));
        st.mediaSession = globalThis.exact.mediaSession?.state(idOf) ?? {
          owner: null,
          claimants: [],
          actions: [],
          playbackState: "none",
          published: "none"
        };
        // LLP 1098 D10: none until a media element loads its glue
        st.focus = {
          logical: active ? idOf(active) : null,
          editor,
          responder: active ? active.localName : null,
          pending: null
        };
        const overlap = Math.max(0, innerHeight - (globalThis.visualViewport?.height ?? innerHeight));
        const policy = document.querySelector("[interactiveWidget]")?.getAttribute("interactiveWidget") ?? "resizes-visual";
        st.keyboard = {
          visible: overlap > 0,
          overlap: r2(overlap),
          policy,
          interactive: false
        };
        st.atoms = globalThis.exact.atoms?.state(idOf) ?? [];
        // LLP 1045.000 D9 (atoms.js)
        st.decorations = globalThis.exact.decorations?.state(views) ?? [];
        st.navigation = navigation.observation(root);
        st.window = { title: document.title };
        if (host.page) {
          st.adopted = host.page.adopted === true;
        }
        if (st.sounds && host.soundOut) {
          st.sounds.output = host.soundOut.state();
        }
        if (st.midi && host.midiOut) {
          st.midi.output = host.midiOut.state();
        }
        // LLP 1048.000 D6; LLP 1096 D10 (a page that plays)
        const wake = globalThis.exact.wakeLockGlue;
        if (wake) {
          const by = wake.holders(root).map(idOf);
          st.wakelock = {
            held: by.length > 0,
            by
          };
        }
        const orient = globalThis.exact.orientationLockGlue;
        if (orient) {
          st.orientationlock = orient.state(root, idOf);
        }
        // LLP 1125 D11: what this page shows holds it (a split's list beside its detail, the routes under a sheet), which the runner cannot see; the JS target's agent.js reads the same
        // The drive's app storage (trivia F7): none unless it names a scratch store, as storage-environment.js's `storageKey`.
        const store = new URL(performance.getEntriesByType?.("navigation")[0]?.name ?? location.href).searchParams.get("storage");
        st.storage = store == null ? {
          available: false,
          code: "agent",
          message: "storage is unavailable in agent mode unless the drive names a scratch store (--storage <name>)"
        } : {
          available: true,
          store
        };
        return st;
      }
      case "layout": {
        const nodes = host.layoutBoxes(views, exact.gpu);
        const reply = {
          clock: now(),
          viewport: {
            w: innerWidth,
            h: innerHeight
          },
          env: environment(),
          nodes
        };
        if (request.id != null) {
          const detail = nodeDetail(request.id, request.plan === true);
          if (detail.error) {
            return detail;
          }
          reply.node = detail;
        }
        if (request.agree) {
          return tagged({
            clock: reply.clock,
            viewport: reply.viewport,
            agreement: { unavailable: "not implemented: app drives run the JS target" }
          });
        } else if (request.native && reply.node) {
          delete reply.nodes;
          reply.node.native.subviews = { unavailable: "the DOM is the tree; layout <target> names the element" };
        }
        // @ref LLP 1080.001 D1, D2
        return tagged(reply);
      }
      case "midi":
      case "prefer": {
        if (request.faults) {
          return loadAfterPaint("./faults.js", "faults").then((m) => tagged(m.faultOp(request.faults)));
        }
        /* LLP 1103 D3: a fetch fault, a form of `prefer` */
        if (request.op === "midi" || request.midi) {
          if (!host.wasm.exact_midi) {
            return { error: "midi: this build has no MIDI export" };
          }
          const { batch } = applyBatch(JSON.parse(readOut(host.wasm.exact_midi(writeIn(JSON.stringify(request))))));
          return batch.error != null ? { error: batch.error } : tagged({ midi: true });
        }
        // @ref LLP 1125 §4: the agent's MIDI ports and a message from one are the runner's
        // @ref LLP 1069.000 D6 — the page group; the driver sets media through CDP. @ref LLP 1078 D7 — the fold group: an empty one re-reads the browser (the driver's CDP override), a filled one is the substitute.
        if (Object.entries(request.mode ?? {}).some((p) => p.some((x) => /[,=]/.test(String(x))))) {
          return {
            malformed: true,
            error: "prefer mode: an axis or value holds no `,` or `=`"
          };
        }
        const pins = Object.entries(request.mode ?? {}).map(([a, v]) => `${a}=${v}`).join(","), checked = request.mode ? JSON.parse(readOut(host.wasm.exact_set_mode(writeIn(`?${pins}`)))) : null;
        if (checked?.error) {
          return { error: `prefer mode: ${checked.error}` };
        }
        try {
          if (request.page) {
            pageFacts.prefer(request.page);
          }
          if (request.fold) {
            preferFold(Object.keys(request.fold).length ? request.fold : null);
          }
        } catch (e) {
          return {
            error: e.message,
            ...e.malformed === true ? { malformed: true } : {}
          };
        }
        pageChanged();
        foldChanged();
        if (request.page?.["gpu-tier"]) {
          globalThis.exact.gpu?.retier?.(request.page["gpu-tier"]);
        }
        /* LLP 1139.003 D2: each world's renderer made again at the new tier's rung */ const modes = request.mode ? JSON.parse(readOut(host.wasm.exact_set_mode(writeIn(pins || "?")))) : null;
        if (pins) {
          applyBatch(modes);
        }
        if (modes?.error) {
          return { error: `prefer mode: ${modes.error}` };
        }
        // @ref LLP 1121 D13 — the mode group: checked before any fact is set, the runner's `setMode`s in one commit after the rest
        return tagged({
          page: { ...pageFacts.read() },
          fold: foldEnv(),
          ...modes ? { mode: modes.modes } : {}
        });
      }
      case "focus": {
        const el = views.get(request.id);
        if (!el) {
          return { error: `no view ${request.id}` };
        }
        el.focus();
        // any element, as the JS target's agent: a key at a grid is its keydown (docs/contract-grammar.md#keys)
        const selected = request.select === false ? null : host.selectAll(el);
        return selected ? selected.then(() => ({ ok: true })) : { ok: true };
      }
      case "tap": {
        const i = request.into;
        // @ref LLP 1070.000 §5 — the runner's request, its batch applied now
        if (i) {
          for (const a of [i.block === undefined ? "start" : i.block, i.inline === undefined ? "nearest" : i.inline]) {
            if (![
              "start",
              "center",
              "end",
              "nearest"
            ].includes(a)) {
              return {
                error: `no alignment ${typeof a === "string" ? a : JSON.stringify(a)}: start, center, end or nearest`,
                malformed: true
              };
            }
          }
          const { batch } = sendBatch(host.wasm.exact_into_view(request.id, writeIn(`${i.key ?? ""}\n${i.block ?? "start"}\n${i.inline ?? "nearest"}`)));
          return batch.error != null ? { error: batch.error } : tagged({
            tapped: request.id,
            into: i
          });
        }
        const frame = views.get(request.id);
        if (request.holdState !== undefined) {
          return (async () => {
            for (let i = 0; i < 40 && !globalThis.exact.hold; i++) {
              await new Promise((ok) => setTimeout(ok, 50));
            }
            return globalThis.exact.hold ? ((r) => r.error ? {
              error: r.error,
              ...r.malformed ? { malformed: true } : {}
            } : tagged({
              tapped: request.id,
              holdState: r.held,
              delivery: "recognized"
            }))(globalThis.exact.hold(frame, request.holdState)) : { error: "the input glue is not loaded" };
          })();
        }
        // LLP 1122 D8: `holdState`, once the glue loads (it loads after paint)
        if (request.history !== undefined || request.back) {
          return request.back ? navigation.back(root) : navigation.travel(frame, request.history);
        }
        // the agent's `back` (LLP 1116.006 D4)
        if (frame && (inertAncestor(frame) || ["hidden", "collapse"].includes(getComputedStyle(host.backButton(frame) ?? frame).visibility))) {
          return {
            handled: true,
            error: `view ${request.id} is hidden or inert`
          };
        }
        return frame instanceof HTMLIFrameElement ? host.guestTap(frame, request) : { guest: false };
      }
      case "type": {
        const frame = views.get(request.id);
        if (frame?.hasAttribute("navigationBack") && request.key == null) {
          const batch = globalThis.exact.navigate(request.text ?? "");
          return {
            typed: request.id,
            delivery: "recognized",
            handled: true,
            ...batch.error ? { error: batch.error } : {}
          };
        }
        return typedControl(frame) && request.key == null ? host.typeControl(frame, request, inertAncestor, settleValue) : frame instanceof HTMLIFrameElement ? host.guestType(frame, request) : { guest: false };
      }
      case "clock": // then the offset at the new virtual date, in case it crossed a DST change (LLP 1069.007 D2)
      return clock(request).then((r) => {
        if (!r.error) {
          followOffset(host.agentClock);
        }
        return tagged(heldNamed(r));
      });
      // naming what the server holds (LLP 1016.000 D9)
      case "tree": return tree(request);
      case "tags": return ask(request);
      case "reveal": return tagged(host.reveal(views.get(request.id), request.id, inertAncestor));
      // before a tap or a type (agent-input.js)
      case "axStamp":
        // @ref LLP 1080.002 D4 — each live view's id where CDP's DOM snapshot reads it, and the document's nonce
        for (const [id, el] of views) {
          if (el.isConnected && el.getAttribute("data-agent-view") !== String(id)) {
            el.setAttribute("data-agent-view", id);
          }
        }
        return tagged({ nonce: performance.timeOrigin });
      default: return ask(request);
    }
  } catch (e) {
    return {
      error: String(e),
      ...e.malformed === true ? { malformed: true } : {}
    };
  }
}
// @ref LLP 1043.000 §3 D7/D8 — reads keep the last settled facts (LLP 1012).
// Await flow only when requested; ordinary agent calls retain their return types.
export async function agentSettled(request) {
  const begun = performance.now(), pieceLoad = pieces.pending();
  if (pieceLoad) {
    await pieceLoad;
  }
  if (presence.pending() && !await presence.landed(Math.max(0, 1e4 - (performance.now() - begun)))) {
    return { error: "the presence module did not load within 10 s" };
  }
  /* inside an agent call's 15 s carrier deadline */
  if (host.markupModule) {
    await host.markupModule;
  }
  if (host.decorationsModule) {
    await host.decorationsModule;
  }
  if (host.liveTimeModule) {
    await host.liveTimeModule;
  }
  if (host.flowLoading) {
    await host.flowLoading;
  }
  if (host.textflow) {
    await host.textflow.settle();
  }
  return agent(request);
}
// Every reply carries the runner's `epoch`, `incarnation` and `clock` (LLP
// 1035.002 D3), read after the operation; a reply's own `clock` (where a
// `clock` call landed) is kept, and an error is left alone. The driver
// tags the input replies it delivers through CDP the same way.
function tagged(reply) {
  if (!reply || reply.error != null) {
    return reply;
  }
  const tags = ask({ op: "tags" });
  if (tags.error != null) {
    return reply;
  }
  for (const key of Object.keys(tags)) {
    if (reply[key] === undefined) {
      reply[key] = tags[key];
    }
  }
  return reply;
}
// To `to`, or to `settle`: a fixed point — advance to when the last thing
// in flight ends, and if the timers crossed on the way started more, again
// (bounded; `settled: false` at the bound). A request in flight (LLP 1016)
// is waited for first: its reply commits, and may start motion or ask for
// more, before the fixed point is measured. What is in flight lands before a
// timer fires — the runner keeps one request per target (LLP 1016 D5), so a
// tick's send would drop the reply of the one before it: a jump that crosses
// timers stops after each timer that sends, and its reply is waited for; past
// the deadline, or 4096 stops, the rest is one advance. The clock lands where
// the runner says; a timer's refusal is the error. A promise: the driver
// awaits it. The agent's reply names the requests the server holds, which no clock waits on (`heldNamed`, LLP 1016.000 D9).
async function clock(request) {
  // The end of an input (LLP 1012 §2): the `then`s of the answers it settled
  // land, the clock unmoved and no timer fired (Runner::land_then). `clock data` (web-js agent.js) first
  // waits for activation and every request in flight, landing until a landing sends nothing (a test's first step).
  if (request.land || request.data) {
    if (request.data) {
      await moduleReady;
    }
    const deadline = performance.now() + SETTLE_DEADLINE_MS, unsettled = {
      clock: host.agentClock,
      settled: false,
      reason: "requests"
    };
    for (let round = 0; round < 16; round++) {
      if (request.data && !(await waitForInflight(deadline, true) && await mediaAttached(deadline))) {
        return unsettled;
      }
      const { batch } = applyBatch(JSON.parse(readOut(host.wasm.exact_advance(host.agentClock, 2))));
      if (batch.error) {
        return {
          error: `clock: ${batch.error}`,
          clock: host.agentClock
        };
      }
      if (!request.data) {
        return { clock: host.agentClock };
      }
      if (!waiting().length && !host.wasm.exact_requests_pending() && !host.mediaAttaching.size) {
        return {
          clock: host.agentClock,
          settled: true
        };
      }
    }
    return unsettled;
  }
  const settle = !!request.settle;
  if (host.imageHold) {
    await (await host.imageHold).ready();
  }
  // an animated image starts on the clock it lands at
  const deadline = performance.now() + SETTLE_DEADLINE_MS, glide = { ms: GLIDE_BOUND_MS };
  let world = {};
  const reply = (settled, requests) => ({
    clock: host.agentClock,
    ...settled === undefined ? {} : { settled },
    ...world.reply,
    ...settled === false && world.pending ? { reason: "world" } : settled === false && requests ? { reason: "requests" } : {}
  });
  for (let rounds = 0;; rounds++) {
    if (settle && host.glidesSettled && !await host.glidesSettled(glide)) {
      return {
        ...reply(false),
        reason: "scroll"
      };
    }
    const scrolled = host.scrollsSeen?.();
    /* a scroller still gliding ends first; one this round's work moves, the next round (agent-input.js) */
    if (settle && !(await waitForInflight(deadline, true) && await mediaAttached(deadline))) {
      return reply(false, true);
    }
    const pieceLoad = pieces.pending();
    if (pieceLoad) {
      await pieceLoad;
    }
    if (presence.pending() && !await presence.landed(Math.max(0, deadline - performance.now()))) {
      return reply(false, true);
    }
    if (settle) {
      collections.settle();
    }
    // rows a list shows are built at this clock, before it moves (as the JS agent does): their animations start here
    if (gpuInPlay()) {
      const pending = await settleGpu();
      if (pending.length) {
        return gpuPendingReply(request, pending);
      }
    }
    const to = settle ? Math.max(settleCandidate(), world.settleAt ?? host.agentClock) : request.to;
    if (!(to >= host.agentClock)) {
      return { error: `the clock cannot go backwards (${host.agentClock} → ${to})` };
    }
    let batch;
    for (let steps = 0, stops = 0;;) {
      // while a view declares a layout transition, each due time is a batch of its own, which presence plays from that time (as the native hosts lay out there), in stops apart from the 4096
      const layout = host.flowDue != null && host.flowDue < to && stops < 36e3 && presence.live?.tracking(), sensing = host.wasm.exact_sensor_inputs?.() ? host.agentClock + 1e3 / 60 : Infinity, at = Math.min(layout ? Math.max(host.flowDue, host.agentClock) : to, sensing), waited = (host.flowDue != null && host.flowDue <= at || host.flowLanding) && await waitForInflight(deadline) && await mediaAttached(deadline, host.sizes), held = waited && steps < 4096;
      // a canvas the substitute feeds a sensor steps a display frame at a time, that frame's samples handed it first (LLP 1133.013 D6)
      if (sensing !== Infinity) {
        globalThis.exact.gpu?.sensors?.(JSON.parse(readOut(host.wasm.exact_sensor_events(at))));
      }
      ({batch} = applyBatch(JSON.parse(readOut(host.wasm.exact_advance(at, held ? 1 : 0)))));
      if (sensing !== Infinity && at < to && !batch.error) {
        globalThis.exact.gpu?.schedule?.();
        globalThis.exact.gpu?.clock?.(false);
      }
      if (batch.error || !held && at >= to) {
        break;
      }
      if (layout && !(host.flowDue != null && host.flowDue <= at)) {
        stops++;
      } else {
        steps++;
      }
    }
    globalThis.exact.gpu?.schedule?.();
    if (host.flowLoading) {
      await host.flowLoading;
    }
    if (host.textflow) {
      await host.textflow.settle();
    }
    if (batch.error) {
      return {
        error: `clock: ${batch.error}`,
        clock: host.agentClock
      };
    }
    if (gpuInPlay()) {
      const pending = await settleGpu();
      if (pending.length) {
        return gpuPendingReply(request, pending);
      }
    }
    world = globalThis.exact.gpu?.clock?.(settle) ?? {};
    if (!settle) {
      while (ask({ op: "polls" }).asking && performance.now() < deadline) {
        await new Promise((r) => setTimeout(r, 5));
      }
      /* a polled module source's ask the jump made lands first (LLP 1133.009 D10 item 7) */
      if (host.imageHold) {
        await (await host.imageHold).ready();
      }
      const n = waiting().length, b = n ? ask({ op: "state" }).background : null, background = b ? b.queued + b.inFlight : 0;
      return {
        ...reply(),
        ...n ? { inflight: n } : {},
        ...background ? { background } : {}
      };
    }
    // what a jump leaves in flight on real time, which the driver names (calendar F10), and the module's storage (LLP 1097 D9)
    collections.settle();
    const scrolls = rounds < 15 ? await collections.fireOwedScrolls() : null;
    if (scrolls?.fired || scrolls?.owed) {
      continue;
    }
    // every list built and measured where it shows (LLP 1070 G3); a list's moved offset owes a `scroll` event, fired at a rendering step with its handler: waited for (collection-glue `owedScrolls`)
    if (waiting().length || host.wasm.exact_requests_pending() || host.mediaAttaching.size) {
      if (rounds >= 15) {
        return reply(false, true);
      }
      continue;
    }
    const next = Math.max(settleCandidate(), world.settleAt ?? host.agentClock);
    if (next <= host.agentClock && !world.pending && settle && host.glidesSettled && !await host.glidesSettled(glide)) {
      return {
        ...reply(false),
        reason: "scroll"
      };
    }
    const moved = settle && next <= host.agentClock && !world.pending && host.scrollsSeen?.() !== scrolled;
    /* two frames: a scroll this round's work began has its first event at the next frame's step; then another round */
    if (next <= host.agentClock && !world.pending && !moved) {
      if (host.imageHold) {
        await (await host.imageHold).ready();
      }
      const held = ask({ op: "holds" });
      return held.holds?.length ? {
        ...reply(false),
        reason: "device",
        tickets: held.tickets
      } : collections.owedScrolls() ? {
        ...reply(false),
        reason: "scroll-event"
      } : reply(true);
    }
    // a hold is never waited on (LLP 1069.007 D3)
    if (rounds >= 15) {
      return moved ? {
        ...reply(false),
        reason: "scroll"
      } : reply(false);
    }
  }
}
