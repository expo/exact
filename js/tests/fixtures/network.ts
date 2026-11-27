// LLP 1162 D1, D2: the answer records the order its fetches resume in.
const delivered: string[] = [];
let tail: Promise<void> = Promise.resolve();

function serial<T>(fn: () => Promise<T>): Promise<T> {
  const next = tail.then(fn, fn);
  tail = next.then(
    () => undefined,
    () => undefined
  );
  return next;
}

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
  grants: "net.fetch http:\nfs.read app:/data\nfs.write app:/data\n",
  async answer(source: string, args: unknown[], _store: unknown, storage: any) {
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
    if (source === "serial") {
      const label = String(args[0]);
      const mode = String(args[1]);
      return serial(async () => {
        if (mode === "storage") {
          await storage.fs.readdir(storage.fs.directories.data);
        } else {
          await (globalThis as any).scheduler.yield();
        }
        return { text: label, order: `${mode}:${label}` };
      });
    }
    throw { kind: "UnknownSource", message: source };
  },
};
