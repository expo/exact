// Loaded by the first native view after paint. The build supplies only the packages that own placed tags.
import { packageContext, packageFiles, packageInstances, packageServices } from '../../web-js/package-session.js';
import { packageViews } from '../../web-js/views.js';
import { coversPath, rawGrantText } from '../navigation.js';
import { storageKey, agentStorageRefusal } from '../storage-environment.js';
import '../native-glue.js';

export function packageHost(views, bound, { host, generation, agentMode, now, foreground, changed }, options, development) {
  const current = () => generation === host.incarnation;
  if (!current()) return { attach() {}, destroy() {} };
  if (host.packageViews) return host.packageViews.native;
  const clocked = [], topics = new Set(), timers = new Set();
  let store, lastClock = now(), shown = foreground();
  const services = packageServices();
  const fileStore = async (extension, operation, paths) => {
    if (!current()) throw new Error('package session ended');
    for (const path of paths) {
      const own = operation === 'fs.write' && path.startsWith(`app:/tmp/${extension}/`);
      if (!own && !coversPath(host.grantSet, operation, path)) throw Object.assign(new Error(`denied: ${operation} ${path}`), { code: 'grant-missing' });
    }
    const key = storageKey(globalThis.exact.compat.inputs.app);
    if (key == null) throw new Error(agentStorageRefusal);
    return store ??= import('../storage-fs.js').then(m => m.createFileStore(key));
  };
  function after(ms, fn) {
    if (typeof fn !== 'function' || !current()) return () => {};
    const delay = Math.max(0, Number(ms) || 0);
    const timer = { due: now() + delay, fire() { stop(); if (current()) fn(); } };
    const stop = () => { timers.delete(timer); clearTimeout(timer.handle); };
    timers.add(timer);
    if (!agentMode) timer.handle = setTimeout(timer.fire, delay);
    return Object.assign(stop, { stop });
  }
  const { chunk, instance, loaded } = packageInstances(bound, async (extension, m) => {
    if (!current()) return null;
    const impl = m.implementation?.(packageContext({
      ...services, files: packageFiles((op, ...paths) => fileStore(extension, op, paths)), after, agent: agentMode, config: m.config ?? {}, now, log: options.log, foreground,
      grants: host.unparsed ? '' : rawGrantText(host.grantSet),
      clocked: fn => clocked.push(fn),
      get authSession() { return globalThis.exact.authSession; },
      changed(topic) {
        if (!current() || topics.has(topic)) return;
        topics.add(topic);
        globalThis.exact.ready.then(() => {
          topics.delete(topic);
          if (current()) changed(topic);
        }, () => topics.delete(topic));
      },
    }, development));
    if (agentMode) await impl?.preferred?.({});
    return impl;
  });
  const serves = tag => current() && Object.hasOwn(views, tag) && host.packageViewOwners?.[tag] === views[tag][4];
  const table = packageViews(views, chunk, instance, { target: 'wasm', viewId: el => el.dataset.view, say: options.log, serves });
  const notify = callback => {
    for (const pending of loaded.values()) pending.then(callback).catch(e => options.log(e.message));
  };
  const native = globalThis.exact.nativeHost({ ...options, packages: table });
  host.packageViews = {
    native,
    get nextDue() { return Math.min(...Array.from(timers, timer => timer.due)); },
    report() {
      table.report();
      const visible = foreground();
      if (shown !== visible) {
        shown = visible;
        notify(impl => impl?.foreground?.(visible));
      }
      const at = now();
      if (agentMode) for (const timer of [...timers]) if (timers.has(timer) && timer.due <= at) timer.fire();
      if (at !== lastClock) {
        lastClock = at;
        for (const fn of clocked) fn(at);
      }
    },
    dispose() {
      table.report(true);
      for (const timer of timers) clearTimeout(timer.handle);
      timers.clear();
      notify(impl => impl?.destroy?.());
      store?.then(s => s.close()).catch(e => options.log(e.message));
    },
  };
  return native;
}
