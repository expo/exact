/** The package reads the trust that built the page, including when the agent drives it. */
export function packageContext(context, development) {
  return Object.defineProperty(context, "development", { value: development, enumerable: true });
}

/** Device handles shared by the web implementations in one session (LLP 1133.009 D10). */
export function packageServices() {
  // The session's track table (LLP 1133.009 D10 item 6): tracks and streams by id, each track with the
  // implementation that opened it, its `owner`, through which a consuming implementation subscribes.
  const table = new Map(), holds = new Set();
  const tracks = {
    // An exclusive hold on a device for a recognizer that opens it itself (LLP 1133.012 D8, D12 item 4): granted
    // only while no track of `kind` has a running capture (its owner's `leased(id)`), and while it lasts the owner
    // opens no capture of that kind. Its release, or a throw `NotReadableError`.
    hold(kind) {
      if ([...table.values()].some(e => e.kind === kind && e.owner?.leased?.(e.id))) throw Object.assign(new Error(`a ${kind} capture runs`), { code: "NotReadableError" });
      const h = { kind };
      holds.add(h);
      return () => { holds.delete(h); };
    },
    held: kind => [...holds].some(h => h.kind === kind),
    add: entry => { table.set(entry.id, entry); },
    lookup: id => table.get(id) ?? null,
    remove: id => { table.delete(id); },
    all: () => [...table.values()],
    /** `consumer` ({live(source), muted(reason), unmuted(), ended()}) joins track `id`; how = {start, origin}. */
    subscribe(id, consumer, how = {}) {
      const entry = table.get(id);
      if (!entry?.owner?.subscribe) throw Object.assign(new Error(`no track ${id}`), { code: "NotFoundError" });
      return entry.owner.subscribe(id, consumer, how);
    },
    /** `observer` hears track `id` as a consumer does and takes no lease (LLP 1133.027 D17 item 1); its leave. */
    observe(id, observer) {
      const entry = table.get(id);
      if (!entry?.owner?.observe) throw Object.assign(new Error(`no track ${id}`), { code: "NotFoundError" });
      return entry.owner.observe(id, observer);
    },
  };
  // The page's Audio Session (W3C Audio Session; LLP 1133.009 D7, LLP 1133.012 D4): `play-and-record` while a
  // capture's claim is held, else the strongest spoken type claimed (`playback` over `transient-solo` over
  // `transient`), else what the page had.
  const claims = new Set(), spoken = ["transient", "transient-solo", "playback"];
  let before = null;
  const session = () => typeof navigator === "object" ? navigator.audioSession : undefined;
  const compose = () => {
    const s = session();
    if (!s) return;
    const types = [...claims].map(c => c.type);
    const want = types.includes("play-and-record") ? "play-and-record" : spoken.filter(t => types.includes(t)).at(-1);
    if (want && before == null) before = s.type;
    if (want) s.type = want;
    else if (before != null) { s.type = before; before = null; }
  };
  const audioSession = {
    /** A capture's claim, or speech's with `{type}`, one of the spoken types. */
    claim(how = {}) {
      const claim = { type: spoken.includes(how.type) ? how.type : "play-and-record", release: () => { claims.delete(claim); compose(); } };
      claims.add(claim);
      compose();
      return claim;
    },
    /** The type in force: the page's, or the strongest claimed where the page has no Audio Session. */
    type() {
      const types = [...claims].map(c => c.type);
      return session()?.type ?? (types.includes("play-and-record") ? "play-and-record" : spoken.filter(t => types.includes(t)).at(-1) ?? "auto");
    },
  };
  return { tracks, audioSession };
}

export function packageFiles(fileStore) {
  return {
    // A file's first write makes its directory, as a recorder's writer needs it.
    write: (path, bytes) => fileStore("fs.write", path).then(async s => { await s.mkdir(path.slice(0, path.lastIndexOf("/"))); return s.writeFile(path, bytes); }),
    append: (path, bytes) => fileStore("fs.write", path).then(s => s.appendFile(path, bytes)),
    read: path => fileStore("fs.read", path).then(s => s.readFile(path)),
    // A directory a handle fills (LLP 1133.012 D12 item 7): emptied, made, and listed.
    remove: path => fileStore("fs.write", path).then(s => s.rm(path)),
    mkdir: path => fileStore("fs.write", path).then(s => s.mkdir(path)),
    // A Blob made the file's entry by reference, as a picked file is (LLP 1133.008.000 D2): the page reads none of it.
    // `refusal` is asked inside the store's transaction, and what it returns stops the write.
    put: (path, blob, refusal) => fileStore("fs.write", path).then(async s => { await s.mkdir(path.slice(0, path.lastIndexOf("/"))); return s.putBlob(path, blob, refusal); }),
    putAll: (entries, refusal) => fileStore("fs.write", ...entries.map(e => e.path)).then(async s => {
      await Promise.all([...new Set(entries.map(e => e.path.slice(0, e.path.lastIndexOf("/"))))].map(path => s.mkdir(path)));
      return s.putBlobs(entries, refusal);
    }),
    list: path => fileStore("fs.read", path).then(s => s.readdir(path)).catch(error => {
      if (error.code === "grant-missing") throw error;
      return [];
    }),
  };
}

/** Names and views of an extension share its first load, including concurrent requests. */
export function packageInstances(bound, create) {
  const chunks = new Map(), loaded = new Map();
  function chunk(extension) {
    if (!Object.hasOwn(bound, extension)) return null;
    if (!chunks.has(extension)) chunks.set(extension, bound[extension]());
    return chunks.get(extension);
  }
  function instance(extension) {
    if (!Object.hasOwn(bound, extension)) return null;
    if (!loaded.has(extension)) loaded.set(extension, chunk(extension).then(m => create(extension, m)));
    return loaded.get(extension);
  }
  return { chunk, instance, loaded };
}
