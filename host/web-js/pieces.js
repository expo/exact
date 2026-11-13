// The pieces a plan loads after first paint, each only where the plan uses it: a canvas's GPU or Canvas 2D surface,
// native module views, presence, motion, text flow, page geometry, the Markdown editor and the web host's input piece.
import { rt as Link } from './rt-link.js';
import { releaseAsset as rel } from './elements.js';
import { liveById, sheetLeaves, inertAncestor } from './navigation.js';
import { startDecoration, decorationPiece } from './editor.js';
import { Owner, onEnd, effect, say, inflight, data, journal, After, Before, Hosts, Probes, Media } from './rt.js';
import { clock, advance } from './clock.js';
import { Views, Ids, viewId, Hooks } from './tree.js';
import { OnHooks } from './events.js';
import { start } from './mount.js';
/** A canvas's surface (LLP 1009 D2): its inputs, evaluated as the runner
 * evaluates them (records as arrays), to the app's GPU module — the web
 * host's own `gpu-glue.js` over `gpu.js`, fetched after the first painted
 * frame, only when a canvas is on the page (a loaded capability). */
let Gpu = null;
export function gs(e, name, values) {
  const id = viewId(e);
  e.exactSurfaceName = name;
  onEnd(() => {
    try {
      globalThis.exact?.gpu?.destroy(id);
    } catch (err) {
      say(`gpu: ${err.message}`);
    }
  });
  effect(() => {
    const v = values();
    const x = (globalThis.exact ??= {});
    // Published after the commit applied, as the runner publishes surface
    // inputs: the canvas is in the document by then.
    if (x.gpu)
      return queueMicrotask(() => {
        try {
          e.isConnected && x.gpu.surface(id, name, v);
        } catch (err) {
          say(`gpu: ${err.message}`);
        }
      });
    const pending = (x.pendingSurfaces ??= []),
      queued = pending.find(p => p.id === id);
    if (queued) queued.values = v;
    else pending.push({ id, name, values: v, generation: 0 });
    if (!Gpu && typeof requestAnimationFrame === 'function' && !globalThis.__exactRender) {
      x.views = Views;
      x.generation = 0;
      x.devAssets = null;
      x.root = document.getElementById('exact-root');
      // a release's asset path, as the wasm glue's `localAssetURL` (LLP 1098 D6)
      x.assetURL = v => rel('src', v);
      // The GPU glue's name NUL JSON wire reaches the reserved readers in facts.js.
      let written = '';
      x.writeIn ??= t => {
        written = t;
        return 0;
      };
      (x.wasm ??= {}).exact_surface_record ??= () => {
        const at = written.indexOf('\0');
        x.surfaceRecord?.(at < 0 ? written : written.slice(0, at), at < 0 ? null : written.slice(at + 1));
        return 0;
      };
      x.wasm.exact_canvas_performance_requested ??= () => (x.canvasPerformanceRequested?.(written) ? 1 : 0);
      x.wasm.exact_canvas_performance_record ??= () => {
        const at = written.indexOf('\0');
        x.canvasPerformanceRecord?.(at < 0 ? written : written.slice(0, at), at < 0 ? null : written.slice(at + 1));
        return 0;
      };
      x.send ??= () => {};
      // A surface's `message` to its canvas's `message=` handler (gpu-glue.js `messages`), as the wasm glue's `message`
      // dispatches it.
      x.message ??= (el, text) =>
        el?.dispatchEvent(new CustomEvent('exact-native', { detail: { kind: 'message', value: text } }));
      if (clock.agent) x.now = () => clock.now;
      inflight.n++;
      Gpu = new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))
        .then(() => import(new URL('gpu-glue.js', document.baseURI).href))
        .then(() => x.gpu?.settled?.())
        .catch(err => say(`gpu: ${err.message}`))
        .finally(() => inflight.n--);
    }
  });
}
/** A Canvas 2D surface (LLP 1056): its arguments, drawn by the data
 * module and replayed by the web host's own glue, both in `canvas2d.js`,
 * fetched two frames after the first 2D canvas mounts (a loaded
 * capability); `types` are the arguments' declared types where known. */
