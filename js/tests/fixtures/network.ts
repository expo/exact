// LLP 1162 D1, D2: the answer records the order its fetches resume in.
const delivered: string[] = [];
let prefetchController: AbortController;

async function read(origin: string, path: string, method: string): Promise<string> {
  const response = await fetch(`${origin}/${path}`, method === "POST" ? {
    method,
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query: "query Row($id: ID!) { row(id: $id) { id } }", variables: { id: path } }),
  } : { method });
  const text = await response.text();
  delivered.push(path);
  return text;
}

(globalThis as any).exact = {
  abi: 1,
  appId: "test.network",
  grants: "net.fetch http:",
  async answer(source: string, args: unknown[]) {
    if (source === "prefetchHolding") {
      if (!args[0]) return "hidden";
      prefetchController = new AbortController();
      return fetch(String(args[1]), { signal: prefetchController.signal, exactPrefetch: args[2] } as RequestInit)
        .then(r => r.text(), () => "aborted");
    }
    if (source === "abortPrefetch") {
      prefetchController.abort();
      return "aborting";
    }
    const origin = String(args[0]);
    if (source === "batch") {
      const values = await Promise.all(Array.from({ length: Number(args[1]) }, (_, i) =>
        read(origin, String(i), String(args[2]))));
      return { text: values.join("/"), order: delivered.join("/") };
    }
    if (source === "one") {
      const text = args[3] === false ? "idle" : await read(origin, String(args[1]), String(args[2]));
      return { text, order: delivered.join("/") };
    }
    throw { kind: "UnknownSource", message: source };
  },
};
