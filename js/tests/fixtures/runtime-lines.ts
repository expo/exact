// What the runtime itself says through `console` (LLP 1027.009 D1), each
// beside a `console.error` of the app's own.

// A fetch with no answer running, which the prelude refuses.
console.error("the app at load");
fetch("https://example.com/early").catch(() => {});

function answer(source: string): unknown {
  console.error("the app's " + source);
  if (source === "microtask") {
    queueMicrotask(() => { throw new Error("task"); });
  } else if (source === "abort") {
    const controller = new AbortController();
    controller.signal.addEventListener("abort", () => { throw new RangeError("listener"); });
    controller.abort();
  } else if (source === "compact") {
    return new Intl.NumberFormat("en-US", { notation: "compact", compactDisplay: "long" }).format(1234);
  } else if (source === "rejection") {
    Promise.reject(new Error("lost"));
  } else if (source === "handoffs") {
    return JSON.stringify(["__exact_runtime_line", "reportError", "__exact_log"].filter((name) => name in globalThis));
  } else {
    throw new Error("unknown source " + source);
  }
  return "reached the return";
}

(globalThis as any).exact = { abi: 1, appId: "test.runtime-lines", grants: "", answer };
