import { host, loadAfterPaint, localAssetURL, agentMode, now, collections, followOffset, readOut, root, log, pageNative, loadPageNative, pieces, mutate, REPLACE_SETTLE_MS, waiting, encoder, writeIn, loadStage, focus, ask, presence, arrange, motion, views, retiredViews, followedScrolls, followScroll, pendingScrolls, lists, messageFrames, messageViews, controllers, forgettable, inflight, heldAsks, pageFacts, loadGpuIfNeeded, moduleCall, rustImports, dataImports, t0, agentKeepsStore, decoder, assetNamespace, documentsGlue, AGENT_ADMITTED, httpHelpers } from "../glue.js";
import { send, applyBatch } from "./commit.js";
import { navigation, launchLocation, launchVisits, reportTime, reportPlace, preferences, updateCode, foldBits, appRootFontSize, releaseAssets, afterPaint, grantOrigins, modalLayers, inertAncestor, editMenu } from "../navigation.js";
import { setInputReady } from "./input.js";
import { WINDOW_STATE } from "../navigation/store.js";
import { startClock } from "./flow.js";
import { forgetList } from "./views.js";
import { engagement } from "../engagement.js";
// A trap leaves the module as it stopped (a Rust borrow held, a batch half
// applied), so every later call traps again. The first one stops the page's
// calls: the clock ends, a later call throws `ModuleStopped` without entering
// the module and is not reported, and the page says so once.
class ModuleStopped extends Error {}
// A split core imports, for each stage function its table can reach, a placeholder from `placeholder.<stage>`
// (`wasm-split`, LLP 1047.000 §9), which the stage, when it loads, writes its function over. One called before then
// throws, leaving the module mid-call as a trap does.
class StageNotLoaded extends Error {}
function withPlaceholders(imports) {
  const stage = (name) => new Proxy({}, {
    get: (_, slot) => () => {
      throw new StageNotLoaded(`app.wasm called the ${name} stage (placeholder ${String(slot)}) before the stage loaded`);
    }
  });
  return new Proxy(imports, {
    get: (given, name) => given[name] ?? (typeof name === "string" && name.startsWith("placeholder.") ? stage(name.slice("placeholder.".length)) : undefined)
  });
}
function guardedExports(exports) {
  const guarded = {};
  for (const [name, value] of Object.entries(exports)) {
    guarded[name] = typeof value !== "function" ? value : (...args) => {
      if (host.trapped) {
        throw new ModuleStopped(`app.wasm stopped after a trap: ${host.trapped}`);
      }
      try {
        return value(...args);
      } catch (error) {
        if (error instanceof WebAssembly.RuntimeError || error instanceof StageNotLoaded) {
          stopModule(error);
        }
        throw error;
      }
    };
  }
  return guarded;
}
function stopModule(error) {
  if (host.trapped) {
    return;
  }
  host.trapped = error;
  host.ticker?.dispose();
  host.ticker = null;
  host.textflow?.dispose();
  host.textflow = null;
  root.dataset.error = String(error);
  const unloaded = error instanceof StageNotLoaded
    ? "; every call into a stage waits for it to load (glue.js `loadStage`, LLP 1047.000 §9): the stack names the caller that entered it before it loaded, and a development build, which is whole, shows the page without the split"
    : "";
  console.error(`exact: app.wasm trapped, and the page no longer calls it${unloaded}:`, error);
  globalThis.exact.onTrap?.(error);
}
addEventListener("error", (e) => e.error instanceof ModuleStopped && e.preventDefault());
addEventListener("unhandledrejection", (e) => e.reason instanceof ModuleStopped && e.preventDefault());
export async function loadRust() {
  if (host.rustLoader) {
    return;
  }
  host.rustLoading ??= loadAfterPaint("./rust-glue.js", "createRustRuntime").then((create) => {
    host.rustLoader = create(() => host.memory);
  }).catch((error) => {
    host.rustLoading = null;
    throw error;
  });
  await host.rustLoading;
}
// LLP 1019 D5: FontFace loading is part of host boot. The DOM remains empty until every local face loaded, or 100 ms elapsed. At the barrier, install
// every face already ready unless its family has a failed sibling; faces that
// finish later remain unused for this generation (no post-paint swap).
function devCard(source, assets) {
  try {
    const url = new URL(source, document.baseURI);
    return url.origin === location.origin ? assets?.get(decodeURIComponent(url.pathname.replace(/^\//, ""))) : null;
  } catch {
    return null;
  }
}
async function prepareFonts(faces, assets) {
  if (!faces?.length) {
    return [];
  }
  const rows = faces.map((face) => ({
    face,
    state: "pending",
    loaded: null
  }));
  // A dev generation's font file, which the page reads when asked (dev.js `later`), is read before the 100 ms
  // barrier starts: the barrier times the face's load, not the dev server.
  const read = await Promise.all(faces.map((face) => {
    const card = devCard(face.source, assets);
    return card?.read ? card.read().catch(() => null) : null;
  }));
  const pending = rows.map(async (row, i) => {
    const { face } = row;
    try {
      const url = new URL(localAssetURL(face.source, assets), document.baseURI).href;
      row.loaded = await new FontFace(face.family, read[i] ?? `url(${JSON.stringify(url)})`, {
        weight: face.weight,
        stretch: face.stretch,
        style: face.style
      }).load();
      row.state = "loaded";
    } catch (e) {
      row.state = "failed";
      console.error("exact: font.registration.failed", face.family, face.source, String(e));
    }
  });
  let timer;
  const ready = Promise.all(pending);
  const timedOut = await Promise.race([ready.then(() => false), new Promise((resolve) => {
    timer = setTimeout(() => resolve(true), 100);
  })]);
  clearTimeout(timer);
  const failed = new Set(rows.filter((row) => row.state === "failed").map((row) => row.face.family));
  if (timedOut) {
    console.error("exact: font.registration.timeout", faces.length);
  }
  return rows.filter((row) => row.state === "loaded" && !failed.has(row.face.family)).map((row) => row.loaded);
}
function commitFonts(faces) {
  for (const face of host.installedFonts) {
    document.fonts.delete(face);
  }
  for (const face of faces) {
    document.fonts.add(face);
  }
  host.installedFonts = faces;
}
function activateData() {
  const batch = JSON.parse(readOut(host.wasm.exact_data_ready()));
  if (batch.error) {
    throw new Error(batch.error);
  }
  let inputOpen;
  const inputOpened = new Promise((resolve) => {
    inputOpen = resolve;
  });
  host.page?.release(applyBatch, inputOpened, () => navigation.activate(root, log));
  applyBatch(batch);
  // early presses replay once input is ready
  const ready = () => {
    setInputReady(true);
    collections.dataReady();
    root.dataset.moduleReady = "true";
    inputOpen();
    if (pageNative) {
      loadPageNative().then((native) => native.drain()).catch((error) => log(`native: ${error}`));
    }
  };
  const pending = pieces.pending();
  // pieces this tree first uses: input waits for their handlers (LLP 1047 D5)
  return pending ? pending.then(ready) : ready();
}
// A replacement commits between interactions (LLP 1029.000.002 D7, D9): it waits for whatever `engagement` says the
// person holds and for what its candidate names itself (`also`: a module's requests). `wait` answers true in the
// first task with neither, and the caller applies in that same task; or false once a newer replacement begins or the
// page goes. `changed` asks again, for a hold no input ends. `pending` names what it
// waits for. One ticket follows a candidate through preparation.
export const replacements = (() => {
  let waiter = null, queued = false, ticket = 0, active = false, preparing = null;
  const channel = new MessageChannel();
  const holds = () => held() ?? waiter?.also?.() ?? null;
  const settle = () => {
    if (waiter && !queued && !holds()) {
      queued = true;
      channel.port2.postMessage(null);
    }
  };
  addEventListener("pagehide", () => { ticket++; active = false; preparing = null; waiter?.resolve(false); waiter = null; }, true);
  const held = engagement(settle);
  channel.port1.onmessage = () => {
    queued = false;
    if (waiter && !holds()) {
      const { resolve } = waiter;
      waiter = null;
      resolve(true);
    }
  };
  return {
    get pending() { return active || waiter ? holds() ?? preparing : null; },
    changed: settle,
    begin() {
      waiter?.resolve(false);
      waiter = null;
      active = true;
      preparing = null;
      return ++ticket;
    },
    preparing(mine, why) { if (mine === ticket) preparing = why; },
    end(mine) { if (mine === ticket) { active = false; preparing = null; } },
    current(mine) { return mine === ticket; },
    wait(mine = ticket, also = null) {
      if (mine !== ticket) return Promise.resolve(false);
      waiter?.resolve(false);
      return new Promise((resolve) => { waiter = { resolve, also }; settle(); });
    },
  };
})();
export async function boot(bytes, assets = host.devAssets, current = () => true, module = null, fresh = false, ticket = replacements.begin()) {
  try {
    if (!current() || !replacements.current(ticket)) return null;
    const request = ++host.bootAttempt;
    const live = () => current() && replacements.current(ticket) && request === host.bootAttempt;
    if (host.incarnation && !await replacements.wait(ticket) || !live()) return null;
    return await mutate(() => live() ? bootNow(bytes, assets, live, module, fresh, ticket) : null);
  } finally { replacements.end(ticket); }
}
// Report both schemes as the page draws them, beneath an app's theme (LLP 1095 D4), so the kernel's canvas ink
// follows the browser's palette at boot and when its contrast changes (src/colors.rs).
export function reportColors() {
  const appleRoles = host.wasm?.exact_report_colors && getComputedStyle(document.documentElement).getPropertyValue("--exact-apple-roles").trim();
  if (!appleRoles) return;
  const probe = document.body.appendChild(document.createElement("i")), seen = getComputedStyle(probe), colors = [];
  for (const dark of [false, true]) {
    probe.style.colorScheme = dark ? "dark" : "light";
    for (const name of JSON.parse(appleRoles).split(" ")) {
      if (seen.getPropertyValue(`--exact-apple-${name}`).trim()) {
        probe.style.color = `var(--exact-apple-${name})`;
        colors.push([name, dark, seen.color]);
      }
    }
  }
  probe.remove();
  host.wasm.exact_report_colors(writeIn(JSON.stringify(colors)));
}
async function bootNow(bytes, assets = host.devAssets, current = () => true, module = null, fresh = false, ticket) {
  const t = performance.now(), request = host.bootAttempt;
  // Decode and load private font faces while the live page keeps running.
  // Carry state only at the synchronous host acceptance point below.
  const bakedLength = bytes ? 0 : host.wasm.exact_plan();
  const plan = bytes ?? new Uint8Array(host.memory.buffer, host.wasm.exact_out(), bakedLength).slice();
  let ptr = host.wasm.exact_in(plan.length);
  new Uint8Array(host.memory.buffer, ptr, plan.length).set(plan);
  const faces = JSON.parse(readOut(host.wasm.exact_plan_fonts(plan.length)));
  if (faces.error) {
    throw new Error(faces.error);
  }
  const preparedFonts = await prepareFonts(faces, assets);
  const shaderCommit = assets !== null && globalThis.exact.gpu ? await globalThis.exact.gpu.prepareShaders(assets) : null;
  if (!bytes && !module && host.page?.answers) await host.page.answers;
  // A retiring optional instance cannot delay boot; its generation guard disposes it.
  if (!current() || request !== host.bootAttempt) {
    return null;
  }
  // A module's replacement lets the running executor finish its answers first: the page's requests and the
  // runner's own, which refuses the swap while one is out (src/abi.rs `boot_module`). An update waits for them with
  // no deadline. The dev loop's waits a second from the first turn they alone hold it, then swaps anyway, as an app
  // may always have a request out: what is still out is let go as a plan restart lets it go, and the new runner
  // asks again for what it has not settled (LLP 1029.000.002 D9, LLP 1007 §6).
  let left = [], timed = false, letGo = false;
  const requests = module && (() => {
    if (letGo || !waiting().length && !host.wasm.exact_requests_pending()) return null;
    if (module.letGo && !timed) {
      timed = true;
      setTimeout(() => { letGo = true; replacements.changed(); }, REPLACE_SETTLE_MS);
    }
    return "requests";
  });
  const keeps = !fresh && (bytes || module);
  if (keeps) await loadStage("inspection");
  // A cold WebView or hidden iframe can have no viewport. Wait without a deadline; module swaps reuse the runner's.
  if (!current() || request !== host.bootAttempt) {
    return null;
  }
  while (!module && !(innerWidth > 0 && innerHeight > 0)) {
    await new Promise((resolve) => addEventListener("resize", resolve, { once: true }));
    if (!current() || request !== host.bootAttempt) {
      return null;
    }
  }
  // What carries is read after the last hold's end, with no task between: the interaction's and the requests' are
  // asked together, so one that began while the other was waited out holds the candidate too. The page's location,
  // history, date and place are read here with the rest, as the person may have navigated during the hold.
  if (!current() || request !== host.bootAttempt) return null;
  if (host.incarnation && !await replacements.wait(ticket, requests) || !current() || request !== host.bootAttempt) return null;
  // Canvas text names the declared family (LLP 1056 D8); the running plan's aliases hold until the swap.
  globalThis.exact.fontAliases = Object.fromEntries(faces.map((f) => [f.declared, f.family]));
  // @ref LLP 1038 D5. After a reload, the visits Back returns to come first, one a line, so the runner launches with
  // them beneath (#527). A rendered document's checkpoint is matched against the location alone (`page_launch`), so
  // it launches without them.
  const launch = encoder.encode([...(host.page?.checkpoint ? [] : launchVisits()), launchLocation()].join("\n"));
  // The date and place the boot's first settle reads (LLP 1027.000.000).
  if (host.wasm.exact_set_launch) {
    const [epoch, offset] = reportTime(now());
    host.wasm.exact_set_launch(epoch, offset, writeIn(reportPlace()));
  }
  const kept = keeps ? focus.keep(ask({ op: "tree" }), Number(document.activeElement?.closest?.("[data-view]")?.dataset.view)) : undefined;
  reportColors();
  let len;
  host.wasm.exact_live_clock?.(agentMode ? 0 : 1);
  // The page's clock owns time, the agent's when it drives (the timers RFC, D1); kept for every boot.
  if (module) {
    const id = module.rust ?? new TextEncoder().encode(JSON.stringify(module.realm.id));
    const payload = new Uint8Array(plan.length + module.receipt.length + id.length);
    payload.set(plan);
    payload.set(module.receipt, plan.length);
    payload.set(id, plan.length + module.receipt.length);
    ptr = host.wasm.exact_in(payload.length);
    new Uint8Array(host.memory.buffer, ptr, payload.length).set(payload);
    if (module.letGo) {
      left = waiting();
    }
    // what this swap lets go, read at the swap: the stage load above yields
    len = host.wasm.exact_boot_module(plan.length, module.receipt.length, id.length, module.letGo ? 1 : 0);
  } else if (bytes) {
    ptr = host.wasm.exact_in(bytes.length + launch.length);
    const payload = new Uint8Array(host.memory.buffer, ptr, bytes.length + launch.length);
    payload.set(bytes);
    payload.set(launch, bytes.length);
    host.wasm.exact_set_update?.(updateCode());
    host.wasm.exact_set_screen_reader(pageFacts.screenReaderCode());
    len = host.wasm.exact_boot_plan(bytes.length, innerWidth, innerHeight, launch.length, preferences() | foldBits());
  } else {
    if (host.page?.checkpoint) {
      const checkpoint = agentMode
        ? (await loadAfterPaint("./faults.js", "drivenCheckpoint"))(host.page.checkpoint)
        : host.page.checkpoint;
      host.wasm.exact_checkpoint(writeIn(checkpoint));
    }
    ptr = host.wasm.exact_in(launch.length);
    // a drive's clock starts at zero (page.rs)
    new Uint8Array(host.memory.buffer, ptr, launch.length).set(launch);
    host.wasm.exact_set_update?.(updateCode());
    host.wasm.exact_set_screen_reader(pageFacts.screenReaderCode());
    len = host.wasm.exact_boot(innerWidth, innerHeight, launch.length, preferences() | foldBits());
  }
  const batch = JSON.parse(readOut(len));
  if (batch.error) {
    throw new Error(batch.error);
  }
  if (host.wasm.exact_presence?.()) {
    presence.preload();
  }
  // after first paint, where the plan names a presence row (navigation.js)
  host.frameSampler?.reset();
  // a new runner numbers its transactions afresh (LLP 1079 D3)
  if (module) {
    host.activeModule?.realm?.dispose();
    host.activeModule = module;
    setInputReady(true);
    if (left.length) {
      const writes = left.filter((p) => p.write).map((p) => p.write);
      log(`module replacement let go of ${left.length} request${left.length === 1 ? "" : "s"} still in flight after ${REPLACE_SETTLE_MS} ms${writes.length ? `; aborted writes the server may or may not have seen: ${writes.join(", ")}` : ""}`);
    }
  }
  navigation.reset(batch.ops.find((op) => op.op === "router"));
  const oldAssets = host.devAssets;
  host.devAssets = assets;
  shaderCommit?.();
  // Tear down without yielding; ownership guards refuse retired views.
  host.incarnation += 1;
  globalThis.exact.generation = host.incarnation;
  // A queued surface belongs to the plan that named it. The GPU device may
  // finish loading across a reload; no old surface request may join the new
  // plan even when view ids are reused.
  globalThis.exact.pendingSurfaces = [];
  host.ticker?.dispose();
  host.ticker = null;
  arrange.reset();
  motion.reset();
  if (host.textflow) {
    for (const el of views.values()) {
      retiredViews.add(el);
    }
  }
  host.textflow?.dispose();
  host.textflow = null;
  host.flowLoading = null;
  host.flowFailed = false;
  host.flowContexts = [];
  host.flowDue = null;
  host.flowFrames = false;
  globalThis.exact?.gpu?.reset(Boolean(bytes));
  for (const el of followedScrolls.keys()) {
    followScroll(el, false);
  }
  pendingScrolls.clear();
  collections.reset();
  for (const el of lists.keys()) {
    forgetList(el);
  }
  for (const id of views.keys()) {
    pieces.canvas2d({
      id,
      forget: true
    });
  }
  for (const el of views.values()) {
    if (el instanceof HTMLMediaElement) {
      globalThis.exact.removeMedia?.(el);
      el.pause();
      el.removeAttribute("src");
      el.load();
    }
  }
  for (const el of views.values()) {
    el.exactUnload?.abort();
    el.exactSelection?.();
    el.exactAppFrame?.dispose();
    el.exactMarkup?.destroy();
    el.exactDecorations?.destroy();
    el.exactLiveTime?.destroy();
    el.exactNative?.destroy();
    el.exactResize?.();
    el.exactIntersect?.();
    el.exactPanRetire?.();
  }
  views.clear();
  messageFrames.clear();
  messageViews.clear();
  if (host.storageRequests) {
    host.storageRequests.then((s) => s.dispose()).catch(() => {});
    host.storageRequests = null;
  }
  host.grants = [];
  host.grantSet = null;
  host.unparsed = "";
  for (const controller of controllers) {
    controller.abort();
  }
  controllers.clear();
  forgettable.clear();
  inflight.clear();
  heldAsks.clear();
  host.mediaAttaching.clear();
  host.sizes.clear();
  globalThis.exact.holdQuiet?.();
  if (!host.page?.holding) {
    root.replaceChildren();
    // The last plan's hidden `portal` movers (commit.js).
    document.querySelector("[data-exact-portals]")?.remove();
  }
  document.getElementById("exact-theme")?.remove();
  document.getElementById("exact-looks")?.remove();
  // a held state goes unsaid (dev.js holds it again); the last plan's theme (LLP 1095 D4) and state looks (LLP 1121 D8): this boot's batch sets its own
  commitFonts(preparedFonts);
  focus.restart(kept, () => {
    host.restoringScheme = fresh;
    try { applyBatch(batch); } finally { host.restoringScheme = false; }
  }, () => ask({ op: "tree" }), (id) => views.get(id));
  host.toldOffset = null;
  followOffset(now());
  // @ref LLP 1027.000.000 — the date, as the clock the runner already reads
  if (host.wasm.exact_set_place) {
    applyBatch(JSON.parse(readOut(host.wasm.exact_set_place(writeIn(reportPlace())))));
  }
  if (host.wasm.exact_set_page) {
    applyBatch(JSON.parse(readOut(host.wasm.exact_set_page(pageFacts.bits()))));
  }
  // @ref LLP 1069.000 D2 r7 — the present of this runner's first picture: the first animation frame after its first
  // commit (under the agent, each frame is presented at its commit, and the runner says so at its boot)
  if (host.wasm.exact_page_presented && !agentMode) {
    const incarnation = host.incarnation;
    requestAnimationFrame(() => {
      if (host.incarnation === incarnation) {
        applyBatch(JSON.parse(readOut(host.wasm.exact_page_presented())));
      }
    });
  }
  appRootFontSize("medium");
  if (host.wasm.exact_set_root_font_size) {
    applyBatch(JSON.parse(readOut(host.wasm.exact_set_root_font_size(pageFacts.rootFontSize()))));
  }
  if (host.wasm.exact_set_display_scale) {
    applyBatch(JSON.parse(readOut(host.wasm.exact_set_display_scale(pageFacts.displayScale()))));
  }
  // a new runner holds no app size (LLP 1069.000 D3), and is told the display's scale, which a hairline is one device pixel of (LLP 1115.000 D10.3)
  globalThis.exact?.gpu?.finishRestart();
  if (bytes && !module && (host.inputReady || root.dataset.error)) {
    activateData();
  }
  // A restart after the first activation.
  if (oldAssets !== assets) {
    releaseAssets(oldAssets);
  }
  // @ref LLP 1043.000 §3 D7 — an optional initial flow load cannot gate paint/readiness.
  if (bytes && host.flowLoading) {
    await host.flowLoading;
  }
  if (bytes && host.textflow) {
    await host.textflow.settle();
  }
  startClock();
  if (bytes) {
    afterPaint(loadGpuIfNeeded, agentMode);
  }
  return performance.now() - t;
}
export async function main() {
  if (host.page) {
    const options = {
      root,
      views,
      log,
      early: host.page.early,
      dispatch: (id, kind = 0, value = "", event = null) => {
        const outer = host.keyEvent;
        host.keyEvent = event;
        try {
          send(host.wasm.exact_dispatch(id, kind, value ? writeIn(value) : 0, now()));
        } finally {
          host.keyEvent = outer;
        }
      }
    };
    host.page = globalThis.exact.documentPage?.connect(options) ?? (await loadAfterPaint("./document-glue.js", "documentBoot"))(options);
    await host.page.started;
  }
  // A served document's download began at its first paint (LLP 1048.000 D6).
  // A navigation that leaves the page stops it, or the glue's own, and their
  // preload, so the next document has the link. A page that stays downloads it
  // again: a task after Stop (`navigateerror`, fired mid-stop) or Back from the
  // bfcache (`pageshow`); a 204 or a download says nothing, so after a second.
  // The page names the build in its preload (`./app.wasm?v=…`, LLP 1047.000 §9), so this file is the same across builds.
  const preload = () => [...document.querySelectorAll("link[rel=\"preload\"]")].find((l) => new URL(l.href).pathname.endsWith("/app.wasm")), url = new URL(preload()?.href ?? "./app.wasm", import.meta.url), imports = {
    exact_grants: grantOrigins(() => host.memory),
    exact_js: { call: moduleCall },
    exact_rust: rustImports,
    exact_data: dataImports,
    exact_clock: { now: () => performance.now() - t0 },
    exact_geometry: {
      shown: (view, first) => {
        if (first) {
          host.structuralModalLayers = modalLayers();
          queueMicrotask(() => {
            host.structuralModalLayers = null;
          });
        }
        const el = views.get(view);
        if (!el?.isConnected || !root.contains(el) || inertAncestor(el)) {
          return 0;
        }
        const live = host.structuralModalLayers;
        if (live && !live.some((layer) => layer.contains(el))) {
          return 0;
        }
        for (let at = el; at && at !== root; at = at.parentElement) {
          if (getComputedStyle(at).display === "none") {
            return 0;
          }
        }
        return 1;
      },
      read: (op, view, out) => host.geometry?.read(op, views.get(view), new Float64Array(host.memory.buffer, out, 4)) ?? 0,
      scroll_metrics: (view, out) => {
        const values = host.geometry?.scrollMetrics(views.get(view));
        if (!values) return 0;
        new Float64Array(host.memory.buffer, out, 6).set(values);
        return 1;
      },
      range: (view, start, end, out) => host.geometry?.range(views.get(view), start, end, new Float64Array(host.memory.buffer, out, 4)) ?? 0,
      point: (x, y) => {
        const el = host.geometry?.point(x, y)?.closest("[data-view]"), id = el && root.contains(el) ? Number(el.dataset.view) : NaN;
        return views.get(id) === el ? id + 1 : 0;
      },
      points: (x, y, out, cap) => {
        const hits = (host.geometry?.points(x, y) ?? []).flatMap((hit) => {
          const el = hit.closest("[data-view]"), id = el && root.contains(el) ? Number(el.dataset.view) : NaN;
          return views.get(id) === el ? [id + 1] : [];
        });
        if (cap) {
          new Uint32Array(host.memory.buffer, out, Math.min(cap, hits.length)).set(hits.slice(0, cap));
        }
        return hits.length;
      }
    }
  }, aborted = (e) => e?.name === "AbortError";
  const download = () => {
    const stop = new AbortController();
    globalThis.navigation?.addEventListener("navigate", (e) => e.destination.sameDocument || e.downloadRequest != null || (stop.abort(), preload()?.remove()), { signal: stop.signal });
    return fetch(url, { signal: stop.signal });
  };
  const stayed = () => new Promise((done) => {
    const later = () => setTimeout(done);
    globalThis.navigation?.addEventListener("navigateerror", later, { once: true });
    addEventListener("pageshow", later, { once: true });
    setTimeout(done, 1e3);
  });
  let response = (globalThis.exact.runtime ??= download()).then((r) => r.url === url.href ? r : download(), (e) => aborted(e) ? Promise.reject(e) : download()), instance;
  let compiled;
  for (;;) {
    try {
      ({instance, module: compiled} = await WebAssembly.instantiateStreaming(response, withPlaceholders(imports)));
      break;
    } catch (e) {
      if (!aborted(e)) {
        throw e;
      }
      await stayed();
      response = download();
    }
  }
  host.exports = instance.exports;
  host.wasm = guardedExports(instance.exports);
  const [staged] = WebAssembly.Module.customSections(compiled, "exact.stages");
  if (staged) {
    host.stages = JSON.parse(new TextDecoder().decode(staged)).stages;
  }
  host.memory = host.wasm.memory;
  if (WebAssembly.Module.imports(compiled).some((i) => i.module === "exact_geometry")) {
    host.geometryLoading = new Promise((r) => requestAnimationFrame(r)).then(() => loadAfterPaint("./geometry-glue.js", "geometry")).then((create) => {
      host.geometry = create(root);
    });
  }
  // an artifact whose actions read geometry imports it (LLP 1051.000 D4)
  globalThis.exact.compat = JSON.parse(readOut(host.wasm.exact_compat()));
  host.logicInfo = typeof host.wasm.exact_module_artifact === "function" && host.wasm.exact_logic ? {
    ...JSON.parse(readOut(host.wasm.exact_logic())),
    native: pageNative
  } : null;
  setInputReady(false);
  // Every data executor activates after the baked first pixel.
  // Restore granted secrets and persisted states before the baked frame (LLP 1018 D6, LLP 1116 D5).
  if (agentKeepsStore) {
    if (agentMode) globalThis.__exactSeedStored?.("exact.state.", "exact.secret.");
    const kept = [];
    try {
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key?.startsWith("exact.secret.") && !key.slice("exact.secret.".length).startsWith(WINDOW_STATE)) {
          kept.push(key.slice("exact.secret.".length), localStorage.getItem(key) ?? "");
        } else if (key?.startsWith("exact.state.") && !key.startsWith(WINDOW_STATE)) {
          kept.push(key, localStorage.getItem(key) ?? "");
        }
      }
    } catch (e) {
      console.warn("exact: store", String(e));
    }
    try {
      for (let i = 0; i < sessionStorage.length; i++) {
        const key = sessionStorage.key(i);
        if (key?.startsWith(WINDOW_STATE)) kept.push(key, sessionStorage.getItem(key) ?? "");
      }
    } catch (e) {
      console.warn("exact: window state", String(e));
    }
    if (kept.length) {
      host.wasm.exact_store(writeIn(kept.join("\0")));
    }
  }
  const first = await globalThis.exact.devFirst?.();
  // A data module's realm: beside the boot on a served document, whose first
  // pixel is painted (LLP 1048.000 D6); a client page's after its baked frame.
  // The baked data source admits only the baked module's revision. A dev
  // generation's module with another (a TypeScript edit since the wasm was
  // built) replaces it at activation through the module path, whose receipt
  // names its revision — no wasm rebuild.
  const realm = () => typeof host.wasm.exact_module_artifact !== "function" ? null : loadAfterPaint("./module-glue.js", "moduleRuntime").then(async (loader) => {
    host.moduleLoader = loader;
    const payload = first?.module ?? await loader.baked();
    const replaces = Boolean(first?.module) && JSON.parse(decoder.decode(payload.receipt)).module?.sha256 !== host.logicInfo.revision;
    return {
      ...payload,
      replaces,
      realm: await loader.prepare(payload, host.logicInfo, replaces ? undefined : 0)
    };
  });
  const prepared = host.page ? realm() : null;
  prepared?.catch(() => {});
  // awaited, and reported, at activation
  await boot(first?.plan ?? null, first ? assetNamespace(first.assets) : null, () => true, null, true);
  // @ref LLP 1007 §6
  if (!host.page) {
    navigation.activate(root, log);
  }
  // A served document stays unsplit until release adopts it (LLP 1115.002 D4).
  root.dataset.bootMs = (performance.now() - t0).toFixed(1);
  const activate = async () => {
    loadGpuIfNeeded();
    if (host.wasm.exact_motion) {
      pieces.preload();
    }
    if (globalThis.launchQueue) {
      documentsGlue().catch(console.error);
    }
    // motion links its export (LLP 1047 D3); an installed app's launch files (LLP 1069.010)
    if (!agentMode) {
      loadAfterPaint("./timer-glue.js", "createTimerScheduler").then((create) => {
        host.timerFactory = create;
        startClock();
      }).catch(console.error);
    }
    if (!agentMode && AGENT_ADMITTED) {
      loadAfterPaint("./frames.js", "createFrameSampler").then((create) => {
        host.frameSampler = globalThis.exact.frames = create({
          origin: () => t0,
          log,
          target: "wasm",
          covers: [
            "input",
            "scroll",
            "batches",
            "animations",
            "canvas"
          ],
          gather: () => loadStage("inspection").then(() => ({
            journal: ask({
              op: "logs",
              since: 0
            }),
            perf: ask({ op: "perf" }),
            traceEvents: (ask({
              op: "state",
              trace_export: true
            }).world ?? []).flatMap((w) => (w.traceEvents ?? []).map((e) => ({
              ...e,
              pid: w.canvas * 2 + e.pid
            })))
          }))
        });
      }).catch(console.error);
    }
    // a development page's frames (LLP 1079 D3)
    // @ref LLP 1043.000 §3 D8 — one optional load, no activation wait or retry queue.
    loadAfterPaint("./input-glue.js", "createInputHandlers").then((create) => {
      host.inputHandlers = create({
        root,
        views,
        retiredViews,
        agentMode,
        pick: el => ask({ op: "showPicker", view: Number(el.dataset.view) }),
        ready: () => host.inputReady,
        inertAncestor,
        editMenu,
        dispatch: (id, payload) => send(host.wasm.exact_dispatch(id, 20, writeIn(payload), now())),
        release: (id, payload) => send(host.wasm.exact_dispatch(id, 28, writeIn(payload), now())),
        velocity: motion.pan,
        log,
        documents: () => documentsGlue()
      });
    }).then(() => host.inputHandlers.arrive()).catch(console.error);
    // the stack as it stands, for the focus to follow (LLP 1115.000 D4.2)
    try {
      const module = await (prepared ?? realm());
      if (module?.replaces) {
        await boot(first.plan, host.devAssets, () => true, module);
      } else {
        host.activeModule = module ?? host.activeModule;
      }
      await activateData();
    } catch (error) {
      root.dataset.error = String(error);
      console.error(error);
    } finally {
      host.resolveModuleReady();
      if (host.logicInfo) {
        httpHelpers();
      }
      // a data module's first response is read without a load
      if (globalThis.exact.compat.inputs.rustModule && globalThis.exact.compat.inputs.rustMode === "browser") {
        loadRust().then(() => globalThis.exact.followRustUpdates(globalThis.exact, import.meta.url)).catch((error) => console.error("Rust update discovery:", error));
      }
    }
  };
  // Nested rAF gives a client page's baked DOM a rendering opportunity before activation (a served document has painted already);
  // a page given no frames (a cross-origin frame out of view: the design tool's canvas) activates a second on, once, as rt.js `input()`; not under the agent.
  let activated;
  const activateOnce = () => activated ??= activate();
  if (host.page) {
    activateOnce();
  } else {
    requestAnimationFrame(() => {
      root.dataset.frameCallbackMs = (performance.now() - t0).toFixed(1);
      requestAnimationFrame(activateOnce);
    });
    if (!agentMode) {
      setTimeout(activateOnce, 1e3);
    }
  }
}
