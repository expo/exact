// A data module's error events (LLP 1175), for prelude.js: the web's names
// on its global, from Ibex's events binding (`addEventListener("error" |
// "unhandledrejection" | "rejectionhandled")`, the `on…` properties,
// `reportError`), dispatched when a page's would be (D2). The listeners only
// observe: every report reaches the journal as before, cancelled or not (D5),
// and Exact's rejection tracker stays the only one. prelude.js takes
// `__exact_errors` and deletes it.
(function (global) {
  "use strict";
  // `my`: the prelude's `hooks` (Ibex's, natively), Hermes's `tracker`,
  // `journal`, `runtimeLine`, `uncaught(prefix, value)` (natively),
  // `errorText`, `leftOpen`, and `swap(call)`, which makes `call` the current
  // answer and returns the one before.
  global.__exact_errors = function (my) {
    var hooks = my.hooks,
      tracker = my.tracker,
      // dispatches running (D3)
      depth = 0,
      // the answer whose turn the outermost dispatch interrupted, or null (D3)
      owner = null,
      // promises whose rejection is journaled and never dispatched (D4): those a listener returned during a dispatch,
      // and those a dispatch's fetch returned or rejected
      quiet = new WeakSet(),
      // the errors made for a dispatch's fetch: a rejection with one as its reason is journaled, never dispatched (D4)
      reasons = new WeakSet(),
      // a report whose journal line its source wrote (a timer's): `{ value, line }`
      written = null,
      // rejections handled after they were reported: `rejectionhandled` at the next checkpoint (D2)
      handled = [],
      // the promise Hermes's tracker is rejecting or handling now, and the one its report is for
      rejecting = null,
      handling = null,
      reported = null,
      // whether the tracker's report is told by the rejection hook it arms it from (`report`)
      hooked = false,
      armed = 0;
    // What a browser's console prints for a report no listener cancelled, to the platform's log (D5).
    function uncaught(prefix, value) {
      if (my.uncaught) my.uncaught(prefix, value);
    }
    if (hooks)
      hooks.configure({
        report: function (type, value, text, cancelled) {
          if (!cancelled) uncaught(type === "error" ? "Uncaught " : "Uncaught (in promise) ", value);
          if (type !== "error") return my.journal("data: unhandled rejection: " + my.errorText(value));
          if (written && written.value === value) {
            my.journal(written.line);
            written = null;
          } else my.runtimeLine("Uncaught " + text);
        },
        result: function (value) {
          if (depth && value !== null && typeof value === "object" && typeof value.then === "function")
            quiet.add(value);
        },
        // A dispatch runs with no answer current; the answer it interrupted is current again after.
        during: function (run) {
          var before = my.swap(null);
          if (!depth) owner = before;
          depth++;
          try {
            return run();
          } finally {
            if (!--depth) owner = null;
            my.swap(before);
          }
        },
      });
    function quietReason(error) {
      return error !== null && typeof error === "object" && reasons.has(error);
    }
    // HTML's "report an exception" for `error`, a throw no code caught: `line`, when given, is its journal line.
    function raise(error, line) {
      // A browser realm's (the wasm target's module realm) reports only the line, as it did.
      if (!hooks) {
        if (line) my.journal(line);
        return;
      }
      if (line) written = { value: error, line: line };
      try {
        hooks.reportException(error);
      } finally {
        written = null;
      }
    }
    // Hermes's tracker, with the promise each report is for: its rejection hook (`Promise._C`) arms the report, and
    // its handled hook (`Promise._B`) says that a reported rejection is handled after all.
    if (tracker && typeof tracker.enablePromiseRejectionTracker === "function") {
      tracker.enablePromiseRejectionTracker({
        allRejections: true,
        onUnhandled: function (id, error) {
          var promise = reported;
          if (hooks && !(promise && quiet.has(promise)) && !quietReason(error)) hooks.onUnhandled(id, error, promise);
          else {
            // Never dispatched, so no listener cancels it: a page's console would print it.
            uncaught("Uncaught (in promise) ", error);
            my.journal("data: unhandled rejection: " + my.errorText(error));
          }
          my.leftOpen();
        },
        onHandled: function (id, error) {
          if (hooks && handling && !quiet.has(handling) && !quietReason(error)) handled.push([id, error, handling]);
        },
      });
      var Promise_ = global.Promise,
        reject = Promise_._C,
        handle = Promise_._B;
      if (typeof reject === "function") {
        hooked = true;
        Promise_._C = function (promise, error) {
          var was = rejecting;
          rejecting = promise;
          armed++;
          try {
            return reject.call(this, promise, error);
          } finally {
            armed--;
            rejecting = was;
          }
        };
      }
      if (typeof handle === "function")
        Promise_._B = function (promise) {
          var was = handling;
          handling = promise;
          try {
            return handle.call(this, promise);
          } finally {
            handling = was;
          }
        };
    }
    return {
      raise: raise,
      dispatching: function () {
        return depth > 0;
      },
      // The answer whose turn the dispatch running interrupted, or null.
      owner: function () {
        return owner;
      },
      // The rejection is journaled, never dispatched.
      quiet: function (promise) {
        quiet.add(promise);
        return promise;
      },
      // An error made for a dispatch's fetch: a rejection with it as its reason is journaled, never dispatched.
      quietReason: function (error) {
        if (error !== null && typeof error === "object") reasons.add(error);
        return error;
      },
      // The tracker's report among the timers its rejection hook asks for (time.js `timers`): what runs it, with the
      // promise it is for, or undefined for any other timer.
      report: function (callback) {
        if (!((armed > 0 || !hooked) && callback.name === "bound onUnhandled")) return undefined;
        var promise = rejecting;
        return function () {
          reported = promise;
          try {
            callback();
          } finally {
            reported = null;
          }
        };
      },
      // At the checkpoint after the tracker's reports: the rejections handled since the last.
      checkpoint: function () {
        var now = handled;
        handled = [];
        now.forEach(function (h) {
          hooks.onHandled(h[0], h[1], h[2]);
        });
        return now.length > 0;
      },
      // What the shim calls with what the module's evaluation threw, before the load fails (D2).
      evaluationThrew: function (error) {
        try {
          raise(error);
        } catch (_) {}
      },
    };
  };
})(globalThis);
