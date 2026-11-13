// A data module's storage (LLP 1097), for prelude.js: `storage.fs` and
// `storage.sqlite` over the capability the host installs, each operation
// queued in the prelude's one storage queue, refused by name where storage
// is not, and its failures coded alike on every host. prelude.js takes
// `__exact_storage_calls` and deletes it. `env` is the prelude's door,
// journal and turn (the answer running, the background, whether answers
// move their storage there) and its storage queue (`full`, `enqueue`).
(function (global) {
  "use strict";
  global.__exact_storage_calls = function (env) {
    var host = env.host,
      journal = env.journal,
      errorText = env.errorText,
      background = env.background,
      owner = env.owner;
    var storing = env.storing,
      unstore = env.unstore,
      storageCounts = env.storageCounts,
      inOverlay = env.inOverlay;
    // Advisory locks (`flock`) held now, each with the answer that took it.
    var heldLocks = new Set();
    // Storage is a capability argument, never an ambient global. Native hosts
    // install Ibex2's objects during trusted initialization; bake and the browser
    // retain this explicit refusal surface until a provider is installed.
    var nativeStorage = null;
    global.__exact_install_storage = function () {
      nativeStorage = global.__exact_storage;
      delete global.__exact_storage;
      delete global.__exact_install_storage;
    };
    // A storage refusal is `{kind: "Unavailable", code, message}`, the code
    // the same on every host (kanban F28; host/web-js/ts-data.js `coded`):
    // storage itself is unavailable ("bake", "agent", "unsupported"), the
    // grants do not cover the operation ("denied"), the filesystem's POSIX
    // name ("ENOENT", "EEXIST", "ENOTDIR", "EISDIR", "ENOTEMPTY", "EBUSY"),
    // else "failed". The message says more and differs by host.
    var REFUSED = {
      bake: "storage is unavailable during bake",
      agent: "storage is unavailable in agent mode unless the drive names a scratch store (--storage <name>)",
      unsupported: "storage is unsupported by this host",
    };
    var toldAgent = false,
      READS = { readFile: true, readdir: true, stat: true, realpath: true };
    // Rust's error numbers are platform-specific. Never treat Windows access
    // denial (5) as EISDIR, or its 17/39 as Unix EEXIST/ENOTEMPTY.
    var windowsStorage = global.__exact_windows_storage === true;
    delete global.__exact_windows_storage;
    var ERRNO = windowsStorage
      ? {
          2: "ENOENT",
          3: "ENOENT",
          32: "EBUSY",
          33: "EBUSY",
          80: "EEXIST",
          145: "ENOTEMPTY",
          170: "EBUSY",
          183: "EEXIST",
          267: "ENOTDIR",
        }
      : { 2: "ENOENT", 16: "EBUSY", 17: "EEXIST", 20: "ENOTDIR", 21: "EISDIR", 39: "ENOTEMPTY", 66: "ENOTEMPTY" };
    // The image calls' own failures (LLP 1069.002 A1.4, LLP 1133.019 D17),
    // named in the message natively; the web sets the code itself.
    var IMAGE_CODE =
      /^(?:filesystem: )?(?:compress|transform)Image: (unsupported|undecodable|too-large|unfit|timeout|outside): /;
    function storageCode(message) {
      if (/^denied: /.test(message)) return "denied";
      var image = IMAGE_CODE.exec(message);
      if (image) return image[1];
      // Ibex's retained-document boundary and SQLite provider type their codes in a prefix; what
      // follows carries the app's paths and SQL, so infer nothing from it (ibex2-sqlite `BUSY`).
      var typed = /^document (ENOENT|EEXIST|ENOTDIR|EISDIR|ENOTEMPTY|EBUSY): doc:\/|^SQLite(?: (EBUSY))?: /.exec(
        message
      );
      if (typed) return typed[1] || typed[2] || "failed";
      // The Windows document adapter emits this only after same-handle type
      // inspection; incidental text and numeric access denial are insufficient.
      if (windowsStorage && /\(filesystem code EISDIR\)$/.test(message)) return "EISDIR";
      var errno = /\(os error (\d+)\)$/.exec(message);
      if (errno && ERRNO[errno[1]]) return ERRNO[errno[1]];
      return /\bbusy\b|database is locked/.test(message) ? "EBUSY" : "failed";
    }
    // As host/web/storage-fs.js `deniedPath` words it on the web (app farm round 2: a bare
    // `denied: sqlite.open` left builds guessing which per-persona file it wanted).
    function deniedPath(operation, path) {
      var dir = path.slice(0, path.lastIndexOf("/"));
      var directory = /^[a-z]+:\/[^/]/.test(dir) ? ", or `" + operation + " " + dir + "` for every file there" : "";
      return (
        "denied: " +
        operation +
        " " +
        path +
        ": no grant covers it; grant `" +
        operation +
        " " +
        path +
        "`" +
        directory +
        " (a grant covers its path and what is below it, by whole names)"
      );
    }
    function storageError(message, code) {
      var error = new Error(message);
      error.kind = "Unavailable";
      error.code = code || storageCode(String(message));
      if (/^unsupported: no (home|bundle): directory here$/.test(String(message)))
        // no such directory on iOS, tvOS, Android (LLP 1133.007 D6); its code stays "failed", as "unsupported" is a
        // host with no app storage
        Object.defineProperty(error, "name", { value: "NotSupportedError", writable: true, configurable: true });
      return error;
    }
    function storageCall(receiver, method, args, convert) {
      if (env.overlaying()) return Promise.reject(inOverlay("storage." + method + "()"));
      var call = env.current();
      // Storage issued after its answer replied (a `.then` chained on a save,
      // a write begun when a background one lands) is background work (D2).
      // Before any answer, during module evaluation, it is refused, as it is
      // on a worker placement, where answers wait for their storage.
      if (env.moving() && (call ? call.replied : !env.initializing())) call = background;
      if (!call || call.replied || call.mapper) {
        var refused = storageError(
          "storage." +
            method +
            "() called " +
            (call && call.mapper ? "in a stream's mapper" : "outside an answer") +
            ": it was never run. Start storage inside an answer"
        );
        journal("storage refused: " + refused.message);
        return Promise.reject(refused);
      }
      var refused;
      // A file operation names its path: a document the person chose
      // (`doc:/`) needs no app storage (LLP 1069.010 D1); a copy into an app file does.
      var path =
        receiver && nativeStorage && receiver === nativeStorage.fs && typeof args[0] === "string" ? args[0] : "";
      try {
        refused =
          host(5, method === "copyFile" && path && /^app:\//.test(args[1]) ? args[1] : path, "") ||
          (receiver ? undefined : "unsupported");
      } catch (e) {
        refused = "bake";
      } // no filesystem or database effects during bake
      // A drive with no scratch store is said once, at its first refused read or write, for the driver's note (LLP 1102
      // §3.17); a read finds an empty store, as on a fresh install (storage-environment.js `emptyRead`, word for word).
      if (refused === "agent" && !toldAgent) {
        toldAgent = true;
        journal("storage refused (agent): " + REFUSED.agent);
      }
      if (refused === "agent" && READS[method] && receiver === (nativeStorage && nativeStorage.fs))
        return method === "readdir" && /^app:\/(data|cache|tmp)$/.test(args[0])
          ? Promise.resolve([])
          : Promise.reject(
              storageError(
                "filesystem: no such file or directory: " +
                  args[0] +
                  " (a drive that names no scratch store reads an empty one; --storage <name> names one)",
                "ENOENT"
              )
            );
      if (refused) return Promise.reject(storageError(REFUSED[refused], refused));
      var what = method + (path ? " " + path : "");
      // The bound (D3): refusal is the overload policy (LLP 1041 D2).
      if (env.full()) {
        journal("storage refused: full (" + what + ")");
        return Promise.reject(storageError("storage queue full: " + env.MAX_QUEUED + " operations wait", "full"));
      }
      call.storage++;
      storing.add(call);
      // The web realm attributes the cell now, while this answer is current.
      // The adapter runs at issue, and a background round may be current then
      // (LLP 1097 D7). Hermes has no hook and runs the adapter at the call.
      var held = null;
      if (typeof global.__exact_reserve_storage === "function") held = global.__exact_reserve_storage();
      var argv = Array.prototype.slice.call(args);
      return new Promise(function (resolve, reject) {
        var op = {
          call: call,
          method: method,
          run: function () {
            try {
              return receiver[method].apply(receiver, argv);
            } catch (e) {
              if (held != null && typeof global.__exact_abandon_storage === "function")
                global.__exact_abandon_storage(held);
              throw e;
            }
          },
        };
        op.settle = function (ok, value) {
          var at = owner(op.call);
          env.setCurrent(at);
          unstore(at);
          if (ok) {
            storageCounts.done++;
            var destination =
              method === "transformImage" ? (typeof value === "string" ? value.split("\t")[3] : value.path) : argv[1];
            if (
              /^(writeFile|atomicWriteFile|appendFile|mkdir|rm|rename|copyFile|compressImage|transformImage)$/.test(
                method
              ) &&
              [
                /^(copyFile|compressImage|transformImage)$/.test(method) ? destination : argv[0],
                method === "rename" ? argv[1] : null,
              ].some(function (path) {
                if (typeof path !== "string" || !/^app:\//.test(path)) return false;
                path = new URL(path).href.replace(/\/$/, "");
                return String(global.exact.grants)
                  .split("\n")
                  .some(function (line) {
                    var grant = line.trim().split(/\s+/),
                      root = (grant[1] || "").replace(/\/$/, "");
                    return (
                      grant[0] === "fs.read" &&
                      /^app:\//.test(root) &&
                      (path === root || path.indexOf(root + "/") === 0 || root.indexOf(path + "/") === 0)
                    );
                  });
              })
            )
              storageCounts.writes++;
            try {
              resolve(convert ? convert(value) : value);
            } catch (e) {
              reject(e);
            }
            return;
          }
          var error = value;
          // The host's grant refusal names no path: name it, and the lines that would admit it.
          if (
            method === "open" &&
            typeof argv[0] === "string" &&
            /^denied: sqlite\.open$/.test((error && error.message) || String(error))
          )
            error = storageError(deniedPath("sqlite.open", argv[0]), "denied");
          // A binding's argument error may carry the app's words: read no code from them.
          else if (!error || !error.kind)
            error = storageError(
              (error && error.message) || String(error),
              error &&
                (error.code || (error instanceof TypeError || error instanceof RangeError ? "failed" : undefined))
            );
          else if (!error.code)
            try {
              error.code = storageCode(String(error.message));
            } catch (_) {
              /* a frozen error keeps its own */
            }
          // Every failed operation is journaled, an answer's or the
          // background's, so one nobody catches is still seen (D8).
          var line = "storage failed: " + what + ": " + error.code + " " + errorText(error);
          journal(line);
          storageCounts.failed++;
          storageCounts.last = line;
          reject(error);
        };
        env.enqueue(op);
      });
    }
    function statement(raw) {
      return Object.freeze({
        execute: function (params) {
          return storageCall(raw, "execute", [params]);
        },
        query: function (params) {
          return storageCall(raw, "query", [params]);
        },
        close: function () {
          return storageCall(raw, "close", []);
        },
      });
    }
    // The databases open now, each with the path and the work that opened it: an answer that
    // fails, its own or one let go (LLP 1097 D7), with one still open is journaled, since the host
    // does not close a handle an app may keep or share (Charlie, 2026-10-07), and the next open
    // would find it locked. A close counts from the moment it is issued.
    var openDatabases = new Set();
    // On a failure (an answer's own rejection, or a rejection nothing handled), each database
    // opened by an answer that has replied, failed or been let go, or by background work: work that
    // runs on in the background, which may have been the work that failed.
    function leftOpen() {
      openDatabases.forEach(function (h) {
        var by = h.by;
        if (h.told || !by || !(by === background || by.replied || by.status !== "pending" || by.letGo || by.lost))
          return;
        h.told = true;
        journal(
          "storage: " +
            h.path +
            " is still open after a failure in background work that opened it: if that work owns it, close it in a finally (finally { db.close() }), or the next open finds it locked (LLP 1097 D7)"
        );
      });
    }
    // A transaction's batch and options as they are at the call: each entry's sql and params read once,
    // `foreignKeys` read once, other keys kept by name (in a record with no prototype, so `__proto__` is a key)
    // for the binding to refuse.
    function batchNow(c) {
      return Array.isArray(c)
        ? c.map(function (e) {
            if (!e || typeof e !== "object") return e;
            var sql = e.sql,
              params = e.params;
            return {
              sql: sql,
              params: Array.isArray(params)
                ? params.map(function (v) {
                    return v instanceof Uint8Array ? new Uint8Array(v) : v;
                  })
                : params,
            };
          })
        : c;
    }
    function optionsNow(o) {
      if (o === null || typeof o !== "object") return o;
      var r = Object.create(null),
        f = o.foreignKeys;
      Object.keys(o).forEach(function (k) {
        if (k !== "foreignKeys") Object.defineProperty(r, k, { value: true, enumerable: true });
      });
      if (f !== undefined) r.foreignKeys = f;
      return r;
    }
    function database(raw, path, by) {
      var handle = { path: path, by: by, told: false };
      openDatabases.add(handle);
      return Object.freeze({
        execute: function (sql, params) {
          return storageCall(raw, "execute", [sql, params]);
        },
        query: function (sql, params) {
          return storageCall(raw, "query", [sql, params]);
        },
        prepare: function (sql) {
          return storageCall(raw, "prepare", [sql], statement);
        },
        // Read now, not when a queued call is issued: what the caller changes after the call never changes what runs.
        transaction: function (commands, options) {
          var args;
          try {
            args = [batchNow(commands), optionsNow(options)];
          } catch (e) {
            return Promise.reject(storageError(String((e && e.message) || e), "failed"));
          }
          return storageCall(raw, "transaction", args);
        },
        // A close counts once issued; one refused or failed leaves the database open, and tracked.
        close: function () {
          openDatabases.delete(handle);
          // A failed close still rejects for the caller, so one left unhandled is still reported.
          return storageCall(raw, "close", []).then(undefined, function (e) {
            openDatabases.add(handle);
            throw e;
          });
        },
      });
    }
    var files = { directories: Object.freeze({ data: "app:/data", cache: "app:/cache", temporary: "app:/tmp" }) };
    [
      "readFile",
      "writeFile",
      "atomicWriteFile",
      "appendFile",
      "readdir",
      "mkdir",
      "rm",
      "stat",
      "rename",
      "copyFile",
      "realpath",
      "lstat",
      "readlink",
      "access",
      "chmod",
      "symlink",
      "digest",
    ].forEach(function (method) {
      files[method] = function () {
        return storageCall(nativeStorage && nativeStorage.fs, method, arguments);
      };
    });
    // LLP 1069.010.000 D4: a kept file the app replaces with its own atomic write stays kept: the host reads what the
    // write replaces, then carries the entry to the file written.
    var atomic = files.atomicWriteFile;
    files.atomicWriteFile = function (path) {
      var t;
      try {
        t = typeof path === "string" && path.indexOf("doc:/") === 0 ? host(6, "kept", "replacing " + path) : undefined;
      } catch (e) {
        t = undefined;
      }
      var done = function (how) {
        if (t)
          try {
            host(6, "kept", how + t);
          } catch (e) {}
      };
      return atomic.apply(null, arguments).then(
        function (v) {
          done("replaced ");
          return v;
        },
        function (e) {
          done("dropped ");
          throw e;
        }
      );
    };
    // LLP 1069.010.000 D2a: `{expect: stat}` refuses a doc:/ write `ECHANGED` if the file changed since; the stat runs
    // just before it.
    ["writeFile", "atomicWriteFile", "appendFile"].forEach(function (method) {
      var write = files[method],
        api = "storage.fs." + method + "()";
      files[method] = function (path, data, options) {
        var expect = options && options.expect;
        if (!expect) return write.apply(null, arguments);
        if (
          typeof path !== "string" ||
          path.indexOf("doc:/") !== 0 ||
          typeof expect.modifiedMs !== "number" ||
          typeof expect.size !== "number"
        )
          return Promise.reject(new TypeError(api + ": expect is a stat (modifiedMs and size) of a doc:/ path"));
        return files.stat(path).then(function (now) {
          if (now.modifiedMs !== expect.modifiedMs || now.size !== expect.size)
            throw storageError("document ECHANGED: " + path + " changed on disk since it was read", "ECHANGED");
          return write(path, data);
        });
      };
    });
    // LLP 1069.010.000 D1: the answer is asked again when the doc:/ path (a folder's own entries, or `recursive` its
    // subtree) changes on disk.
    // An advisory lock (LLP 1133.007 D6a), natively only: released by its `unlock()`, the end of the answer that took
    // it (`freeLocks`), or the session's.
    files.flock = function (path, options) {
      var t = options && options.timeout,
        fs = nativeStorage && nativeStorage.fs,
        by = env.current();
      return storageCall(fs, "flock", [path, t === undefined ? 0 : t], function (h) {
        var lock = { h: h, by: by };
        heldLocks.add(lock);
        if (by && by.replied) freeLocks();
        return Object.freeze({
          unlock: function () {
            return heldLocks.delete(lock) ? storageCall(fs, "unlock", [h]) : Promise.resolve();
          },
        });
      });
    };
    function freeLocks() {
      heldLocks.forEach(function (l) {
        if (!l.by || l.by === background || !l.by.replied) return;
        heldLocks.delete(l);
        try {
          Promise.resolve(nativeStorage.fs.unlock(l.h)).then(undefined, function () {});
        } catch (e) {}
      });
    }
    files.watch = function (path, options) {
      if (env.overlaying()) throw inOverlay("storage.fs.watch()");
      var currentCall = env.current();
      if (!currentCall || currentCall.status !== "pending") throw new Error("storage.fs.watch() outside an answer");
      if (typeof path !== "string" || path.indexOf("doc:/") !== 0)
        throw new TypeError("storage.fs.watch(): path must be a doc:/ path");
      host(6, "watch", (options && options.recursive ? "doc-watch-recursive:" : "doc-watch:") + path);
    };
    // LLP 1069.010.000 D5 (doc.keep): the host's kept list, which watch("doc:/") hears change, and the app's own
    // removal.
    files.recentDocuments = function () {
      return web("recentDocuments", []) || kept("");
    };
    files.forgetDocument = function (path) {
      return web("forgetDocument", [path]) || kept("forget " + path);
    };
    function web(method, args) {
      var fs = nativeStorage && nativeStorage.fs;
      return fs && fs[method] ? storageCall(fs, method, args) : null;
    }
    function kept(ask) {
      try {
        var r = host(6, "kept", ask);
        return Promise.resolve(r ? JSON.parse(r) : undefined);
      } catch (e) {
        var m = String((e && e.message) || e);
        return Promise.reject(m === "bake" ? storageError(REFUSED.bake, "bake") : storageError(m));
      }
    }
    // The image calls' options (`imageCall`, below), checked before
    // anything is queued, as the host's arguments after `from` and `to`:
    // `compressImage`'s two numbers and type (LLP 1069.002 A1, A1.11), then its
    // edit, and `transformImage`'s all, each as the one JSON value the codec
    // reads (LLP 1133.019 D17).
    var IMAGE_WORDS = {
      flip: ["horizontal", "vertical"],
      resizeQuality: ["pixelated", "low", "medium", "high"],
      metadata: ["keep", "no-location", "none"],
    };
    function imageOptions(api, options, transform) {
      var out = {},
        crop;
      function fail(says) {
        throw new TypeError(api + ": " + says);
      }
      function take(name, ok, says) {
        var value = options[name];
        if (value === undefined) return;
        if (!ok(value)) fail(name + " must be " + says);
        out[name] = value;
      }
      function word(name) {
        take(
          name,
          function (w) {
            return IMAGE_WORDS[name].indexOf(w) >= 0;
          },
          IMAGE_WORDS[name]
            .map(function (w) {
              return JSON.stringify(w);
            })
            .join(" or ")
        );
      }
      function finite(n) {
        return typeof n === "number" && isFinite(n);
      }
      function side(n) {
        return Number.isInteger(n) && n >= 1 && n <= 16384;
      }
      if (
        transform
          ? options !== undefined && (!options || typeof options !== "object")
          : !options || typeof options !== "object"
      )
        fail(transform ? "options must be an object" : "options must be {maxDimension, maxBytes}");
      options = options || {};
      crop = options.crop;
      take("rotate", finite, "a finite number of degrees");
      word("flip");
      if (crop !== undefined) {
        if (
          !crop ||
          typeof crop !== "object" ||
          ![crop.x, crop.y, crop.width, crop.height].every(finite) ||
          !(crop.width > 0 && crop.height > 0)
        )
          fail("crop must be {x, y, width, height}, finite, with a positive width and height");
        out.crop = { x: crop.x, y: crop.y, width: crop.width, height: crop.height };
      }
      if (!transform) {
        var dimension = options.maxDimension,
          bytes = options.maxBytes,
          type = options.type === undefined ? "image/jpeg" : options.type;
        if (!Number.isInteger(dimension) || dimension < 1 || dimension > 8192)
          fail("maxDimension must be an integer from 1 to 8192");
        if (!Number.isInteger(bytes) || bytes < 1 || bytes > 67108864)
          fail("maxBytes must be an integer from 1 to 67108864");
        if (type !== "image/jpeg" && type !== "image/png") fail('type must be "image/jpeg" or "image/png"');
        return [dimension, bytes, type].concat(Object.keys(out).length ? [JSON.stringify(out)] : []);
      }
      take("resizeWidth", side, "an integer from 1 to 16384");
      take("resizeHeight", side, "an integer from 1 to 16384");
      word("resizeQuality");
      take(
        "type",
        function (t) {
          return typeof t === "string" && /^image\/[a-z0-9.+-]+$/.test(t);
        },
        'a media type such as "image/jpeg"'
      );
      take(
        "quality",
        function (q) {
          return typeof q === "number" && q >= 0 && q <= 1;
        },
        "a number from 0 to 1"
      );
      word("metadata");
      return [JSON.stringify(out)];
    }
    // `fs.compressImage(from, to, {maxDimension, maxBytes, type, rotate, flip, crop})` (LLP 1069.002 A1,
    // A1.11; LLP 1133.019 D15) and `fs.transformImage(from, to, options)` (D14): their options are checked
    // before anything is queued and travel as `imageOptions` words them; natively the reply is
    // `width\theight\tsize`, a transform's then its path and the codec's report, and on the web the record.
    function imageCall(method, from, to, options, record) {
      var tail;
      try {
        if (typeof from !== "string" || typeof to !== "string")
          throw new TypeError("storage.fs." + method + "(): from and to must be app:/ paths");
        tail = imageOptions("storage.fs." + method + "()", options, method === "transformImage");
      } catch (e) {
        return Promise.reject(e);
      }
      return storageCall(nativeStorage && nativeStorage.fs, method, [from, to].concat(tail), function (value) {
        return typeof value === "string" ? record(value.split("\t"), tail) : value;
      });
    }
    files.compressImage = function (from, to, options) {
      return imageCall("compressImage", from, to, options, function (parts, tail) {
        return { path: to, type: tail[2], size: +parts[2], width: +parts[0], height: +parts[1] };
      });
    };
    files.transformImage = function (from, to, options) {
      return imageCall("transformImage", from, to, options, function (parts) {
        var report = JSON.parse(parts.slice(4).join("\t"));
        return {
          path: parts[3],
          type: report.type,
          size: +parts[2],
          width: +parts[0],
          height: +parts[1],
          colorSpace: report.colorSpace,
          hdr: report.hdr,
          metadata: report.metadata,
        };
      });
    };
    var storage = Object.freeze({
      fs: Object.freeze(files),
      sqlite: Object.freeze({
        open: function (path) {
          // Its owner as storageCall reckons it: storage after an answer replied is the background's.
          var currentCall = env.current();
          var by = env.moving() && (currentCall ? currentCall.replied : !env.initializing()) ? background : currentCall;
          return storageCall(nativeStorage && nativeStorage.sqlite, "open", [path], function (raw) {
            return database(raw, path, by);
          });
        },
      }),
    });
    return {
      storage: storage,
      storageError: storageError,
      leftOpen: leftOpen,
      freeLocks: freeLocks,
      native: function () {
        return nativeStorage;
      },
    };
  };
})(globalThis);
