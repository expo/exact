import type { Storage } from "../../../vendor/ibex/crates/ibex2/src/bindings/storage";

const API = "https://liveness.test";
const grants = "net.fetch https://liveness.test\nnet.websocket wss://liveness.test\nfs.read app:/data\nfs.write app:/data\n";
type Row = { text: string };
const row = (text: string): Row => ({ text });
let shared: Promise<unknown>;
let next: Promise<unknown>;
let resolve: (value?: unknown) => void;
let timer: ReturnType<typeof setTimeout>;
let cancel: AbortController;
let release: () => void;

function owner(kind: string, storage: Storage): unknown {
  if (kind === "overlay" || kind === "draw" || kind === "retire") {
    shared = new Promise(r => { resolve = r; });
    return fetch(API + "/ready").then(() => row("owner"));
  }
  if (kind === "completion") {
    shared = fetch(API + "/ready").then(() => row("owner"));
    return shared;
  }
  if (kind === "yield-reply") {
    shared = scheduler.yield();
    return fetch(API + "/ready").then(() => row("owner"));
  }
  if (kind === "yield-resume") {
    shared = scheduler.yield();
    return shared.then(() => row("owner"));
  }
  if (kind === "timer-clear" || kind === "timer-rearm" || kind === "timer-first") {
    return fetch(API + "/ready").then(() => {
      clearTimeout(timer);
      if (kind === "timer-rearm") timer = setTimeout(() => {}, 100);
      return row("owner");
    });
  }
  if (kind === "abort") {
    cancel = new AbortController();
    shared = new Promise(r => cancel.signal.addEventListener("abort", () => r("aborted")));
    return fetch(API + "/ready").then(() => { cancel.abort(); return row("owner"); });
  }
  if (kind === "storage" || kind === "storage-head") {
    shared = storage.fs.writeFile(storage.fs.directories.data + "/owner", new Uint8Array([1]));
    return shared.then(() => row("owner"));
  }
  if (kind.startsWith("stream") || kind.startsWith("socket")) {
    shared = new Promise(r => { resolve = r; });
    const map = () => { resolve("message"); return row("owner"); };
    const asyncMap = async () => { await scheduler.yield(); return map(); };
    const url = kind.startsWith("socket") ? "wss://liveness.test/feed" : API + "/feed";
    return fetch(url, { exactStream: kind.endsWith("async") ? asyncMap : map } as RequestInit);
  }
  if (kind === "serialize" || kind === "serialize-throw") {
    shared = new Promise(r => { resolve = r; });
    return fetch(API + "/ready").then(() => ({ get text() { resolve("serialized"); if (kind === "serialize-throw") throw new Error("serialization failed"); return "owner"; } }));
  }
  if (kind === "checkpoint") {
    shared = new Promise(r => { resolve = r; });
    scheduler.yield().then(() => { Promise.reject({ get message() { resolve("reported"); return "rejected"; } }); });
    return fetch(API + "/ready").then(() => row("owner"));
  }
  if (kind === "forget-yield") {
    shared = scheduler.yield();
    return fetch(API + "/ready").then(() => row("owner"));
  }
  if (kind === "timer-set") {
    shared = fetch(API + "/ready");
    return shared.then(() => row("owner"));
  }
  if (kind === "repark") next = new Promise(r => { release = () => r("second"); });
  shared = fetch(API + "/ready", { method: kind === "module" ? "POST" : "GET" })
    .then(() => "first");
  if (kind === "forget") shared = shared.catch(() => "forgotten");
  if (kind === "mutation") return (async () => { await shared; await fetch(API + "/continuation"); return row("owner"); })();
  return shared.then(() => row("owner"));
}

(globalThis as any).exact = {
  abi: 1, appId: "test.liveness", grants,
  overlay(_source: string, args: unknown[], answer: Row) { if (args[0] === "overlay") resolve("overlay"); return answer; },
  drawCanvas() { resolve("draw"); return ""; },
  retireCanvases() { resolve("retired"); },
  answer(_source: string, args: unknown[], _store: unknown, storage: Storage) {
    const kind = String(args[0]), role = String(args[1]);
    if (role === "owner") return owner(kind, storage);
    if (role === "held") return fetch(API + "/held").then(() => row("held"));
    if (role === "release") return fetch(API + "/second").then(() => { release(); return row("release"); });
    if (kind === "timer-clear" || kind === "timer-rearm" || kind === "timer-first") {
      return new Promise(r => { timer = setTimeout(() => r(row("late")), 2147483647); });
    }
    if (kind === "timer-set") return (async () => {
      await shared;
      return await new Promise(r => setTimeout(() => r(row(role)), 50));
    })();
    if (kind === "storage-head") return storage.fs.writeFile(storage.fs.directories.data + "/" + role, new Uint8Array([2])).then(() => row(role));
    if (kind === "then") return shared.then(() => Promise.resolve("then")).then(() => row(role));
    if (kind === "repark") return shared.then(() => next).then(() => row(role));
    if (kind === "fetch" || kind === "worker" || kind === "mutation" || kind === "module") {
      return (async () => { await shared; return row(role); })();
    }
    return shared.then(() => row(role), () => row(role + " rejected"));
  },
};
