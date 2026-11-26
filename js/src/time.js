// What a data module has of time, for prelude.js: virtualized clock reads
// (`virtualClock`, LLP 1195.000 D2) over the host's epoch + runner clock with
// bake UTC pinning, and an answer's own timers on the runner's clock (`timers`).
// prelude.js takes `__exact_time` and deletes it.
(function (global) {
  "use strict";
  function fixed(object, name, value) {
    Object.defineProperty(object, name, { value: value, writable: false, configurable: false });
  }
  function parseClockPair(raw) {
    if (typeof raw !== "string" || !raw) return [0, 0];
    var comma = raw.indexOf(",");
    if (comma < 0) return [0, Number(raw) || 0];
    var epoch = Number(raw.slice(0, comma)),
      clock = Number(raw.slice(comma + 1));
    return [epoch === epoch ? epoch : 0, clock === clock ? clock : 0];
  }
  var DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  function pad2(n) {
    return n < 10 ? "0" + n : String(n);
  }
  function pad4(y) {
    var sign = y < 0 ? "-" : "",
      a = Math.abs(y),
      s = String(a);
    while (s.length < 4) s = "0" + s;
    return sign + s;
  }
  // Pin native Date local-timezone methods and omitted-zone Intl.DateTimeFormat
  // to UTC during bake so build-machine timezone never leaks (LLP 1195.000 D2.6).
  function pinBakeUtc(NativeDate) {
    var proto = NativeDate.prototype;
    var getTime = proto.getTime;
    function checkDate(self) {
      return getTime.call(self);
    }
    function utcDateStr(self) {
      var t = checkDate(self);
      if (t !== t) return "Invalid Date";
      return (
        DAYS[proto.getUTCDay.call(self)] +
        " " +
        MONTHS[proto.getUTCMonth.call(self)] +
        " " +
        pad2(proto.getUTCDate.call(self)) +
        " " +
        pad4(proto.getUTCFullYear.call(self))
      );
    }
    function utcTimeStr(self) {
      var t = checkDate(self);
      if (t !== t) return "Invalid Date";
      return (
        pad2(proto.getUTCHours.call(self)) +
        ":" +
        pad2(proto.getUTCMinutes.call(self)) +
        ":" +
        pad2(proto.getUTCSeconds.call(self)) +
        " GMT+0000 (Coordinated Universal Time)"
      );
    }
    proto.getTimezoneOffset = function getTimezoneOffset() {
      var t = checkDate(this);
      return t !== t ? NaN : 0;
    };
    proto.getFullYear = proto.getUTCFullYear;
    proto.getMonth = proto.getUTCMonth;
    proto.getDate = proto.getUTCDate;
    proto.getDay = proto.getUTCDay;
    proto.getHours = proto.getUTCHours;
    proto.getMinutes = proto.getUTCMinutes;
    proto.getSeconds = proto.getUTCSeconds;
    proto.getMilliseconds = proto.getUTCMilliseconds;
    proto.toDateString = function toDateString() {
      return utcDateStr(this);
    };
    proto.toTimeString = function toTimeString() {
      return utcTimeStr(this);
    };
    proto.toString = function toString() {
      var t = checkDate(this);
      return t !== t ? "Invalid Date" : utcDateStr(this) + " " + utcTimeStr(this);
    };
  }
  // LLP 1195.000 D2: virtualize Date.now(), new Date(), Date(), performance.now(),
  // and omitted-date Intl.DateTimeFormat over Exact's deterministic host clock
  // (epochAtZero + runner_clock_ms), marking runtime clock taint on every read.
  function virtualClock(tracker, readClock, isBake, inOverlay) {
    var NativeDate = global.Date.prototype.constructor,
      construct = Reflect.construct;
    if (isBake) pinBakeUtc(NativeDate);
    function wallNow(api) {
      if (inOverlay && inOverlay()) throw new Error(api + " is unavailable in an overlay");
      var pair = parseClockPair(readClock("wall"));
      return Math.floor(pair[0] + pair[1]);
    }
    function perfNow() {
      if (inOverlay && inOverlay()) throw new Error("performance.now() is unavailable in an overlay");
      var pair = parseClockPair(readClock("perf"));
      return pair[1];
    }
    var InputDate = new Proxy(NativeDate, {
      apply: function () {
        return new NativeDate(wallNow("Date()")).toString();
      },
      construct: function (target, args, newTarget) {
        if (!args.length) return construct(target, [wallNow("new Date()")], newTarget);
        return construct(target, args, newTarget);
      },
    });
    fixed(NativeDate, "now", function now() {
      return wallNow("Date.now()");
    });
    fixed(NativeDate.prototype, "constructor", InputDate);
    fixed(global, "Date", InputDate);
    fixed(
      global,
      "performance",
      Object.freeze({
        now: perfNow,
      })
    );
    if (global.Intl && global.Intl.DateTimeFormat) {
      var dateFormat = global.Intl.DateTimeFormat.prototype,
        engineFormat = dateFormat;
      while (engineFormat && !(Object.getOwnPropertyDescriptor(engineFormat, "format") || {}).get)
        engineFormat = Object.getPrototypeOf(engineFormat);
      if (engineFormat && engineFormat !== dateFormat) {
        dateFormat = engineFormat;
        global.Intl.DateTimeFormat = engineFormat.constructor;
      }
      if (isBake) {
        var BaseDTF = global.Intl.DateTimeFormat;
        var argsInUTC = function (args) {
          var locales = args[0],
            options = args[1];
          if (options === null || (options && options.timeZone !== undefined)) return args;
          return [locales, Object.create(Object(options), { timeZone: { value: "UTC", enumerable: true } })];
        };
        global.Intl.DateTimeFormat = new Proxy(BaseDTF, {
          apply: function (target, receiver, args) {
            return Reflect.apply(target, receiver, argsInUTC(args));
          },
          construct: function (target, args, derived) {
            return Reflect.construct(target, argsInUTC(args), derived);
          },
        });
      }
      var getFormat = Object.getOwnPropertyDescriptor(dateFormat, "format").get,
        formats = new WeakMap(),
        apply = Reflect.apply,
        engineOf = function (f) {
          return f;
        };
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
      function virtualFormat(fn, name) {
        return new Proxy(fn, {
          apply: function (target, receiver, args) {
            var nextArgs = args[0] === undefined ? [wallNow("Intl.DateTimeFormat." + name + "()")] : args;
            return apply(target, engineOf(receiver), nextArgs);
          },
        });
      }
      Object.defineProperty(dateFormat, "format", {
        configurable: false,
        get: function () {
          var native = getFormat.call(engineOf(this));
          if (!formats.has(this)) formats.set(this, virtualFormat(native, "format"));
          return formats.get(this);
        },
      });
      if (typeof dateFormat.formatToParts === "function") {
        fixed(dateFormat, "formatToParts", virtualFormat(dateFormat.formatToParts, "formatToParts"));
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
  // runs them between tasks. A callback's throw is reported (`raise`, with
  // its journal line), as a browser reports it. An answer that gives its value or is let go
  // clears the rest (`drop`): their callbacks never run. `setInterval`, the
  // frame callbacks and the clock reads stay refused. Hermes's rejection
  // tracker schedules its report with `setTimeout` too: `report(callback)`
  // gives what runs that one, alone, at the prelude's next `checkpoint`, which
  // is when a browser fires `unhandledrejection`.
  function timers(owner, clock, report, raise, sources) {
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
      var reporting = typeof callback === "function" ? report(callback) : undefined;
      if (reporting) {
        reports.push({ id: next, run: reporting });
        return next++;
      }
      if (typeof callback !== "function")
        throw new TypeError("setTimeout(): the callback must be a function; a string of code is never run");
      // `long`, as WebIDL converts it
      return arm("setTimeout()", callback, Math.max(Number(delay) | 0, 0), Array.prototype.slice.call(arguments, 2));
    });
    fixed("clearTimeout", function clearTimeout(id) {
      id = Number(id) | 0; // `long`, as WebIDL converts it: a handle's string or a boxed number clears it too
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
          raise(e, "data: a timer's callback threw: " + (e && e.message !== undefined ? e.message : String(e)));
        }
      },
      drop: function (call) {
        armed.forEach(function (t, id) {
          if (t.call === call) armed.delete(id);
        });
      },
      // An `AbortSignal.timeout` still to fire, or one composed of it: the
      // whole ms left, rounded up so the host's deadline is never before the
      // signal's (the runner's clock is fractional; the wire's deadline is an
      // integer), the deadline a fetch carrying it gives the host; `expire` aborts
      // it when that passes, its timer dropped or not, as a browser's fires:
      // a fetch its answer let go runs to its end (LLP 1146 D2), and another
      // answer may carry the same signal.
      left: function (signal) {
        var s = deadline(signal);
        return s && !signal.aborted ? Math.max(1, Math.ceil(s.due - time())) : undefined;
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
  global.__exact_time = Object.freeze({ virtualClock: virtualClock, timers: timers });
})(globalThis);
