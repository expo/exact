// A synchronous native call from the module's top level, as a package's
// generated binding makes one (`__exact_module_sync`), which the prelude
// refuses outside an answer; the same call inside an answer is made.
const sync = (globalThis as any).__exact_module_sync as (request: Record<string, unknown>) => unknown;
let refused = "";
try {
  sync({ at: "top level" });
} catch (error) {
  refused = String((error as Error).message);
}

function answer(source: string): unknown {
  if (source === "refused") return refused;
  if (source === "inside") return JSON.stringify(sync({ at: "answer" }));
  throw { kind: "UnknownSource", message: source };
}

(globalThis as any).exact = {
  abi: 1,
  appId: "test.native-init",
  grants: "",
  answer,
};
