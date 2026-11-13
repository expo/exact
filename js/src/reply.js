// An answer's reply for the seam, for prelude.js: its value as JSON, with
// large native strings captured beside it, or its failure, coded as
// `failure(x)` reads it (js/src/reply.rs reads both). prelude.js takes
// `__exact_reply` and deletes it. `env.captureString` is the native
// capture, absent on the web; `env.owe()` says a full collection is owed.
(function (global) {
  "use strict";
  global.__exact_reply = function (env) {
    var captureString = env.captureString,
      keptReply = env.keptReply;
    function ok(value, held) {
      var reply = { tag: 0, value: value === undefined ? null : value };
      if (typeof captureString !== "function") return keptReply(reply, null);
      var head = null,
        root = true;
      return keptReply(reply, function (key, item) {
        var first = root;
        root = false;
        var large = typeof item === "string" && item.length >= 65536;
        var object = item !== null && typeof item === "object";
        var parent = null;
        if (!first && (large || object)) {
          parent = head;
          while (parent && parent.value !== this) parent = parent.parent;
        }
        if (large) {
          head = parent;
          // The root link identifies its holder but has no result-path key.
          var depth = first ? 0 : 1;
          for (var link = parent; link && link.parent; link = link.parent) depth++;
          var path = [];
          function pathKey(key) {
            Object.defineProperty(path, --depth, { value: key, enumerable: true, writable: true, configurable: true });
          }
          if (!first) pathKey(key);
          for (var link = parent; link && link.parent; link = link.parent) pathKey(link.key);
          // Application toJSON hooks apply to its values, never our path metadata.
          Object.defineProperty(path, "toJSON", { value: undefined });
          captureString(JSON.stringify(path), item);
          if (held) env.owe();
          return "";
        }
        // Only ancestors of the current object are needed for later capture paths.
        if (object) head = { value: item, parent: parent, key: key };
        return item;
      });
    }
    // `failure(x)`'s code for what a module let through (LLP 1109 D3), as
    // host/web-js/rt.js `failureCode` reads it: a fetch's rejection by its
    // kind, a coded storage refusal, else the module's own error.
    var FETCH_FAILURE = { Network: "offline", Timeout: "timeout", Refused: "refused" };
    function failureCode(e) {
      if (!e || typeof e !== "object") return "error";
      if (e.name === "FetchError") return FETCH_FAILURE[e.kind] || "error";
      return e.kind === "Unavailable" && typeof e.code === "string" ? "storage" : "error";
    }
    function fail(e) {
      var kind = e && typeof e === "object" ? e.kind : undefined;
      if (kind !== "UnknownSource" && kind !== "BadArguments" && kind !== "Unavailable") kind = "Unavailable";
      var message = e && typeof e === "object" && e.message !== undefined ? e.message : e;
      // `reason`: what `failure(x).reason` reads, a string the error carries (the snapback4 driver's refusal code).
      var reason = e && typeof e === "object" && typeof e.reason === "string" ? e.reason : "";
      return JSON.stringify({
        tag: 2,
        kind: kind,
        code: e && e.code,
        bakeWarning: e && e.bakeWarning === true,
        failure: failureCode(e),
        reason: reason,
        message: String(message),
      });
    }
    return { ok: ok, fail: fail };
  };
})(globalThis);
