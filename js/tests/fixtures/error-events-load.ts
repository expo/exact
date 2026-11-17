// LLP 1175 D2: a module whose evaluation throws, after it listens for the
// global's `error` event. The listener's fetch is refused (D6).
addEventListener("error", (e: Event) => {
  console.log("heard " + (e as ErrorEvent).error.message);
  fetch("https://example.test/report", { method: "POST", body: "at load" });
});
(globalThis as any).exact = { abi: 1, appId: "test.error-events-load", grants: "", answer() {} };
throw new Error("at load");
