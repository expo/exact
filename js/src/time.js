// What a data module has of time, for prelude.js: no clock to read
// (`refuseClock`), and an answer's own timers on the runner's clock
// (`timers`). prelude.js takes `__exact_time` and deletes it.
(function (global) {
  "use strict";
  function refuseAmbient(api) {
    throw new Error(api + " is unavailable in data sources; pass time or a random seed as an argument");
  }
  function fixed(object, name, value) {
    Object.defineProperty(object, name, { value: value, writable: false, configurable: false });
  }
  // LLP 1027.000: time and seeds are source arguments, so bake and cache
  // see them. This VM belongs to one module; no page/guest globals change.
  // Install before the app can capture an alias, including Date's prototype
  // constructor. Keep explicit-value Date construction and UTC arithmetic.
  // `tracker`: Hermes's, natively, where intl.js follows.
  function refuseClock(tracker) {
    // The engine's own Date, through its prototype: a browser realm under a
    // page's fake clock (Playwright's `clock.install`) has a fake `Date` that
    // keeps the real prototype.
    var NativeDate = global.Date.prototype.constructor,
      construct = Reflect.construct;
    var InputDate = new Proxy(NativeDate, {
      apply: function () {
        return refuseAmbient("Date()");
      },
      construct: function (target, args, newTarget) {
        if (!args.length) return refuseAmbient("new Date()");
        return construct(target, args, newTarget);
      },
    });
    fixed(NativeDate, "now", function () {
      return refuseAmbient("Date.now()");
    });
    fixed(NativeDate.prototype, "constructor", InputDate);
    fixed(global, "Date", InputDate);
    fixed(
      global,
      "performance",
      Object.freeze({
        now: function () {
          return refuseAmbient("performance.now()");
        },
      })
    );
    // Intl's formatting methods also default an omitted/undefined date to
    // machine time. Guard the prototype before an app can capture its bound
    // format getter or formatToParts method; explicit timestamps still use
    // the engine's locale/timezone implementation unchanged.
    if (global.Intl && global.Intl.DateTimeFormat) {
      // The engine's prototype, the one with the `format` accessor: a fake
      // clock's DateTimeFormat only inherits it, and answers an omitted date
      // from its clock, so the engine's constructor takes its place.
      var dateFormat = global.Intl.DateTimeFormat.prototype,
        engineFormat = dateFormat;
      while (engineFormat && !(Object.getOwnPropertyDescriptor(engineFormat, "format") || {}).get)
        engineFormat = Object.getPrototypeOf(engineFormat);
      if (engineFormat && engineFormat !== dateFormat) {
        dateFormat = engineFormat;
        global.Intl.DateTimeFormat = engineFormat.constructor;
      }
      var getFormat = Object.getOwnPropertyDescriptor(dateFormat, "format").get,
        formats = new WeakMap(),
        apply = Reflect.apply,
        engineOf = function (f) {
          return f;
        };
      // Natively intl.js follows, whose formatters share an engine formatter (`engineOf` finds it).
      if (tracker)
        Object.defineProperty(global, "__exact_dates", {
          configurable: true,
          value: {
            getFormat: getFormat,
            engineOf: function (f) {
              engineOf = f;
            },
          },
        });
      function explicitFormat(fn, name) {
        return new Proxy(fn, {
          apply: function (target, receiver, args) {
            if (args[0] === undefined) return refuseAmbient("Intl.DateTimeFormat." + name + "()");
            return apply(target, engineOf(receiver), args);
          },
        });
      }
      Object.defineProperty(dateFormat, "format", {
        configurable: false,
        get: function () {
          // a function for each object, as each has its own bound format
          var native = getFormat.call(engineOf(this));
          if (!formats.has(this)) formats.set(this, explicitFormat(native, "format"));
          return formats.get(this);
        },
      });
      if (typeof dateFormat.formatToParts === "function") {
        fixed(dateFormat, "formatToParts", explicitFormat(dateFormat.formatToParts, "formatToParts"));
      }
    }
  }
  // An answer's own timers (LLP 1027.000 D1 as amended 2026-10-28), for
  // prelude.js. `setTimeout` arms one due on the
  // runner's clock as the call began (`clock`, host door op 21), owned by the
  // answer running (`owner`, which refuses one outside an answer). An answer
  // that awaits only its timers parks on the earliest (`wait`), and the
  // runner asks it again once its clock is there (`fire`): one callback a
  // turn, so the microtasks each queues run before the next, as a browser
  // runs them between tasks. An answer that gives its value or is let go
  // clears the rest (`drop`): their callbacks never run. `setInterval`, the
  // frame callbacks and the clock reads stay refused. Hermes's rejection
  // tracker schedules its report with `setTimeout` too: that one runs alone,
  // at the prelude's next `checkpoint`, which is when a browser fires
  // `unhandledrejection`.
  function timers(owner, clock, journal, report, sources) {
    var armed = new Map(),
      reports = [],
      signals = new WeakMap(),
      next = 1,
      now = 0;
    function fixed(name, value) {
      Object.defineProperty(global, name, { value: value, writable: false, configurable: false });
    }
    // The runner's clock as the call began; where none is (at bake), the last time a timer ran at. Not kept
    // monotonic: a runner that restarts at zero beside this realm (a dev reload) is the clock from then on.
    function time() {
      var c = clock(),
        t = Number(c);
      if (c !== undefined && c !== "" && t === t) now = t;
      return now;
    }
    function arm(api, run, ms, args) {
      var call = owner(api),
        id = next++;
      armed.set(id, { call: call, due: Math.ceil(time() + ms), run: run, args: args });
      return id;
    }
    fixed("setTimeout", function setTimeout(callback, delay) {
      if (typeof callback === "function" && report(callback)) {
        reports.push({ id: next, run: callback });
        return next++;
      }
      if (typeof callback !== "function")
        throw new TypeError("setTimeout(): the callback must be a function; a string of code is never run");
      // `long`, as WebIDL converts it
      return arm("setTimeout()", callback, Math.max(Number(delay) | 0, 0), Array.prototype.slice.call(arguments, 2));
    });
    fixed("clearTimeout", function clearTimeout(id) {
      var timer = armed.get(id);
      if (timer) timer.call.ran = true; // Its parked deadline must be read again (LLP 1162 D3).
      armed.delete(id);
      reports = reports.filter(function (r) {
        return r.id !== id;
      });
    });
    ["setInterval", "requestAnimationFrame", "requestIdleCallback"].forEach(function (name) {
      fixed(name, function () {
        throw new Error(
          name + "() is unavailable in data sources: an answer waits with setTimeout; pass time as an argument"
        );
      });
    });
    ["clearInterval", "cancelAnimationFrame", "cancelIdleCallback"].forEach(function (name) {
      fixed(name, function () {});
    });
    if (global.AbortSignal)
      global.AbortSignal.timeout = function timeout(ms) {
        var n = Number(ms);
        if (!(n >= 0 && n <= 9007199254740991))
          throw new TypeError("AbortSignal.timeout(): the delay is a number of milliseconds from 0");
        var controller = new global.AbortController();
        var abort = function () {
          controller.abort(new global.DOMException("signal timed out", "TimeoutError"));
        };
        var id = arm("AbortSignal.timeout()", abort, Math.floor(n), []);
        // Its deadline outlives the timer, which its answer's end drops: a request carrying it still has it.
        signals.set(controller.signal, { id: id, due: armed.get(id).due, abort: abort });
        return controller.signal;
      };
    // A timeout signal's deadline, or the earliest among the signals one was composed of (`sources`).
    function deadline(signal) {
      var s = signal && signals.get(signal),
        composed = !s && signal && sources(signal);
      if (composed)
        composed.forEach(function (input) {
          var t = signals.get(input);
          if (t && (!s || t.due < s.due)) s = t;
        });
      return s;
    }
    function first(call, by) {
      var found;
      armed.forEach(function (t, id) {
        if (t.call === call && t.due <= by && (!found || t.due < found[1].due)) found = [id, t];
      });
      return found;
    }
    return {
      // The earliest of `call`'s timers: when it is due, or undefined.
      wait: function (call) {
        var t = first(call, Infinity);
        return t ? (call.timerDue = t[1].due) : undefined;
      },
      armed: function (call) {
        return first(call, Infinity) !== undefined;
      },
      // When `call`'s earliest timer comes due before the fetch carrying
      // `signal` ends on its own deadline (`left`, which the host keeps): its
      // time, or undefined. The runner resumes the answer then if no reply came.
      race: function (call, signal) {
        var t = first(call, Infinity),
          s = deadline(signal);
        return t && !(s && !signal.aborted && s.due <= t[1].due) ? (call.timerDue = t[1].due) : undefined;
      },
      now: time,
      // `call`'s earliest timer due by the clock `at` runs: the runner's, or,
      // where none is (at bake), the last time a timer ran at. `due`: the
      // runner says the timer it waited for is due, so that one runs.
      fire: function (call, at, due) {
        var t = Number(at);
        if (at !== "" && t === t) now = t;
        if (due) now = Math.max(now, call.timerDue || 0);
        t = first(call, now);
        if (!t) return;
        armed.delete(t[0]);
        try {
          t[1].run.apply(undefined, t[1].args);
        } catch (e) {
          journal("data: a timer's callback threw: " + (e && e.message !== undefined ? e.message : String(e)));
        }
      },
      drop: function (call) {
        armed.forEach(function (t, id) {
          if (t.call === call) armed.delete(id);
        });
      },
      // An `AbortSignal.timeout` still to fire, or one composed of it: the ms
      // left, the deadline a fetch carrying it gives the host; `expire` aborts
      // it when that passes, its timer dropped or not, as a browser's fires:
      // a fetch its answer let go runs to its end (LLP 1146 D2), and another
      // answer may carry the same signal.
      left: function (signal) {
        var s = deadline(signal);
        return s && !signal.aborted ? Math.max(1, s.due - time()) : undefined;
      },
      expire: function (signal) {
        var s = deadline(signal);
        if (s && !signal.aborted) {
          armed.delete(s.id);
          s.abort();
        }
      },
      checkpoint: function () {
        var ran = reports.length > 0;
        while (reports.length) {
          var report = reports.shift();
          try {
            report.run();
          } catch (e) {
            /* the tracker's own; nothing to tell */
          }
        }
        return ran;
      },
    };
  }
  global.__exact_time = Object.freeze({ refuseClock: refuseClock, timers: timers });
})(globalThis);