let Canvas2d = null;
export function c2(e, name, values, types, names) {
  const c = { e, name, types, names, mounted: clock.now, args: null };
  onEnd(() => Canvas2d?.then(m => m?.gone(c)));
  effect(() => {
    c.args = values();
    if (typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return;
    if (!Canvas2d) {
      inflight.n++;
      Canvas2d = new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))
        .then(() => import('./canvas2d.js'))
        .then(m => m.engine({ data, clock, inflight, journal, advance, After, views: Views, viewId, wall: Link.wall }))
        .catch(err => say(`canvas2d: ${err.message}`))
        .finally(() => inflight.n--);
    }
    // After the commit applied, as the runner publishes surface inputs, and before the canvas module's restyle
    // (`After`).
    Canvas2d.then(m => m?.args(c));
  });
}
/** When native code may load (LLP 1024 D7): after the browser's first paint entry, or two frames and 250 ms where it
 * records none (glue.js `afterNativePaint`). */
let Painted = null,
  Native = null;
export const painted = () =>
  (Painted ??= new Promise(r => {
    let o;
    const done = () => {
      o?.disconnect();
      r();
    };
    try {
      o = new PerformanceObserver(() => requestAnimationFrame(done));
      o.observe({ type: 'paint', buffered: true });
    } catch {}
    requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 250)));
  }));
/** A native module's element (LLP 1024 D3): the real custom element, empty until the web host's adapter (`native.js`)
 * and the app's module artifact load (`painted`); the module renders into it, its events reaching the handlers as
 * `exact-native` events (`on`). */
export function nm(e) {
  if (typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return;
  const id = viewId(e),
    st = (e.exactNative = {
      id,
      name: e.localName,
      state: 'loading',
      status() {
        return { name: this.name, state: this.state, ...(this.error ? { error: this.error } : {}) };
      },
    });
  onEnd(() => {
    st.destroyed = true;
    Native?.then(
      h => h.destroy(e),
      () => {},
    );
  });
  // In flight until the module is attached and its first events are in (`clock settle` waits for them).
  inflight.n++;
  (Native ??= painted()
    .then(() => import('./native.js'))
    .then(m => m.viewHost(say, () => data.appId)))
    .then(
      h => h.attach(e),
      err => {
        st.state = 'unavailable';
        st.error = String(err?.message ?? err);
        say(`native ${st.name} #${id}: unavailable: ${st.error}`);
      },
    )
    .finally(() => setTimeout(() => inflight.n--));
}
// ---------------------------------------------------------------- presence (LLP 1063)
// `-exact-exit-animation` and `-exact-layout-transition`: the web host's own presence-glue.js, fetched after the first
// painted frame by a plan with either row (its node calls `pr`; a bound layout transition's, `lt`; a bound exit's,
// `px`), plays both. Each commit it measures the views that declare a layout transition, or may gain one (a bound
// one's, as the wasm host's style op), before the tree changes and plays back each that moved after; a region's removed
// root that declares an exit stays, out of flow at its last box, until its animations end, where it was: `clear` passes
// over it. Until the piece is here, rows jump and leave at once.
export let Pres = null,
  Presence = null,
  Present = null,
  Leave = null,
  // shared.js, loaded with presence-glue.js, runs the commits that hand on a shared element's name in a view transition
  // (LLP 1013.000 D7)
  Sh = null;
/** The after-paint pieces on their way (the agent waits for them before an
 * operation, as glue.js's `agentSettled` waits for `pieces.pending()`). */
export const pieces = () =>
  Promise.all(
    [Motion, Inputs, Presence, Flow, Native, Media?.mediaPiece(), decorationPiece(), OnHooks.loading].filter(Boolean),
  ).then(
    () => {},
    () => {},
  );
const EXIT = '--exact-exit-animation';
// An element's exit: a bound one's value from the commit's start (presence-bindings.js `px`), else its style's.
export const exitOf = n => (n.$exit ?? n.style.getPropertyValue(EXIT)).trim();
/** A virtualized list's row wrapper `el` leaves with the exit its row's `root` declares (list.js): whether it stays,
 * leaving, for presence-glue.js to remove. */
export function exitView(el, root) {
  const css = root && exitOf(root);
  if (!Pres || !css) return false;
  Pres.exit(el, css);
  return exiting(el);
}
const Created = [];
export function pr(e) {
  Created.push(e);
  if (Presence || typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return;
  inflight.n++;
  Presence = new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))
    .then(() => {
      globalThis.exact ??= {};
      return Promise.all([
        import('./presence-glue.js'),
        import('./shared.js').then(m => {
          Sh = m;
        }),
      ]);
    })
    .then(() => {
      const x = globalThis.exact;
      Pres = x.presenceLive = x.presence(document.getElementById('exact-root'));
      Created.length = 0;
      // After a commit's tree: which views declare the row now (the created
      // ones), and every tracked one that moved plays back from where it was.
      Present = () =>
        Pres.after(
          {
            ops: [
              ...Created.splice(0)
                .filter(e => e.isConnected)
                .map(e => ({ op: 'create', id: viewId(e) })),
              ...(Probes?.restyled() ?? []),
            ],
          },
          Views,
        );
      // A removed region node that declares an exit leaves with it (the
      // kernel's `exit`: a destroyed root whose parent stays).
      Leave = n => {
        const css = n.nodeType === 1 && exitOf(n);
        if (css) Pres.exit(n, css);
        if (!exiting(n) && !sheetLeaves(n)) n.remove();
      };
    })
    .catch(err => say(`presence: unavailable; motion skipped: ${err.message}`))
    .finally(() => inflight.n--);
}
/** The motion piece (motion.js over `motion.wasm` and the web host's
 * `motion-glue.js`), fetched after the first painted frame by a plan that
 * uses motion; `f(m)` runs at once when it is here, else once it is. */
