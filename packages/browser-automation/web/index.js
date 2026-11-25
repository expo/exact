// @exact/browser-automation on the web (LLP 1020.002 D9, §11): a page reaches
// into a frame only of its own origin, and only in the frame's page world, so
// `evaluate` and `callFunction` run there for a same-origin frame. A `sandbox`
// (an isolated world), input (a page dispatches only untrusted events, and the
// browser's own input reaches no frame from script), the browser's own dialogs
// and downloads, a preload script and a picture of the frame are
// NotSupportedError, as is any call to a cross-origin frame.
const LIMIT = 64 * 1024;

function frameWindow(id, target, sandbox) {
  if (target != null) {
    if (typeof target !== 'object' || id != null || sandbox != null ||
        (target.context == null) === (target.realm == null) || (target.realm != null && target.sandbox != null) ||
        ['context', 'sandbox', 'realm'].some(key => target[key] != null && typeof target[key] !== 'string')) {
      return { error: 'invalid argument', message: 'use a context and optional sandbox, or a realm' };
    }
    if (target.realm != null) return { error: 'NotSupportedError', message: 'the web cannot target a script realm' };
    id = target.context;
  }
  if (id == null) return { error: 'invalid argument', message: 'a script needs an iframe id or a target' };
  const frames = [...document.querySelectorAll('#exact-root iframe')].filter(e => e.id === id && !e.closest('[data-exiting]'));
  if (frames.length !== 1) return { error: 'NotFoundError', message: `no unique live iframe with id ${id}` };
  try {
    const win = frames[0].contentWindow;
    void win.document.documentElement;
    return { win };
  } catch {
    return { error: 'NotSupportedError', message: `iframe ${id} is cross-origin: the web reaches only a frame of the page's own origin` };
  }
}

async function settle(run, { sandbox, awaitPromise = true }) {
  if (sandbox != null) return { error: 'NotSupportedError', message: 'the web has no isolated worlds: a sandbox runs nowhere' };
  try {
    let value = run();
    if (awaitPromise && value && typeof value.then === 'function') value = await value;
    else if (value && typeof value.then === 'function') value = {};
    const text = JSON.stringify(value === undefined ? null : value);
    if (text === undefined) return { result: null };
    if (text.length > LIMIT) return { error: 'QuotaExceededError', message: `the result is ${text.length} bytes of JSON; at most ${LIMIT}` };
    return { result: JSON.parse(text) };
  } catch (e) {
    return { error: typeof e?.name === 'string' && e.name ? e.name : 'Error', message: String(e?.message ?? e) };
  }
}

const hasChannel = args => args?.some(arg => arg?.type === 'channel' && Object.keys(arg).length === 2 &&
  typeof arg.value?.channel === 'string' && Object.keys(arg.value).length === 1);
const noChannel = () => ({ error: 'NotSupportedError', message: 'script.message is not supported on the web' });

const unsupported = what => async () => ({ error: 'NotSupportedError', message: `${what} is not offered on the web (LLP 1020.002 §11)` });

export function implementation() {
  return {
    async evaluate({ id, target, expression, sandbox, awaitPromise }) {
      const { win, ...refused } = frameWindow(id, target, sandbox);
      if (!win) return refused;
      // An expression, as on Apple: a script of statements goes in `callFunction`'s body.
      return settle(() => win.eval(`(${String(expression)}\n)`), { sandbox: target?.sandbox ?? sandbox, awaitPromise });
    },
    async callFunction({ id, target, functionDeclaration, arguments: args = [], sandbox, awaitPromise }) {
      if (hasChannel(args)) return noChannel();
      const { win, ...refused } = frameWindow(id, target, sandbox);
      if (!win) return refused;
      return settle(() => win.eval(`(${functionDeclaration}\n)`)(...(args ?? [])), { sandbox: target?.sandbox ?? sandbox, awaitPromise });
    },
    async addPreloadScript({ arguments: args } = {}) {
      return hasChannel(args) ? noChannel() : unsupported('a preload script')();
    },
    removePreloadScript: unsupported('a preload script'),
    performActions: unsupported('input to a frame'),
    releaseActions: unsupported('input to a frame'),
    handleUserPrompt: unsupported('answering a frame\'s dialog'),
    setDownloadBehavior: unsupported('a frame\'s download behaviour'),
    captureScreenshot: unsupported('a picture of a frame'),
    release() {},
  };
}
