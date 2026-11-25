// The durable store's persistence (LLP 1018; rt.js `Store`): its keys loaded from `localStorage` at mount, a commit's
// writes kept once it stands, another tab's taken, and `exclusive` leases. Linked by a plan with a data module, the only
// code that reads or writes the store (host/web-js/build.mjs).
import { Store, Kept, say } from './rt.js';
Object.assign(Store, {
  exclusive(k, work) {
    return import('./storage-environment.js').then(m => m.exclusiveSecret(this, Kept('secret'), k, work));
  },
  persist() {
    const p = Kept('secret'),
      jobs = [];
    for (const [k, v] of this.writes.splice(0)) this.pending.set(k, { v, warned: false });
    for (const [k, entry] of this.pending) {
      const live = () => this.pending.get(k) === entry,
        failed = e => {
          if (!live()) return;
          if (!Object.hasOwn(entry, 'before')) this.pending.delete(k);
          if (!entry.warned) {
            entry.warned = true;
            say(`store ${k} failed: ${e}`);
          }
        },
        write = () => {
          try {
            const before = localStorage.getItem(p + k);
            if (Object.hasOwn(entry, 'before') && entry.before !== before) {
              this.pending.delete(k);
              return;
            }
            entry.before = before;
            entry.v == null ? localStorage.removeItem(p + k) : localStorage.setItem(p + k, entry.v);
            this.pending.delete(k);
          } catch (e) {
            failed(e);
          }
        };
      if (Object.hasOwn(entry, 'before') && !this.leased?.has(k)) {
        entry.retry ??= import('./storage-environment.js')
          .then(m => m.retrySecretWrite(p + k, live, write))
          .catch(failed)
          .finally(() => {
            delete entry.retry;
          });
        jobs.push(entry.retry);
      } else write();
    }
    return Promise.all(jobs);
  },
  load() {
    if (globalThis.__exactRender) return;
    const p = Kept('secret');
    try {
      for (let i = 0; i < localStorage.length; i++) {
        const k = localStorage.key(i);
        if (k.startsWith(p)) this.map.set(k.slice(p.length), localStorage.getItem(k));
      }
    } catch {}
    // Another tab of the app wrote or removed a key, or cleared the store (LLP 1030.004 D2).
    globalThis.addEventListener?.('storage', e => {
      if (e.storageArea !== globalThis.localStorage) return;
      if (e.key == null) for (const k of [...this.map.keys()]) this.peer(k, null);
      else if (e.key.startsWith(p)) this.peer(e.key.slice(p.length), e.newValue);
    });
  },
});
