// Work that outlives its answer (LLP 1146 D2): a fetch with effects whose
// answer was let go is adopted, not rejected (`__exact_adopt`), and its reply
// reaches the let-go answer's continuation in a module turn, which the
// executor runs outside every answer and commit (js/src/module_work.rs), so
// what that continuation writes to the store stands on its own. A live
// answer's step that such a turn settles runs in that answer's own turn, so
// its reads, writes and fetches are its own. prelude.js takes
// `__exact_module_work` and deletes it.
(function (global) {
  "use strict";
  // The prelude's hooks: `host` (the door) and `live(call)` (a call still answering).
  global.__exact_module_work = function (hooks) {
    var host = hooks.host;
    // The turn running now, by its call's id: an answer's, MODULE's, or null
    // (a let-go chain, a background delivery, module evaluation). An id, so
    // that no ended call's value is kept alive by it.
    var MODULE = Object.freeze({ id: -1 });
    var turn = null;
    var ready = new Map(); // live answer's call -> [steps for its own turn]
    var discarded = []; // steps of answers let go before their turn, run with no store
    var tearingDown = false;
    var adopted = 0; // adopted fetches in flight
    function waiting() {
      var n = 0;
      ready.forEach(function (steps) {
        n += steps.length;
      });
      return n;
    }
    // What may still settle an answer waiting on module work, for the executor.
    function note() {
      host(22, "state", JSON.stringify({ outstanding: adopted + waiting() }));
    }
    function runDiscarded() {
      turn = null;
      for (var n = 0; discarded.length && n < 1024; n++) discarded.shift()();
    }
    return Object.freeze({
      MODULE: MODULE,
      // The turn now (the executor's `__exact_turn`): an answer's call, MODULE or null.
      setTurn: function (t) {
        turn = t ? t.id : null;
        var steps = t && t !== MODULE ? ready.get(t) : null;
        if (steps) {
          ready.delete(t);
          steps.forEach(function (s) {
            s();
          });
          note();
        }
      },
      // Finish readied storage chains when no further answer turn can run
      teardown: function () {
        tearingDown = true;
        ready.forEach(function (steps) {
          steps.forEach(function (s) {
            discarded.push(s);
          });
        });
        ready.clear();
        runDiscarded();
        note();
      },
      // The async helper asks before each step whether it runs here. In a
      // module or background turn a live answer's step waits for its own turn;
      // anything else runs where it is, the let-go answer's continuation
      // included. `step(key, arg)` is the helper's next step, bound only when
      // deferred: this runs before every step of every async function.
      route: function (at, step, key, arg) {
        if (tearingDown || !at || (turn !== null && turn !== MODULE.id) || !hooks.live(at)) return false;
        if (!ready.has(at)) ready.set(at, []);
        ready.get(at).push(function () {
          step(key, arg);
        });
        note();
        return true;
      },
      // After a module turn, with no store: what let-go answers had waiting.
      runDiscarded: runDiscarded,
      // An answer let go: its steps waiting for its turn move to the steps
      // the let-go chain runs with no store, and stop counting as
      // outstanding. How many moved.
      forgot: function (call) {
        var steps = ready.get(call);
        if (!steps) return 0;
        ready.delete(call);
        steps.forEach(function (s) {
          discarded.push(s);
        });
        note();
        return steps.length;
      },
      inModuleTurn: function () {
        return turn === MODULE.id;
      },
      readied: function (call) {
        return ready.has(call);
      },
      adopted: function () {
        adopted++;
        note();
      },
      landed: function () {
        adopted--;
        note();
      },
      outstanding: function () {
        return adopted + waiting() > 0;
      },
    });
  };
})(globalThis);

// A parked answer needs a turn only when its chain or its next operation moved (LLP 1162 D3): the prelude's
// `__exact_ready`. prelude.js takes `__exact_waiting_answers` and deletes it.
(function (global) {
  "use strict";
  global.__exact_waiting_answers = function (calls, pending, work, headOwner) {
    return function (ids) {
      var own = new Set(),
        unclaimed = false,
        at = headOwner(),
        ready = [];
      pending.forEach(function (p) {
        if (p.claimed || p.stream || p.module) return;
        if (p.call && p.call.status === "pending" && !p.call.letGo && calls.has(p.call.id)) own.add(p.call.id);
        else unclaimed = true;
      });
      JSON.parse(ids).forEach(function (id) {
        var c = calls.get(id);
        if (!c) return;
        var claims = unclaimed && c.status === "pending" && !c.storage;
        if (claims) unclaimed = false;
        if (
          c.ran ||
          c.status !== "pending" ||
          work.readied(c) ||
          own.has(id) ||
          (c.storage && (!at || at === c)) ||
          claims
        )
          ready.push(id);
      });
      return JSON.stringify(ready);
    };
  };
})(globalThis);