let Motion = null,
  Mo = null;
function motion(f) {
  if (Mo) return f(Mo);
  if (typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return;
  if (!Motion) {
    inflight.n++;
    Motion = new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))
      .then(() => import('./motion.js'))
      .then(m =>
        m.engine({ clock, wall: () => performance.now() - start, views: Views, viewId, hooks: Hooks, say, inflight }),
      )
      .then(m => {
        Mo = m;
        After.push(() => Mo.flush());
      })
      .catch(err => say(`motion: ${err.message}`))
      .finally(() => inflight.n--);
  }
  Motion.then(() => Mo && f(Mo));
}
/** A node the motion engine follows (the kernel's motion seam): `f` gives
 * its `translate`, `scale`, `rotate`, `opacity`, `transition` and release velocity as the
 * plan binds them, told the engine at every commit that changes them. */
export function mo(e, f) {
  const id = viewId(e);
  onEnd(() => motion(m => m.gone(id)));
  effect(() => {
    const v = f();
    motion(m => m.observe(id, v));
  });
}
/** `swiperight` and `swipeleft`: the motion piece's (motion-glue.js `attachSwipe`, one
 * for both); the action runs when the hold passes the knee that way (motion.js `action`). */
export function onSwipe(e, f) {
  e.$swipe = f;
  motion(m => m.swipe(e));
}
export function onSwipeLeft(e, f) {
  e.$swipeleft = f;
  motion(m => m.swipe(e));
}
/** A height drag's owner (the node a handle's `heightDragFor` names): its
 * numeric height and `transition`, told the motion engine as they change. */
export function mh(e, f) {
  const id = viewId(e);
  onEnd(() => motion(m => m.gone(id)));
  effect(() => {
    const v = f();
    motion(m => m.height(id, v));
  });
}
/** `heightrelease`: motion-glue's height drag on the handle's named ancestor. */
export function onHeight(e, f) {
  e.$heightrelease = f;
  onEnd(() => motion(m => m.gone(viewId(e))));
  motion(m => m.heightDrag(e));
}
/** `transformgeometry`: the pair's geometry, the page's observation. */
export function onTGeom(e, f) {
  e.$tgeom = f;
}
/** `transformrelease`: motion-glue's transform drag on the handle, holding
 * its named ancestor's translate and scale inside its parent clip (LLP 1057.001 §4). */
export function onTRelease(e, f) {
  e.$trelease = f;
  onEnd(() => motion(m => m.gone(viewId(e))));
  motion(m => m.transformDrag(e));
}
/** A `reorderFor` grip on the strict ancestor it names, and a
 * virtualized list's `reorderdrop`: Arrange, motion-glue's reorder drag
 * over the list's preview (list.js) and the motion piece (arrange.js). The
 * grip names its view (`data-view`): the list's browser half pins the row
 * a contact on it holds by that name (collection-glue.js `liveView`). */
