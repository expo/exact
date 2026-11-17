// An iframe on the JS target: the guest origin a `message` from it must match (LLP 1020 D2), the sources it shows
// (navigation.js `frameURL`, LLP 1020 §10) and an app file a page of the app's origin is given (LLP 1020.001 D7).
// Imported by an app's module only when its plan has an iframe (exact-web-js `emit.rs`, LLP 1047.002 D1); tree.js
// reaches it through `Hooks.frame` and the `src` and `sandbox` prop hooks.
import { commitGuestOrigin, pageGivenFiles, frameURL } from './navigation.js';
import { Hooks, PropHooks, onEnd, inflight, data } from './rt.js';

const shown = (e, v) => frameURL(v, e.getAttribute('sandbox'));
const write = (e, name, v) => {
  if (v == null) e.removeAttribute(name);
  else if (e.getAttribute(name) !== v) e.setAttribute(name, v);
};

/** Whether `v` is an app file the frame loads itself (picker-glue.js), in place of its `src`. */
function frame(e, v) {
  commitGuestOrigin(e);
  // an app file, or a page of the app's origin whose query may name one (LLP 1020.001 D7; picker-glue.js decides)
  const app = !!v?.startsWith('app:/') || pageGivenFiles(v ?? '');
  if (app) e.setAttribute('data-app-src', v);
  else e.removeAttribute('data-app-src');
  if (globalThis.__exactRender) return app;
  if (!e.$frame) {
    e.$frame = true;
    e.addEventListener(
      'load',
      ev => {
        // Else the src that loaded, for the agent's tree, which loads after boot and cannot read another origin's
        // readiness.
        if (e.exactAppFrame && !e.exactAppFrame.url) ev.stopImmediatePropagation();
        else e.exactLoaded = e.getAttribute('src');
      },
      true,
    );
    onEnd(() => e.exactAppFrame?.dispose());
  }
  if (e.exactAppFrame?.path === v) return true;
  e.exactAppFrame?.dispose();
  e.exactAppFrame = null;
  if (!app) return false;
  const s = (e.exactAppFrame = {
    path: v,
    live: true,
    url: null,
    dispose() {
      this.live = false;
      if (this.url) URL.revokeObjectURL(this.url);
    },
  });
  e.removeAttribute('src');
  inflight.n++;
  globalThis.exact ??= {};
  import(new URL('./picker-glue.js', import.meta.url).href)
    .then(m =>
      m.appFrame(
        e,
        s,
        data.appId,
        () => commitGuestOrigin(e),
        u => e.setAttribute('src', shown(e, u) ? u : 'about:blank'),
      ),
    )
    .finally(() => inflight.n--);
  return true;
}
Hooks.frame = frame;

// A source the policy refuses shows about:blank, and the frame keeps it (`$src`) for its sandbox to decide again.
const src = PropHooks.src;
PropHooks.src = (e, v) => {
  if (e.localName !== 'iframe') return src ? src(e, v) : false;
  e.$src = v;
  if (frame(e, v)) return true;
  write(e, 'src', v != null && !shown(e, v) ? 'about:blank' : v);
  commitGuestOrigin(e);
  return true;
};
// A new sandbox takes effect on navigation, even to the same source: a literal source is in `src`, a refused `data:`
// source in `$src`.
PropHooks.sandbox = (e, v) => {
  if (e.localName !== 'iframe') return false;
  const changed = e.getAttribute('sandbox') !== v;
  write(e, 'sandbox', v);
  const source = e.exactAppFrame ? null : (e.$src ?? e.getAttribute('src'));
  if (source != null) {
    const s = shown(e, source) ? source : 'about:blank';
    if (changed || e.getAttribute('src') !== s) e.setAttribute('src', s);
  }
  commitGuestOrigin(e);
  return true;
};
