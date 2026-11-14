// The executor's prelude (LLP 1027 D1a, D10): what a data module finds in
// its global object beyond the language — `fetch` over the host's ticket
// path, the store the seam hands to `answer`, and the three functions the
// executor calls. Compiled to bytecode by build.rs with the same hermesc as
// the module; evaluated once, before the module, at `Module::load`.
//
// Nothing here does I/O. `fetch` records a request through the one host
// function and returns a Promise the executor resolves when the host has
// run the request under the app's grants (LLP 1016 D1/D6); the store's
// reads are counted and its writes grant-checked in Rust (LLP 1018).
(function (global) {
  "use strict";
  // (op, a, b) -> string | undefined, js/src/door.rs's ops: 1 a request, 2-4 and 18-20 the store, 5 storage,
  // 6 native, 7 pure, 8 secure randomness drawn, 9/10 canvas, 11 the agent's random bytes, 12 auth, 13 a journal line,
  // 16 a request withdrawn unsent, 21 the runner's clock as the call began, 22 module work's state (module-work.js), 23
  // a handed-out request the app aborted, 27 a native request/call id.
  var host = global.__exact_host;
  delete global.__exact_host;
  var browserSecrets = global.__exact_browser_secrets;
  delete global.__exact_browser_secrets;
  // The seam's bytes (binary.js): base64, UTF-8 lengths, a stream message's data.
  var binary = global.__exact_binary;
  delete global.__exact_binary;
  var toBase64 = binary.toBase64,
    copyBytes = binary.copyBytes;
  function fixed(object, name, value) {
    Object.defineProperty(object, name, { value: value, writable: false, configurable: false });
  }
  // The prelude's parts, each a script of its own evaluated just before this one
  // (time.js, crypto.js, module-work.js, fetch.js, auth.js, store.js, storage.js,
  // reply.js; host/web-js/glue.mjs `MODULE_PRELUDE`), each given what it needs of
  // this one where it is taken below. First, no clock to read (time.js),
  // installed before the app can capture an alias.
  var time = global.__exact_time;
  delete global.__exact_time;
  var tracker = global.HermesInternal;
  time.refuseClock(tracker);
  // The runtime's own journal line (LLP 1097 D8): a failed storage
  // operation, an unhandled rejection. Natively it is the runner's journal
  // (`logs`) as it is; a realm without that door writes it to its console.
  function journal(line) {
    var written;
    try {
      written = host(13, String(line), "");
    } catch (e) {
      written = undefined;
    }
    if (written !== "1" && global.console) global.console.error(String(line));
  }
  // The bake answers on an engine without some of Intl's constructors, where `new Intl.Collator()` says only
  // "undefined cannot be used as a constructor" (expo/exact#447). At bake such an error names the ones missing.
  // Nothing is installed: Intl is frozen by then, and an answer that tests for one must still see it absent.
  var INTL_CONSTRUCTORS = ["Collator", "PluralRules", "RelativeTimeFormat", "ListFormat", "Segmenter", "DisplayNames"];
  function bakeIntl(e) {
    try {
      if (!(e instanceof TypeError) || !/cannot be used as a constructor|is not a function/.test(String(e.message))) return e;
      var intl = global.Intl;
      if (!intl || host(12, "bake", "") !== "1") return e;
      var missing = INTL_CONSTRUCTORS.filter(function (name) { return typeof intl[name] !== "function"; });
      if (!missing.length) return e;
      e.message += " (the bake's engine has no Intl." + missing.join(", Intl.") + ": if the answer uses one of them," +
        (missing.indexOf("Collator") >= 0 ? " compare strings with a.localeCompare(b), which the bake has, or" : "") +
        " compute it where the app runs, not in an answer the build bakes)";
    } catch (_) {}
    return e;
  }
  function errorText(e) {
    try {
      return e && typeof e === "object" && e.message !== undefined ? String(e.message) : String(e);
    } catch (_) {
      return "(an error that cannot be read)";
    }
  }
  // An answer's own timers, on the runner's clock (op 21), and the timer
  // Hermes's rejection tracker asks for (time.js `timers`). Every
  // unhandled rejection in the module reaches the journal, as a page's
  // `unhandledrejection` reaches its console (LLP 1097 D8).
  // The tracker's report is the timer it arms from inside its rejection
  // hook (Hermes's `Promise._C`), told by that place, not by its name alone.
  var reporting = 0,
    hooked = false;
  var timers = time.timers(
    timerOwner,
    function () {
      return host(21, "", "");
    },
    journal,
    function (callback) {
      return (reporting > 0 || !hooked) && callback.name === "bound onUnhandled";
    },
    function (signal) {
      try {
        return abortHooks ? abortHooks.own(signal).sources : null;
      } catch (e) {
        return null;
      }
    }
  );
  if (tracker && typeof tracker.enablePromiseRejectionTracker === "function") {
    tracker.enablePromiseRejectionTracker({
      allRejections: true,
      onUnhandled: function (id, error) {
        journal("data: unhandled rejection: " + errorText(error));
        leftOpen();
      },
      onHandled: function () {},
    });
    var reject = global.Promise._C;
    if (typeof reject === "function") {
      hooked = true;
      global.Promise._C = function (promise, error) {
        reporting++;
        try {
          return reject.call(this, promise, error);
        } finally {
          reporting--;
        }
      };
    }
  }
  // A browser realm (the wasm host's module realm) reports its own rejections (module-glue.js).
  if (!tracker && typeof global.addEventListener === "function")
    global.addEventListener("unhandledrejection", function () {
      leftOpen();
    });
  // Natively: `(op, name, arrayBuffer) -> string`, the byte work in Rust.
  var bytesDoor = global.__exact_bytes;
  delete global.__exact_bytes;
  // LLP 1133.003 §9's stub decode, natively and under the agent only (op
  // 11 answers there): the gate's fixture calls it until `VideoDecoder`.
  if (binary.gateDecode && host(11, "", "0") !== undefined) global.__exact_gate_decode = binary.gateDecode;
  var initializing = true; // until the executor first calls in
  function answerOnly(api) {
    if (!currentCall || currentCall.status !== "pending")
      throw new Error(
        api +
          " is unavailable " +
          (initializing ? "during module initialization" : "outside an answer") +
          "; call it inside an answer"
      );
  }
  // Host work an answer waits on that is neither a fetch nor storage (the
  // browser's digest settles in a later task): on the web it joins the
  // answer's storage turn, so the answer is pending on it, not on nothing.
  // Natively the work is already done and the promise settles in the drain.
  function hostWork(promise) {
    var call = currentCall;
    var nativeStorage = storageCalls.native(),
      work = nativeStorage && nativeStorage.work;
    if (!call || call.status !== "pending" || typeof work !== "function") return promise;
    call.storage++;
    storing.add(call);
    return work(promise).then(
      function (value) {
        currentCall = call;
        unstore(call);
        return value;
      },
      function (error) {
        currentCall = call;
        unstore(call);
        throw error;
      }
    );
  }
  // `crypto` (crypto.js), whose kept keys the store keeps (store.js).
  var keys = global.__exact_crypto({
    host: host,
    copyBytes: copyBytes,
    bytesDoor: bytesDoor,
    answerOnly: answerOnly,
    hostWork: hostWork,
    current: function () {
      return currentCall;
    },
    overlaying: function () {
      return overlaying;
    },
    inOverlay: inOverlay,
    store: function () {
      return store;
    },
  });
  delete global.__exact_crypto;
  // --- fetch: a request the host runs; a Promise for its reply (fetch.js) --
  // Native ids survive runtime retirement and are shared across modules.
  var nextLocalId = 1;
  function nextId() {
    return bytesDoor ? Number(host(27, "", "")) : nextLocalId++;
  }
  // ticket -> { resolve, reject, call, claimed, stream, signal, release }:
  // `claimed` once a settle has handed the executor its request, `release`
  // ends its watch on `signal`.
  var pending = new Map(),
    // the answer a fetch belongs to
    currentCall = null,
    // a socket's mapper answering a frame (D7)
    mapping = null,
    // any mapper call
    inMapper = false;
  // Work that outlives its answer (LLP 1146 D2, module-work.js): an adopted fetch's reply reaches its let-go answer
  // in a module turn; `route` keeps a live answer's async step in its own turn.
  var work = global.__exact_module_work({
    host: host,
    live: function (c) {
      return !!c && c !== background && !c.stream && !c.module && c.status === "pending" && !c.replied && !c.letGo;
    },
  });
  delete global.__exact_module_work;
  // The bundle's async helper resumes each step under its answer (LLP 1133.002 D14).
  global.__exact_async_context = Object.freeze({
    current: function () {
      return currentCall;
    },
    swap: function (call) {
      var before = currentCall;
      currentCall = call;
      if (call) call.ran = true;
      return before;
    },
    route: work.route,
  });
  // Calls with storage steps in flight, settled or not: a step a settled
  // call left behind is still the module's work (see `settle`).
  var storing = new Set();
  function unstore(call) {
    call.storage--;
    if (!call.storage) storing.delete(call);
  }
  // --- background work (LLP 1097 D1–D3) ------------------------------------
  // An answer replies when its value is ready, as in a browser: storage it
  // started and did not await moves to `background`, the module's own owner,
  // never replied to, let go or claimed. The executor runs it under a ticket
  // of its own and delivers it with `currentCall = background`. Only on the
  // main thread (D6): a worker placement's answer still waits for its
  // storage, so the flag is set by the executor that runs inline alone.
  var moving = false;
  var background = {
    id: 0,
    status: "pending",
    value: undefined,
    error: undefined,
    tickets: [],
    storage: 0,
    replied: false,
    background: true,
  };
  // The module's storage operations that landed and failed, and the last
  // failure's line (`state.background`, LLP 1097 D8).
  var storageCounts = { writes: 0, done: 0, failed: 0, last: null };
  // Set before the first answer, never after: an app cannot turn it on.
  global.__exact_main_thread = function () {
    if (initializing) moving = true;
  };
  // Where a completion's count lives: the call that issued it, or the
  // background once that call's storage moved there.
  function owner(call) {
    return call.moved ? background : call;
  }
  // One queue for every storage operation of the module (D3), in the order
  // issued; one in flight at a time. So only the issuer of the operation in
  // flight waits on the store's shared completions: one completion wakes one
  // waiter, and the next answer's read sees the write issued before it.
  var MAX_QUEUED = 256,
    queue = [],
    head = null;
  function issue(op) {
    head = op;
    var promise;
    try {
      promise = op.run();
    } catch (e) {
      promise = Promise.reject(e);
    }
    op.promise = promise;
    promise.then(
      function (value) {
        landed(op, true, value);
      },
      function (error) {
        landed(op, false, error);
      }
    );
  }
  function landed(op, ok, value) {
    // A revoked image's late completion must not clear the next head.
    if (op.settled) return;
    op.settled = true;
    if (head === op) {
      head = null;
      // The next operation was issued before anything this one's reaction
      // issues: it starts first.
      if (queue.length) issue(queue.shift());
    }
    op.settle(ok, value);
  }
  // The operation in flight's owner, or null.
  function headOwner() {
    return head ? owner(head.call) : null;
  }
  // Native image cancellation uses Ibex's identity on the original Promise.
  // Other promises may resolve to raw capabilities (a database or statement).
  global.__exact_storage_promise = function () {
    return head && (head.method === "compressImage" || head.method === "transformImage") ? head.promise : undefined;
  };
  // Neither Ibex's abort hooks nor a browser's dependent signal can be stopped by a listener on the caller's signal.
  var abortHooks = global.__exact_ibex2_abort_hooks;
  delete global.__exact_ibex2_abort_hooks;
  // --- scheduler.yield (WICG Prioritized Task Scheduling; LLP 1141.001 D10): `settle` parks the
  // answer as `yielded`, the host asks it again at once (`Dispatch::Again`), and `__exact_yielded`
  // resolves its yields in that new turn. Counted: an answer awaiting another's yielding work waits.
  var yielded = 0;
  function yieldTurn() {
    if (overlaying) return Promise.reject(inOverlay("scheduler.yield()"));
    var call = currentCall;
    if (call && (call.letGo || (call.replied && call.status === "pending")))
      return Promise.reject(new DOMException("the answer was let go", "AbortError"));
    // No answer's turn to end (module evaluation, a mapper where it cannot await, background work, a continuation after
    // its reply): go on.
    if (!call || call === background || (inMapper && !call.mapper) || call.status !== "pending" || call.replied)
      return Promise.resolve();
    return new Promise(function (resolve, reject) {
      (call.yields || (call.yields = [])).push([resolve, reject]);
      yielded++;
    });
  }
  // End a call's yields, resolved or rejected with `error`: how many there were.
  function endYields(call, error) {
    var ys = call.yields || [];
    call.yields = [];
    yielded -= ys.length;
    ys.forEach(function (y) {
      if (error) y[1](error);
      else y[0]();
    });
    return ys.length;
  }
  fixed(
    global,
    "scheduler",
    Object.freeze({
      yield: yieldTurn,
      postTask: function () {
        throw new Error(
          "scheduler.postTask() is unavailable in data sources; split long work with await scheduler.yield()"
        );
      },
    })
  );
  var responses = binary.responses(),
    Response = responses.Response,
    hostReply = responses.reply;

  var FetchError = binary.FetchError;

  // A mutation does one kind of effect, socket operations or a fetch: whichever
  // comes second refuses, and the mutation with it, before either leaves.
  var SOCKET_AND_FETCH = "a mutation cannot both send on a socket and fetch";
  // A resource's two-way socket (LLP 1016.000 D7) and a listener's connections (LLP 1133.005 D6): their handles
  // (binary.js `sockets`).
  var sockets = binary.sockets({
    host: host,
    current: function () {
      return currentCall;
    },
    mapping: function () {
      return mapping;
    },
    inMapper: function () {
      return inMapper;
    },
    conflict: SOCKET_AND_FETCH,
  });
  var socketOwner = sockets.socketOwner,
    listenOwner = sockets.listenOwner,
    handSocketOps = sockets.hand;
  // `fetch`, `Response` and `exactListen` (fetch.js).
  var fetches = global.__exact_fetch({
    nextId: nextId,
    host: host,
    binary: binary,
    journal: journal,
    pending: pending,
    work: work,
    background: background,
    timers: timers,
    abortHooks: abortHooks,
    bytesDoor: bytesDoor,
    sockets: sockets,
    Response: Response,
    FetchError: FetchError,
    conflict: SOCKET_AND_FETCH,
    current: function () {
      return currentCall;
    },
    mapping: function () {
      return mapping;
    },
    overlaying: function () {
      return overlaying;
    },
    inOverlay: inOverlay,
  });
  delete global.__exact_fetch;
  var request = fetches.request,
    settled = fetches.settled,
    withdrawFetches = fetches.withdrawFetches;

  // Signing in through the system browser (auth.js).
  global.__exact_auth({ host: host, answerOnly: answerOnly });
  delete global.__exact_auth;

  // --- the store (store.js) and storage (storage.js) ------------------------
  var store = global.__exact_store({
    host: host,
    presence: binary.presence,
    browserSecrets: browserSecrets,
    keys: keys,
    call: function (id) {
      return calls.get(Number(id));
    },
    current: function () {
      return currentCall;
    },
    setCurrent: function (call) {
      currentCall = call;
    },
    overlaying: function () {
      return overlaying;
    },
    inOverlay: inOverlay,
  });
  delete global.__exact_store;
  var storageCalls = global.__exact_storage_calls({
    host: host,
    journal: journal,
    errorText: errorText,
    background: background,
    owner: owner,
    storing: storing,
    unstore: unstore,
    storageCounts: storageCounts,
    current: function () {
      return currentCall;
    },
    setCurrent: function (call) {
      currentCall = call;
    },
    moving: function () {
      return moving;
    },
    initializing: function () {
      return initializing;
    },
    overlaying: function () {
      return overlaying;
    },
    inOverlay: inOverlay,
    full: function () {
      return head && queue.length >= MAX_QUEUED;
    },
    MAX_QUEUED: MAX_QUEUED,
    enqueue: function (op) {
      if (head) queue.push(op);
      else issue(op);
    },
  });
  delete global.__exact_storage_calls;
  var storage = storageCalls.storage,
    storageError = storageCalls.storageError;
  var leftOpen = storageCalls.leftOpen,
    freeLocks = storageCalls.freeLocks;

  // --- the seam ------------------------------------------------------------
  var calls = new Map(),
    gcOwed = false; // calls: id -> { id, status, value, error, tickets }
  // Large native strings travel beside the JSON skeleton by explicit paths (no property
  // spelling reserved); the serializer owns getters, toJSON, omissions and cycles. One
  // whose answer outlived a collection owes a full one, once no answer awaits (shim.cc).
  var captureString = global.__exact_capture_string,
    gcEpoch =
      global.__exact_gc_epoch ||
      function () {
        return 0;
      };
  delete global.__exact_capture_string;
  delete global.__exact_gc_epoch;
  // Undrained synchronous app callbacks can queue any waiter's reactions (LLP 1162 D3).
  function readyAll() {
    calls.forEach(function (c) {
      c.ran = true;
    });
  }
  function cleanUp() {
    freeLocks();
    var waiting = false;
    calls.forEach(function (c) {
      if (!c.stream && !c.letGo) waiting = true;
    });
    if (gcOwed && !waiting) {
      gcOwed = false;
      gcEpoch(true);
    }
  }
  // An answer's value or failure as the seam carries it (reply.js).
  var replies = global.__exact_reply({
    captureString: captureString,
    keptReply: binary.keptReply,
    owe: function () {
      gcOwed = true;
    },
  });
  delete global.__exact_reply;
  var ok = replies.ok,
    fail = replies.fail;
  // A fetch with no live owner is claimed by one waiter (LLP 1162 D2).
  var moduleWide = bytesDoor !== undefined;
  function claim(call, ticket) {
    var p = pending.get(ticket);
    p.claimed = true;
    if (p.call !== call) {
      p.call = call;
      call.tickets.push(ticket);
    }
    // Its own timer, if due first, resumes it while the fetch runs on as module work (`__exact_adopt`), as a browser
    // races them. A worker's answer, or one in the web's module realm, waits on one thing at a time: the timer runs
    // after.
    var due = moduleWide && moving && !p.stream ? timers.race(call, p.signal) : undefined;
    if (due === undefined && !p.timed && !call.toldRace && timers.armed(call)) {
      call.toldRace = true;
      journal(
        "data: an answer awaits a fetch while a timer of its own is armed: the timer runs only after the fetch lands, so it cannot cut the fetch short as it can on the web; give the fetch AbortSignal.timeout or exactTimeout"
      );
    }
    return JSON.stringify({ tag: 1, call: call.id, ticket: ticket, timer: due });
  }
  // An answer is given when its value is ready, as in a browser (LLP 1097
  // D1): the storage it started and did not await finishes after it, as the
  // background's, so nothing is stranded (kanban F22's reason to wait). On a
  // worker placement, which runs one turn to its end, it still waits.
  function reply(call, text) {
    endYields(call); // a branch its value did not wait for goes on, as a browser's does
    timers.drop(call); // its timers are its own (time.js `timers`)
    call.replied = true;
    calls.delete(call.id);
    cleanUp();
    return text;
  }
  function settle(call, final) {
    var secret = call.secrets && call.secrets[0];
    if (secret && !secret.held) return JSON.stringify({ tag: 1, call: call.id, ticket: 0, secret: secret.name });
    // A fetch its continuation tried after it staged socket operations
    // (refused at the fetch) refuses the mutation too, before any frame.
    if (call.socketConflict && call.status === "done") {
      withdrawFetches(call);
      call.status = "failed";
      call.error = new Error(SOCKET_AND_FETCH);
    }
    // Claims follow async ownership, including a waiter's continuation another reply ran (LLP 1162 D2).
    var finished = call.status === "done" || call.status === "failed",
      others = false;
    if (finished && moduleWide)
      calls.forEach(function (c) {
        if (c !== call && !c.letGo && c.status === "pending") others = true;
      });
    for (var i = 0; i < call.tickets.length; i++) {
      var own = pending.get(call.tickets[i]);
      if (own && !own.claimed && own.call === call && (own.early || !others)) {
        if (!moduleWide || !call.set) return claim(call, call.tickets[i]);
        var out = [],
          held = false;
        pending.forEach(function (p) {
          if (p.call === call && p.claimed && !p.module && (!own.set || !p.set)) held = true;
        });
        if (held) break;
        for (var j = i; j < call.tickets.length; j++) {
          var p = pending.get(call.tickets[j]);
          if (!p || p.claimed || p.call !== call || !(p.early || !others)) continue;
          if (out.length && (!own.set || !p.set)) break;
          out.push(JSON.parse(claim(call, call.tickets[j])));
        }
        var first = out.shift();
        call.more = out;
        return JSON.stringify(first);
      }
    }
    if (!finished && call.yields && call.yields.length)
      return JSON.stringify({ tag: 1, call: call.id, ticket: 0, yielded: true });
    if (call.storage > 0 && !call.lost) {
      if (finished && moving) {
        // Its value is ready: what it started and did not await is the
        // background's (D1, D2), and it replies now.
        background.storage += call.storage;
        call.storage = 0;
        storing.delete(call);
        storing.add(background);
        call.moved = true;
      } else {
        // Only the head's owner can deliver the shared storage queue (LLP 1097 D3).
        var at = headOwner();
        if (!at || at === call) return JSON.stringify({ tag: 1, call: call.id, ticket: 0 });
        return JSON.stringify({ tag: 1, call: call.id, ticket: 0, waiting: true });
      }
    }
    if (finished) {
      // A fetch of its own already handed out and still in flight: asked
      // again once it lands (natively; the module realm runs one at a time).
      // (One its timer outran runs on alone.)
      var inFlight = false;
      pending.forEach(function (p) {
        if (p.call === call && p.claimed && !p.module) inFlight = true;
      });
      if (moduleWide && inFlight) return JSON.stringify({ tag: 1, call: call.id, ticket: 0, waiting: true });
      if (call.status === "done") handSocketOps(call);
      return reply(call, call.status === "done" ? ok(call.value, call.epoch !== gcEpoch()) : fail(bakeIntl(call.error)));
    }
    // Its own timers (time.js `timers`): the earliest is due on the runner's clock.
    var due = timers.wait(call),
      later = due === undefined || due > timers.now();
    if (moduleWide) {
      var unclaimed;
      pending.forEach(function (p, ticket) {
        if (
          unclaimed === undefined &&
          !p.claimed &&
          !p.stream &&
          !p.module &&
          (!p.call || p.call.status !== "pending" || p.call.letGo || !calls.has(p.call.id))
        )
          unclaimed = ticket;
      });
      if (unclaimed !== undefined) return claim(call, unclaimed);
      var left = false;
      // Never the background's: only its own round holds the storage
      // ticket (D2); an answer awaiting it waits with the module below.
      storing.forEach(function (c) {
        if (c !== background && !calls.has(c.id)) left = true;
      });
      if (left) return JSON.stringify({ tag: 1, call: call.id, ticket: 0 });
      // Another answer's work may settle it before its timer: whichever comes first asks it again (not on a
      // worker, which runs one turn to its end and so cannot wait on another's).
      if (
        !final &&
        (due === undefined || (moving && later)) &&
        (pending.size || storing.size || yielded || work.outstanding())
      )
        return JSON.stringify({ tag: 1, call: call.id, ticket: 0, waiting: true, timer: due });
    } else if (due === undefined && moving && !final && background.storage > 0) {
      // The page's realm keeps its background-delivery rule (LLP 1097 D7).
      return JSON.stringify({ tag: 1, call: call.id, ticket: 0, waiting: true });
    }
    // A later turn can clear even a timer that parked alone: keep its owner releasable (LLP 1162 D3).
    if (moduleWide && moving && due !== undefined && later && host(21, "", "") !== undefined)
      return JSON.stringify({ tag: 1, call: call.id, ticket: 0, waiting: true, timer: due });
    if (due !== undefined) return JSON.stringify({ tag: 1, call: call.id, ticket: 0, timer: due });
    call.replied = true;
    calls.delete(call.id);
    cleanUp();
    if (moduleWide)
      return fail(
        new Error(
          "the answer is pending on nothing: it awaits a promise that no fetch or storage step " +
            "in flight in this module will settle"
        )
      );
    return fail(
      new Error(
        "the answer is pending on nothing: no host operation it started will resolve it. " +
          "An answer that awaits a promise another answer started (a fetch shared between answers, or a queue chained through a " +
          "fetch another answer is waiting on) waits on work it does not own in the web's module realm; make each answer's own " +
          "fetch, or share the resolved value rather than the promise. Storage is different: an answer queued behind another's " +
          "storage turn waits for it"
      )
    );
  }
  var nativeCall = (global.__exact_module_sync = function (request) {
    // `native.call`, and a module binding's synchronous form (LLP 1133.002 D14)
    if (overlaying) throw inOverlay("native.call()");
    if (currentCall === background) {
      journal("data: native.call() called from background work: it was never run");
      throw new Error("native.call() called from background work: it was never run. Call it inside an answer");
    }
    if (!currentCall || currentCall.status !== "pending") throw new Error("native call outside an answer");
    try {
      return JSON.parse(host(6, "call", JSON.stringify(request)));
    } catch (e) {
      // at bake, no module: storage's refusal (LLP 1067 D4)
      throw e && e.message === "bake"
        ? storageError("native.call() is unavailable during bake: the build links no native module", "bake")
        : e;
    }
  });
  // The executor: `__exact_call(source, argsJson)` → tag 3 with a call id (a value given at once too: see its end) —
  // then it drains microtasks and asks `__exact_settle(id)`, which is tag 0/2, or tag 1 with the ticket of the fetch
  // the answer is waiting on (0: its storage; 0 and `waiting`: another answer's work). `__exact_fulfill(ticket,
  // outcomeJson)` resolves that fetch; drain and settle again. `__exact_settle(id, "final")` refuses an answer still
  // waiting on nothing of its own.
  // A handler a module calls back (LLP 1133.016 D9): the `tools` export's, its value as JSON text.
  function tool(name, args) {
    var handlers = global.exact.tools,
      f = handlers && Object.prototype.hasOwnProperty.call(handlers, name) ? handlers[name] : undefined,
      text = function (v) {
        return JSON.stringify(v === undefined ? null : v);
      },
      e,
      r;
    if (typeof f !== "function") {
      e = new Error("the data module's `tools` has no `" + name + "`");
      e.kind = "UnknownSource";
      throw e;
    }
    r = f(args, store, storage);
    return r && typeof r.then === "function" ? r.then(text) : text(r);
  }
  global.__exact_call = function (source, argsJson, mutation) {
    initializing = false;
    var call = {
      id: nextId(),
      status: "pending",
      value: undefined,
      error: undefined,
      tickets: [],
      storage: 0,
      replied: false,
      mutation: mutation === "mutation",
      tool: mutation === "tool",
      answer: mutation === "resource",
      set: mutation === "resource",
      sync: false,
      socketOps: null,
      epoch: gcEpoch(),
    };
    var result;
    currentCall = call;
    work.setTurn(call);
    calls.set(call.id, call);
    try {
      // Null only where no module can be (a page with no page module). Else an
      // object; whether a module is linked is the device's fact, not the
      // build's, so asking `available` marks the answer as the device's: the
      // bake leaves it uncompiled and the host asks it again.
      var native =
        host(6, "kind", "") !== "native"
          ? null
          : Object.freeze({
              get available() {
                return host(6, "available", "") === "native";
              },
              call: nativeCall,
              // This answer depends on a device topic the module announces when it
              // changes (a level, a step, new words): the host asks it again then,
              // instead of the app polling (LLP 1016.002).
              watch: function (topic) {
                if (overlaying) throw inOverlay("native.watch()");
                if (!currentCall || currentCall.status !== "pending") throw new Error("native.watch outside an answer");
                host(6, "watch", String(topic));
              },
              // Long work: off the source's thread and outside its budget, settled
              // when the module's own work replies. It travels as a request the
              // host hands to the module (on the web, the app's page module), so
              // the answer waits for it as for a fetch. A module that takes no
              // long calls answers through `call`, now.
              later: function (request) {
                if (overlaying) return Promise.reject(inOverlay("native.later()"));
                if (host(6, "later", "") !== "later") {
                  try {
                    return Promise.resolve(nativeCall(request));
                  } catch (e) {
                    return Promise.reject(e);
                  }
                }
                return global
                  .fetch("exact-native:", {
                    method: "POST",
                    body: JSON.stringify(request),
                    exactIndependentHttp: { maxResponseBytes: 1048576 },
                  })
                  .then(function (r) {
                    return r.text().then(function (t) {
                      if (r.status === 200) return JSON.parse(t);
                      throw new Error(t || "native call failed");
                    });
                  });
              },
            });
      call.sync = true;
      try {
        result =
          mutation === "tool"
            ? tool(source.slice(5), JSON.parse(JSON.parse(argsJson)[0]))
            : global.exact.answer(source, JSON.parse(argsJson), store, storage, native);
      } finally {
        call.sync = false;
      }
      // Staged socket operations go out with no fetch, a stream answer's included (D7); a source that answers with a
      // promise is an answer in flight from its call on, and what it staged is that turn's (the amendment of
      // 2026-10-18).
      if (call.socketOps && call.tickets.length) call.socketConflict = true;
      if (call.socketConflict) {
        if (result && typeof result.then === "function") result.then(undefined, function () {});
        throw new Error(SOCKET_AND_FETCH);
      }
      if (call.socketOps && result && typeof result.then === "function") handSocketOps(call, 17);
    } catch (e) {
      if (call.socketConflict) {
        withdrawFetches(call);
        e = new Error(SOCKET_AND_FETCH);
      }
      // A refused socket mutation makes no fetch later either: its
      // continuation's are refused as the second effect.
      if (call.socketOps) call.socketConflict = true;
      call.socketOps = null;
      call.status = "failed";
      call.error = e;
      return JSON.stringify({ tag: 3, call: call.id });
    }
    if (result && typeof result.then === "function") {
      result.then(
        function (v) {
          if (call.lost) return;
          call.status = "done";
          call.value = v;
        },
        function (e) {
          // A lost call's answer is not delivered, but a database it left open still is said.
          if (call.lost) {
            leftOpen();
            return;
          }
          call.status = "failed";
          call.error = e;
          leftOpen();
        }
      );
      return JSON.stringify({ tag: 3, call: call.id });
    }
    // A value given at once is replied after the microtask checkpoint that
    // follows the call, as a browser runs one after each task, and work the
    // answer queued there is its own and lands first: a save chained behind
    // a resolved promise was never run on macOS, with nothing in the logs
    // (drums R10), because a reply given here left it queued with no answer
    // in flight to run it. The owner stays installed while that turn drains.
    // (The reply is serialized at `settle`, as a promised value is: the
    // engine keeps one call's large strings.)
    call.status = "done";
    call.value = result;
    return JSON.stringify({ tag: 3, call: call.id });
  };
  // Canvas 2D (LLP 1056 D1): the module's draw seam, when it exports `draw`.
  // Text is measured and image handles are answered where the draw runs
  // (LLP 1056 D8, D9): the page's functions on the web, else the host door.
  global.__exact_draw = function (request, measure, image) {
    readyAll();
    initializing = false;
    if (!global.exact.drawCanvas) throw new Error("the module exports no draw");
    return global.exact.drawCanvas(request, {
      measure:
        measure ||
        function (json) {
          return host(9, json, "");
        },
      image:
        image ||
        function (json) {
          var r = host(10, json, "");
          return r === undefined ? "" : r;
        },
    });
  };
  global.__exact_retire = function (retired) {
    readyAll();
    if (global.exact.retireCanvases) global.exact.retireCanvases(retired);
    return "";
  };
  global.__exact_calls_open = function () {
    return String(calls.size);
  };
  global.__exact_call_id = function () {
    return String(currentCall ? currentCall.id : 0);
  };
  global.__exact_more = function (id) {
    var call = calls.get(Number(id)),
      more = (call && call.more) || [];
    if (call) call.more = [];
    return JSON.stringify(more);
  };
  global.__exact_ready = global.__exact_waiting_answers(calls, pending, work, headOwner);
  delete global.__exact_waiting_answers;
  global.__exact_settle = function (id, final) {
    timers.checkpoint();
    currentCall = null;
    var call = calls.get(Number(id));
    if (call) call.ran = false;
    return call ? settle(call, final === "final") : fail(new Error("no such call"));
  };
  // The runner let this call's request go (LLP 1016 D5): drop the call, and reject the fetches it waits on, so its
  // continuation runs (a `finally` clears what the call set) rather than vanishing: the runner drops the reply when it
  // comes. The request may already be on the wire. A send that needs every reply is a `queue` mutation (LLP 1092).
  // One let go between storage steps answers "storage": its steps are running and the chain behind them goes on, so the
  // executor delivers them until `__exact_let_go` says none is left, and its answer is never given (ledger F12). A
  // fetch it makes then is refused: no one wants its reply.
  global.__exact_forget = function (id) {
    var call = calls.get(Number(id));
    if (!call) return "";
    call.letGo = true; // a fetch, yield or timer it makes from here is refused
    var secretRejected = 0;
    (call.secrets || []).forEach(function (task) {
      if (!task.held) {
        secretRejected++;
        task.reject(new DOMException("the answer was let go", "AbortError"));
      }
    });
    // its steps waiting for its own turn run in the let-go chain, with no store (LLP 1146 D2)
    var moved = work.forgot(call);
    // A released resource's socket handle is stale from now on (D7).
    if (call.socket) {
      call.socket.state = 3;
      if (call.socket.close) call.socket.close();
    }
    // Withdraw any request still unsent; keep a fetch another answer claimed.
    var dropped = [];
    for (var i = 0; i < call.tickets.length; i++) {
      var p = pending.get(call.tickets[i]);
      // A stream's promise never settles (LLP 1016.000), ended or let go.
      if (p && p.call === call && !p.module) {
        settled(call.tickets[i]);
        host(16, String(call.tickets[i]), "");
        if (!p.stream) dropped.push(p);
      }
    }
    // Rejected now, run at the next drain with no answer current.
    dropped.forEach(function (p) {
      p.reject(
        new FetchError({
          kind: "Aborted",
          message: "the answer was let go before this reply; the request may already have been sent",
        })
      );
    });
    var rejected =
      secretRejected + dropped.length + endYields(call, new DOMException("the answer was let go", "AbortError")) + moved
        ? " rejected"
        : "";
    timers.drop(call);
    if (call.storage > 0 && !call.lost) return "storage" + rejected;
    call.replied = true;
    calls.delete(call.id);
    cleanUp();
    return rejected.trim();
  };
  global.__exact_let_go = function (failed, message) {
    work.setTurn(null);
    // A let-go call's compression so failed is settled, and what is queued
    // behind it issued; the call keeps its other storage (LLP 1069.002
    // A1.5). Said so, and the caller delivers on.
    if (failed && head && headOwner() && headOwner().letGo && failAbandonedImage(String(message))) return "settled";
    // Any other wait that failed fails only the answer whose operation is in flight. That operation stays at the
    // head, now the module's, and the other let-go answers' queued storage starts once it completes.
    var timedOut = failed ? headOwner() : null;
    if (timedOut && timedOut.letGo) failStorage(timedOut, String(message));
    var owed = false;
    calls.forEach(function (c) {
      if (!c.letGo) return;
      if (c.storage > 0 && !c.lost) owed = true;
      else {
        c.replied = true;
        calls.delete(c.id);
      }
    });
    cleanUp();
    if (failed) return timedOut && timedOut.letGo ? "settled" : "";
    if (!owed) return "";
    // Deliver only when the operation in flight is the let-go call's.
    // One queued behind a live answer stays there: delivering it here would
    // run the live answer's step with no store (a934686a0).
    var at = headOwner();
    return at && at.letGo ? "storage" : "queued";
  };
  // One message of the stream answer `id` began, or its end (`{ type: "error" }` with the failure's kind and message,
  // or the status and body of a reply that was not 2xx; an upload's reply is `load`), mapped to the answer (LLP
  // 1016.000 D1).
  global.__exact_message = function (id, outcomeJson) {
    var call = calls.get(Number(id));
    if (!call || !call.stream) return fail(new Error("no stream answer " + id));
    var o = JSON.parse(outcomeJson),
      event,
      socket = call.socket,
      listened = socket && socket.listen && o.message ? sockets.listenEvent(socket, o.message) : null;
    if (listened) event = listened.event;
    else if (call.upload && (o.response || (o.message && o.message.event === "exact:progress")))
      event = binary.uploadEvent(o);
    else if (o.message)
      event = {
        type: o.message.event || "message",
        data: binary.messageData(o.message),
        lastEventId: o.message.id,
        coalesced: o.message.coalesced,
        protocol: o.message.protocol,
      };
    else if (o.closed)
      event = {
        type: "close",
        data: "",
        lastEventId: "",
        coalesced: 0,
        code: o.closed.code,
        reason: o.closed.reason,
        wasClean: o.closed.wasClean,
      };
    else if (o.failed)
      event = {
        type: "error",
        data: "",
        lastEventId: "",
        coalesced: 0,
        kind: o.failed.kind,
        message: o.failed.message,
        status: 0,
      };
    else
      event = {
        type: "error",
        data: o.response.body,
        lastEventId: "",
        coalesced: 0,
        kind: "Response",
        message: "HTTP " + o.response.status,
        status: o.response.status,
      };
    // The socket's state as of this event (D7): `open` opens it, an end closes it. A listener's stream ends as its
    // `error`. `AddressInUse` travels as `Refused`, its message so prefixed (host/executor/src/listen.rs).
    if (socket && !o.message && socket.close) {
      var f = o.failed || { kind: "Network", message: event.message },
        inUse = /^AddressInUse: /.test(f.message);
      socket.close();
      event = {
        type: "error",
        kind: inUse ? "AddressInUse" : f.kind,
        message: inUse ? f.message.slice(14) : f.message,
      };
    }
    // A child's owner reads its own events (js/src/process.js).
    if (socket && socket.event) event = socket.event(o, event);
    else if (socket) socket.state = !o.message ? 3 : event.type === "open" && socket.state === 0 ? 1 : socket.state;
    // Each mapper call is a call of its own (`mapper`), settled as an answer's: one that returns a promise is tag 3,
    // its yields turns with their own budget, its value the stream's when it settles (Charlie, 2026-11-10). Not on
    // wasm.
    var mine = moduleWide
      ? {
          id: nextId(),
          status: "pending",
          tickets: [],
          storage: 0,
          replied: false,
          mapper: call,
          answer: true,
          sync: true,
          socketOps: null,
          epoch: gcEpoch(),
        }
      : call;
    currentCall = mine;
    inMapper = true;
    if (mine !== call) work.setTurn(mine);
    // Its mapper may reply on it answering `open` or a `message` (never on wasm, where it never opens): handed over
    // however the call ends, sent if the frame's commit stands.
    // A listener's, on the connection whose `connection` or `message` it answers (LLP 1133.005 D6).
    var own =
      listened && listened.connection && (event.type === "connection" || event.type === "message")
        ? listened.connection
        : null;
    var turn = (mapping =
      socket && o.message && socket.state === 1 && (!socket.listen || own)
        ? { owner: socket, connection: own, socketOps: null, overload: null }
        : null);
    try {
      var value = !socket
        ? call.stream(event)
        : socket.listen
          ? call.stream(event, listened && listened.connection ? listened.connection.handle : undefined)
          : call.stream(event, socket.handle);
      if (value && typeof value.then === "function" && mine === call)
        throw new Error("exactStream answers each event now on this target; it cannot await");
      if (turn && turn.overload) throw turn.overload;
      if (!value || typeof value.then !== "function") {
        if (mine !== call) {
          endYields(mine);
          mine.replied = true;
        }
        return ok(value);
      }
      calls.set(mine.id, mine);
      value.then(
        function (v) {
          mine.status = "done";
          mine.value = v;
        },
        function (e) {
          mine.status = "failed";
          mine.error = e;
        }
      );
      return JSON.stringify({ tag: 3, call: mine.id });
    } catch (e) {
      if (mine !== call) {
        endYields(mine);
        mine.replied = true;
      }
      return fail(e);
    } finally {
      // A synchronous mapper has no drain: its queued reactions still need a waiter's turn (LLP 1162 D3).
      if (mine !== call && mine.replied) readyAll();
      mine.sync = false;
      currentCall = null;
      mapping = null;
      inMapper = false;
      if (turn) handSocketOps(turn);
    }
  };
  // The module's storage, after a call or a delivery (LLP 1097 D5, D8):
  // whether the operation in flight is the background's (`head`), the
  // operations queued behind it and in flight, an answer's or the
  // background's, and what landed and failed.
  global.__exact_background = function () {
    // Reporting can read an app's error getters after the last drain (LLP 1162 D3).
    if (timers.checkpoint()) readyAll();
    currentCall = null;
    return JSON.stringify({
      head: headOwner() === background,
      queued: queue.length,
      inFlight: head ? 1 : 0,
      writes: storageCounts.writes,
      done: storageCounts.done,
      failed: storageCounts.failed,
      last: storageCounts.last,
    });
  };
  // An overlay (`overlay(source, args, answer, writes)`): what a resource
  // shows while writes affect it, called synchronously with no answer current. It has
  // no effects: `fetch`, storage, the store, `native` and `crypto` refuse.
  var overlaying = false;
  function inOverlay(api) {
    var e = new Error(
      api +
        " is unavailable in an overlay, which runs synchronously and has no effects; do the work in the source's answer or the mutation, and pass the overlay what it needs in the mutation's arguments"
    );
    journal("overlay refused: " + e.message);
    return e;
  }
  // Whether the module has an overlay, read once when it has loaded (the host keeps the answer for the instance,
  // as the JS target decides at install): anything but `undefined`, or a read that throws, counts as one, so the
  // overlay is then asked each time as before. A getter or a proxy's trap runs here as initialization code.
  global.__exact_has_overlay = function () {
    try {
      return global.exact.overlay === undefined ? "" : "1";
    } catch (e) {
      return "1";
    }
  };
  global.__exact_overlay = function (source, argsJson, payloadJson) {
    readyAll();
    initializing = false;
    // A realm whose answer spans tasks (the web's) has one current between them: put it back after. Both reads of
    // `exact.overlay` run guarded, so a getter's store call is refused as the overlay's is.
    var saved = currentCall;
    currentCall = null;
    overlaying = true;
    try {
      if (typeof global.exact.overlay !== "function") return JSON.stringify({ tag: 4 });
      var payload = JSON.parse(payloadJson);
      // An answered write the answer does not include yet goes on showing
      // when the overlay calls its `keep()`.
      var keep = [];
      var writes = payload.writes.map(function (w) {
        return Object.freeze(
          Object.assign({}, w, {
            keep: function () {
              if (keep.indexOf(w.id) < 0) keep.push(w.id);
            },
          })
        );
      });
      var value = global.exact.overlay(source, JSON.parse(argsJson), payload.answer, writes);
      if (value && typeof value.then === "function")
        throw new Error(
          "an overlay returned a promise: it runs synchronously inside a commit and cannot await; compute its value from `answer` and `writes` and return it"
        );
      return JSON.stringify(value === undefined ? { tag: 4 } : { tag: 0, value: value, keep: keep });
    } catch (e) {
      return JSON.stringify({ tag: 2, message: e && e.message ? String(e.message) : String(e) });
    } finally {
      overlaying = false;
      currentCall = saved;
    }
  };
  // A background round delivers with the background current: what its
  // completion's reaction issues is the background's too.
  global.__exact_enter_background = function (teardown) {
    currentCall = background;
    if (teardown) work.teardown();
    else work.setTurn(null);
  };
  // A compression whose wait gave up, and whose right to write the waiter
  // took away (LLP 1069.002 A1.5): it can write nothing now, so it is
  // failed and the queue moves on. Any other operation whose wait ran out
  // may still write, so its queue waits for its actual completion.
  var IMAGE_ABANDONED = /^compressImage: timeout: the storage wait ran out/;
  function failAbandonedImage(message) {
    if (
      !head ||
      (head.method !== "compressImage" && head.method !== "transformImage") ||
      !IMAGE_ABANDONED.test(message)
    )
      return false;
    landed(head, false, storageError(message.replace(/^compressImage/, head.method)));
    return true;
  }
  // The background's operation in flight, so failed: its rejection runs.
  global.__exact_background_failed = function (outcomeJson) {
    if (!head || owner(head.call) !== background) return "";
    return failAbandonedImage(JSON.parse(outcomeJson).failed.message) ? "1" : "";
  };
  function failStorage(call, message) {
    call.lost = true;
    call.status = "failed";
    call.error = storageError(message);
    // Remove unsent work before any rejection can issue more work or a
    // revoked image can release the queue's head.
    var dropped = [];
    queue = queue.filter(function (op) {
      if (op.call !== call) return true;
      dropped.push(op);
      return false;
    });
    dropped.forEach(function (op) {
      op.abandon();
      landed(op, false, call.error);
    });
    if (head && head.call === call && !failAbandonedImage(message)) {
      // Reject its promise now, but keep serialization until the host
      // returns: an uncancellable sync or rename can still publish.
      head.settle(false, call.error);
      head.failed = true;
      head.call = background;
      background.storage++;
      storing.add(background);
    }
    storing.delete(call);
  }
  global.__exact_storage_failed = function (id, outcomeJson) {
    var call = calls.get(Number(id));
    if (call) failStorage(call, JSON.parse(outcomeJson).failed.message);
  };
  // The answer a timer belongs to (time.js `timers`), or the refusal.
  function timerOwner(api) {
    if (overlaying) throw inOverlay(api);
    if (
      inMapper ||
      !currentCall ||
      currentCall.mapper ||
      currentCall === background ||
      currentCall.replied ||
      currentCall.letGo
    )
      throw new Error(
        api +
          " is unavailable here: a timer belongs to the answer that arms it, and none is running (module initialization, a stream's mapper, background work, or an answer that has given its value or was let go). A cache needs no timer to expire: fetch keeps each answer as long as its HTTP headers allow"
      );
    return currentCall;
  }
  // A timer's turn: the answer's earliest callback due by the runner's clock `at` runs in the drain that follows;
  // `due`: the runner says the timer it waits for is due (where it has no clock, at bake, the time it waited for).
  global.__exact_timer = function (id, at, due) {
    var call = calls.get(Number(id));
    if (call) {
      currentCall = call;
      timers.fire(call, at, due === "due");
    }
  };
  // A yielded answer's new turn: its yields resolve in the drain that follows.
  global.__exact_yielded = function (id) {
    var call = calls.get(Number(id));
    if (call) {
      currentCall = call;
      endYields(call);
    }
  };
  // The executor's turns (js/src/module_work.rs): the turn's call is current, so a plain `.then` reaction runs as it
  // (LLP 1146 D2).
  global.__exact_turn = function (id) {
    var t = id === "module" ? work.MODULE : id ? calls.get(Number(id)) || null : null;
    currentCall = t === work.MODULE ? null : t;
    work.setTurn(t);
  };
  global.__exact_module_discarded = function () {
    work.runDiscarded();
  };
  // A let-go call's fetch with effects the runner keeps (LLP 1146 D2): only fetches handed out (`claimed`, so an
  // abort is a cancel, door 23); forgetting the call leaves it in flight and outstanding until its reply reaches the
  // call's continuation in a module turn.
  global.__exact_adopt = function (id, ticket) {
    var p = pending.get(Number(ticket));
    if (!p || !p.claimed || p.stream || p.module || p.call !== calls.get(Number(id))) return "";
    p.module = true;
    work.adopted();
    return "adopted";
  };
  // The child-process module's seam (LLP 1133.007), natively only: js/src/process.js takes it, and it is gone.
  if (global.__exact_process_host)
    global.__exact_process_seam = Object.freeze({
      request: function (url, init, listen, own) {
        return request(url, init, listen, own, true);
      },
      socketOwner: socketOwner,
      host: host,
      copyBytes: copyBytes,
      toBase64: toBase64,
    });
  global.__exact_fulfill = function (ticket, outcomeJson) {
    var p = settled(Number(ticket));
    if (!p) return;
    var o = JSON.parse(outcomeJson);
    // The continuation runs in the drain that follows, and a fetch it
    // makes belongs to this call.
    currentCall = p.module ? null : p.call;
    // an adopted fetch's: its let-go call's steps run as it (the async helper), a plain reaction as no answer
    if (p.module) work.landed();
    // Its `AbortSignal.timeout`'s deadline passed on the host: the signal aborts.
    if (o.failed && o.failed.kind === "Timeout" && p.timed) timers.expire(p.signal);
    // A reply delivered after the signal aborts still rejects with that signal's reason.
    if (p.signal && p.signal.aborted) p.reject(p.signal.reason);
    else if (o.failed) p.reject(new FetchError(o.failed));
    else p.resolve(hostReply(o.response, p.bodyTo, p.head));
  };
})(globalThis);