export function onReorder(e) {
  const id = viewId(e);
  e.dataset.view = id;
  onEnd(() => motion(m => m.gone(id)));
  motion(m => m.reorderHandle(e));
}
export function onDrop(e, f) {
  e.$reorderdrop = f;
  motion(() => {});
}
/** `pan`: the web host's input piece's (input-glue.js), after first paint. */
export function onPan(e, f) {
  e.$pan = f;
  e.exactHandlers ??= e.dataset.exactOn.split(' ');
  const id = viewId(e);
  let p;
  input();
  e.addEventListener('pointerdown', ev => (p ??= Input?.pan(e, id, (t, g) => e.addEventListener(t, g)))?.(ev));
}
/** `panrelease`: its velocity is the motion piece's tracker (LLP 1057 §10.6). */
export function onPanRelease(e, f) {
  e.$panrelease = f;
  motion(() => {});
}
/** A node's `wrap-flow` (LLP 1043.000): an absolutely positioned `both`
 * is an exclusion the text around it flows past, laid out by the text flow
 * piece (flow.js over the web host's `textflow-glue.js`), fetched after
 * first paint by a plan with the row. */
let Flow = null,
  Fl = null;
const Wraps = new Set();
export function wf(e, f) {
  Wraps.add(e);
  onEnd(() => Wraps.delete(e));
  effect(() => {
    e.$wrap = f();
  });
  if (Flow || typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return;
  inflight.n++;
  Flow = new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))
    .then(() => import('./flow.js'))
    .then(m => m.flow({ views: Views, viewId, wraps: Wraps, clock, wall: () => performance.now() - start, say }))
    .then(m => {
      Fl = m;
      Before.push(() => Fl.before());
      After.push(() => Fl.after());
      (globalThis.exact ??= {}).flowSettle = () => Fl.settle();
      Fl.after();
    })
    .catch(err => say(`text flow: ${err.message}`))
    .finally(() => inflight.n--);
}
/** Page geometry (LLP 1051.000), fetched after first paint by `geo`; unavailable before loading.
 * Answers use the runner's Geometry: x, y, width, height, provisional, unavailable. */
let Geo = null,
  Geometry = null;
