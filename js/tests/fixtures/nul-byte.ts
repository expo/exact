// Strings holding U+0000 through the host door (`store.set`, `store.get`) and the console: each crosses whole.
type Store = { get(name: string): string | null; set(name: string, value: string): void };

function answer(source: string, args: unknown[], store: Store): unknown {
  if (source !== "nul") throw new Error("unknown source " + source);
  const s = "a\u0000b";
  store.set("k2", s);
  const back = store.get("k2") ?? "";
  console.log("x\u0000y");
  console.log("second line");
  return JSON.stringify({ direct: s.length, back: back.length, same: back === s });
}

(globalThis as any).exact = { abi: 1, appId: "test.nul", grants: "secret.keep k2\n", answer };
