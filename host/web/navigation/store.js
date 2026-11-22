export const WINDOW_STATE = "exact.state.#window.";

// A store write in the page (LLP 1018 D6, LLP 1116 D5, LLP 1027 D4): a secret or state the quota refuses sets the kept answers aside, a cache the runner bounds together, and is tried once more, the kept answers put back if it fails again; a kept answer it refuses leaves the older one, which the next launch may still match, and is said.
export function storeInPage(storage, name, value, warn = console.warn) {
  const key = name.startsWith("exact.state.") ? name : "exact.secret." + name, write = () => value == null ? storage.removeItem(key) : storage.setItem(key, value);
  try {
    try {
      write();
    } catch (e) {
      if (name.startsWith("exact.kept.") || name.startsWith(WINDOW_STATE) || !/quota/i.test(e?.name ?? "")) {
        throw e;
      }
      const aside = Array.from({ length: storage.length }, (_, i) => storage.key(i)).filter((k) => k?.startsWith("exact.secret.exact.kept.")).map((k) => [k, storage.getItem(k)]);
      for (const [k] of aside) {
        storage.removeItem(k);
      }
      try {
        write();
      } catch (again) {
        for (const [k, v] of aside) {
          try {
            storage.setItem(k, v);
          } catch {}
        }
        throw again;
      }
    }
  } catch (e) {
    warn("exact: store", name, String(e));
    return false;
  }
  return true;
}
/** Failed writes retry under the shared lease, against their original durable value. */ export function pageStore(load, storage = name => name.startsWith(WINDOW_STATE) ? sessionStorage : localStorage, warn = console.warn, locks = globalThis.navigator?.locks) {
  const pending = new Map();
  return {
    discard(name) {
      pending.delete(name);
    },
    set(name, value) {
      pending.set(name, {
        value,
        warned: false
      });
    },
    flush() {
      const jobs = [];
      for (const [name, entry] of pending) {
        const key = name.startsWith("exact.state.") ? name : "exact.secret." + name, live = () => pending.get(name) === entry;
        const report = (...line) => {
          if (!entry.warned) {
            entry.warned = true;
            warn(...line);
          }
        }, failed = (e) => {
          if (live()) {
            if (!Object.hasOwn(entry, "before")) {
              pending.delete(name);
            }
            report("exact: store", name, String(e));
          }
        };
        const write = () => {
          try {
            const disk = storage(name), before = disk.getItem(key);
            if (Object.hasOwn(entry, "before") && entry.before !== before) {
              pending.delete(name);
              return;
            }
            entry.before = before;
            if (storeInPage(disk, name, entry.value, report)) {
              pending.delete(name);
            }
          } catch (e) {
            failed(e);
          }
        };
        if (Object.hasOwn(entry, "before")) {
          entry.retry ??= load("./storage-environment.js", "retrySecretWrite").then((retry) => retry(key, live, write, locks)).catch(failed).finally(() => {
            delete entry.retry;
          });
          jobs.push(entry.retry);
        } else {
          write();
        }
      }
      return Promise.all(jobs);
    }
  };
}
