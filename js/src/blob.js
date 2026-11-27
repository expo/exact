// WHATWG Blob, File, FormData, and Request for Hermes (LLP 1195.000 D5 P0a).
// Installed by prelude.js after time.virtualClock() so File.lastModified reads
// the virtualized clock. Browser realms already provide these classes natively.
(function (global) {
  "use strict";
  global.__exact_blob = function (env) {
    var DateNow = env.DateNow,
      copyBytes = env.copyBytes,
      encoder = new global.TextEncoder(),
      decoder = new global.TextDecoder(),
      HeadersCtor = global.Headers,
      AbortControllerCtor = global.AbortController,
      ReadableStreamCtor = global.ReadableStream,
      URLSearchParamsCtor = global.URLSearchParams,
      formText = URLSearchParamsCtor && URLSearchParamsCtor.prototype.toString,
      boundaryCounter = 0;

    var blobs = new WeakMap(),
      files = new WeakMap(),
      forms = new WeakMap(),
      formIterators = new WeakMap(),
      requests = new WeakMap();

    function domString(v) {
      if (typeof v === "symbol") throw new TypeError("cannot convert a Symbol to a string");
      return String(v);
    }
    function usv(v) {
      var s = domString(v),
        out = "";
      for (var i = 0; i < s.length; i++) {
        var c = s.charCodeAt(i);
        if (c >= 0xd800 && c <= 0xdbff) {
          if (i + 1 < s.length) {
            var n = s.charCodeAt(i + 1);
            if (n >= 0xdc00 && n <= 0xdfff) {
              out += String.fromCharCode(c, n);
              i++;
              continue;
            }
          }
          out += "\ufffd";
        } else if (c >= 0xdc00 && c <= 0xdfff) out += "\ufffd";
        else out += String.fromCharCode(c);
      }
      return out;
    }
    function normalizedType(v) {
      var s = domString(v === undefined ? "" : v);
      for (var i = 0; i < s.length; i++) {
        var c = s.charCodeAt(i);
        if (c < 0x20 || c > 0x7e) return "";
      }
      return s.toLowerCase();
    }
    function clampLongLong(v) {
      var n = Number(v);
      if (n !== n || n === 0) return 0;
      if (n <= -9007199254740991) return -9007199254740991;
      if (n >= 9007199254740991) return 9007199254740991;
      var lo = Math.floor(n),
        f = n - lo;
      if (f < 0.5) return lo;
      if (f > 0.5) return lo + 1;
      return lo % 2 === 0 ? lo : lo + 1;
    }
    function sliceBytes(src, start, len) {
      var out = new Uint8Array(len);
      if (len > 0) out.set(src.subarray(start, start + len));
      return out;
    }
    function concatChunks(chunks, total) {
      var out = new Uint8Array(total),
        at = 0;
      for (var i = 0; i < chunks.length; i++) {
        out.set(chunks[i], at);
        at += chunks[i].byteLength;
      }
      return out;
    }
    function requireSlot(map, obj, name) {
      var s = map.get(obj);
      if (!s) throw new TypeError("Illegal invocation: not a " + name);
      return s;
    }
    function collectParts(parts) {
      if (parts == null || (typeof parts !== "object" && typeof parts !== "function"))
        throw new TypeError("Blob parts must be iterable");
      var iterFn = parts[Symbol.iterator];
      if (typeof iterFn !== "function") throw new TypeError("Blob parts must be iterable");
      var iter = iterFn.call(parts),
        chunks = [],
        total = 0,
        step;
      while (!(step = iter.next()).done) {
        var part = step.value,
          chunk,
          bs = blobs.get(part);
        if (bs) {
          chunk = sliceBytes(bs.bytes, 0, bs.size);
        } else if (part instanceof ArrayBuffer || ArrayBuffer.isView(part)) {
          chunk = new Uint8Array(copyBytes(part, "Blob"));
        } else {
          chunk = encoder.encode(usv(part));
        }
        chunks.push(chunk);
        total += chunk.byteLength;
      }
      return concatChunks(chunks, total);
    }
    function initBlob(obj, parts, options) {
      if (parts === undefined) parts = [];
      if (options == null) options = {};
      if (typeof options !== "object" && typeof options !== "function")
        throw new TypeError("Blob options must be an object");
      var bytes = collectParts(parts);
      var endings = options.endings === undefined ? "transparent" : domString(options.endings);
      if (endings !== "transparent" && endings !== "native") throw new TypeError("invalid Blob endings value");
      blobs.set(obj, { bytes: bytes, size: bytes.byteLength, type: normalizedType(options.type) });
    }

    function Blob(parts, options) {
      if (!new.target) throw new TypeError("Blob must be constructed with new");
      initBlob(this, parts, options);
    }
    Object.defineProperties(Blob.prototype, {
      size: {
        get: function () {
          return requireSlot(blobs, this, "Blob").size;
        },
        enumerable: true,
        configurable: true,
      },
      type: {
        get: function () {
          return requireSlot(blobs, this, "Blob").type;
        },
        enumerable: true,
        configurable: true,
      },
    });
    Blob.prototype.slice = function slice(start, end, contentType) {
      var s = requireSlot(blobs, this, "Blob"),
        size = s.size;
      var rs = start === undefined ? 0 : clampLongLong(start);
      var first = rs < 0 ? Math.max(size + rs, 0) : Math.min(rs, size);
      var re = end === undefined ? size : clampLongLong(end);
      var last = re < 0 ? Math.max(size + re, 0) : Math.min(re, size);
      var len = Math.max(last - first, 0);
      var out = Object.create(Blob.prototype);
      var bytes = sliceBytes(s.bytes, first, len);
      blobs.set(out, { bytes: bytes, size: len, type: normalizedType(contentType) });
      return out;
    };
    Blob.prototype.arrayBuffer = function arrayBuffer() {
      try {
        var s = requireSlot(blobs, this, "Blob");
        return Promise.resolve(sliceBytes(s.bytes, 0, s.size).buffer);
      } catch (e) {
        return Promise.reject(e);
      }
    };
    Blob.prototype.bytes = function bytes() {
      try {
        var s = requireSlot(blobs, this, "Blob");
        return Promise.resolve(sliceBytes(s.bytes, 0, s.size));
      } catch (e) {
        return Promise.reject(e);
      }
    };
    Blob.prototype.text = function text() {
      try {
        var s = requireSlot(blobs, this, "Blob");
        return Promise.resolve(decoder.decode(s.bytes));
      } catch (e) {
        return Promise.reject(e);
      }
    };
    Blob.prototype.stream = function stream() {
      var s = requireSlot(blobs, this, "Blob");
      var copy = sliceBytes(s.bytes, 0, s.size);
      return new ReadableStreamCtor({
        start: function (controller) {
          if (copy.byteLength > 0) controller.enqueue(copy);
          controller.close();
        },
      });
    };
    Object.defineProperty(Blob.prototype, Symbol.toStringTag, { value: "Blob", configurable: true });

    function File(parts, name, options) {
      if (!new.target) throw new TypeError("File must be constructed with new");
      if (arguments.length < 2) throw new TypeError("File requires fileBits and fileName");
      var bytes = collectParts(parts);
      var fileName = usv(name);
      if (options == null) options = {};
      if (typeof options !== "object" && typeof options !== "function")
        throw new TypeError("File options must be an object");
      var endings = options.endings === undefined ? "transparent" : domString(options.endings);
      if (endings !== "transparent" && endings !== "native") throw new TypeError("invalid File endings value");
      var lm =
        options.lastModified === undefined
          ? DateNow()
          : isFinite(Number(options.lastModified))
            ? Math.trunc(Number(options.lastModified))
            : 0;
      blobs.set(this, { bytes: bytes, size: bytes.byteLength, type: normalizedType(options.type) });
      files.set(this, { name: fileName, lastModified: lm });
    }
    File.prototype = Object.create(Blob.prototype, {
      constructor: { value: File, writable: true, configurable: true },
    });
    Object.defineProperties(File.prototype, {
      name: {
        get: function () {
          return requireSlot(files, this, "File").name;
        },
        enumerable: true,
        configurable: true,
      },
      lastModified: {
        get: function () {
          return requireSlot(files, this, "File").lastModified;
        },
        enumerable: true,
        configurable: true,
      },
    });
    Object.defineProperty(File.prototype, Symbol.toStringTag, { value: "File", configurable: true });

    function fileFromBlob(blob, name) {
      var bs = requireSlot(blobs, blob, "Blob"),
        fs = files.get(blob);
      var out = Object.create(File.prototype);
      blobs.set(out, { bytes: sliceBytes(bs.bytes, 0, bs.size), size: bs.size, type: bs.type });
      files.set(out, { name: usv(name), lastModified: fs ? fs.lastModified : DateNow() });
      return out;
    }
    function formEntry(name, value, filename, hasFilename) {
      var n = usv(name);
      if (blobs.get(value)) {
        if (hasFilename) value = fileFromBlob(value, filename);
        else if (!files.get(value)) value = fileFromBlob(value, "blob");
        return { name: n, value: value };
      }
      if (hasFilename) throw new TypeError("FormData: a filename requires a Blob value");
      return { name: n, value: usv(value) };
    }
    function FormData(form) {
      if (!new.target) throw new TypeError("FormData must be constructed with new");
      if (arguments.length !== 0 && form !== undefined)
        throw new TypeError("FormData from an HTML form is not supported without a DOM");
      forms.set(this, []);
    }
    FormData.prototype.append = function append(name, value, filename) {
      if (arguments.length < 2) throw new TypeError("FormData.append requires name and value");
      requireSlot(forms, this, "FormData").push(
        formEntry(name, value, filename, arguments.length >= 3 && filename !== undefined)
      );
    };
    FormData.prototype["delete"] = function (name) {
      if (arguments.length < 1) throw new TypeError("FormData.delete requires a name");
      var n = usv(name),
        list = requireSlot(forms, this, "FormData");
      for (var i = list.length - 1; i >= 0; i--) if (list[i].name === n) list.splice(i, 1);
    };
    FormData.prototype.get = function get(name) {
      if (arguments.length < 1) throw new TypeError("FormData.get requires a name");
      var n = usv(name),
        list = requireSlot(forms, this, "FormData");
      for (var i = 0; i < list.length; i++) if (list[i].name === n) return list[i].value;
      return null;
    };
    FormData.prototype.getAll = function getAll(name) {
      if (arguments.length < 1) throw new TypeError("FormData.getAll requires a name");
      var n = usv(name),
        list = requireSlot(forms, this, "FormData"),
        out = [];
      for (var i = 0; i < list.length; i++) if (list[i].name === n) out.push(list[i].value);
      return out;
    };
    FormData.prototype.has = function has(name) {
      if (arguments.length < 1) throw new TypeError("FormData.has requires a name");
      var n = usv(name),
        list = requireSlot(forms, this, "FormData");
      for (var i = 0; i < list.length; i++) if (list[i].name === n) return true;
      return false;
    };
    FormData.prototype.set = function set(name, value, filename) {
      if (arguments.length < 2) throw new TypeError("FormData.set requires name and value");
      var entry = formEntry(name, value, filename, arguments.length >= 3 && filename !== undefined);
      var list = requireSlot(forms, this, "FormData"),
        first = -1;
      for (var i = 0; i < list.length; i++) {
        if (list[i].name !== entry.name) continue;
        if (first < 0) {
          first = i;
          list[i] = entry;
        } else list.splice(i--, 1);
      }
      if (first < 0) list.push(entry);
    };
    var IterProto = Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]()));
    var FormIterProto = Object.create(IterProto);
    FormIterProto.next = function next() {
      var it = requireSlot(formIterators, this, "FormData iterator");
      var list = requireSlot(forms, it.form, "FormData");
      if (it.index >= list.length) return { done: true, value: undefined };
      var e = list[it.index++];
      if (it.kind === 0) return { done: false, value: [e.name, e.value] };
      return { done: false, value: it.kind === 1 ? e.name : e.value };
    };
    function makeFormIter(form, kind) {
      requireSlot(forms, form, "FormData");
      var it = Object.create(FormIterProto);
      formIterators.set(it, { form: form, kind: kind, index: 0 });
      return it;
    }
    FormData.prototype.entries = function entries() {
      return makeFormIter(this, 0);
    };
    FormData.prototype.keys = function keys() {
      return makeFormIter(this, 1);
    };
    FormData.prototype.values = function values() {
      return makeFormIter(this, 2);
    };
    FormData.prototype[Symbol.iterator] = FormData.prototype.entries;
    FormData.prototype.forEach = function forEach(cb, thisArg) {
      if (typeof cb !== "function") throw new TypeError("FormData.forEach: callback must be a function");
      var list = requireSlot(forms, this, "FormData");
      for (var i = 0; i < list.length; i++) cb.call(thisArg, list[i].value, list[i].name, this);
    };
    Object.defineProperty(FormData.prototype, Symbol.toStringTag, { value: "FormData", configurable: true });

    function escapeParam(s) {
      return s.replace(/\r/g, "%0D").replace(/\n/g, "%0A").replace(/"/g, "%22");
    }
    function encodeMultipart(entries) {
      var boundary = "----ExactFormBoundary" + (++boundaryCounter).toString(16).padStart(16, "0");
      var chunks = [],
        total = 0;
      function pushStr(s) {
        var b = encoder.encode(s);
        chunks.push(b);
        total += b.byteLength;
      }
      for (var i = 0; i < entries.length; i++) {
        var e = entries[i],
          fs = files.get(e.value);
        pushStr("--" + boundary + '\r\nContent-Disposition: form-data; name="' + escapeParam(e.name) + '"');
        if (fs) {
          var bs = requireSlot(blobs, e.value, "Blob");
          pushStr('; filename="' + escapeParam(fs.name) + '"\r\nContent-Type: ' + (bs.type || "application/octet-stream") + "\r\n\r\n");
          var copy = sliceBytes(bs.bytes, 0, bs.size);
          chunks.push(copy);
          total += copy.byteLength;
          pushStr("\r\n");
        } else {
          pushStr("\r\n\r\n");
          pushStr(String(e.value).replace(/\r\n|\r|\n/g, "\r\n"));
          pushStr("\r\n");
        }
      }
      pushStr("--" + boundary + "--\r\n");
      return { bytes: concatChunks(chunks, total), type: "multipart/form-data; boundary=" + boundary };
    }
    function extractBody(value) {
      var bs = blobs.get(value);
      if (bs) return { bytes: sliceBytes(bs.bytes, 0, bs.size), type: bs.type || null };
      var entries = forms.get(value);
      if (entries) return encodeMultipart(entries);
      return null;
    }

    // WHATWG Request class for Hermes (LLP 1195.000 D5 P0a).
    var NORMALIZED_METHODS = { delete: "DELETE", get: "GET", head: "HEAD", options: "OPTIONS", post: "POST", put: "PUT" };
    function normalizeMethod(m) {
      var s = domString(m);
      if (!/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(s)) throw new TypeError("Request: invalid HTTP method " + JSON.stringify(s));
      var low = s.toLowerCase();
      return NORMALIZED_METHODS[low] || s;
    }
    function isSearchParams(v) {
      if (!formText || v == null || typeof v !== "object") return false;
      try {
        formText.call(v);
        return true;
      } catch (e) {
        return false;
      }
    }
    function copyHeaders(h) {
      var out = new HeadersCtor();
      h.forEach(function (val, key) {
        out.append(key, val);
      });
      return out;
    }
    function extractRequestBody(raw) {
      if (raw == null) return { bytes: null, type: null };
      var bf = extractBody(raw);
      if (bf) return bf;
      if (raw instanceof ArrayBuffer || ArrayBuffer.isView(raw))
        return { bytes: new Uint8Array(copyBytes(raw, "Request")), type: null };
      if (isSearchParams(raw))
        return { bytes: encoder.encode(formText.call(raw)), type: "application/x-www-form-urlencoded;charset=UTF-8" };
      return { bytes: encoder.encode(domString(raw)), type: "text/plain;charset=UTF-8" };
    }
    function Request(input, init) {
      if (!new.target) throw new TypeError("Request must be constructed with new");
      if (arguments.length < 1) throw new TypeError("Request requires an input");
      var opts = init == null ? {} : init;
      if (typeof opts !== "object" && typeof opts !== "function")
        throw new TypeError("Request init must be an object");
      var src = requests.get(input),
        url,
        method = "GET",
        headers,
        redirect = "follow",
        cache = "default",
        signal = null,
        bodyBytes = null,
        bodyType = null,
        hasNewBody = opts.body !== undefined;
      if (src) {
        url = src.url;
        method = src.method;
        headers = copyHeaders(src.headers);
        redirect = src.redirect;
        cache = src.cache;
        signal = src.signal;
        if (!hasNewBody && src.body !== null) {
          if (src.used) throw new TypeError("Request: cannot construct a Request with a Request object that has already been used");
          bodyBytes = sliceBytes(src.body, 0, src.body.byteLength);
          src.used = true;
        }
      } else {
        url = domString(input);
        headers = new HeadersCtor();
      }
      if (opts.method !== undefined) method = normalizeMethod(opts.method);
      if (opts.headers !== undefined) headers = new HeadersCtor(opts.headers);
      if (opts.redirect !== undefined) {
        redirect = domString(opts.redirect);
        if (redirect !== "follow" && redirect !== "manual" && redirect !== "error")
          throw new TypeError("Request: redirect must be follow, manual or error");
      }
      if (opts.cache !== undefined) {
        cache = domString(opts.cache);
        if (["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"].indexOf(cache) < 0)
          throw new TypeError("Request: invalid cache mode " + cache);
      }
      if (opts.signal !== undefined) signal = opts.signal;
      if (signal === null) signal = new AbortControllerCtor().signal;
      if (hasNewBody && opts.body !== null) {
        var extracted = extractRequestBody(opts.body);
        bodyBytes = extracted.bytes;
        bodyType = extracted.type;
      } else if (hasNewBody && opts.body === null) {
        bodyBytes = null;
      }
      if (bodyBytes !== null && (method === "GET" || method === "HEAD"))
        throw new TypeError("Request with GET/HEAD method cannot have body");
      if (bodyType && !headers.has("content-type")) headers.set("content-type", bodyType);
      requests.set(this, {
        url: url,
        method: method,
        headers: headers,
        redirect: redirect,
        cache: cache,
        signal: signal,
        body: bodyBytes,
        used: false,
      });
    }
    ["url", "method", "headers", "redirect", "cache", "signal"].forEach(function (prop) {
      Object.defineProperty(Request.prototype, prop, {
        get: function () {
          return requireSlot(requests, this, "Request")[prop];
        },
        enumerable: true,
        configurable: true,
      });
    });
    Object.defineProperties(Request.prototype, {
      bodyUsed: {
        get: function () {
          var s = requireSlot(requests, this, "Request");
          return s.body !== null && s.used;
        },
        enumerable: true,
        configurable: true,
      },
      mode: { get: function () { requireSlot(requests, this, "Request"); return "cors"; }, enumerable: true, configurable: true },
      credentials: { get: function () { requireSlot(requests, this, "Request"); return "same-origin"; }, enumerable: true, configurable: true },
      destination: { get: function () { requireSlot(requests, this, "Request"); return ""; }, enumerable: true, configurable: true },
      referrer: { get: function () { requireSlot(requests, this, "Request"); return "about:client"; }, enumerable: true, configurable: true },
      integrity: { get: function () { requireSlot(requests, this, "Request"); return ""; }, enumerable: true, configurable: true },
      keepalive: { get: function () { requireSlot(requests, this, "Request"); return false; }, enumerable: true, configurable: true },
    });
    function consumeReq(req, read) {
      try {
        var s = requireSlot(requests, req, "Request");
        if (s.body === null) return Promise.resolve(read(new Uint8Array(0)));
        if (s.used) throw new TypeError("Request body was already read");
        s.used = true;
        var b = s.body;
        return Promise.resolve().then(function () {
          return read(b);
        });
      } catch (e) {
        return Promise.reject(e);
      }
    }
    Request.prototype.arrayBuffer = function arrayBuffer() {
      return consumeReq(this, function (b) {
        return sliceBytes(b, 0, b.byteLength).buffer;
      });
    };
    Request.prototype.bytes = function bytes() {
      return consumeReq(this, function (b) {
        return sliceBytes(b, 0, b.byteLength);
      });
    };
    Request.prototype.text = function text() {
      return consumeReq(this, function (b) {
        return decoder.decode(b);
      });
    };
    Request.prototype.json = function json() {
      return consumeReq(this, function (b) {
        return JSON.parse(decoder.decode(b));
      });
    };
    Request.prototype.blob = function blob() {
      try {
        var s = requireSlot(requests, this, "Request");
        var type = (s.headers.get("content-type") || "").trim();
        return consumeReq(this, function (b) {
          return new global.Blob([b], { type: type });
        });
      } catch (e) {
        return Promise.reject(e);
      }
    };
    Request.prototype.clone = function clone() {
      var s = requireSlot(requests, this, "Request");
      if (s.body !== null && s.used) throw new TypeError("Request.clone(): body has already been used");
      var out = Object.create(Request.prototype);
      requests.set(out, {
        url: s.url,
        method: s.method,
        headers: copyHeaders(s.headers),
        redirect: s.redirect,
        cache: s.cache,
        signal: s.signal,
        body: s.body === null ? null : sliceBytes(s.body, 0, s.body.byteLength),
        used: false,
      });
      return out;
    };
    Object.defineProperty(Request.prototype, Symbol.toStringTag, { value: "Request", configurable: true });

    function cloneBlobValue(value) {
      var bs = blobs.get(value);
      if (!bs) return undefined;
      var fs = files.get(value);
      if (fs) {
        var f = Object.create(File.prototype);
        blobs.set(f, { bytes: sliceBytes(bs.bytes, 0, bs.size), size: bs.size, type: bs.type });
        files.set(f, { name: fs.name, lastModified: fs.lastModified });
        return f;
      }
      var b = Object.create(Blob.prototype);
      blobs.set(b, { bytes: sliceBytes(bs.bytes, 0, bs.size), size: bs.size, type: bs.type });
      return b;
    }

    var installedBlob = false;
    if (typeof global.Blob !== "function") {
      Object.defineProperty(global, "Blob", { value: Blob, writable: true, configurable: true });
      installedBlob = true;
    }
    if (typeof global.File !== "function")
      Object.defineProperty(global, "File", { value: File, writable: true, configurable: true });
    if (typeof global.FormData !== "function")
      Object.defineProperty(global, "FormData", { value: FormData, writable: true, configurable: true });
    if (typeof global.Request !== "function")
      Object.defineProperty(global, "Request", { value: Request, writable: true, configurable: true });

    // Wrap Hermes/Ibex's structuredClone so Blob and File clone their internal bytes
    // and FormData/Request reject with DataCloneError (LLP 1195.000 D5 P0a).
    if (installedBlob && typeof global.structuredClone === "function") {
      var origStructuredClone = global.structuredClone;
      var dataCloneErr = function (what) {
        return new global.DOMException(what + " could not be cloned.", "DataCloneError");
      };
      var hasBlobGraph = function (v, seen) {
        if (v === null || typeof v !== "object") return false;
        if (blobs.has(v) || forms.has(v) || requests.has(v)) return true;
        if (seen.has(v)) return false;
        seen.add(v);
        if (Array.isArray(v)) {
          for (var i = 0; i < v.length; i++) if (hasBlobGraph(v[i], seen)) return true;
        } else if (v instanceof Map) {
          var found = false;
          v.forEach(function (val, k) { if (hasBlobGraph(k, seen) || hasBlobGraph(val, seen)) found = true; });
          if (found) return true;
        } else if (v instanceof Set) {
          var sfound = false;
          v.forEach(function (val) { if (hasBlobGraph(val, seen)) sfound = true; });
          if (sfound) return true;
        } else if (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null) {
          var ks = Object.keys(v);
          for (var j = 0; j < ks.length; j++) if (hasBlobGraph(v[ks[j]], seen)) return true;
        }
        return false;
      };
      var cloneWithBlobs = function (v, memo, options) {
        if (v === null || typeof v !== "object") return v;
        if (memo.has(v)) return memo.get(v);
        if (forms.has(v)) throw dataCloneErr("#<FormData>");
        if (requests.has(v)) throw dataCloneErr("#<Request>");
        var cb = cloneBlobValue(v);
        if (cb !== undefined) {
          memo.set(v, cb);
          return cb;
        }
        if (Array.isArray(v)) {
          var arr = new Array(v.length);
          memo.set(v, arr);
          for (var i = 0; i < v.length; i++) if (i in v) arr[i] = cloneWithBlobs(v[i], memo, options);
          return arr;
        }
        if (v instanceof Map) {
          var m = new Map();
          memo.set(v, m);
          v.forEach(function (val, k) { m.set(cloneWithBlobs(k, memo, options), cloneWithBlobs(val, memo, options)); });
          return m;
        }
        if (v instanceof Set) {
          var s = new Set();
          memo.set(v, s);
          v.forEach(function (val) { s.add(cloneWithBlobs(val, memo, options)); });
          return s;
        }
        if (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null) {
          var obj = {};
          memo.set(v, obj);
          var ks = Object.keys(v);
          for (var j = 0; j < ks.length; j++) {
            Object.defineProperty(obj, ks[j], { value: cloneWithBlobs(v[ks[j]], memo, options), writable: true, enumerable: true, configurable: true });
          }
          return obj;
        }
        return origStructuredClone(v, options);
      };
      Object.defineProperty(global, "structuredClone", {
        value: function structuredClone(value, options) {
          if (!arguments.length) return origStructuredClone();
          if (!hasBlobGraph(value, new WeakSet())) return origStructuredClone(value, options);
          if (options !== undefined) origStructuredClone(null, options);
          return cloneWithBlobs(value, new Map(), options);
        },
        writable: true,
        configurable: true,
      });
    }

    function unwrapRequest(input, init) {
      var s = requests.get(input);
      if (!s) return null;
      var given = init || {},
        options = Object.create(given);
      var hasBody = given.body !== undefined;
      if (!hasBody && s.body !== null) {
        if (s.used) throw new TypeError("fetch(): the Request's body was already read");
        s.used = true;
      }
      Object.defineProperties(options, {
        method: { value: given.method === undefined ? s.method : given.method, enumerable: true },
        headers: { value: given.headers === undefined ? copyHeaders(s.headers) : given.headers, enumerable: true },
        body: { value: hasBody ? given.body : s.body === null ? undefined : sliceBytes(s.body, 0, s.body.byteLength), enumerable: true },
        signal: { value: given.signal === undefined ? s.signal : given.signal, enumerable: true },
        cache: { value: given.cache === undefined ? s.cache : given.cache, enumerable: true },
        redirect: { value: given.redirect === undefined ? s.redirect : given.redirect, enumerable: true },
      });
      return { url: s.url, init: options };
    }
    return Object.freeze({
      extractBody: extractBody,
      unwrapRequest: unwrapRequest,
      cloneBlob: cloneBlobValue,
    });
  };
})(globalThis);
