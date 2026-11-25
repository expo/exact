// @exact/window-management on the web (LLP 1178 D1, D4, D6): the browser's
// own Window Management API, `window.getScreenDetails()`, in the
// interface's shapes. The page runtime (host/web-js/modules.js) calls it
// outside the agent; under the agent the runner's substitute answers.
//
// - Only Chromium on the desktop has the API; elsewhere `getScreenDetails`
//   fails `NotSupportedError` and the status reads `available: false`.
// - A read while the permission is `prompt` is the request, and Chrome
//   takes it only with transient activation: a read at load fails
//   `NotAllowedError`, and one from a press (`refresh`) shows the browser's
//   prompt; a refusal fails `NotAllowedError`. When the permission changes,
//   both sources are read again.
// - The source subscribes at its first read to `screenschange`,
//   `currentscreenchange` and each screen's `change`, and announces its
//   topic on each; the last reader's release unsubscribes. The status
//   subscribes to the permission's own `change`.
// - The web has no way to ask where the pointer is: `getPointerPosition`
//   fails `NotSupportedError` (D6).

const FIELDS = ['left', 'top', 'width', 'height', 'availLeft', 'availTop', 'availWidth', 'availHeight', 'devicePixelRatio', 'isPrimary', 'isInternal'];

/** A `ScreenDetailed` in the interface's shape. */
function screen(s) {
  const out = { label: String(s.label ?? '') };
  for (const k of FIELDS) out[k] = k.startsWith('is') ? !!s[k] : Number(s[k] ?? 0);
  return out;
}

export function implementation({ changed }) {
  const win = typeof window === 'object' ? window : {};
  const available = typeof win.getScreenDetails === 'function';
  let stop = null;      // ends the subscription of the watched ScreenDetails
  let permission = null;
  let generation = 0;   // releases of getScreenDetails, so a late read subscribes nothing

  function watch(d) {
    const heard = () => changed('window-management.getScreenDetails');
    let screens = [];
    const each = (on) => { for (const s of screens) s[on ? 'addEventListener' : 'removeEventListener']('change', heard); };
    // A screen added or removed: listen to the screens there are now.
    const rearranged = () => { each(false); screens = [...d.screens]; each(true); heard(); };
    screens = [...d.screens];
    each(true);
    d.addEventListener('screenschange', rearranged);
    d.addEventListener('currentscreenchange', heard);
    stop = () => {
      each(false);
      d.removeEventListener('screenschange', rearranged);
      d.removeEventListener('currentscreenchange', heard);
      stop = null;
    };
  }

  async function state() {
    try {
      permission ??= await navigator.permissions.query({ name: 'window-management' });
      // A grant or a refusal in the browser's settings: the status and the
      // displays, which a read refused while `prompt` did not answer, are read again.
      permission.onchange ??= () => {
        changed('window-management.windowManagementStatus');
        changed('window-management.getScreenDetails');
      };
      return permission.state;
    } catch {
      return 'prompt';
    }
  }

  return {
    async getScreenDetails() {
      if (!available) return { error: 'NotSupportedError' };
      const asked = generation;
      let details;
      try {
        details = await win.getScreenDetails();
      } catch (e) {
        await state();
        return { error: e?.name === 'NotAllowedError' ? 'NotAllowedError' : 'NotSupportedError', message: e?.message };
      }
      if (asked === generation && !stop) watch(details);
      return { screens: [...details.screens].map(screen), currentScreen: screen(details.currentScreen) };
    },
    async getWindowBounds() {
      const { screenX, screenY, outerWidth, outerHeight } = win;
      if (![screenX, screenY, outerWidth, outerHeight].every(Number.isFinite))
        return { error: 'NotSupportedError', message: 'getWindowBounds needs a browser window' };
      return { screenX, screenY, outerWidth, outerHeight };
    },
    async getPointerPosition() {
      return { error: 'NotSupportedError' };
    },
    async windowManagementStatus() {
      if (!available) return { available: false, state: 'denied' };
      return { available: true, state: await state() };
    },
    /** The last reader of `name` is gone: stop listening. */
    release(name) {
      if (name !== 'getScreenDetails') return;
      generation++;
      stop?.();
    },
  };
}
