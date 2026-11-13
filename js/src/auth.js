// Signing in through the system browser (LLP 1069.006), for prelude.js:
// `authCallback` and `openAuthSession`. prelude.js takes `__exact_auth`,
// calls it with its door and its refusal outside an answer, and deletes it.
(function (global) {
  "use strict";
  global.__exact_auth = function (env) {
    var host = env.host,
      answerOnly = env.answerOnly;
    // `authCallback()` is this carrier's callback, known before PAR: a native
    // build's granted `auth.callback`, the web's `<origin>/.exact/auth/callback`
    // (D2); a device fact, so bake compiles no answer that asks.
    // `openAuthSession(url, {callback, state, ephemeral, message})` is a request to `exact-auth:` on the independent
    // lane, as `native.later` is (D1): it resolves with the callback URL (200) or rejects with the status and message
    // (499 cancelled, 403, 409, 428, 501, 502) as `error.status`; `message` is a loopback page's sentence (LLP 1133.005
    // D9).
    global.authCallback = function authCallback() {
      answerOnly("authCallback()");
      var callback = host(12, "callback", "");
      if (callback === undefined) throw new Error("authCallback(): the grants name no auth.callback");
      return callback;
    };
    global.openAuthSession = function openAuthSession(url, options) {
      var o = options || {};
      if (host(12, "placement", "") === "worker") {
        return Promise.reject(
          new Error(
            "openAuthSession() is unavailable in a worker-placed source on the web: its popup opens in the press's call stack (LLP 1069.006); place this source on main"
          )
        );
      }
      if (
        typeof o.callback !== "string" ||
        typeof o.state !== "string" ||
        !o.state ||
        (o.message !== undefined && typeof o.message !== "string")
      ) {
        return Promise.reject(
          new TypeError(
            "openAuthSession(url, {callback, state, message}): callback is authCallback(), state a non-empty string and message, if given, a string"
          )
        );
      }
      var body = JSON.stringify({
        url: String(url),
        callback: o.callback,
        state: o.state,
        ephemeral: !!o.ephemeral,
        message: o.message,
      });
      return global
        .fetch("exact-auth:", { method: "POST", body: body, exactIndependentHttp: { maxResponseBytes: 65536 } })
        .then(function (r) {
          return r.text().then(function (t) {
            if (r.status === 200) return t;
            var e = new Error(t || "the auth session failed");
            Object.defineProperty(e, "status", { value: r.status, enumerable: true });
            throw e;
          });
        });
    };
  };
})(globalThis);
