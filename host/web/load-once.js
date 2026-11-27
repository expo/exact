// Failed loads leave the slot reusable; concurrent callers share the same promise.
export function loadOnce(cache, key, load, refused) {
  if (cache[key] != null) return cache[key];
  let loaded;
  try { loaded = load(); } catch (error) { loaded = Promise.reject(error); }
  const pending = Promise.resolve(loaded).catch(error => {
    if (cache[key] === pending) cache[key] = null;
    if (refused) return refused(error);
    throw error;
  });
  return cache[key] = pending;
}
