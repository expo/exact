import { oauth } from "../../../packages/oauth/index.ts";
// Answers that await a promise another answer made, then fetch their own
// rows (LLP 1133.002 D14): each fetch after the await must be its own
// answer's, through `Promise.all` and an imported helper too. The test's
// transport sees which answer each request arrives under.

const API = "https://context.test";

let shared: Promise<string> | null = null;

// One load, made by whichever answer asks first.
function load(): Promise<string> {
  shared ??= fetch(`${API}/shared`).then((r) => r.text());
  return shared;
}

async function helper(name: string): Promise<string> {
  return (await fetch(`${API}/${name}/helper`)).text();
}

async function work(name: string): Promise<{ text: string }> {
  const loaded = await load();
  const [one, two] = await Promise.all([
    fetch(`${API}/${name}/one`).then((r) => r.text()),
    fetch(`${API}/${name}/two`).then((r) => r.text()),
  ]);
  const nested = await helper(name);
  return { text: [loaded, one, two, nested].join(" ") };
}

const auth = oauth({ key: "token", clientId: "native-test", scopes: [], apis: [API],
  endpoints: { authorization: `${API}/authorize`, token: `${API}/token` } });
function answer(source: string, args: unknown[], store: any): unknown {
  if (source === "timerSet") return timerSet();
  if (source === "batch") return batch(Number(args[0]));
  if (source === "writes") return batch(3, store);
  if (source === "wave") return wave();
  if (source === "shared") return sharedAnswer();
  if (source === "waiter") return waiter(String(args[0]));
  if (source === "ownership") return ownership(String(args[0]));
  if (source === "handoff") return handoff(String(args[0]));
  if (source === "secret" && args[0] === "protected") return protectedPosts(store);
  if (source === "gone") return letGoSet();
  if (source === "rejected") return { text: String(rejectedFetches) };
  if (source === "throws") { void fetch(`${API}/before-throw`); throw new Error("after sending"); }
  if (source === "secret" && String(args[0]).startsWith("read-")) return store.exclusive("token", () => ({ text: store.get("token") ?? "empty" }));
  if (source === "secret") return auth.fetch(store, `${API}/data`).then(r => r.text()).then(text => ({ text }), e => ({ text: `failed:${e.message}` }));
  if (source === "work" && typeof args[0] === "string") return work(args[0]);
  throw { kind: "UnknownSource", message: source };
}

(globalThis as any).exact = {
  abi: 1,
  appId: "test.context",
  grants: "net.fetch https://context.test\nsecret.keep token\n",
  answer,
};

let rejectedFetches = 0;
async function batch(count: number, store?: any): Promise<{ text: string }> {
  const values = await Promise.all(Array.from({ length: count }, (_, i) =>
    fetch(`${API}/batch/${i}`).then(async r => {
      const value = await r.text();
      if (store) { const before = store.get("token"); store.set("token", value); console.log(`wrote ${value} after ${before}`); }
      return value;
    })));
  return { text: values.join("/") };
}
async function wave(): Promise<{ text: string }> {
  await fetch(`${API}/start`);
  return batch(3);
}
async function waiter(name: string): Promise<{ text: string }> {
  await load();
  return { text: await (await fetch(`${API}/${name}`)).text() };
}
function letGoSet(): Promise<{ text: string }> {
  return Promise.all([0, 1, 2].map(i => fetch(`${API}/gone/${i}`).catch(e => {
    if (e.kind === "Aborted") rejectedFetches++;
    throw e;
  }))).then(() => ({ text: "done" }));
}

async function sharedAnswer(): Promise<{ text: string }> {
  const text = await load();
  await fetch(`${API}/shared-tail`);
  return { text };
}

async function timerSet(): Promise<{ text: string }> {
  const signal = AbortSignal.timeout(100);
  const replies = await Promise.all([
    fetch(`${API}/timed-first`, { signal }), fetch(`${API}/timed-second`),
  ]);
  return { text: String(replies.length) };
}

let resolveDetail: (text: string) => void;
const detailDone = new Promise<string>(resolve => { resolveDetail = resolve; });
async function ownership(role: string): Promise<{ text: string }> {
  if (role === "observer") return { text: await detailDone };
  const loaded = await load();
  if (role === "loader") return { text: loaded };
  const text = await (await fetch(`${API}/detail`)).text();
  resolveDetail(text);
  return { text };
}

let resolveHandoff: (text: string) => void;
const handedOff = new Promise<string>(resolve => { resolveHandoff = resolve; });
function handoff(role: string): unknown {
  if (role === "waiter") return handedOff.then(text => ({ text }));
  void load().then(async () => {
    resolveHandoff(await (await fetch(`${API}/orphan`)).text());
  });
  return { text: "owner" };
}

function protectedPosts(store: any): Promise<{ text: string }> {
  return store.exclusive("token", async () => {
    const values = await Promise.all(["a", "b"].map(async name => {
      const text = await (await fetch(`${API}/protected/${name}`, { method: "POST" })).text();
      store.set("token", `${store.get("token")}/${name}:${text}`);
      return text;
    }));
    return { text: values.join("/") };
  });
}
