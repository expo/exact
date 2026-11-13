import { views, retiredViews, presence, host, followedScrolls, motion, loadAfterPaint, root, agentMode, log, t0, readOut, writeIn, loadLiveTime, lists, arrange, pieces, loadGpuIfNeeded, afterNativePaint, deferFulfill, holds, agentKeepsStore, pendingStore, moduleReady, safelyFulfill, encoder, track, byTicket, forgettable, surfaceGranted, HOST_WORK_BASE64, loadPageNative, localAssetURL, controllers, pageFile, heldAsks, httpHelpers, now, postSurface, ask, picker, inflight, documentsGlue, frameNav, followScroll, messageFrames, messageViews, dropSurface, register, seek, settleFollow, pendingScrolls, rememberScroll, ownFollowed, releaseFollowed, collections, focusAutofocus, markScrollDocument, letGo, holdWake, holdOrientation, clocks } from "../glue.js";
import { prepareContexts, positionContexts } from "./context.js";
import { pageTitle, navigation, noteFit, syncListItemRoles, listItem, settleValue, afterPaint, createGrantSet, rawGrantText, grantError, scopedGrantSet, refusal, appRootFontSize, liveById, navigableURL, sheetLeaves, popoverCommand, runFocusCommands, inertAncestor, refitTexts } from "../navigation.js";
import { applyProps, nativeCreate, listView, appSource, syncMedia, forgetList, syncViewportFit, refreshSymbols, syncLists } from "./views.js";
import { attach } from "./input.js";
import { flowBatch } from "./flow.js";
function viewFor(op, id) {
  const el = views.get(id);
  if (!el) {
    console.error(`exact: ${op} names missing view ${id}`);
  }
  return el;
}
function apply(batch) {
  // Retire before teardown blur. Keep removed descendants attached to their removed parent for the outgoing focusout; presence reads leaving boxes before any op (LLP 1063).
  for (const op of batch.ops ?? []) {
    if (op.op === "destroy") {
      const el = views.get(op.id);
      if (el) {
        retiredViews.add(el);
        el.exactUnload?.abort();
        el.exactSelection?.();
        el.exactAppFrame?.dispose();
        el.exactIntersect?.();
        el.exactResize?.();
      }
    }
  }
  presence.live?.before(batch, views);
  host.listSelection?.before();
  prepareContexts(batch);
  for (const s of followedScrolls.values()) {
    s.observed();
  }
  const focusCommands = [];
  const collectionOp = batch.ops?.find((op) => op.op === "collections");
  if (batch.error) {
    console.error("exact:", batch.error);
  }
  for (const op of batch.ops ?? []) {
    try {
      switch (op.op) {
        case "textflow": break;
        // consumed once after the complete DOM batch
        case "keyframes":
          motion.keyframes(op.name, op.css);
          break;
        case "looks": {
          const sheet = document.getElementById("exact-looks") ?? document.head.appendChild(Object.assign(document.createElement("style"), { id: "exact-looks" })), rules = sheet.exactRules ??= new Map();
          if (op.css) {
            rules.set(op.id, op.css);
          } else {
            rules.delete(op.id);
          }
          if (!sheet.exactFlush) {
            queueMicrotask(sheet.exactFlush = () => {
              sheet.exactFlush = null;
              sheet.textContent = [...rules.values()].join("");
            });
          }
          break;
        }
        case "theme":
          (document.getElementById("exact-theme") ?? document.head.appendChild(Object.assign(document.createElement("style"), { id: "exact-theme" }))).textContent = op.css;
          break;
        case "large-title":
          root.style.setProperty("--exact-large-title-size", `${op.size}px`);
          break;
        // LLP 1055 D7: the page's @keyframes; LLP 1121 D8: a view's state looks (src/host/looks.rs); LLP 1095 D4: the theme's :root rule
        case "language":
          if (op.lang) {
            document.documentElement.lang = op.lang;
          }
          document.documentElement.dir = op.dir;
          break;
        // a plan with no strings table names no language: the page keeps the manifest's (LLP 1115.000 D4.6) // a plan with no strings table names no language: the page keeps the manifest's (LLP 1115.000 D4.6)
        case "head":
          pageTitle(op.title ?? null);
          (host.headGlue ??= loadAfterPaint("./document-glue.js", "documentHead")).then((head) => head(op));
          break;
        // the title at once: the head's, else the route's (LLP 1115.000 D4.1)
        case "router":
          navigation.apply(op);
          if (op.scroll) {
            op.since = navigation.personScrolls;
            (host.fragmentGlue ??= loadAfterPaint("./fragment.js", "fragment")).then((f) => f.arm(root, op, navigation));
          }
          break;
        // a scroll to the visit's fragment, made at the runner's `scrollToFragment` once its content has arrived, which a scroll by the person since cancels (LLP 1153 D5)
        case "sound":
          if (!agentMode) {
            (host.soundModule ??= new Promise((r) => requestAnimationFrame(r)).then(() => loadAfterPaint("./sound-glue.js", "installSound")).then((install) => install({
              files: op.files ?? [],
              log,
              origin: () => t0
            }))).then((s) => {
              host.soundOut = s;
              if (op.files && op.files !== s.files) {
                s.reset(op.files);
              }
              s.ops(op.ops);
            }).catch(console.error);
          }
          break;
        case "midi":
          if (!agentMode) {
            (host.midiModule ??= new Promise((r) => requestAnimationFrame(r)).then(() => loadAfterPaint("./midi-glue.js", "installMidi")).then((install) => install({
              log,
              origin: () => t0,
              deliver: (m) => {
                if (host.wasm?.exact_midi) {
                  applyBatch(JSON.parse(readOut(host.wasm.exact_midi(writeIn(JSON.stringify(m))))));
                }
              }
            }))).then((m) => {
              host.midiOut = m;
              m.ops(op.ops, op.boot);
            }).catch(console.error);
          }
          break;
        // LLP 1096 D7: the voice table's ops, after first paint; never under the agent, whose clock is virtual (D10); LLP 1125 D7: the MIDI queue's ops and the ports, after first paint; never under the agent, whose runner answers a request itself
        case "create": {
          // Canvas overlays use a div; data-surface is the host-owned drawing leaf.
          const adopted = host.page?.adopting?.get(op.id);
          const el = adopted ?? (op.ns ? document.createElementNS(op.ns, op.tag) : document.createElement(op.tag === "canvas" ? "div" : op.tag));
          // an adopted document's element (LLP 1048.000 D6); SVG in its namespace (LLP 1055 D4)
          if (op.tag === "canvas" && el.firstElementChild?.dataset.surface === undefined) {
            const surface = document.createElement("canvas");
            surface.dataset.surface = "";
            surface.style.cssText = "position:absolute;inset:0;width:100%;height:100%;display:block;z-index:-1";
            el.append(surface);
          }
          applyProps(el, op.props, [], !!adopted);
          const css = op.css + (el.hasAttribute("data-action") ? ";touch-action:none" : "");
          if ((el.getAttribute("style") ?? "") !== css) {
            el.style.cssText = css;
          }
          noteFit(css);
          if (el.hasAttribute("data-time-reservation")) {
            loadLiveTime(el);
          }
          // an adopted element's is already there; a fitted text's scale follows each batch (LLP 1116.001 D1.4)
          attach(el, op.id, op.handlers);
          views.set(op.id, el);
          if (op.tag.includes("-") && !op.ns && !el.exactNative) {
            nativeCreate(el, op.id);
          }
          listView(el, op.id, collectionOp?.items.some((item) => item.view === op.id));
          break;
        }
        case "props": {
          const el = viewFor("props", op.id);
          if (el) {
            applyProps(el, op.set, op.clear);
            if (op.set?.role !== undefined || op.clear?.includes("role")) {
              syncListItemRoles(el);
            } else if ((op.set?.popover !== undefined || op.clear?.includes("popover")) && el.parentElement?.hasAttribute("data-exact-list-item")) {
              listItem(el);
            }
            if (el.hasAttribute("data-time-reservation") || el.exactLiveTime) {
              loadLiveTime(el);
            }
          }
          break;
        }
        case "style": {
          const el = viewFor("style", op.id);
          if (el) {
            exact.gpu?.beforeStyle(el);
            motion.style(op.id, op.css);
            noteFit(op.css);
            if (el.hasAttribute("data-action")) {
              el.style.touchAction = "none";
            }
            exact.gpu?.afterStyle(el);
          }
          break;
        }
        case "children": {
          const el = viewFor("children", op.id);
          if (!el) {
            break;
          }
          const want = [];
          for (const id of op.ids) {
            const child = viewFor("children", id);
            if (child) {
              want.push(el.hasAttribute("data-exact-list") && !lists.has(el) ? listItem(child, el) : child);
            }
          }
          // Reorder in place: keyed rows keep their elements (and their state).
          // A canvas's surface element is skipped: not a child, never removed.
          const skip = (n) => {
            while (n?.hasAttribute("data-surface") || n?.hasAttribute("data-exiting")) {
              n = n.nextElementSibling;
            }
            return n;
          };
          // a leaving view stays (LLP 1063)
          let cursor = skip(el.firstElementChild);
          for (const child of want) {
            if (child === cursor) {
              cursor = skip(cursor.nextElementSibling);
              continue;
            }
            el.insertBefore(child, cursor);
          }
          while (cursor) {
            const next = skip(cursor.nextElementSibling);
            cursor.remove();
            cursor = next;
          }
          settleValue(el);
          // a select shows its committed value among its new options
          break;
        }
        case "animate": {
          motion.animate(op);
          break;
        }
        case "timelines": {
          host.timelinesMoved = true;
          break;
        }
        case "retire-motion": {
          motion.retire(op.id, op.property, op.token, op.runtime);
          break;
        }
        case "height-drag": {
          motion.heightBinding(op);
          break;
        }
        case "transform-drag": {
          motion.transformBinding(op);
          break;
        }
        case "reorder-drag": {
          arrange.binding(op);
          break;
        }
        case "reorder-state": {
          arrange.state(op);
          break;
        }
        case "reorder-group": {
          arrange.group(op);
          break;
        }
        case "canvas2d": {
          pieces.canvas2d(op);
          break;
        }
        // LLP 1056 D7
        case "surface": {
          const surfaceView = views.get(op.id);
          if (surfaceView) {
            surfaceView.exactSurfaceName = op.name;
          }
          // A canvas's inputs (LLP 1009 D2): to the GPU module when it is
          // loaded, queued until then. The module itself is fetched only
          // after a rendering opportunity, and only when a canvas exists.
          if (globalThis.exact.gpu) {
            globalThis.exact.gpu.surface(op.id, op.name, op.values);
          } else {
            const pending = globalThis.exact.pendingSurfaces ??= [];
            const queued = pending.find((entry) => entry.id === op.id && entry.generation === host.incarnation);
            if (queued) {
              queued.name = op.name;
              queued.values = op.values;
            } else {
              pending.push({
                id: op.id,
                name: op.name,
                values: op.values,
                generation: host.incarnation
              });
              afterPaint(loadGpuIfNeeded, agentMode);
            }
          }
          break;
        }
        case "grants": {
          host.grantSet = createGrantSet(op.set);
          host.grants = rawGrantText(host.grantSet).split("\n").filter(Boolean);
          host.unparsed = grantError(host.grantSet) ?? "";
          if (host.unparsed) {
            console.warn("exact:", host.unparsed);
          }
          if (host.grants.some((l) => /^\s*auth\.session /.test(l))) {
            host.authHost ??= afterNativePaint().then(() => loadAfterPaint("./auth-glue.js", "authHost")).then((h) => host.authHost = h);
          }
          break;
        }
        case "auth": {
          const inc = host.incarnation, env = {
            agent: agentMode,
            log,
            call: (r) => JSON.parse(readOut(host.wasm.exact_auth(writeIn(JSON.stringify(r))))),
            deliver: (t) => deferFulfill(inc, t, 9, 0, "", new Uint8Array()),
            active: (t) => holds(t, inc)
          };
          if (host.authHost?.arm) {
            host.authHost.arm(op, env);
          } else if (agentMode && host.authHost) {
            host.authHost.then((h) => h.arm(op, env));
          } else {
            env.call({
              op: "arm",
              ticket: op.ticket,
              origin: location.origin,
              popup: false
            });
            env.deliver(op.ticket);
          }
          break;
        }
        // LLP 1069.006 D4: armed in the press's call stack; unloaded glue is 428
        case "store": {
          // A secret the app kept or forgot (LLP 1018 D6): `localStorage`, origin-scoped, is the web's secret store; a persisted
          // state (LLP 1116 D5) is kept under its own name, as persist.js keeps it. A nameless agent drive
          // starts from nothing; a named `--storage` keeps this origin's keys.
          if (agentKeepsStore) {
            if (op.durable) {
              pendingStore.discard(op.name);
            } else {
              pendingStore.set(op.name, op.value);
            }
          }
          break;
        }
        case "storage": {
          const requestIncarnation = host.incarnation;
          const p = Promise.resolve().then(async () => {
            await moduleReady;
            if (!host.inputReady) {
              throw new Error("data executor is unavailable");
            }
            if (requestIncarnation !== host.incarnation) {
              throw new Error("storage source unloaded");
            }
            if (!host.storageRequests) {
              const app = globalThis.exact.compat.inputs.app;
              const pending = loadAfterPaint("./storage-request.js", "createStorageRequests").then((create) => create(app, host.grantSet)).catch((error) => {
                if (host.storageRequests === pending) {
                  host.storageRequests = null;
                }
                throw error;
              });
              host.storageRequests = pending;
            }
            const service = await host.storageRequests;
            if (requestIncarnation !== host.incarnation) {
              throw new Error("storage source unloaded");
            }
            return service.run(op.payload, scopedGrantSet(host.grantSet, op.scope));
          }).then((bytes) => safelyFulfill(requestIncarnation, op.ticket, 5, 0, "", bytes)).catch((error) => safelyFulfill(requestIncarnation, op.ticket, 3, 0, "", encoder.encode(String(error))));
          track(p, op.ticket);
          break;
        }
        case "refuse": {
          deferFulfill(...refusal(op, host.incarnation));
          break;
        }
        case "abort": {
          const c = byTicket.get(op.ticket);
          byTicket.delete(op.ticket);
          forgettable.delete(c);
          c?.abort();
          break;
        }
        // the app aborted it: cancelled whatever its method, as a browser's fetch is
        case "continue": {
          const requestIncarnation = host.incarnation;
          const p = Promise.resolve().then(() => host.moduleLoader.run(op.token, (result) => safelyFulfill(requestIncarnation, op.ticket, 0, 200, "", encoder.encode(JSON.stringify(result))))).catch((error) => safelyFulfill(requestIncarnation, op.ticket, 3, 0, "", encoder.encode(String(error))));
          track(p, op.ticket);
          break;
        }
        case "surfaceWork": {
          const requestIncarnation = host.incarnation;
          const p = Promise.resolve().then(async () => {
            if (op.refusal) {
              throw Object.assign(new Error(op.refusal), { kind: 2 });
            }
            if (!surfaceGranted(op)) {
              throw Object.assign(new Error(`refused by grant: surface ${op.name}${host.unparsed && `: ${host.unparsed}`}`), { kind: 2 });
            }
            loadGpuIfNeeded();
            await host.gpuLoading;
            if (!exact.gpu) {
              throw Object.assign(new Error(`surface ${op.name}: expected one live surface, found 0`), { kind: 2 });
            }
            if (requestIncarnation !== host.incarnation || host.wasm.exact_request_active(op.ticket) !== 1) {
              throw Object.assign(new Error("surface request retired"), { kind: 4 });
            }
            let bytes;
            if (op.mode === "restore") {
              if ((op.body ?? "").length > HOST_WORK_BASE64) {
                throw Object.assign(new Error("surface restore exceeds 16 MiB"), { kind: 2 });
              }
              bytes = Uint8Array.from(atob(op.body ?? ""), (c) => c.charCodeAt(0));
            }
            return exact.gpu.surfaceWork(op.name, op.mode, bytes, () => requestIncarnation === host.incarnation && host.wasm.exact_request_active(op.ticket) === 1);
          }).then((bytes) => safelyFulfill(requestIncarnation, op.ticket, op.mode === "capture" ? 6 : 7, 0, "", bytes ?? new Uint8Array())).catch((error) => safelyFulfill(requestIncarnation, op.ticket, error.kind ?? 3, 0, "", encoder.encode(String(error.message ?? error))));
          track(p, op.ticket);
          break;
        }
        case "request": {
          // A safe read the runner lets go of is aborted after the commit that
          // forgot it (`letGo`), as the native executor does: a write that
          // was sent was sent, and runs on, uncounted (LLP 1016 D5).
          const requestIncarnation = host.incarnation, controller = new AbortController(), started = performance.now();
          let p, first, messages = 0, batch = [];
          const opened = new Promise((r) => {
            first = r;
          });
          const requestHost = {
            grantSet: host.grantSet,
            loadPageNative,
            moduleLoader: host.moduleLoader,
            localAssetURL,
            controllers,
            controller,
            bodyFile: pageFile("requestBody"),
            bodyTarget: pageFile("bodyTarget"),
            // `exactBodyFrom` and `exactBodyTo` (LLP 1108 D6 R2): the page's store
            active: () => requestIncarnation === host.incarnation,
            // A stream's message (LLP 1016.000): after its first, the stream is open, not in flight, so `clock settle`
            // stops waiting on it (D5), an upload only at its reply — what is counted ends there, so a wait already racing it wakes (LLP 1069.004).
            // Messages that arrive together (`more`) are one delivery, one commit (D4; host.rs `outcome_from`).
            message: (m) => {
              if (messages++ === 0) {
                first();
              }
              batch.push(m);
              if (m.more) {
                return;
              }
              const data = batch.map((m) => encoder.encode(m.data)), body = new Uint8Array(data.reduce((n, d) => n + d.length, 0));
              data.reduce((at, d) => (body.set(d, at), at + d.length), 0);
              const head = batch.map((m, i) => `event: ${m.event}\nid: ${m.id}\ncoalesced: ${m.coalesced}\nprotocol: ${m.protocol ?? ""}${batch.length > 1 ? `\nlength: ${data[i].length}` : ""}`).join("\n\n");
              batch = [];
              safelyFulfill(requestIncarnation, op.ticket, 8, 0, head, body);
            }
          };
          controllers.add(controller);
          if (op.longPoll) {
            controller.replied = () => {
              first();
              if (!heldAsks.has(op.ticket)) {
                const entry = {
                  target: op.target,
                  owner: requestIncarnation
                };
                heldAsks.set(op.ticket, entry);
                p.finally(() => {
                  if (heldAsks.get(op.ticket) === entry) {
                    heldAsks.delete(op.ticket);
                  }
                });
              }
            };
          }
          // a long poll's reply of any kind (LLP 1016.000 D8 on D9): out of the clocks' count, held and named
          if (op.url !== "exact-native:" && (/^(GET|HEAD)$/i.test(op.method) || op.upload)) {
            forgettable.set(controller, op.ticket);
          }
          // an upload let go is aborted, as natively (LLP 1016.000 D10)
          if (op.url !== "exact-native:") {
            byTicket.set(op.ticket, controller);
          }
          p = httpHelpers().then(({ request }) => request(op, requestHost)).then((r) => safelyFulfill(requestIncarnation, op.ticket, r.kind, r.status, r.headers, r.body, performance.now() - started)).catch((error) => safelyFulfill(requestIncarnation, op.ticket, 1, 0, "", encoder.encode(String(error)), performance.now() - started));
          if (op.held) {
            const entry = {
              target: op.target,
              owner: requestIncarnation
            };
            heldAsks.set(op.ticket, entry);
            p.finally(() => {
              if (heldAsks.get(op.ticket) === entry) {
                heldAsks.delete(op.ticket);
              }
            });
          } else {
            const counted = op.upload ? p : Promise.race([p, opened]);
            if (!/^(GET|HEAD)$/i.test(op.method) && /^https?:/i.test(op.url)) {
              counted.write = `${op.method} ${op.url}`;
            }
            track(counted, op.ticket);
          }
          p.finally(() => {
            forgettable.delete(controller);
            controllers.delete(controller);
            if (byTicket.get(op.ticket) === controller) {
              byTicket.delete(op.ticket);
            }
          });
          // a reply the server holds (`exactHeld`, LLP 1016.000 D9) is in flight, but no clock waits on it: listed, not tracked
          break;
        }
        case "command": {
          if (op.name === "relaunch") {
            location.reload();
            break;
          }
          // A capability an action called (LLP 1005 §3). `setScheme` is the
          // document's colour scheme, on the root's own style. `system` removes
          // it: the page's sheet says `light dark` (index.html), or a dev pin's
          // rule its scheme, the system's beneath the app's (LLP 1122 D4).
          // `light`/`dark` are the property's own values.
          if (op.name === "setScheme") {
            const s = String(op.args[0] ?? "");
            document.documentElement.style.colorScheme = s === "system" ? "" : s;
          } else if (op.name === "haptic") {
            navigator.vibrate?.(op.args?.[0] === "selection" ? 5 : 12);
          } else if (op.name === "setRootFontSize") {
            appRootFontSize(op.args?.[0], log);
          } else if (op.name === "scrollToFragment") {
            // LLP 1077 D14; LLP 1069.000 D3: the runner laid out at the size, the document's root takes it
            const turn = host.fragmentTurn, [visit, fragment = "", activation] = op.args ?? [];
            (host.fragmentGlue ??= loadAfterPaint("./fragment.js", "fragment")).then((f) => {
              if (turn === host.fragmentTurn && f.shows(visit, fragment)) {
                f.scroll(root, visit, fragment, log, activation);
              }
            });
          } else if (op.name === "focus" || op.name === "selectText" || op.name === "removeAllRanges" || op.name === "setSelectionRange" || op.name === "setRangeText" || op.name === "blur" || op.name === "scrollIntoView") {
            /* only while its visit still shows that fragment, and no scrollIntoView of the app's came since (LLP 1153 D5) */
            focusCommands.push({
              name: op.name,
              args: op.args
            });
            if (op.name === "scrollIntoView") {
              host.fragmentTurn++;
              globalThis.exact?.fragment?.cancel();
            }
          } else if ([
            "showPopover",
            "hidePopover",
            "togglePopover"
          ].includes(op.name)) {
            focusCommands.push({
              name: op.name,
              args: op.args
            });
          } else if (op.name === "preventDefault") {
            // an element's scrollIntoView (a list row's is the runner's), which a fragment's scroll held or yet to come gives way to
            host.keyEvent?.preventDefault();
            if (host.keyEvent?.type === "beforeunload") {
              host.keyEvent.returnValue = "";
            }
            if (!host.keyEvent && host.sending === host.pointerDepth && host.pointerEvent?.type === "pointerdown") {
              host.pointerEvent.preventDefault();
            }
          } else if (op.name === "close" && op.args?.[0] == null) {
            window.close();
          } else if (op.name === "showModal" || op.name === "close") {
            /* studio diary R17 */
            const id = op.args?.[0], el = [...views.values()].find((el) => el.id === id), close = op.name === "close";
            if (el instanceof HTMLDialogElement) {
              if (close) {
                el.close();
              } else if (!el.open) {
                el.showModal();
              } else {
                log(`showModal "${id}" ignored: dialog is already open`);
              }
            } else if (el?.popover != null && el.getAttribute("role") === "alertdialog") {
              if (close) {
                el.hidePopover();
              } else if (!el.matches(":popover-open")) {
                el.showPopover();
              }
            } else if (!close) {
              log(`showModal "${id}" refused: no dialog or alertdialog popover has that id`);
            }
          } else if (op.name === "stopPropagation") {
            // `showModal(id)`, `close(id)` from an action: the dialog's own, or an alertdialog popover's (LLP 1101.001 P5, LLP 1115 D6), as host/web-js/commands.js `modal`
            const e = host.keyEvent ?? (host.sending === host.pointerDepth ? host.pointerEvent : null);
            if (e) {
              e.exactStopped = true;
            }
          } else if (op.name === "requestPictureInPicture" || op.name === "exitPictureInPicture") {
            const why = "the wasm target does not float an image; the JS target does", el = op.name === "requestPictureInPicture" ? liveById(String(op.args?.[0] ?? "")) : null;
            log(`${op.name} refused: ${why}`);
            /* the image hears the refusal after this batch, as its pictureinpictureerror (LLP 1042 §9) */
            if (el?.dataset.view) {
              setTimeout(() => send(host.wasm.exact_dispatch(Number(el.dataset.view), 19, writeIn("pictureinpictureerror\n" + why), now())));
            }
          } else if (op.name === "requestFullscreen") {
            const el = [...views.values()].find((el) => el.id === op.args?.[0] && el.exactMedia);
            if (!el) {
              log(`requestFullscreen: refused: no video with id "${op.args?.[0]}"`);
            } else {
              el.requestFullscreen().catch((e) => log(`requestFullscreen: refused: ${e.name}`));
            }
          } else if (op.name === "postMessage") {
            // HTML's Element.requestFullscreen(); the element reports `fullscreenchange`
            const text = String(op.args?.[0] ?? ""), name = String(op.args?.[1] ?? ""), at = now(), frames = [...views.values()].filter((e) => e instanceof HTMLIFrameElement && e.isConnected && !retiredViews.has(e) && e.id === name);
            // the inverse of `message=`: text into the named surface, every one in order
            if (op.args?.[2] === "iframe") {
              const captured = frames.map((e) => [e, e.exactFrameChannel]);
              (host.frameModule ??= loadAfterPaint("./frame-messages.js", "frameMessages")).then((m) => m.postFrameMessage(text, name, views, log, captured));
              break;
            }
            postSurface(name, text, at);
          } else if (op.name === "showPicker") {
            // LLP 1069.002 D2, D9: the element's own picker, inside the press's activation; under the agent, a hold
            const el = [...views.values()].find((el) => el.id === op.args?.[0] && el.type === "file");
            if (agentMode) {
              const r = ask({
                op: "showPicker",
                id: String(op.args?.[0] ?? "")
              });
              if (r.error) {
                console.warn("exact:", r.error);
              } else {
                globalThis.exact.pickerOpened?.();
              }
            } else if (!el) {
              log(`picker: refused: no file input with id "${op.args?.[0]}"`);
            } else {
              try {
                el.showPicker();
              } catch (e) {
                log(`picker: refused: ${e.name}`);
                const p = picker().then((m) => m.cancel(Number(el.dataset.view)));
                inflight.add(p);
                p.finally(() => inflight.delete(p));
              }
            }
          } else if (op.name === "fastSeek" || op.name === "load") {
            const el = [...views.values()].find((el) => el.id === op.args?.[0]), queue = () => ((el.exactMedia ??= {
              props: {},
              handlers: []
            }).commands ??= []).push([op.name, op.args?.[1]]);
            if (!(el instanceof HTMLMediaElement)) {
              log(`${op.name} "${op.args?.[0]}" refused: ${el ? "not a video or audio" : "no live node with that id"}`);
            } else if (op.name === "load" && el["exactApp-src"]) {
              appSource(el, "src", el["exactApp-src"], queue);
            } else {
              queue();
              syncMedia(el);
            }
          } else if (op.name === "format") {
            // a media element's, by HTML's method names (podcast F8, F18): queued for media-glue.js `run`; a `load` of an `app:/` source resolves the file again first
            const owner = host.incarnation, run = () => {
              const el = [...views.values()].find((el) => el.id === op.args?.[0]);
              if (host.inputReady && host.incarnation === owner) {
                el?.exactMarkup?.format(op.args[1], op.args[2] ?? "");
              }
            };
            if (host.markupModule) {
              host.markupModule.then(run);
            } else {
              run();
            }
          } else if (op.name === "openURL") {
            let opened = false;
            const answer = op.args?.[1], asked = host.incarnation;
            if (!op.args?.length || op.args.length > 2 || typeof op.args[0] !== "string") {
              console.error("exact: openURL requires a string, then optionally an element id");
            } else {
              try {
                // `change` at the element named second hears `opened` (handed to the browser) or `failed`, after the batch
                const target = navigableURL(new URL(op.args[0]).href);
                if (!target) {
                  throw Error("unsupported external URL scheme");
                }
                window.open(target, "_blank", "noopener,noreferrer");
                opened = true;
              } catch (error) {
                console.error("exact: openURL refused", String(error));
              }
            }
            if (typeof answer === "string") {
              queueMicrotask(() => {
                const view = [...views].find(([, el]) => el.id === answer)?.[0];
                if (host.incarnation === asked && view != null) {
                  send(host.wasm.exact_dispatch(view, 1, writeIn(opened ? "opened" : "failed"), now()));
                }
              });
            }
          } else if (op.name === "copyText") {
            if (op.args?.length !== 1 || typeof op.args[0] !== "string") {
              console.error("exact: copyText requires one string");
            } else if (!navigator.clipboard?.writeText) {
              console.error("exact: copyText unavailable; a secure clipboard context is required");
            } else {
              // Start inside the input dispatch while browser user activation
              // is live. No clipboard read or focus/selection manipulation.
              const pending = navigator.clipboard.writeText(op.args[0]).catch((error) => console.error("exact: copyText failed", String(error)));
              inflight.add(pending);
              pending.finally(() => inflight.delete(pending));
            }
          } else if (op.name === "saveFile") {
            const [id, from, suggestedName, text] = op.args ?? [], r = JSON.parse(readOut(host.wasm.exact_command(writeIn(JSON.stringify({
              command: "saveFile",
              id,
              from: from ?? undefined,
              suggestedName,
              text,
              agent: agentMode
            }))))), chosen = r.present && typeof showSaveFilePicker === "function" ? showSaveFilePicker({ suggestedName: r.suggestedName }) : null;
            // LLP 1069.010 D3: the runner rules; the save picker starts inside the press's activation, else a download
            if (r.present || r.view != null) {
              chosen?.catch(() => {});
              const p = picker().then((m) => m.save(r, chosen));
              inflight.add(p);
              p.finally(() => inflight.delete(p));
            }
          } else if (/^show(OpenFile|Directory|SaveFile)Picker$/.test(op.name)) {
            // LLP 1069.010 D2: the runner rules; a browser without the picker refuses
            const [id, second] = op.args ?? [], r = JSON.parse(readOut(host.wasm.exact_command(writeIn(JSON.stringify({
              command: op.name,
              id,
              multiple: second === true,
              suggestedName: typeof second === "string" ? second : undefined,
              agent: agentMode,
              available: op.name === "showSaveFilePicker" || typeof globalThis[op.name] === "function"
            })))));
            if (r.present || r.view != null) {
              const p = documentsGlue().then((m) => m.show(r, op.name));
              inflight.add(p);
              p.finally(() => inflight.delete(p));
            }
          } else if (op.name === "requestPermission") {
            const [id, path, mode] = op.args ?? [], r = JSON.parse(readOut(host.wasm.exact_command(writeIn(JSON.stringify({
              command: op.name,
              id,
              path,
              mode
            })))));
            if (r.view != null) {
              const asked = host.incarnation, p = documentsGlue().then(() => globalThis.exact.documents.permission(r.path, r.mode, (v) => {
                if (host.incarnation === asked && views.has(r.view)) {
                  send(host.wasm.exact_dispatch(r.view, 1, writeIn(v), now()));
                }
              }));
              inflight.add(p);
              p.finally(() => inflight.delete(p));
            }
          } else if (op.name === "share") {
            // LLP 1069.010.000 D7: the runner rules; the browser asks inside the press's activation
            // LLP 1069.003: the runner rules (refused, or held for the agent, D6;
            // a picture of an element refused on the web, LLP 1125 D12); else the
            // browser's sheet, started inside the input dispatch while activation
            // is live (D4); files are read first (picker-glue.js). The outcome is a journal line (D2).
            const [title, text, url, files] = op.args ?? [], ruling = JSON.parse(readOut(host.wasm.exact_command(writeIn(JSON.stringify({
              command: "share",
              title,
              text,
              url,
              files,
              source: op.source,
              agent: agentMode,
              web: true
            })))));
            if (ruling.present && typeof navigator.share !== "function") {
              log("share: refused: unavailable");
            } else if (ruling.present && ruling.files?.length) {
              const p = picker().then((m) => m.share({
                title,
                text,
                url
              }, ruling.files)).then(log);
              inflight.add(p);
              p.finally(() => inflight.delete(p));
            } else if (ruling.present) {
              navigator.share(Object.fromEntries(Object.entries({
                title,
                text,
                url
              }).filter(([, v]) => v != null))).then(() => "share: shared", (e) => e?.name === "AbortError" ? "share: dismissed" : `share: refused: ${e?.name ?? e}`).then(log);
            }
          } else if (/^(navigate|traverseHistory|stopLoading|captureScreenshot)$/.test(op.name) || op.name === "reload" && op.args?.[0] != null) {
            frameNav().then((m) => m.command(op.name, op.args ?? [], log, [...views.values()].filter((e) => e instanceof HTMLIFrameElement && !retiredViews.has(e))));
          } else if (op.name === "openDevTools") {
            log(`openDevTools("${op.args?.[0]}") refused: a page cannot open the browser's developer tools; open them from the browser's menu`);
          } else if (/^(start|stop)Trace_EXPERIMENTAL$/.test(op.name)) {
            log(`${op.name} refused: the web host does not record traces; use the browser's performance panel`);
          } else if (op.name === "exitFullscreen") {
            /* a frame's (LLP 1020.002 D3) */
            /* a frame's inspector (D11) */
            /* the Apple hosts' */
            if (document.fullscreenElement) {
              document.exitFullscreen().catch((e) => log(`exitFullscreen: refused: ${e.name}`));
            }
          } else if (op.name === "setWindowOpacity") {
            log("setWindowOpacity refused: a page cannot change its window's opacity");
          } else if (op.name === "hideWindow") {
            log("hideWindow refused: a page cannot hide its window");
          } else if (!/^(playSound|playSounds|stopSounds|setSounds|sendMIDI|requestMIDIAccess)$/.test(op.name)) {
            console.warn(`exact: unknown command ${op.name}`);
          }
          /* LLP 1069.010 */
          // the voice table's and the MIDI queue's are the runner's own, played from its `sound` and `midi` ops (LLP 1096 D7, LLP 1125 D7)
          break;
        }
        case "destroy": {
          pieces.canvas2d({
            id: op.id,
            forget: true
          });
          arrange.destroy(op.id);
          motion.destroy(op.id);
          const el = views.get(op.id);
          if (el) {
            retiredViews.add(el);
            el.exactUnload?.abort();
            el.exactSelection?.();
            el.exactAppFrame?.dispose();
            el.exactIntersect?.();
            el.exactResize?.();
            el.exactMarkup?.destroy();
            el.exactDecorations?.destroy();
            el.exactLiveTime?.destroy();
            el.exactNative?.destroy();
            if (el instanceof HTMLMediaElement) {
              globalThis.exact.removeMedia?.(el);
              el.pause();
              el.removeAttribute("src");
              el.load();
            }
            forgetList(el);
            followScroll(el, false);
            messageFrames.delete(el);
            if (!presence.live?.keeps(el) && !retiredViews.has(el.parentElement) && !sheetLeaves(el)) {
              el.remove();
            }
          }
          views.delete(op.id);
          messageViews.delete(op.id);
          globalThis.exact.gpu?.destroy(op.id);
          dropSurface(op.id);
          break;
        }
        case "roots": {
          const roots = [];
          for (const id of op.ids) {
            const el = viewFor("roots", id);
            if (el) {
              roots.push(el);
            }
          }
          if (roots.length !== root.children.length || roots.some((el, i) => root.children[i] !== el)) {
            root.replaceChildren(...roots);
          }
          syncViewportFit();
          break;
        }
        case "at": {
          // The ops that follow were committed at this clock (a timer's due
          // time inside one advance): what the ops before it started belongs
          // to the clock so far; the animations are then seeked to this
          // instant before the next ops see them (LLP 1012; LLP 1002 D3).
          if (agentMode) {
            register(host.agentClock);
            host.agentClock = Math.max(host.agentClock, op.ms);
            seek(host.agentClock);
          }
          break;
        }
      }
    } catch (e) {
      // A malformed op is isolated: the runner already committed the whole
      // batch, so leaving the DOM at a prefix would be the worst outcome.
      console.error(`exact: ${String(op?.op ?? "unknown")} op failed`, e);
    }
  }
  if (agentKeepsStore) {
    pendingStore.flush();
  }
  navigation.project(root, log);
  refreshSymbols();
  for (const snapshot of collectionOp?.items ?? []) {
    followScroll(views.get(snapshot.view), false);
  }
  // A scroll the batch asks for wins over the end-follow, and owns the
  // followed scrollers it moves until it settles (LLP 1070.000 §15).
  const reveals = [];
  for (const command of focusCommands) {
    const el = command.name === "scrollIntoView" ? liveById(command.args?.[0], root) : null;
    if (el) {
      reveals.push([ownFollowed(el), command.args?.[3] ?? "auto"]);
    }
  }
  for (const [el, offsets] of pendingScrolls) {
    if (followedScrolls.has(el) && Object.entries(offsets).some(([name, offset]) => el[name] !== offset)) {
      ownFollowed(el);
    }
  }
  for (const s of followedScrolls.values()) {
    settleFollow(s);
  }
  const jumps = [];
  // a collection builds a jump's rows, then moves, on its own axis (LLP 1050.000 §6)
  for (const [el, offsets] of pendingScrolls) {
    if (el.isConnected) {
      // Mirroring the current offset must not restart snapping or cancel a pan.
      // `scroll-behavior: smooth` (the row's CSS) animates the assignment itself (CSSOM View §7),
      // once; the agent's clock cannot seek that animation, so under it the scroll lands at once.
      for (const [name, offset] of Object.entries(offsets)) {
        if (el[name] !== offset) {
          if ((name === "scrollTop" || name === "scrollLeft") && lists.has(el)) {
            jumps.push([
              Number(el.dataset.view),
              offset,
              name
            ]);
          } else if (agentMode && el.style.scrollBehavior === "smooth") {
            el.scrollTo({
              [name === "scrollTop" ? "top" : "left"]: offset,
              behavior: "instant"
            });
          } else {
            el[name] = offset;
          }
        }
      }
      const s = followedScrolls.get(el);
      if (s?.owned && !agentMode && el.style.scrollBehavior === "smooth") {
        // settles at its `scrollend`
      } else if (s?.owned) {
        releaseFollowed(s);
      } else if (s) {
        rememberScroll(s);
      }
    }
  }
  pendingScrolls.clear();
  if (collectionOp) {
    collections.commit(collectionOp.items);
  } else {
    collections.restyled();
  }
  for (const [view, offset, name] of jumps) {
    collections.jump(view, offset, name);
  }
  host.listSelection?.after();
  syncLists();
  // Focusing can dispatch an action: every node/value of the batch is committed before its focus handler runs.
  for (const command of focusCommands) {
    if ([
      "showPopover",
      "hidePopover",
      "togglePopover"
    ].includes(command.name)) {
      popoverCommand(command.name, command.args?.[0], command.args?.[1], log);
    } else {
      runFocusCommands([command], {
        root,
        ready: host.inputReady,
        inertAncestor,
        log
      });
    }
  }
  // An instant reveal has landed; a smooth one settles at its `scrollend`.
  for (const [owned, behavior] of reveals) {
    for (const s of owned) {
      if (behavior === "instant" || behavior === "auto" && getComputedStyle(s.el).scrollBehavior !== "smooth") {
        releaseFollowed(s);
      }
    }
  }
  focusAutofocus();
  host.inputHandlers?.arrive();
  // then the focus follows the route, unless either set it (input-glue.js; LLP 1115.000 D4.2)
  positionContexts();
  presence.live?.after(batch, views);
  markScrollDocument();
  return batch.timers;
}
const heldLinks = new WeakMap();
export function applyBatch(batch) {
  for (const line of batch.alerts?.splice(0) ?? []) {
    console.error("exact:", line);
  }
  const link = heldLinks.get(batch) ?? navigation.holdLink();
  if (host.page?.hold(batch) || presence.hold(batch)) {
    if (link) heldLinks.set(batch, link);
    return {
      timers: batch.timers,
      batch
    };
  }
  heldLinks.delete(batch);
  host.textflow?.beforeBatch(batch);
  const began = host.frameSampler ? performance.now() : 0;
  // what applying it cost, for the next frame's record (LLP 1079 D3)
  globalThis.exact.applyDepth = (globalThis.exact.applyDepth ?? 0) + 1;
  try {
    const timers = link ? navigation.withLink(link, () => apply(batch)) : apply(batch);
    letGo();
    refitTexts();
    if (host.wakeLock) {
      holdWake();
    }
    if (host.orientationLock) {
      holdOrientation();
    }
    motion.commit();
    arrange.commit();
    if (agentMode) {
      // What the ops since the last marker started belongs to that marker's
      // time — register before the clock moves on to where the batch landed.
      register(host.agentClock);
      const moved = batch.clock != null && batch.clock > host.agentClock;
      if (moved) {
        host.agentClock = batch.clock;
      }
      // A same-clock seek registers the batch's animations; its timelines op, not the agent, owns post-commit reconciliation.
      host.followOnSeek = moved;
      try {
        seek(host.agentClock);
      } finally {
        host.followOnSeek = true;
      }
      if (host.timelinesMoved) {
        motion.followTimelines();
      }
      arrange.commit();
    } else {
      clocks.sync();
      if (host.timelinesMoved) {
        motion.followTimelines();
      }
    }
    // synced animations join their clocks (LLP 1055.002); a boot or commit while drag timelines are bound (LLP 1057.003 D4) follows them; the agent's register and seek do both
    host.timelinesMoved = false;
    flowBatch(batch);
    return {
      timers,
      batch
    };
  } finally {
    const outer = --globalThis.exact.applyDepth === 0;
    if (outer) {
      // After a commit (its `at` op), a held press paints again over the boundaries as they are now (input-glue.js
      // `repaintPress`), as modes.js does after each JS-target commit: a boundary's `data-exact-from` may have moved.
      if (batch.ops?.some((op) => op.op === "at")) globalThis.exact.repaintPress?.();
      globalThis.exact.gpu?.drainRecords();
      globalThis.exact.gpu?.layout?.();
      globalThis.exact.committed?.();
    }
    host.frameSampler?.batch(batch.seq, outer ? performance.now() - began : 0);
  }
}
export function send(len) {
  return sendBatch(len).timers;
}
export function sendBatch(len) {
  host.sending++;
  try {
    return applyBatch(JSON.parse(readOut(len)));
  } finally {
    host.sending--;
  }
}
