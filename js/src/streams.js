// The WHATWG Streams standard's default streams (https://streams.spec.whatwg.org/)
// and the Compression standard's `DecompressionStream` and `CompressionStream`
// (https://compression.spec.whatwg.org/), for a data module on Hermes, which
// has none of them. Hermes only: a browser realm has its own, and this file
// is part of the native prelude alone (js/build.rs). The algorithms follow
// the standard's by name, so a module's ordering (when a write's promise
// settles, which read a chunk fulfils, when backpressure lifts) is the
// browser's. Not built: byte streams (`type: "bytes"`) and BYOB readers,
// which refuse by name, and the `brotli` format.
//
// The codecs run in Rust through the bytes door (ops 12-15,
// js/src/compression.rs): each chunk written is decompressed whole, at native
// speed, in the turn that writes it, and yields one chunk of all its output.
// Issue 20261023-a-stream-mapper-cannot-yield (Charlie, 2026-11-10).
// @ref LLP 1027 D10 — what the module can use
(function (global) {
  "use strict";
  if (typeof global.ReadableStream === "function") return;
  // (prelude.js takes and deletes both after this file has captured them)
  var door = global.__exact_bytes, keep = global.__exact_key;
  var defineProperty = Object.defineProperty, PromiseC = Promise, then = Promise.prototype.then;
  var typedName = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Uint8Array.prototype), Symbol.toStringTag).get;
  var readables = new WeakSet();
  function noop() {}
  function resolved(v) { return PromiseC.resolve(v); }
  function rejected(e) { return PromiseC.reject(e); }
  function handled(p) { then.call(p, undefined, noop); return p; }
  function upon(p, onFulfilled, onRejected) { return then.call(p, onFulfilled, onRejected); }
  function deferred() {
    var d = { state: "pending" };
    d.promise = new PromiseC(function (resolve, reject) {
      d.resolve = function (v) { if (d.state === "pending") { d.state = "fulfilled"; resolve(v); } };
      d.reject = function (e) { if (d.state === "pending") { d.state = "rejected"; reject(e); } };
    });
    return d;
  }
  function settledWith(ok, value) { var d = deferred(); if (ok) d.resolve(value); else { d.reject(value); handled(d.promise); } return d; }
  // A callback of an underlying source, sink or transformer, its result as a promise.
  function promiseCall(f, self, args) { try { return resolved(f.apply(self, args)); } catch (e) { return rejected(e); } }
  function method(object, name, where) {
    var f = object == null ? undefined : object[name];
    if (f !== undefined && typeof f !== "function") throw new TypeError(where + ": " + name + " is not a function");
    return f;
  }
  function methods(target, fns) {
    for (var name in fns) defineProperty(target, name, { value: fns[name], writable: true, configurable: true });
  }
  function getters(target, fns) {
    for (var name in fns) defineProperty(target, name, { get: fns[name], configurable: true });
  }
  function tag(C, name) { defineProperty(C.prototype, Symbol.toStringTag, { value: name, configurable: true }); }
  function illegal() { throw new TypeError("Illegal constructor"); }
  function brand(x, C, what) {
    if (C === ReadableStream ? !isReadable(x) : !(x instanceof C)) throw new TypeError("Illegal invocation: not a " + what);
  }

  // --- queue-with-sizes and strategies ---------------------------------------
  function resetQueue(c) { c._queue = []; c._queueTotalSize = 0; }
  function enqueueValueWithSize(c, value, size) {
    size = Number(size);
    if (!(size >= 0) || size === Infinity) throw new RangeError("a chunk's size must be a finite, non-negative number");
    c._queue.push({ value: value, size: size });
    c._queueTotalSize += size;
  }
  function dequeueValue(c) {
    var pair = c._queue.shift();
    c._queueTotalSize -= pair.size;
    if (c._queueTotalSize < 0) c._queueTotalSize = 0;
    return pair.value;
  }
  function extractHighWaterMark(strategy, fallback) {
    var hwm = strategy == null ? undefined : strategy.highWaterMark;
    if (hwm === undefined) return fallback;
    hwm = Number(hwm);
    if (hwm !== hwm || hwm < 0) throw new RangeError("highWaterMark must be a non-negative number");
    return hwm;
  }
  function extractSizeAlgorithm(strategy) {
    var size = method(strategy, "size", "QueuingStrategy");
    return size === undefined ? function () { return 1; } : function (chunk) { return size(chunk); };
  }
  function CountQueuingStrategy(init) {
    if (!(this instanceof CountQueuingStrategy)) throw new TypeError("Constructor requires 'new'");
    defineProperty(this, "_hwm", { value: Number(init.highWaterMark) });
  }
  function ByteLengthQueuingStrategy(init) {
    if (!(this instanceof ByteLengthQueuingStrategy)) throw new TypeError("Constructor requires 'new'");
    defineProperty(this, "_hwm", { value: Number(init.highWaterMark) });
  }
  getters(CountQueuingStrategy.prototype, { highWaterMark: function () { return this._hwm; }, size: function () { return function size() { return 1; }; } });
  getters(ByteLengthQueuingStrategy.prototype, { highWaterMark: function () { return this._hwm; }, size: function () { return function size(chunk) { return chunk.byteLength; }; } });
  tag(CountQueuingStrategy, "CountQueuingStrategy");
  tag(ByteLengthQueuingStrategy, "ByteLengthQueuingStrategy");

  // --- ReadableStream ----------------------------------------------------------
  function ReadableStream(source, strategy) {
    if (!new.target) throw new TypeError("Constructor requires 'new'");
    if (source === undefined) source = null;
    if (source !== null && typeof source !== "object" && typeof source !== "function") throw new TypeError("ReadableStream: the underlying source must be an object");
    var type = source == null ? undefined : source.type;
    if (type !== undefined) {
      if (String(type) === "bytes") throw new TypeError("ReadableStream: byte streams (type: \"bytes\") are unavailable in data sources");
      throw new TypeError("ReadableStream: type must be \"bytes\" or undefined");
    }
    initializeReadableStream(this);
    var size = extractSizeAlgorithm(strategy), hwm = extractHighWaterMark(strategy, 1);
    var start = method(source, "start", "ReadableStream"), pull = method(source, "pull", "ReadableStream"), cancel = method(source, "cancel", "ReadableStream");
    var c = Object.create(ReadableStreamDefaultController.prototype);
    setUpReadableController(this, c,
      start ? function () { return start.call(source, c); } : noop,
      pull ? function () { return promiseCall(pull, source, [c]); } : function () { return resolved(); },
      cancel ? function (r) { return promiseCall(cancel, source, [r]); } : function () { return resolved(); },
      hwm, size);
  }
  function initializeReadableStream(s) {
    readables.add(s);
    s._state = "readable"; s._reader = undefined; s._storedError = undefined; s._disturbed = false;
  }
  function createReadableStream(start, pull, cancel, hwm, size) {
    var s = Object.create(ReadableStream.prototype);
    initializeReadableStream(s);
    setUpReadableController(s, Object.create(ReadableStreamDefaultController.prototype), start, pull, cancel,
      hwm === undefined ? 1 : hwm, size || function () { return 1; });
    return s;
  }
  function ReadableStreamDefaultController() { illegal(); }
  function setUpReadableController(stream, c, start, pull, cancel, hwm, size) {
    c._stream = stream; resetQueue(c);
    c._started = c._closeRequested = c._pullAgain = c._pulling = false;
    c._size = size; c._hwm = hwm; c._pull = pull; c._cancel = cancel;
    stream._controller = c;
    var started = start();
    upon(resolved(started), function () { c._started = true; callPullIfNeeded(c); }, function (r) { readableControllerError(c, r); });
  }
  function callPullIfNeeded(c) {
    if (!shouldCallPull(c)) return;
    if (c._pulling) { c._pullAgain = true; return; }
    c._pulling = true;
    upon(c._pull(), function () {
      c._pulling = false;
      if (c._pullAgain) { c._pullAgain = false; callPullIfNeeded(c); }
    }, function (e) { readableControllerError(c, e); });
  }
  function shouldCallPull(c) {
    var s = c._stream;
    if (!canCloseOrEnqueue(c) || !c._started) return false;
    if (s._reader && s._reader._readRequests.length > 0) return true;
    return readableDesiredSize(c) > 0;
  }
  function clearReadableAlgorithms(c) { c._pull = c._cancel = c._size = undefined; }
  function readableControllerClose(c) {
    if (!canCloseOrEnqueue(c)) return;
    c._closeRequested = true;
    if (c._queue.length === 0) { clearReadableAlgorithms(c); readableStreamClose(c._stream); }
  }
  function readableControllerEnqueue(c, chunk) {
    if (!canCloseOrEnqueue(c)) return;
    var s = c._stream;
    if (s._reader && s._reader._readRequests.length > 0) s._reader._readRequests.shift().chunk(chunk);
    else {
      var size;
      try { size = c._size(chunk); enqueueValueWithSize(c, chunk, size); }
      catch (e) { readableControllerError(c, e); throw e; }
    }
    callPullIfNeeded(c);
  }
  function readableControllerError(c, e) {
    if (c._stream._state !== "readable") return;
    resetQueue(c); clearReadableAlgorithms(c); readableStreamError(c._stream, e);
  }
  function readableDesiredSize(c) {
    var state = c._stream._state;
    return state === "errored" ? null : state === "closed" ? 0 : c._hwm - c._queueTotalSize;
  }
  function readableHasBackpressure(c) { return !shouldCallPull(c); }
  function canCloseOrEnqueue(c) { return !c._closeRequested && c._stream._state === "readable"; }
  function pullSteps(c, request) {
    if (c._queue.length > 0) {
      var chunk = dequeueValue(c);
      if (c._closeRequested && c._queue.length === 0) { clearReadableAlgorithms(c); readableStreamClose(c._stream); }
      else callPullIfNeeded(c);
      request.chunk(chunk);
    } else {
      c._stream._reader._readRequests.push(request);
      callPullIfNeeded(c);
    }
  }
  getters(ReadableStreamDefaultController.prototype, { desiredSize: function () { brand(this, ReadableStreamDefaultController, "ReadableStreamDefaultController"); return readableDesiredSize(this); } });
  methods(ReadableStreamDefaultController.prototype, {
    close: function () {
      brand(this, ReadableStreamDefaultController, "ReadableStreamDefaultController");
      if (!canCloseOrEnqueue(this)) throw new TypeError("The stream is not in a state that permits close");
      readableControllerClose(this);
    },
    enqueue: function (chunk) {
      brand(this, ReadableStreamDefaultController, "ReadableStreamDefaultController");
      if (!canCloseOrEnqueue(this)) throw new TypeError("The stream is not in a state that permits enqueue");
      readableControllerEnqueue(this, chunk);
    },
    error: function (e) { brand(this, ReadableStreamDefaultController, "ReadableStreamDefaultController"); readableControllerError(this, e); },
  });
  tag(ReadableStreamDefaultController, "ReadableStreamDefaultController");

  function readableStreamClose(s) {
    s._state = "closed";
    var reader = s._reader;
    if (!reader) return;
    reader._closed.resolve(undefined);
    var requests = reader._readRequests;
    reader._readRequests = [];
    requests.forEach(function (r) { r.close(); });
  }
  function readableStreamError(s, e) {
    s._state = "errored"; s._storedError = e;
    var reader = s._reader;
    if (!reader) return;
    reader._closed.reject(e); handled(reader._closed.promise);
    var requests = reader._readRequests;
    reader._readRequests = [];
    requests.forEach(function (r) { r.error(e); });
  }
  function readableStreamCancel(s, reason) {
    s._disturbed = true;
    if (s._state === "closed") return resolved(undefined);
    if (s._state === "errored") return rejected(s._storedError);
    readableStreamClose(s);
    var c = s._controller;
    resetQueue(c);
    var result = c._cancel(reason);
    clearReadableAlgorithms(c);
    return upon(result, noop);
  }
  function isReadable(x) { return readables.has(x); }

  function ReadableStreamDefaultReader(stream) {
    if (!(this instanceof ReadableStreamDefaultReader)) throw new TypeError("Constructor requires 'new'");
    if (!isReadable(stream)) throw new TypeError("ReadableStreamDefaultReader: not a ReadableStream");
    if (stream._reader) throw new TypeError("ReadableStreamDefaultReader: the stream is locked to another reader");
    this._stream = stream; stream._reader = this; this._readRequests = [];
    this._closed = stream._state === "readable" ? deferred() : settledWith(stream._state === "closed", stream._storedError);
  }
  function readerRead(reader, request) {
    var s = reader._stream;
    s._disturbed = true;
    if (s._state === "closed") request.close();
    else if (s._state === "errored") request.error(s._storedError);
    else pullSteps(s._controller, request);
  }
  function readerRelease(reader) {
    var s = reader._stream, e = new TypeError("the reader was released");
    if (s._state === "readable") reader._closed.reject(e);
    else reader._closed = settledWith(false, e);
    handled(reader._closed.promise);
    s._reader = undefined; reader._stream = undefined;
    var requests = reader._readRequests;
    reader._readRequests = [];
    requests.forEach(function (r) { r.error(e); });
  }
  function readRequest(d) {
    return {
      chunk: function (v) { d.resolve({ value: v, done: false }); },
      close: function () { d.resolve({ value: undefined, done: true }); },
      error: function (e) { d.reject(e); },
    };
  }
  getters(ReadableStreamDefaultReader.prototype, { closed: function () { brand(this, ReadableStreamDefaultReader, "ReadableStreamDefaultReader"); return this._closed.promise; } });
  methods(ReadableStreamDefaultReader.prototype, {
    read: function () {
      if (!(this instanceof ReadableStreamDefaultReader)) return rejected(new TypeError("Illegal invocation"));
      if (!this._stream) return rejected(new TypeError("the reader has no stream: it was released"));
      var d = deferred();
      readerRead(this, readRequest(d));
      return d.promise;
    },
    releaseLock: function () { brand(this, ReadableStreamDefaultReader, "ReadableStreamDefaultReader"); if (this._stream) readerRelease(this); },
    cancel: function (reason) {
      if (!(this instanceof ReadableStreamDefaultReader)) return rejected(new TypeError("Illegal invocation"));
      if (!this._stream) return rejected(new TypeError("the reader has no stream: it was released"));
      return readableStreamCancel(this._stream, reason);
    },
  });
  tag(ReadableStreamDefaultReader, "ReadableStreamDefaultReader");

  // ReadableStreamDefaultTee; Fetch clones the second branch's chunks.
  function tee(stream, cloneForBranch2) {
    var reader = new ReadableStreamDefaultReader(stream);
    var reading = false, readAgain = false, canceled1 = false, canceled2 = false, reason1, reason2, branch1, branch2;
    var cancelled = deferred();
    function pull() {
      if (reading) { readAgain = true; return resolved(); }
      reading = true;
      readerRead(reader, {
        chunk: function (chunk) {
          global.queueMicrotask(function () {
            readAgain = false;
            var second = chunk;
            if (!canceled2 && cloneForBranch2) {
              try { second = global.structuredClone(chunk); }
              catch (e) {
                readableControllerError(branch1._controller, e);
                readableControllerError(branch2._controller, e);
                cancelled.resolve(readableStreamCancel(stream, e));
                return;
              }
            }
            if (!canceled1) readableControllerEnqueue(branch1._controller, chunk);
            if (!canceled2) readableControllerEnqueue(branch2._controller, second);
            reading = false;
            if (readAgain) pull();
          });
        },
        close: function () {
          reading = false;
          if (!canceled1) readableControllerClose(branch1._controller);
          if (!canceled2) readableControllerClose(branch2._controller);
          if (!canceled1 || !canceled2) cancelled.resolve(undefined);
        },
        error: function () { reading = false; },
      });
      return resolved();
    }
    function cancel(first) {
      return function (reason) {
        if (first) { canceled1 = true; reason1 = reason; } else { canceled2 = true; reason2 = reason; }
        if (canceled1 && canceled2) cancelled.resolve(readableStreamCancel(stream, [reason1, reason2]));
        return cancelled.promise;
      };
    }
    branch1 = createReadableStream(noop, pull, cancel(true));
    branch2 = createReadableStream(noop, pull, cancel(false));
    upon(reader._closed.promise, undefined, function (r) {
      readableControllerError(branch1._controller, r);
      readableControllerError(branch2._controller, r);
      if (!canceled1 || !canceled2) cancelled.resolve(undefined);
    });
    return [branch1, branch2];
  }

  function pipe(source, dest, preventClose, preventAbort, preventCancel, signal) {
    var reader = new ReadableStreamDefaultReader(source), writer = new WritableStreamDefaultWriter(dest);
    source._disturbed = true;
    var shuttingDown = false, currentWrite = resolved(undefined), d = deferred(), abort;
    if (signal !== undefined) {
      abort = function () {
        var error = signal.reason, actions = [];
        if (!preventAbort) actions.push(function () { return dest._state === "writable" ? writableStreamAbort(dest, error) : resolved(undefined); });
        if (!preventCancel) actions.push(function () { return source._state === "readable" ? readableStreamCancel(source, error) : resolved(undefined); });
        shutdownWithAction(function () { return PromiseC.all(actions.map(function (a) { return a(); })); }, true, error);
      };
      if (signal.aborted) { abort(); return d.promise; }
      signal.addEventListener("abort", abort);
    }
    function pipeStep() {
      if (shuttingDown) return resolved(true);
      return upon(writer._ready.promise, function () {
        return new PromiseC(function (resolveRead, rejectRead) {
          readerRead(reader, {
            chunk: function (chunk) { currentWrite = upon(writerWrite(writer, chunk), undefined, noop); resolveRead(false); },
            close: function () { resolveRead(true); },
            error: rejectRead,
          });
        });
      });
    }
    function pipeLoop() {
      return new PromiseC(function (resolveLoop, rejectLoop) {
        function next(done) { if (done) resolveLoop(); else upon(pipeStep(), next, rejectLoop); }
        next(false);
      });
    }
    function waitForWrites() {
      var old = currentWrite;
      return upon(currentWrite, function () { return old !== currentWrite ? waitForWrites() : undefined; });
    }
    function shutdownWithAction(action, isError, error) {
      if (shuttingDown) return;
      shuttingDown = true;
      function rest() { upon(action(), function () { finalize(isError, error); }, function (e) { finalize(true, e); }); }
      if (dest._state === "writable" && !closeQueuedOrInFlight(dest)) upon(waitForWrites(), rest);
      else rest();
    }
    function shutdown(isError, error) {
      if (shuttingDown) return;
      shuttingDown = true;
      if (dest._state === "writable" && !closeQueuedOrInFlight(dest)) upon(waitForWrites(), function () { finalize(isError, error); });
      else finalize(isError, error);
    }
    function finalize(isError, error) {
      writerRelease(writer);
      readerRelease(reader);
      if (signal !== undefined) signal.removeEventListener("abort", abort);
      if (isError) d.reject(error); else d.resolve(undefined);
    }
    function whenErrored(stream, promise, action) {
      if (stream._state === "errored") action(stream._storedError);
      else upon(promise, undefined, action);
    }
    whenErrored(source, reader._closed.promise, function (e) {
      if (!preventAbort) shutdownWithAction(function () { return writableStreamAbort(dest, e); }, true, e);
      else shutdown(true, e);
    });
    whenErrored(dest, writer._closed.promise, function (e) {
      if (!preventCancel) shutdownWithAction(function () { return readableStreamCancel(source, e); }, true, e);
      else shutdown(true, e);
    });
    if (source._state === "closed") closed();
    else upon(reader._closed.promise, closed, noop);
    function closed() {
      if (!preventClose) shutdownWithAction(function () { return writerCloseWithErrorPropagation(writer); });
      else shutdown();
    }
    if (closeQueuedOrInFlight(dest) || dest._state === "closed") {
      var destClosed = new TypeError("the destination writable stream closed before all data could be piped to it");
      if (!preventCancel) shutdownWithAction(function () { return readableStreamCancel(source, destClosed); }, true, destClosed);
      else shutdown(true, destClosed);
    }
    handled(pipeLoop());
    return d.promise;
  }
  function pipeOptions(options) {
    options = options == null ? {} : options;
    var signal = options.signal;
    if (signal !== undefined && !(signal instanceof global.AbortSignal)) throw new TypeError("pipeTo: signal is not an AbortSignal");
    return { preventClose: !!options.preventClose, preventAbort: !!options.preventAbort, preventCancel: !!options.preventCancel, signal: signal };
  }

  function iterator(stream, preventCancel) {
    var reader = new ReadableStreamDefaultReader(stream), finished = false, ongoing;
    function nextSteps() {
      if (finished) return resolved({ value: undefined, done: true });
      var d = deferred();
      readerRead(reader, {
        chunk: function (v) { ongoing = undefined; d.resolve({ value: v, done: false }); },
        close: function () { ongoing = undefined; finished = true; readerRelease(reader); d.resolve({ value: undefined, done: true }); },
        error: function (e) { ongoing = undefined; finished = true; readerRelease(reader); d.reject(e); },
      });
      return d.promise;
    }
    var it = {
      next: function () { ongoing = ongoing ? upon(ongoing, nextSteps, nextSteps) : nextSteps(); return ongoing; },
      return: function (value) {
        function steps() {
          if (finished) return resolved({ value: value, done: true });
          finished = true;
          if (!preventCancel) {
            var result = readableStreamCancel(stream, value);
            readerRelease(reader);
            return upon(result, function () { return { value: value, done: true }; });
          }
          readerRelease(reader);
          return resolved({ value: value, done: true });
        }
        return ongoing ? upon(ongoing, steps, steps) : steps();
      },
    };
    if (typeof Symbol.asyncIterator === "symbol") it[Symbol.asyncIterator] = function () { return this; };
    return it;
  }

  getters(ReadableStream.prototype, { locked: function () { brand(this, ReadableStream, "ReadableStream"); return this._reader !== undefined; } });
  methods(ReadableStream.prototype, {
    cancel: function (reason) {
      if (!isReadable(this)) return rejected(new TypeError("Illegal invocation"));
      if (this._reader) return rejected(new TypeError("cancel: the stream is locked to a reader"));
      return readableStreamCancel(this, reason);
    },
    getReader: function (options) {
      brand(this, ReadableStream, "ReadableStream");
      var mode = options == null ? undefined : options.mode;
      if (mode === undefined) return new ReadableStreamDefaultReader(this);
      if (String(mode) === "byob") throw new TypeError("getReader({ mode: \"byob\" }) reads a byte stream, which is unavailable in data sources");
      throw new TypeError("getReader: mode must be \"byob\" or undefined");
    },
    pipeThrough: function (transform, options) {
      brand(this, ReadableStream, "ReadableStream");
      var writable = transform && transform.writable, readable = transform && transform.readable;
      if (!(writable instanceof WritableStream)) throw new TypeError("pipeThrough: transform.writable is not a WritableStream");
      if (!isReadable(readable)) throw new TypeError("pipeThrough: transform.readable is not a ReadableStream");
      var o = pipeOptions(options);
      if (this._reader) throw new TypeError("pipeThrough: the stream is locked to a reader");
      if (writable._writer) throw new TypeError("pipeThrough: the writable side is locked to a writer");
      handled(pipe(this, writable, o.preventClose, o.preventAbort, o.preventCancel, o.signal));
      return readable;
    },
    pipeTo: function (dest, options) {
      if (!isReadable(this)) return rejected(new TypeError("Illegal invocation"));
      if (!(dest instanceof WritableStream)) return rejected(new TypeError("pipeTo: the destination is not a WritableStream"));
      var o;
      try { o = pipeOptions(options); } catch (e) { return rejected(e); }
      if (this._reader) return rejected(new TypeError("pipeTo: the stream is locked to a reader"));
      if (dest._writer) return rejected(new TypeError("pipeTo: the destination is locked to a writer"));
      return pipe(this, dest, o.preventClose, o.preventAbort, o.preventCancel, o.signal);
    },
    tee: function () { brand(this, ReadableStream, "ReadableStream"); return tee(this); },
    values: function (options) {
      brand(this, ReadableStream, "ReadableStream");
      return iterator(this, !!(options && options.preventCancel));
    },
  });
  if (typeof Symbol.asyncIterator === "symbol") defineProperty(ReadableStream.prototype, Symbol.asyncIterator, { value: ReadableStream.prototype.values, writable: true, configurable: true });
  // ReadableStream.from(asyncIterable): an async iterable's values, or a sync one's, each awaited.
  methods(ReadableStream, {
    from: function (iterable) {
      var get = iterable == null ? undefined : (typeof Symbol.asyncIterator === "symbol" ? iterable[Symbol.asyncIterator] : undefined), sync = false;
      if (get == null) { get = iterable == null ? undefined : iterable[Symbol.iterator]; sync = true; }
      if (typeof get !== "function") throw new TypeError("ReadableStream.from: the argument is not iterable");
      var it = get.call(iterable);
      if (it === null || typeof it !== "object") throw new TypeError("ReadableStream.from: the iterator is not an object");
      var next = it.next, stream;
      function step(result) {
        if (result === null || typeof result !== "object") throw new TypeError("ReadableStream.from: an iterator result is not an object");
        if (!sync) return result;
        return upon(resolved(result.value), function (v) { return { value: v, done: result.done }; });
      }
      stream = createReadableStream(noop, function () {
        var r;
        try { r = resolved(step(next.call(it))); } catch (e) { return rejected(e); }
        return upon(r, function (result) {
          if (result === null || typeof result !== "object") throw new TypeError("ReadableStream.from: an iterator result is not an object");
          if (result.done) readableControllerClose(stream._controller);
          else readableControllerEnqueue(stream._controller, result.value);
        });
      }, function (reason) {
        var ret;
        try { ret = it.return; } catch (e) { return rejected(e); }
        if (ret == null) return resolved(undefined);
        return upon(promiseCall(ret, it, [reason]), function (r) {
          if (r === null || typeof r !== "object") throw new TypeError("ReadableStream.from: return() gave no object");
        });
      }, 0);
      return stream;
    },
  });
  tag(ReadableStream, "ReadableStream");

  // --- WritableStream ----------------------------------------------------------
  var CLOSE = {};
  function WritableStream(sink, strategy) {
    if (!(this instanceof WritableStream)) throw new TypeError("Constructor requires 'new'");
    if (sink === undefined) sink = null;
    if (sink !== null && typeof sink !== "object" && typeof sink !== "function") throw new TypeError("WritableStream: the underlying sink must be an object");
    if (sink != null && sink.type !== undefined) throw new RangeError("WritableStream: type is not a valid sink property");
    initializeWritableStream(this);
    var size = extractSizeAlgorithm(strategy), hwm = extractHighWaterMark(strategy, 1);
    var start = method(sink, "start", "WritableStream"), write = method(sink, "write", "WritableStream"),
      close = method(sink, "close", "WritableStream"), abort = method(sink, "abort", "WritableStream");
    var c = Object.create(WritableStreamDefaultController.prototype);
    setUpWritableController(this, c,
      start ? function () { return start.call(sink, c); } : noop,
      write ? function (chunk) { return promiseCall(write, sink, [chunk, c]); } : function () { return resolved(); },
      close ? function () { return promiseCall(close, sink, []); } : function () { return resolved(); },
      abort ? function (r) { return promiseCall(abort, sink, [r]); } : function () { return resolved(); },
      hwm, size);
  }
  function initializeWritableStream(s) {
    s._state = "writable"; s._storedError = undefined; s._writer = undefined; s._controller = undefined;
    s._inFlightWrite = s._closeRequest = s._inFlightClose = s._pendingAbort = undefined;
    s._writeRequests = []; s._backpressure = false;
  }
  function createWritableStream(start, write, close, abort, hwm, size) {
    var s = Object.create(WritableStream.prototype);
    initializeWritableStream(s);
    setUpWritableController(s, Object.create(WritableStreamDefaultController.prototype), start, write, close, abort, hwm, size);
    return s;
  }
  function WritableStreamDefaultController() { illegal(); }
  function setUpWritableController(stream, c, start, write, close, abort, hwm, size) {
    c._stream = stream; stream._controller = c; resetQueue(c);
    c._abortController = new global.AbortController();
    c._started = false; c._size = size; c._hwm = hwm; c._write = write; c._close = close; c._abort = abort;
    updateBackpressure(stream, writableBackpressure(c));
    var started = start();
    upon(resolved(started), function () { c._started = true; advanceQueueIfNeeded(c); },
      function (r) { c._started = true; dealWithRejection(stream, r); });
  }
  function clearWritableAlgorithms(c) { c._write = c._close = c._abort = c._size = undefined; }
  function writableDesiredSize(c) { return c._hwm - c._queueTotalSize; }
  function writableBackpressure(c) { return writableDesiredSize(c) <= 0; }
  function writableControllerError(c, e) { clearWritableAlgorithms(c); startErroring(c._stream, e); }
  function writableErrorIfNeeded(c, e) { if (c._stream._state === "writable") writableControllerError(c, e); }
  function advanceQueueIfNeeded(c) {
    var s = c._stream;
    if (!c._started || s._inFlightWrite !== undefined) return;
    if (s._state === "closed" || s._state === "errored") return;
    if (s._state === "erroring") { finishErroring(s); return; }
    if (c._queue.length === 0) return;
    var value = c._queue[0].value;
    if (value === CLOSE) {
      s._inFlightClose = s._closeRequest; s._closeRequest = undefined;
      dequeueValue(c);
      var closing = c._close();
      clearWritableAlgorithms(c);
      upon(closing, function () { finishInFlightClose(s); }, function (r) { finishInFlightCloseWithError(s, r); });
    } else {
      s._inFlightWrite = s._writeRequests.shift();
      upon(c._write(value), function () {
        s._inFlightWrite.resolve(undefined); s._inFlightWrite = undefined;
        dequeueValue(c);
        if (!closeQueuedOrInFlight(s) && s._state === "writable") updateBackpressure(s, writableBackpressure(c));
        advanceQueueIfNeeded(c);
      }, function (r) {
        if (s._state === "writable") clearWritableAlgorithms(c);
        s._inFlightWrite.reject(r); s._inFlightWrite = undefined;
        dealWithRejection(s, r);
      });
    }
  }
  getters(WritableStreamDefaultController.prototype, { signal: function () { brand(this, WritableStreamDefaultController, "WritableStreamDefaultController"); return this._abortController.signal; } });
  methods(WritableStreamDefaultController.prototype, {
    error: function (e) {
      brand(this, WritableStreamDefaultController, "WritableStreamDefaultController");
      if (this._stream._state === "writable") writableControllerError(this, e);
    },
  });
  tag(WritableStreamDefaultController, "WritableStreamDefaultController");

  function writableStreamAbort(s, reason) {
    if (s._state === "closed" || s._state === "errored") return resolved(undefined);
    s._controller._abortController.abort(reason);
    if (s._state === "closed" || s._state === "errored") return resolved(undefined);
    if (s._pendingAbort) return s._pendingAbort.promise;
    var already = s._state === "erroring";
    if (already) reason = undefined;
    var d = deferred();
    d.reason = reason; d.wasAlreadyErroring = already;
    s._pendingAbort = d;
    if (!already) startErroring(s, reason);
    return d.promise;
  }
  function writableStreamClose(s) {
    if (s._state === "closed" || s._state === "errored") return rejected(new TypeError("the stream is closed or errored"));
    var d = deferred();
    s._closeRequest = d;
    if (s._writer && s._backpressure && s._state === "writable") s._writer._ready.resolve(undefined);
    enqueueValueWithSize(s._controller, CLOSE, 0);
    advanceQueueIfNeeded(s._controller);
    return d.promise;
  }
  function dealWithRejection(s, e) { if (s._state === "writable") startErroring(s, e); else finishErroring(s); }
  function startErroring(s, reason) {
    var c = s._controller;
    s._state = "erroring"; s._storedError = reason;
    if (s._writer) ensureReadyRejected(s._writer, reason);
    if (s._inFlightWrite === undefined && s._inFlightClose === undefined && c._started) finishErroring(s);
  }
  function finishErroring(s) {
    s._state = "errored";
    resetQueue(s._controller);
    var e = s._storedError, requests = s._writeRequests;
    s._writeRequests = [];
    requests.forEach(function (r) { r.reject(e); });
    var abort = s._pendingAbort;
    if (!abort) { rejectCloseAndClosed(s); return; }
    s._pendingAbort = undefined;
    if (abort.wasAlreadyErroring) { abort.reject(e); rejectCloseAndClosed(s); return; }
    var c = s._controller, result = c._abort(abort.reason);
    clearWritableAlgorithms(c);
    upon(result, function () { abort.resolve(undefined); rejectCloseAndClosed(s); }, function (r) { abort.reject(r); rejectCloseAndClosed(s); });
  }
  function finishInFlightClose(s) {
    s._inFlightClose.resolve(undefined); s._inFlightClose = undefined;
    if (s._state === "erroring") {
      s._storedError = undefined;
      if (s._pendingAbort) { s._pendingAbort.resolve(undefined); s._pendingAbort = undefined; }
    }
    s._state = "closed";
    if (s._writer) s._writer._closed.resolve(undefined);
  }
  function finishInFlightCloseWithError(s, e) {
    s._inFlightClose.reject(e); s._inFlightClose = undefined;
    if (s._pendingAbort) { s._pendingAbort.reject(e); s._pendingAbort = undefined; }
    dealWithRejection(s, e);
  }
  function closeQueuedOrInFlight(s) { return s._closeRequest !== undefined || s._inFlightClose !== undefined; }
  function rejectCloseAndClosed(s) {
    if (s._closeRequest) { s._closeRequest.reject(s._storedError); s._closeRequest = undefined; }
    if (s._writer) { s._writer._closed.reject(s._storedError); handled(s._writer._closed.promise); }
  }
  function updateBackpressure(s, bp) {
    if (s._writer && bp !== s._backpressure) {
      if (bp) s._writer._ready = deferred();
      else s._writer._ready.resolve(undefined);
    }
    s._backpressure = bp;
  }
  getters(WritableStream.prototype, { locked: function () { brand(this, WritableStream, "WritableStream"); return this._writer !== undefined; } });
  methods(WritableStream.prototype, {
    abort: function (reason) {
      if (!(this instanceof WritableStream)) return rejected(new TypeError("Illegal invocation"));
      if (this._writer) return rejected(new TypeError("abort: the stream is locked to a writer"));
      return writableStreamAbort(this, reason);
    },
    close: function () {
      if (!(this instanceof WritableStream)) return rejected(new TypeError("Illegal invocation"));
      if (this._writer) return rejected(new TypeError("close: the stream is locked to a writer"));
      if (closeQueuedOrInFlight(this)) return rejected(new TypeError("close: the stream is already closing"));
      return writableStreamClose(this);
    },
    getWriter: function () { brand(this, WritableStream, "WritableStream"); return new WritableStreamDefaultWriter(this); },
  });
  tag(WritableStream, "WritableStream");

  function WritableStreamDefaultWriter(stream) {
    if (!(this instanceof WritableStreamDefaultWriter)) throw new TypeError("Constructor requires 'new'");
    if (!(stream instanceof WritableStream)) throw new TypeError("WritableStreamDefaultWriter: not a WritableStream");
    if (stream._writer) throw new TypeError("WritableStreamDefaultWriter: the stream is locked to another writer");
    this._stream = stream; stream._writer = this;
    var state = stream._state;
    if (state === "writable") {
      this._ready = !closeQueuedOrInFlight(stream) && stream._backpressure ? deferred() : settledWith(true);
      this._closed = deferred();
    } else if (state === "erroring") {
      this._ready = settledWith(false, stream._storedError);
      this._closed = deferred();
    } else if (state === "closed") {
      this._ready = settledWith(true); this._closed = settledWith(true);
    } else {
      this._ready = settledWith(false, stream._storedError);
      this._closed = settledWith(false, stream._storedError);
    }
  }
  function ensureReadyRejected(w, e) {
    if (w._ready.state === "pending") w._ready.reject(e);
    else w._ready = settledWith(false, e);
    handled(w._ready.promise);
  }
  function ensureClosedRejected(w, e) {
    if (w._closed.state === "pending") w._closed.reject(e);
    else w._closed = settledWith(false, e);
    handled(w._closed.promise);
  }
  function writerRelease(w) {
    var e = new TypeError("the writer was released");
    ensureReadyRejected(w, e); ensureClosedRejected(w, e);
    w._stream._writer = undefined; w._stream = undefined;
  }
  function writerWrite(w, chunk) {
    var s = w._stream, c = s._controller, size;
    try { size = c._size(chunk); } catch (e) { writableErrorIfNeeded(c, e); size = 1; }
    if (s !== w._stream) return rejected(new TypeError("the writer was released"));
    if (s._state === "errored") return rejected(s._storedError);
    if (closeQueuedOrInFlight(s) || s._state === "closed") return rejected(new TypeError("the stream is closing or closed"));
    if (s._state === "erroring") return rejected(s._storedError);
    var d = deferred();
    s._writeRequests.push(d);
    try { enqueueValueWithSize(c, chunk, size); } catch (e) { writableErrorIfNeeded(c, e); return d.promise; }
    if (!closeQueuedOrInFlight(s) && s._state === "writable") updateBackpressure(s, writableBackpressure(c));
    advanceQueueIfNeeded(c);
    return d.promise;
  }
  function writerCloseWithErrorPropagation(w) {
    var s = w._stream;
    if (closeQueuedOrInFlight(s) || s._state === "closed") return resolved(undefined);
    if (s._state === "errored") return rejected(s._storedError);
    return writableStreamClose(s);
  }
  function writer(self) {
    if (!(self instanceof WritableStreamDefaultWriter)) throw new TypeError("Illegal invocation");
    if (!self._stream) throw new TypeError("the writer has no stream: it was released");
    return self._stream;
  }
  getters(WritableStreamDefaultWriter.prototype, {
    closed: function () { brand(this, WritableStreamDefaultWriter, "WritableStreamDefaultWriter"); return this._closed.promise; },
    ready: function () { brand(this, WritableStreamDefaultWriter, "WritableStreamDefaultWriter"); return this._ready.promise; },
    desiredSize: function () {
      var s = writer(this);
      return s._state === "errored" || s._state === "erroring" ? null : s._state === "closed" ? 0 : writableDesiredSize(s._controller);
    },
  });
  methods(WritableStreamDefaultWriter.prototype, {
    abort: function (reason) {
      var s;
      try { s = writer(this); } catch (e) { return rejected(e); }
      return writableStreamAbort(s, reason);
    },
    close: function () {
      var s;
      try { s = writer(this); } catch (e) { return rejected(e); }
      if (closeQueuedOrInFlight(s)) return rejected(new TypeError("close: the stream is already closing"));
      return writableStreamClose(s);
    },
    releaseLock: function () { brand(this, WritableStreamDefaultWriter, "WritableStreamDefaultWriter"); if (this._stream) writerRelease(this); },
    write: function (chunk) {
      try { writer(this); } catch (e) { return rejected(e); }
      return writerWrite(this, chunk);
    },
  });
  tag(WritableStreamDefaultWriter, "WritableStreamDefaultWriter");

  // --- TransformStream ---------------------------------------------------------
  function TransformStream(transformer, writableStrategy, readableStrategy) {
    if (!(this instanceof TransformStream)) throw new TypeError("Constructor requires 'new'");
    if (transformer === undefined) transformer = null;
    if (transformer != null && transformer.readableType !== undefined) throw new RangeError("TransformStream: readableType is not a valid transformer property");
    if (transformer != null && transformer.writableType !== undefined) throw new RangeError("TransformStream: writableType is not a valid transformer property");
    var rhwm = extractHighWaterMark(readableStrategy, 0), rsize = extractSizeAlgorithm(readableStrategy);
    var whwm = extractHighWaterMark(writableStrategy, 1), wsize = extractSizeAlgorithm(writableStrategy);
    var start = method(transformer, "start", "TransformStream"), transform = method(transformer, "transform", "TransformStream"),
      flush = method(transformer, "flush", "TransformStream"), cancel = method(transformer, "cancel", "TransformStream");
    var started = deferred();
    initializeTransformStream(this, started.promise, whwm, wsize, rhwm, rsize);
    var c = Object.create(TransformStreamDefaultController.prototype);
    setUpTransformController(this, c,
      transform ? function (chunk) { return promiseCall(transform, transformer, [chunk, c]); }
        : function (chunk) { try { transformEnqueue(c, chunk); return resolved(); } catch (e) { return rejected(e); } },
      flush ? function () { return promiseCall(flush, transformer, [c]); } : function () { return resolved(); },
      cancel ? function (r) { return promiseCall(cancel, transformer, [r]); } : function () { return resolved(); });
    if (start) started.resolve(start.call(transformer, c));
    else started.resolve(undefined);
  }
  function initializeTransformStream(s, startPromise, whwm, wsize, rhwm, rsize) {
    function start() { return startPromise; }
    s._writable = createWritableStream(start,
      function (chunk) { return sinkWrite(s, chunk); }, function () { return sinkClose(s); }, function (r) { return sinkAbort(s, r); },
      whwm, wsize);
    s._readable = createReadableStream(start, function () { return sourcePull(s); }, function (r) { return sourceCancel(s, r); }, rhwm, rsize);
    s._backpressure = undefined; s._backpressureChange = undefined;
    setBackpressure(s, true);
    s._controller = undefined;
  }
  function TransformStreamDefaultController() { illegal(); }
  function setUpTransformController(s, c, transform, flush, cancel) {
    c._stream = s; s._controller = c;
    c._transform = transform; c._flush = flush; c._cancel = cancel; c._finish = undefined;
  }
  function clearTransformAlgorithms(c) { c._transform = c._flush = c._cancel = undefined; }
  function setBackpressure(s, bp) {
    if (s._backpressureChange) s._backpressureChange.resolve(undefined);
    s._backpressureChange = deferred();
    s._backpressure = bp;
  }
  function transformStreamError(s, e) { readableControllerError(s._readable._controller, e); errorWritableAndUnblockWrite(s, e); }
  function errorWritableAndUnblockWrite(s, e) {
    clearTransformAlgorithms(s._controller);
    writableErrorIfNeeded(s._writable._controller, e);
    if (s._backpressure) setBackpressure(s, false);
  }
  function transformEnqueue(c, chunk) {
    var s = c._stream, rc = s._readable._controller;
    if (!canCloseOrEnqueue(rc)) throw new TypeError("the readable side is not in a state that permits enqueue");
    try { readableControllerEnqueue(rc, chunk); } catch (e) { errorWritableAndUnblockWrite(s, e); throw s._readable._storedError; }
    if (readableHasBackpressure(rc) !== s._backpressure) setBackpressure(s, true);
  }
  function performTransform(c, chunk) {
    return upon(c._transform(chunk), undefined, function (r) { transformStreamError(c._stream, r); throw r; });
  }
  function sinkWrite(s, chunk) {
    var c = s._controller;
    if (s._backpressure) {
      return upon(s._backpressureChange.promise, function () {
        if (s._writable._state === "erroring") throw s._writable._storedError;
        return performTransform(c, chunk);
      });
    }
    return performTransform(c, chunk);
  }
  function sinkAbort(s, reason) {
    var c = s._controller;
    if (c._finish) return c._finish.promise;
    var readable = s._readable;
    c._finish = deferred();
    var cancelled = c._cancel(reason);
    clearTransformAlgorithms(c);
    upon(cancelled, function () {
      if (readable._state === "errored") c._finish.reject(readable._storedError);
      else { readableControllerError(readable._controller, reason); c._finish.resolve(undefined); }
    }, function (r) { readableControllerError(readable._controller, r); c._finish.reject(r); });
    return c._finish.promise;
  }
  function sinkClose(s) {
    var c = s._controller;
    if (c._finish) return c._finish.promise;
    var readable = s._readable;
    c._finish = deferred();
    var flushed = c._flush();
    clearTransformAlgorithms(c);
    upon(flushed, function () {
      if (readable._state === "errored") c._finish.reject(readable._storedError);
      else { readableControllerClose(readable._controller); c._finish.resolve(undefined); }
    }, function (r) { readableControllerError(readable._controller, r); c._finish.reject(r); });
    return c._finish.promise;
  }
  function sourcePull(s) { setBackpressure(s, false); return s._backpressureChange.promise; }
  function sourceCancel(s, reason) {
    var c = s._controller;
    if (c._finish) return c._finish.promise;
    var writable = s._writable;
    c._finish = deferred();
    var cancelled = c._cancel(reason);
    clearTransformAlgorithms(c);
    upon(cancelled, function () {
      if (writable._state === "errored") c._finish.reject(writable._storedError);
      else { writableErrorIfNeeded(writable._controller, reason); if (s._backpressure) setBackpressure(s, false); c._finish.resolve(undefined); }
    }, function (r) { writableErrorIfNeeded(writable._controller, r); if (s._backpressure) setBackpressure(s, false); c._finish.reject(r); });
    return c._finish.promise;
  }
  getters(TransformStreamDefaultController.prototype, {
    desiredSize: function () { brand(this, TransformStreamDefaultController, "TransformStreamDefaultController"); return readableDesiredSize(this._stream._readable._controller); },
  });
  methods(TransformStreamDefaultController.prototype, {
    enqueue: function (chunk) { brand(this, TransformStreamDefaultController, "TransformStreamDefaultController"); transformEnqueue(this, chunk); },
    error: function (e) { brand(this, TransformStreamDefaultController, "TransformStreamDefaultController"); transformStreamError(this._stream, e); },
    terminate: function () {
      brand(this, TransformStreamDefaultController, "TransformStreamDefaultController");
      var s = this._stream;
      readableControllerClose(s._readable._controller);
      errorWritableAndUnblockWrite(s, new TypeError("the TransformStream was terminated"));
    },
  });
  tag(TransformStreamDefaultController, "TransformStreamDefaultController");
  getters(TransformStream.prototype, {
    readable: function () { brand(this, TransformStream, "TransformStream"); return this._readable; },
    writable: function () { brand(this, TransformStream, "TransformStream"); return this._writable; },
  });
  tag(TransformStream, "TransformStream");

  // --- Compression -------------------------------------------------------------
  // "Set up a TransformStream" with the codec as its transform and flush. Each
  // chunk is the bytes of an ArrayBuffer or a view, as the standard's
  // BufferSource; a codec error is a TypeError that errors both sides.
  var EMPTY = new ArrayBuffer(0), FORMATS = ["deflate", "deflate-raw", "gzip"];
  function codec(self, kind, name, format) {
    format = String(format);
    if (FORMATS.indexOf(format) < 0) throw new TypeError(name + ": the format \"" + format + "\" is not supported; use \"deflate\", \"deflate-raw\" or \"gzip\"");
    // The context lives in Rust as long as the closures below can reach it.
    var handle;
    try { handle = door(12, kind + " " + format, EMPTY); }
    catch (e) { throw new TypeError(name + ": " + (e && e.message !== undefined ? e.message : e)); }
    var state = { handle: handle };
    keep(Number(state.handle), state);
    function run(op, a, buffer) {
      var n;
      try { n = Number(door(op, a, buffer)); }
      catch (e) { throw new TypeError(name + ": " + (e && e.message !== undefined ? e.message : e)); }
      if (!n) return undefined;
      var out = new ArrayBuffer(n);
      door(15, "", out);
      return new Uint8Array(out);
    }
    var s = Object.create(TransformStream.prototype);
    initializeTransformStream(s, resolved(undefined), 1, function () { return 1; }, 0, function () { return 1; });
    var c = Object.create(TransformStreamDefaultController.prototype);
    setUpTransformController(s, c, function (chunk) {
      try {
        var buffer, at = 0, length;
        if (chunk instanceof ArrayBuffer) { buffer = chunk; length = chunk.byteLength; }
        else if (ArrayBuffer.isView(chunk) && chunk.buffer instanceof ArrayBuffer) { buffer = chunk.buffer; at = chunk.byteOffset; length = chunk.byteLength; }
        else throw new TypeError(name + ": a chunk must be an ArrayBuffer or an ArrayBufferView");
        var out = run(13, state.handle + " " + at + " " + length, buffer);
        if (out) transformEnqueue(c, out);
        return resolved();
      } catch (e) { return rejected(e); }
    }, function () {
      try {
        var out = run(14, state.handle, EMPTY);
        if (out) transformEnqueue(c, out);
        return resolved();
      } catch (e) { return rejected(e); }
    }, function () {
      door(10, state.handle, EMPTY);
      return resolved();
    });
    defineProperty(self, "_transform", { value: s });
  }
  function DecompressionStream(format) {
    if (!(this instanceof DecompressionStream)) throw new TypeError("Constructor requires 'new'");
    codec(this, "d", "DecompressionStream", format);
  }
  function CompressionStream(format) {
    if (!(this instanceof CompressionStream)) throw new TypeError("Constructor requires 'new'");
    codec(this, "c", "CompressionStream", format);
  }
  [[DecompressionStream, "DecompressionStream"], [CompressionStream, "CompressionStream"]].forEach(function (pair) {
    var C = pair[0], name = pair[1];
    getters(C.prototype, {
      readable: function () { brand(this, C, name); return this._transform._readable; },
      writable: function () { brand(this, C, name); return this._transform._writable; },
    });
    tag(C, name);
  });

  // Fetch's stream-body operations for binary.js, which takes and deletes this
  // handoff. Browser realms use their native Response for the same operations.
  function StreamBody(stream) {
    if (stream._reader || stream._disturbed) throw new TypeError("Response: the ReadableStream is locked or was read already");
    this._body = stream;
  }
  getters(StreamBody.prototype, {
    body: function () { return this._body; },
    bodyUsed: function () { return this._body._disturbed; },
  });
  methods(StreamBody.prototype, {
    arrayBuffer: function () {
      var stream = this._body;
      if (stream._reader || stream._disturbed) return rejected(new TypeError("Response: the ReadableStream is locked or was read already"));
      var reader = new ReadableStreamDefaultReader(stream), chunks = [], length = 0, result = deferred();
      function next() {
        readerRead(reader, {
          chunk: function (chunk) {
            try {
              if (typedName.call(chunk) !== "Uint8Array") throw new TypeError("Response: a ReadableStream chunk must be a Uint8Array");
              // Fetch snapshots a chunk before the next pull can reuse its buffer.
              var copy = new Uint8Array(chunk);
              chunks.push(copy); length += copy.byteLength;
              upon(resolved(), next, result.reject);
            } catch (e) { result.reject(e); }
          },
          close: function () {
            try {
              var all = new Uint8Array(length), at = 0;
              chunks.forEach(function (chunk) { all.set(chunk, at); at += chunk.byteLength; });
              result.resolve(all.buffer);
            } catch (e) { result.reject(e); }
          },
          error: result.reject,
        });
      }
      next();
      return result.promise;
    },
    clone: function () {
      var stream = this._body;
      if (stream._reader || stream._disturbed) throw new TypeError("Response.clone(): the ReadableStream is locked or was read already");
      var branches = tee(stream, true);
      this._body = branches[0];
      return new StreamBody(branches[1]);
    },
  });
  global.__exact_stream_body = StreamBody;

  [["ReadableStream", ReadableStream], ["ReadableStreamDefaultReader", ReadableStreamDefaultReader],
    ["ReadableStreamDefaultController", ReadableStreamDefaultController], ["WritableStream", WritableStream],
    ["WritableStreamDefaultWriter", WritableStreamDefaultWriter], ["WritableStreamDefaultController", WritableStreamDefaultController],
    ["TransformStream", TransformStream], ["TransformStreamDefaultController", TransformStreamDefaultController],
    ["CountQueuingStrategy", CountQueuingStrategy], ["ByteLengthQueuingStrategy", ByteLengthQueuingStrategy],
    ["DecompressionStream", DecompressionStream], ["CompressionStream", CompressionStream]].forEach(function (pair) {
    defineProperty(global, pair[0], { value: pair[1], writable: true, configurable: true });
  });
})(globalThis);
