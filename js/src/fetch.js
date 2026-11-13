// A data module's `fetch` (LLP 1016 D1/D6), for prelude.js: a request the
// host runs under the app's grants, recorded through the one host door, and
// a Promise for its reply; its options checked here (binary.js words most),
// its signal watched, and `exactListen`, a local listener. prelude.js takes
// `__exact_fetch` and deletes it. `env` is the prelude's door, journal and
// turn (the answer running, a socket's mapper, the overlay's refusal), its
// pending fetches by ticket and the answer timers' deadlines.
(function (global) {
  "use strict";
  global.__exact_fetch = function (env) {
    var host = env.host,
      binary = env.binary,
      journal = env.journal,
      pending = env.pending,
      work = env.work;
    var background = env.background,
      timers = env.timers,
      abortHooks = env.abortHooks,
      bytesDoor = env.bytesDoor;
    var Response = env.Response,
      FetchError = env.FetchError,
      SOCKET_AND_FETCH = env.conflict,
      inOverlay = env.inOverlay;
    var toBase64 = binary.toBase64,
      longPollOf = binary.longPollOf,
      fileRefusal = binary.fileRefusal,
      uploadOf = binary.uploadOf;
    var idleRefusal = binary.idleRefusal,
      socketOwner = env.sockets.socketOwner,
      listenOwner = env.sockets.listenOwner;
    var nextTicket = 1;
    // Capture the browser's composition before app code can replace it.
    var PlatformAbortSignal = global.AbortSignal, composeSignals = PlatformAbortSignal.any;
    function watchAbort(signal, aborted) {
      if (abortHooks) return abortHooks.subscribe(signal, aborted);
      signal = composeSignals.call(PlatformAbortSignal, [signal]);
      signal.addEventListener("abort", aborted);
      return function () {
        signal.removeEventListener("abort", aborted);
      };
    }
    function settled(ticket) {
      var p = pending.get(ticket);
      if (p) {
        pending.delete(ticket);
        p.release();
      }
      return p;
    }
    // An aborted fetch's request, as a browser cancels one: a request its answer
    // awaits (handed out, `claimed`) is cancelled by the host, which resumes the
    // answer at once (door 23); one not yet handed out is withdrawn unsent (16).
    // A stream runs until the runner lets its ticket go, as before. A host that
    // knows neither door leaves the request running, the old behaviour.
    function abortRequest(ticket, p) {
      if (p.stream) return;
      try {
        if (p.claimed) host(23, String(ticket), "");
        else {
          var at = p.call.tickets.indexOf(ticket);
          if (at >= 0) p.call.tickets.splice(at, 1);
          host(16, String(ticket), "");
        }
      } catch (e) {
        /* an older host's door: the request runs, its reply dropped */
      }
    }
    // An adopted fetch's request, cancelled by the host (door 23), which then
    // answers it aborted in a module turn: whether the host took it.
    function cancelAdopted(ticket) {
      try {
        host(23, String(ticket), "");
        return true;
      } catch (e) {
        return false;
      }
    }
    // A refused mutation's fetches, withdrawn before any was handed out.
    function withdrawFetches(call) {
      for (var i = 0; i < call.tickets.length; i++) {
        settled(call.tickets[i]);
        host(16, String(call.tickets[i]), "");
      }
      call.tickets = [];
      call.socketOps = null;
    }
    global.Response = Response;
    global.fetch = function (url, init) {
      return request(url, init);
    };
    // `listen`: a listener's options (`exactListen`, LLP 1133.005 D1); `own(ticket,
    // call)`: a child's stream owner and `child`: the call is js/src/process.js's, the one path to `exact-process:`.
    function request(url, init, listen, own, child) {
      var overlaying = env.overlaying(),
        mapping = env.mapping(),
        running = env.current();
      if (overlaying || mapping || (running && running.mapper))
        return Promise.reject(
          overlaying
            ? inOverlay("fetch()")
            : new Error(
                mapping
                  ? "fetch() is unavailable in a socket's mapper: it answers each frame now"
                  : "fetch() is unavailable in a stream's mapper: fetch in the answer that opens the stream"
              )
        );
      url = String(url); // read once: the URL checked here and below is the one the host is sent
      if (!child && /^exact-process:/i.test(url))
        return Promise.reject(
          new TypeError(
            "fetch: exact-process: is @exact/child-process's: import spawn or execFile from it (LLP 1133.007 D12)"
          )
        );
      var call = env.current();
      // Refused with no answer running, and below in a let-go answer's continuation, as no answer waits for the reply
      // (where the web lets it run; a request with effects already sent runs to its end, LLP 1146 D2, §2.4).
      if (!call) {
        var refused = new Error(
          work.inModuleTurn()
            ? "fetch() called from a plain .then callback a let-go answer's request settled: it was never run. Await that result in an async function, which resumes in its own answer's turn"
            : "fetch() called outside an answer: it was never run. Start a fetch inside an answer"
        );
        if (global.console) global.console.error(refused.message);
        return Promise.reject(refused);
      }
      // No fetch from background work (LLP 1097 D2; a background fetch is
      // deferred, §7): no answer waits for its reply, and no ticket is owed.
      if (call === background) {
        var orphan = new Error("fetch() called from background work: it was never run. Fetch inside an answer");
        journal("data: " + orphan.message);
        return Promise.reject(orphan);
      }
      if (call.letGo)
        return Promise.reject(new FetchError({ kind: "Aborted", message: "the answer was let go before this fetch" }));
      // A stream's own options, refused (LLP 1016.000 D11): at bake marked as storage's refusal is, so a source that
      // lets it through keeps its placeholder. (The door throws its refusal as a string: marked, it is an Error.)
      var streamOption = init ? init.exactStream : undefined,
        marks = streamOption !== undefined && !listen && !own && !child;
      function mark(e) {
        if (marks && host(12, "bake", "") === "1") {
          if (!e || typeof e !== "object") e = new Error(String(e));
          e.kind = "Unavailable";
          e.code = "bake";
          e.bakeWarning = true;
        }
        return e;
      }
      function optionRefused(e) {
        return Promise.reject(mark(e));
      }
      if (call.socketConflict || (call.socketOps && call.socketOps.length)) {
        call.socketConflict = true;
        return Promise.reject(new Error(SOCKET_AND_FETCH));
      }
      var method = init && init.method ? String(init.method).toUpperCase() : "GET";
      var headers = new global.Headers(init && init.headers);
      var raw = init ? init.body : undefined,
        extracted;
      try {
        extracted = binary.requestBody(raw, method, headers);
      } catch (e) {
        return Promise.reject(e);
      }
      var bytes = extracted.bytes,
        body = extracted.text;
      // A body from an app file (LLP 1108 D6 R2): only the path goes to the
      // host, which reads the file when it runs the request, under `fs.read`,
      // at most 64 MiB. The bytes never enter JavaScript. A missing file, a
      // denied path or one over the bound rejects the fetch before it is sent.
      // A body to an app file (`exactBodyTo`, 2026-10-26): the host writes a
      // 2xx body there as it arrives, under `fs.write`, at most the ceiling or
      // 1 GiB, and the file appears only whole; any other reply is read as usual.
      var bodyFrom = init ? init.exactBodyFrom : undefined,
        bodyTo = init ? init.exactBodyTo : undefined;
      var fileWhy = fileRefusal(init, method, url, raw); // binary.js
      if (fileWhy) return optionRefused(new TypeError(fileWhy));
      // LLP 1041 §8.4: an explicit promise about both operation and settlement.
      // Browsers ignore this native scheduling hint; their admission is unchanged.
      var independent = init ? init.exactIndependentHttp : undefined;
      var ceiling;
      if (independent !== undefined) {
        ceiling = independent && independent.maxResponseBytes;
        var most = bodyTo === undefined ? 67108864 : 1073741824; // a body written to a file: 1 GiB
        if (!Number.isInteger(ceiling) || ceiling <= 0 || ceiling > most)
          return optionRefused(
            new TypeError("exactIndependentHttp.maxResponseBytes must be an integer from 1 to " + most)
          );
      }
      // An answer that keeps coming (LLP 1016.000): `exactStream` maps each
      // server-sent event to the answer, and the end too. The promise never
      // settles: the answer is what the mapper returns, message by message.
      var stream = streamOption;
      if (stream !== undefined && typeof stream !== "function")
        return optionRefused(new TypeError("exactStream maps each event to the answer: (event) => value"));
      if (stream && call.stream) return Promise.reject(new Error("an answer streams one request"));
      if (stream && ceiling === undefined) ceiling = 1048576;
      // An upload whose stream gets its progress, then its reply (`exactUploadProgress`, LLP 1016.000 D10).
      var upload = uploadOf(init, stream, method, url);
      if (typeof upload === "string") return optionRefused(new TypeError(upload));
      // A deadline for the whole exchange (headers and body), in milliseconds: the host cancels the request
      // when it passes and the fetch rejects with a FetchError of kind "Timeout". A stream has none; an upload may.
      var timeout = init ? init.exactTimeout : undefined,
        held = init ? init.exactHeld : undefined;
      if (timeout !== undefined && (!Number.isInteger(timeout) || timeout < 1 || timeout > 3600000))
        return optionRefused(new TypeError("exactTimeout must be an integer number of milliseconds from 1 to 3600000"));
      if (timeout !== undefined && stream && !upload)
        return optionRefused(new TypeError("exactTimeout: a stream has no timeout"));
      // A quiet WebSocket ends as a Timeout (LLP 1016.000 D11).
      var idle = init ? init.exactIdleTimeout : undefined,
        idleWhy = idleRefusal(
          idle,
          !!stream && !upload && !listen && !own && init.exactLongPoll === undefined && /^wss?:/i.test(url)
        );
      if (idleWhy) return optionRefused(new TypeError(idleWhy));
      // `exactHeld`: the server may hold the reply, and no clock waits on it (LLP 1016.000 D9); `exactLongPoll` (D8):
      // the host asks again with each reply's cursor (binary.js `longPollOf`).
      if ((held !== undefined && held !== true && held !== false) || (held && stream))
        return optionRefused(
          new TypeError(
            held === true
              ? "exactHeld: a stream is never held; it stops counting at its first message"
              : "exactHeld is true or false"
          )
        );
      var poll = longPollOf(init, stream, method, url);
      if (typeof poll === "string") return optionRefused(new TypeError(poll));
      var cache, preload, prefetch;
      try {
        cache = binary.cacheMode(init);
        preload = binary.preload(init);
        prefetch = binary.prefetch(
          init,
          !call.mutation &&
            !call.tool &&
            !stream &&
            !listen &&
            !own &&
            !child &&
            bodyFrom === undefined &&
            bodyTo === undefined &&
            ceiling === undefined &&
            /^https?:/i.test(url)
        );
      } catch (e) {
        return optionRefused(e);
      }
      // `redirect` (Fetch §5.4, the Request constructor): read and checked once,
      // before the signal, and a throw rejects. Rust follows redirects, checking
      // each hop's grant, so "manual" answers with the 3xx and its Location.
      var redirect;
      try {
        var asked = init ? init.redirect : undefined;
        redirect = asked !== undefined ? String(asked) : undefined;
      } catch (e) {
        return Promise.reject(e);
      }
      if (redirect !== undefined && redirect !== "follow" && redirect !== "manual" && redirect !== "error")
        return optionRefused(new TypeError("redirect must be follow, manual or error"));
      if (upload && redirect !== undefined && redirect !== "follow")
        return optionRefused(new TypeError("exactUploadProgress: an upload follows its redirects"));
      // The web's `signal`: an aborted fetch rejects with its reason at once,
      // and the host cancels its request (`abortRequest`).
      var signal = init ? init.signal : undefined;
      if (signal != null && abortHooks) {
        try {
          abortHooks.subscribe(signal, function () {})();
        } catch (e) {
          return Promise.reject(new TypeError("fetch: init.signal is not an AbortSignal"));
        }
      }
      if (signal && signal.aborted) return Promise.reject(signal.reason);
      // An `AbortSignal.timeout`'s time left is the host's deadline, unless an earlier `exactTimeout` is; past the
      // longest a request waits (an hour), the hour is, and the signal does not abort when it passes.
      var left = stream ? undefined : timers.left(signal),
        timed = left !== undefined && !(timeout < left);
      if (timed) {
        timeout = Math.min(left, 3600000);
        timed = left <= 3600000;
      }
      var ticket = nextTicket++;
      // A WebSocket stream owns a two-way socket (LLP 1016.000 D7); an HTTP one's body that is not events is bytes.
      var socket = stream && (!!listen || !!own || /^wss?:/i.test(String(url)));
      var protocols;
      try {
        protocols = binary.socketProtocols(init ? init.exactProtocols : undefined, !!stream && /^wss?:/i.test(url));
      } catch (e) {
        return optionRefused(e);
      }
      var wire = JSON.stringify({
        protocols: protocols,
        method: method,
        url: String(url),
        headers: Array.from(headers.entries()),
        body: body,
        body_base64: bytes && !bytesDoor ? toBase64(bytes) : undefined,
        max_response_bytes: ceiling,
        stream: stream ? true : undefined,
        socket: socket ? true : undefined,
        bytes: stream && !socket && !poll && !upload ? true : undefined,
        upload_progress: upload ? true : undefined,
        long_poll: poll,
        timeout_ms: timeout,
        idle_timeout_ms: idle,
        body_from: bodyFrom,
        body_to: bodyTo,
        redirect: redirect,
        cache: cache,
        preload: preload,
        prefetch: prefetch,
        held: held ? true : undefined,
        listen: listen,
      });
      var error;
      try {
        error = binary.sendRequest(host, ticket, wire, bytes);
      } catch (e) {
        throw mark(e);
      } // only the door's own refusal, never what building the request threw
      if (error !== undefined) return optionRefused(new Error(error));
      call.tickets.push(ticket);
      if (stream) {
        call.stream = stream;
        call.upload = !!upload;
      }
      var owner = socket
        ? (call.socket = listen ? listenOwner(ticket) : own ? own(ticket, call) : socketOwner(ticket, call.mutation))
        : null;
      return new Promise(function (resolve, reject) {
        // `early`: made while its answer was still in flight (its body, or its
        // own continuation), so that answer's own, whoever else is pending.
        var p = {
          resolve: resolve,
          reject: reject,
          call: call,
          claimed: false,
          stream: !!stream,
          signal: signal,
          timed: timed,
          release: function () {},
          early: call.status === "pending",
          set: !stream && !poll && !upload && bodyFrom === undefined && bodyTo === undefined && /^https?:/i.test(url),
          bodyTo: bodyTo,
          head: method === "HEAD",
        };
        pending.set(ticket, p);
        // The caller's abort also makes its socket handle stale (D7); the
        // stream itself runs until the runner lets its ticket go.
        // An adopted fetch (LLP 1146 D2) stays pending: the host cancels it and
        // answers its kept ticket aborted, and `__exact_fulfill` rejects it in a
        // module turn, not in the turn that aborted it, so what its let-go
        // call's continuation writes stands when that turn's commit is refused.
        // A host without door 23 leaves it to settle here, as before.
        if (signal)
          p.release = watchAbort(signal, function () {
            if (owner) owner.state = 3;
            if (p.module && pending.get(ticket) === p && cancelAdopted(ticket)) return;
            var was = settled(ticket);
            if (was) {
              if (p.module) work.landed();
              reject(signal.reason);
              abortRequest(ticket, was);
            }
          });
      });
    }
    // A local listener (LLP 1133.005 D1): a resource's answer, as `fetch(url, {exactStream})` is, whose events
    // are mapped now with the connection's handle beside them. Natively on desktop hosts only (door 12 `listen`);
    // elsewhere the global is absent, so `"exactListen" in globalThis` is the feature test (§8).
    if (host(12, "listen", "") === "1")
      global.exactListen = function exactListen(address, options) {
        var o = options || {},
          call = env.current();
        if (call && call.mutation)
          throw new DOMException(
            "exactListen() is a resource's answer: a mutation's listener would have no end",
            "InvalidStateError"
          );
        if (typeof o.exactStream !== "function")
          return Promise.reject(
            new TypeError("exactListen(address, {exactStream(event, connection)}) maps each event to the answer")
          );
        var bound = function (name, most) {
          var v = o[name];
          if (v !== undefined && (!Number.isInteger(v) || v < 1 || v > most))
            throw new TypeError("exactListen: " + name + " is an integer from 1 to " + most);
          return v;
        };
        try {
          var listen = {
            takeover: !!o.takeover,
            first_line_timeout_ms: bound("firstLineTimeout", 600000),
            max_line_bytes: bound("maxLineBytes", 1048576),
            max_connections: bound("maxConnections", 64),
          };
        } catch (e) {
          return Promise.reject(e);
        }
        return request(
          String(address),
          { exactStream: o.exactStream, exactIndependentHttp: { maxResponseBytes: 1048576 } },
          listen
        );
      };
    return { request: request, settled: settled, withdrawFetches: withdrawFetches };
  };
})(globalThis);