// its load once `geo` asked (else null): a first element size (observers.js) waits for it
export const geoLoading = () => Geometry;
export function geo() {
  if (Geometry || typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return;
  inflight.n++;
  Geometry = new Promise(r => requestAnimationFrame(() => r()))
    .then(() => import('./geometry-glue.js'))
    .then(() => {
      Geo = globalThis.exact.geometry(document.getElementById('exact-root'));
    })
    .catch(err => say(`geometry: ${err.message}`))
    .finally(() => inflight.n--);
}
function geoRead(op, id) {
  const out = new Float64Array(4),
    bits = Geo ? Geo.read(op, liveById(id), out) : 0;
  return bits & 1 ? [out[0], out[1], out[2], out[3], !!(bits & 2), false] : [0, 0, 0, 0, false, true];
}
export const x_frame = id => geoRead(0, id),
  x_measure = id => geoRead(1, id);
export const x_rangeFrame = (id, start, end) => {
  const out = new Float64Array(4),
    bits = Geo ? Geo.range(liveById(id), start, end, out) : 0;
  return bits & 1 ? [...out, !!(bits & 2), false] : [0, 0, 0, 0, false, true];
};
export const decorate = e => startDecoration(e, { own: Owner, onEnd, inflight, say });
/** `elementFromPoint(x,y)` (LLP 1094 D10): nearest front-most untransformed id at a viewport point; none before
 * loading. `elementsFromPoint` returns every one. */
export const x_elementFromPoint = (x, y) => {
  const root = document.getElementById('exact-root'),
    n = Geo?.point(x, y)?.closest('[id]:not([id=""])');
  return (n && n !== root && root.contains(n) && n.id) || null;
};
export const x_elementsFromPoint = (x, y) => Geo?.namedPoints(x, y) ?? [];
/** A Markdown field (LLP 1045 D5): markup-editor.js over its wasm replaces
 * the textarea on demand. Runtime writes proxy to the editor; its events
 * forward to the detached textarea's handlers. */
let Editor = null;
export function mde(e) {
  if (typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return;
  inflight.n++;
  if (!Editor) {
    Editor = import('./markup-editor.js').then(() => globalThis.exact.installMarkupEditor);
    Hosts.format = (id, name, arg) => Editor.then(() => liveById(id)?.exactMarkup?.format(name, arg ?? ''));
  }
  Editor.then(install => {
    if (!e.isConnected) return;
    const id = viewId(e);
    install(e, {
      live: n => n.isConnected,
      replace(n) {
        Ids.set(n, id);
        Views.set(id, n);
        Object.defineProperties(e, {
          value: {
            configurable: true,
            get: () => n.value,
            set: v => {
              n.value = v;
            },
          },
          ...Object.fromEntries(
            ['selectionStart', 'selectionEnd', 'selectionDirection'].map(k => [
              k,
              { configurable: true, get: () => n[k] },
            ]),
          ),
        });
        for (const t of [
          'input',
          'beforeinput',
          'selectionchange',
          'change',
          'focus',
          'blur',
          'keydown',
          'pointerenter',
          'pointerleave',
          'click',
          'atompress',
        ])
          n.addEventListener(
            t,
            ev => {
              const forwarded = new ev.constructor(t, ev);
              forwarded.$stopped = ev.$stopped;
              if (ev.defaultPrevented) forwarded.preventDefault();
              if (!e.dispatchEvent(forwarded)) ev.preventDefault();
              if (forwarded.$stopped) ev.$stopped = true;
            },
            // before the editor applies its own key default
            t === 'keydown',
          );
        new MutationObserver(ms => {
          for (const m of ms) {
            const v = e.getAttribute(m.attributeName);
            v == null ? n.removeAttribute(m.attributeName) : n.setAttribute(m.attributeName, v);
          }
          n.exactMarkup?.sync();
        }).observe(e, { attributes: true });
      },
      select: (n, p) => {
        const [formats, mixed, unavailable, atom, ...link] = p.split('\n');
        e.$select?.([formats, mixed === '1', link.join('\n'), unavailable, atom ?? '']);
      },
    });
  })
    .catch(err => say(`markup editor: ${err.message}`))
    .finally(() => inflight.n--);
}
/** Press feedback (LLP 1061): the web host's input piece shows it, from
 * the node's own `--exact-press` (css.rs). */
export const pressFeedback = () => input();
/** The web host's input piece (input-glue.js), after first paint: pans. */
export let Input = null,
  Inputs = null;
export function input() {
  if (Inputs || typeof requestAnimationFrame !== 'function' || globalThis.__exactRender) return Inputs;
  inflight.n++;
  // Its link handler asks the wasm's route table; this runtime routes its own.
  const x = (globalThis.exact ??= {});
  (x.wasm ??= {}).exact_route_match ??= () => 0;
  const to = (id, p, k) => Views.get(id)?.[k]?.(...p.split(',').map(Number));
  // after paint, or a second on where frames are held (a frame scrolled out of view)
  return (Inputs = new Promise(r => {
    requestAnimationFrame(() => setTimeout(r));
    setTimeout(r, 1000);
  })
    .then(() => import('./input-glue.js'))
    .then(m => {
      Input = m.createInputHandlers({
        root: document.getElementById('exact-root'),
        views: Views,
        retiredViews: new WeakSet(),
        ready: () => true,
        inertAncestor,
        agentMode: clock.agent,
        pick: el => x.files?.activate(el),
        dispatch: (id, p) => to(id, p, '$pan'),
        release: (id, p) => to(id, p, '$panrelease'),
        velocity: { sample: (...a) => Mo?.pan.sample(...a), velocity: (...a) => Mo?.pan.velocity(...a) },
        log: say,
      });
      // the stack as it stands, for the focus to follow (LLP 1115.000 D4.2)
      Input.arrive();
    })
    .catch(err => say(`input: ${err.message}`))
    .finally(() => inflight.n--));
}
// A view leaving with its exit animation stays where it was until it ends
// (presence-glue.js removes it): never moved, since moving cancels a CSS animation.
export const exiting = n => n.nodeType === 1 && n.hasAttribute('data-exiting');
