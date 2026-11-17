// Each source exposes one owner of work that a terminal heap error retires.
let count = 0;
let database: any;
function exhaust(): string {
  const held: object[] = [];
  for (let i = 0; i < 3000000; i++) held.push({ id: i, title: `task ${i}`, done: false });
  return String(held.length);
}
function answer(source: string, args: unknown[], _store: unknown, storage: any): unknown {
  if (source === "post") {
    if (args[1]) return "fresh";
    return fetch(`https://oom.test/post/${args[0]}`, { method: "POST" }).then(() => "done");
  }
  if (source === "counter") return String(++count);
  if (source === "maybeOom") return args[0] ? exhaust() : "ready";
  if (source === "refuse") {
    if (args[0]) throw { kind: "UnknownSource", message: "refused" };
    return "ready";
  }
  if (source === "oom") return exhaust();
  if (source === "timerOom") return new Promise(resolve => setTimeout(() => resolve(exhaust()), 10));
  if (source === "sleep") return new Promise(resolve => setTimeout(() => resolve("awake"), 100000));
  if (source === "pending") return fetch("https://oom.test/pending").then(() => "done");
  if (source === "feed") return fetch("wss://oom.test/feed", { exactStream: (event: any) => event.data === "oom" ? exhaust() : event.data === "wait" ? (globalThis as any).scheduler.yield().then(() => "open") : "open" } as RequestInit);
  if (source === "open") return storage.sqlite.open("app:/data/notes.db").then((db: any) => { database = db; return "opened"; });
  if (source === "write") {
    database.execute("INSERT INTO notes VALUES ('submitted')");
    database.execute("INSERT INTO notes VALUES ('queued')");
    return "started";
  }
  throw new Error(`unknown source ${source}`);
}
(globalThis as any).exact = {
  abi: 1, appId: "test.oom",
  grants: "net.fetch https://oom.test\nnet.websocket wss://oom.test\nfs.read app:/data\nfs.write app:/data\nsqlite.open app:/data/notes.db\n",
  answer,
  tools: { fetched: async () => { const call = ++count; const response = await fetch(`https://oom.test/tool/${call}`); return { call, body: await response.text() }; }, yielding: async () => { const call = ++count; await (globalThis as any).scheduler.yield(); return { call }; }, h: (_args: unknown, _store: unknown, storage: any) => storage.sqlite.open("app:/data/notes.db").then(() => "opened") },
};

// The adoption probe prelude calls this after claiming each fetch.
let adopts = 0;
(globalThis as any).exact.adoptOom = () => { if (++adopts === 2) exhaust(); };
(globalThis as any).exact.drawCanvas = (request: string) => {
  if (JSON.parse(request).seq === 0) exhaust();
  return JSON.stringify({ lists: [], frame: true, notes: [String(++count)] });
};
(globalThis as any).exact.retireCanvases = () => { count++; };
