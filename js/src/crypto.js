// A data module's `crypto` (LLP 1069.005), for prelude.js: digests are pure;
// entropy is a device read. Each draw inside an answer counts as an external
// read (op 8), as a secret or SQLite read does: bake compiles no value that
// drew one, and the host asks again on the device. A draw outside an answer
// (during module initialization) refuses: it would be one hidden input every
// answer shares (D2, D3). `subtle` has what is built, the same on every
// executor: over ibex2's `crypto` and the bytes door natively, over the
// realm's own on the web. Every other `subtle` member refuses by name (D1).
// prelude.js takes `__exact_crypto` and deletes it. `env` is the prelude's
// door, the bytes door, the answer running and the overlay's refusal; the
// store's kept keys (`keepKey`, `keptKey`) go back to it.
(function (global) {
  "use strict";
  global.__exact_crypto = function (env) {
    var host = env.host,
      copyBytes = env.copyBytes,
      bytesDoor = env.bytesDoor,
      answerOnly = env.answerOnly;
    var hostWork = env.hostWork,
      inOverlay = env.inOverlay;
    function fixed(object, name, value) {
      Object.defineProperty(object, name, { value: value, writable: false, configurable: false });
    }
    // Not a second door to randomness: `crypto` below is the secure one.
    fixed(global.Math, "random", function () {
      throw new Error(
        "Math.random() is unavailable in data sources; seed from crypto.getRandomValues in a source (docs/recipes/random-and-shuffle.md), or pass a seed as an argument; a roll in the view needs no source (docs/recipes/random-rolls.md)"
      );
    });
    var platformCrypto = global.crypto,
      fillRandom = platformCrypto.getRandomValues.bind(platformCrypto);
    var platformSubtle = platformCrypto.subtle; // the web's, in a secure context
    // A LAN dev page (no secure context): the dev protocol's SHA-256, which
    // module integrity already uses there. `(Uint8Array) -> Promise<hex>`.
    var pageDigest = global.__exact_digest;
    delete global.__exact_digest;
    function formatUuid(b) {
      var text = "";
      b[6] = (b[6] & 0x0f) | 0x40;
      b[8] = (b[8] & 0x3f) | 0x80;
      for (var i = 0; i < 16; i++)
        text += (i === 4 || i === 6 || i === 8 || i === 10 ? "-" : "") + (b[i] < 16 ? "0" : "") + b[i].toString(16);
      return text;
    }
    // A LAN dev page is not a secure context, so it has no `randomUUID`: the
    // same v4 bits as ibex2's `format_uuid`, from `getRandomValues`.
    var platformUuid =
      typeof platformCrypto.randomUUID === "function"
        ? platformCrypto.randomUUID.bind(platformCrypto)
        : function () {
            return formatUuid(fillRandom(new Uint8Array(16)));
          };
    function Crypto() {
      throw new TypeError("Illegal constructor");
    }
    var cryptoObject = Object.create(Crypto.prototype);
    function draw(receiver, api, take) {
      if (receiver !== cryptoObject) throw new TypeError("Illegal invocation");
      if (env.overlaying()) throw inOverlay(api);
      answerOnly(api);
      var value = take();
      // An empty fill drew nothing
      if (typeof value === "string" || value.byteLength > 0) host(8, "", "");
      return value;
    }
    // Under the agent, the host's repeatable stream (D2b): `n` bytes, or
    // null outside the agent, where the draw is the platform's.
    function agentBytes(n) {
      var hex = host(11, "", String(n));
      return hex === undefined ? null : new Uint8Array(hexBuffer(hex));
    }
    Crypto.prototype.getRandomValues = function getRandomValues(view) {
      return draw(this, "crypto.getRandomValues()", function () {
        // The platform's call checks the view and its quota, as it always does.
        var filled = fillRandom(view),
          stream = agentBytes(filled.byteLength);
        if (stream) new Uint8Array(filled.buffer, filled.byteOffset, filled.byteLength).set(stream);
        return filled;
      });
    };
    Crypto.prototype.randomUUID = function randomUUID() {
      return draw(this, "crypto.randomUUID()", function () {
        var stream = agentBytes(16);
        return stream ? formatUuid(stream) : platformUuid();
      });
    };
    function hexBuffer(hex) {
      var bytes = new Uint8Array(hex.length / 2);
      for (var i = 0; i < bytes.length; i++) bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
      return bytes.buffer;
    }
    function algorithmName(algorithm) {
      var name = typeof algorithm === "string" ? algorithm : algorithm && algorithm.name;
      if (typeof name !== "string") throw new TypeError("an algorithm is a name or an object with a name");
      return name;
    }
    var DIGESTS = { "SHA-256": 1, "SHA-384": 1, "SHA-512": 1 };
    function SubtleCrypto() {
      throw new TypeError("Illegal constructor");
    }
    var subtleObject = Object.create(SubtleCrypto.prototype);
    function subtleCall(receiver, member, run) {
      try {
        if (receiver !== subtleObject) throw new TypeError("Illegal invocation");
        if (env.overlaying()) throw inOverlay("crypto.subtle." + member + "()");
        return Promise.resolve(run());
      } catch (e) {
        return Promise.reject(e);
      }
    }
    // Every member the web has; each refuses by name until it is built here.
    [
      "encrypt",
      "decrypt",
      "sign",
      "verify",
      "digest",
      "generateKey",
      "deriveKey",
      "deriveBits",
      "importKey",
      "exportKey",
      "wrapKey",
      "unwrapKey",
    ].forEach(function (member) {
      SubtleCrypto.prototype[member] = function () {
        return subtleCall(this, member, function () {
          throw new DOMException("crypto.subtle." + member + "() is unavailable in data sources", "NotSupportedError");
        });
      };
    });
    SubtleCrypto.prototype.digest = function digest(algorithm, data) {
      return subtleCall(this, "digest", function () {
        var given = algorithmName(algorithm),
          name = given.toUpperCase();
        if (!DIGESTS[name])
          throw new DOMException(
            "crypto.subtle.digest(): " + given + " is unavailable in data sources; use SHA-256, SHA-384 or SHA-512",
            "NotSupportedError"
          );
        var bytes = copyBytes(data, "crypto.subtle.digest()");
        if (bytesDoor) return hexBuffer(bytesDoor(1, name, bytes));
        if (platformSubtle) return hostWork(platformSubtle.digest(name, bytes));
        if (name === "SHA-256" && pageDigest) return hostWork(pageDigest(new Uint8Array(bytes))).then(hexBuffer);
        throw new DOMException(
          "crypto.subtle.digest(): " + name + " needs a secure context (HTTPS or localhost)",
          "NotSupportedError"
        );
      });
    };
    // ECDSA P-256 for DPoP (D1b): generate, sign (ES256, raw r‖s), and JWK
    // import and export, with the web's names, argument shapes and errors.
    // Generating and signing draw a nonce: each is a counted read, and refuses
    // outside an answer. Natively the key's bytes stay in Rust behind a handle
    // (`exact_data::crypto`, bytes door ops 2–7); on the web a key is the
    // browser's own `CryptoKey` and the work its `subtle`, joined to the answer.
    var PlatformCryptoKey = global.CryptoKey;
    var keysDoor = global.__exact_keys; // the web realm's IndexedDB, for kept keys
    delete global.__exact_keys;
    var keeping = new Map(); // a name whose pair keepKey is writing -> the newest call's handle
    var ownNativeKey = global.__exact_key;
    delete global.__exact_key;
    var nativeKeys = new WeakMap(); // natively: CryptoKey -> its handle
    function CryptoKey() {
      throw new TypeError("Illegal constructor");
    }
    function fromDoor(e) {
      var text = String(e && e.message !== undefined ? e.message : e),
        match = /^([A-Za-z]+Error): /.exec(text);
      return new DOMException(match ? text.slice(match[0].length) : text, match ? match[1] : "DataError");
    }
    function door(op, a, bytes) {
      try {
        return bytesDoor(op, a, bytes || new ArrayBuffer(0));
      } catch (e) {
        throw fromDoor(e);
      }
    }
    function ecdsa(algorithm, api, needCurve) {
      var name = algorithmName(algorithm);
      if (name.toUpperCase() !== "ECDSA")
        throw new DOMException(
          api + ": " + name + " is unavailable in data sources; use ECDSA with P-256",
          "NotSupportedError"
        );
      if (needCurve && algorithm.namedCurve !== "P-256")
        throw new DOMException(
          api + ": the curve " + algorithm.namedCurve + " is unavailable in data sources; use P-256",
          "NotSupportedError"
        );
    }
    function usageList(usages, allowed, api) {
      if (!Array.isArray(usages)) throw new TypeError(api + ": usages is an array");
      var out = [];
      for (var i = 0; i < usages.length; i++) {
        if (allowed.indexOf(usages[i]) < 0)
          throw new DOMException(api + ": the usage " + usages[i] + " is not allowed for this key", "SyntaxError");
        if (out.indexOf(usages[i]) < 0) out.push(usages[i]);
      }
      return out;
    }
    function only(usages, which) {
      return usages.filter(function (u) {
        return u === which;
      });
    }
    function entropyRead(api) {
      answerOnly(api);
      // Natively Rust marks the read where it draws (`exact_data::crypto`).
      if (!bytesDoor) host(8, "", "");
    }
    function makeKey(handle, type, extractable, usages) {
      var key = Object.create(CryptoKey.prototype);
      Object.defineProperties(key, {
        type: { value: type, enumerable: true },
        extractable: { value: !!extractable, enumerable: true },
        algorithm: { value: Object.freeze({ name: "ECDSA", namedCurve: "P-256" }), enumerable: true },
        usages: { value: Object.freeze(usages.slice()), enumerable: true },
      });
      nativeKeys.set(key, handle);
      ownNativeKey(handle, key);
      return Object.freeze(key);
    }
    function nativePair(json, extractable, usages) {
      var r = JSON.parse(json);
      return {
        privateKey: makeKey(r["private"], "private", extractable, only(usages, "sign")),
        publicKey: makeKey(r["public"], "public", true, only(usages, "verify")),
      };
    }
    function isKey(key) {
      return bytesDoor ? nativeKeys.has(key) : !!PlatformCryptoKey && key instanceof PlatformCryptoKey;
    }
    function checkKey(key, api) {
      if (!isKey(key)) throw new TypeError(api + ": the key is not a CryptoKey");
      if (key.algorithm.name !== "ECDSA" || key.algorithm.namedCurve !== "P-256")
        throw new DOMException(api + ": the key is not ECDSA P-256", "InvalidAccessError");
    }
    function webSubtle(api) {
      if (!platformSubtle)
        throw new DOMException(api + ": ECDSA needs a secure context (HTTPS or localhost)", "NotSupportedError");
      return platformSubtle;
    }
    SubtleCrypto.prototype.generateKey = function generateKey(algorithm, extractable, usages) {
      return subtleCall(this, "generateKey", function () {
        var api = "crypto.subtle.generateKey()";
        ecdsa(algorithm, api, true);
        var list = usageList(usages, ["sign", "verify"], api);
        if (!only(list, "sign").length)
          throw new DOMException(api + ": a private key needs the sign usage", "SyntaxError");
        if (!bytesDoor) webSubtle(api);
        entropyRead(api);
        if (bytesDoor) return nativePair(door(2, extractable ? "1" : "0"), extractable, list);
        return hostWork(platformSubtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, !!extractable, list));
      });
    };
    SubtleCrypto.prototype.sign = function sign(algorithm, key, data) {
      return subtleCall(this, "sign", function () {
        var api = "crypto.subtle.sign()";
        ecdsa(algorithm, api, false);
        var hash = algorithm && algorithm.hash;
        if (algorithmName(hash === undefined ? "" : hash).toUpperCase() !== "SHA-256")
          throw new DOMException(api + ": ECDSA signs with SHA-256 (ES256) in data sources", "NotSupportedError");
        checkKey(key, api);
        if (key.type !== "private" || key.usages.indexOf("sign") < 0)
          throw new DOMException(api + ": the key is not for signing", "InvalidAccessError");
        var bytes = copyBytes(data, api);
        if (!bytesDoor) webSubtle(api);
        entropyRead(api);
        if (bytesDoor) return hexBuffer(door(3, String(nativeKeys.get(key)), bytes));
        return hostWork(platformSubtle.sign({ name: "ECDSA", hash: "SHA-256" }, key, bytes));
      });
    };
    SubtleCrypto.prototype.importKey = function importKey(format, keyData, algorithm, extractable, usages) {
      return subtleCall(this, "importKey", function () {
        var api = "crypto.subtle.importKey()";
        if (format !== "jwk")
          throw new DOMException(
            api + ": the format " + format + " is unavailable in data sources; use jwk",
            "NotSupportedError"
          );
        ecdsa(algorithm, api, true);
        if (!keyData || typeof keyData !== "object") throw new TypeError(api + ": a JWK is an object");
        if (keyData.kty !== "EC" || keyData.crv !== "P-256")
          throw new DOMException(api + ": the JWK is not EC P-256", "DataError");
        var isPrivate = typeof keyData.d === "string";
        var list = usageList(usages, isPrivate ? ["sign"] : ["verify"], api);
        if (isPrivate && !list.length)
          throw new DOMException(api + ": a private key needs the sign usage", "SyntaxError");
        if (!bytesDoor)
          return hostWork(
            webSubtle(api).importKey("jwk", keyData, { name: "ECDSA", namedCurve: "P-256" }, !!extractable, list)
          );
        var jwk = { kty: "EC", crv: "P-256", x: String(keyData.x), y: String(keyData.y) };
        if (isPrivate) jwk.d = keyData.d;
        var handle = Number(door(4, JSON.stringify({ jwk: jwk, extractable: !!extractable })));
        return makeKey(handle, isPrivate ? "private" : "public", isPrivate ? extractable : true, list);
      });
    };
    SubtleCrypto.prototype.exportKey = function exportKey(format, key) {
      return subtleCall(this, "exportKey", function () {
        var api = "crypto.subtle.exportKey()";
        if (format !== "jwk")
          throw new DOMException(
            api + ": the format " + format + " is unavailable in data sources; use jwk",
            "NotSupportedError"
          );
        checkKey(key, api);
        if (!key.extractable) throw new DOMException(api + ": the key is not extractable", "InvalidAccessError");
        if (!bytesDoor) return hostWork(webSubtle(api).exportKey("jwk", key));
        var jwk = JSON.parse(door(5, String(nativeKeys.get(key))));
        jwk.ext = true;
        jwk.key_ops = key.usages.slice();
        return jwk;
      });
    };
    Object.defineProperty(CryptoKey.prototype, Symbol.toStringTag, { value: "CryptoKey", configurable: true });
    // "Keep this key" (D1b), one spelling on every host: `store.keepKey(name,
    // pair)` under the `secret.keep <name>` grant, `store.key(name)` →
    // `CryptoKeyPair | null` (a counted read), `store.forget(name)` drops it.
    // Natively Rust writes the pair's JWK itself, so the private scalar never
    // enters this heap; on the web the pair goes to the realm's IndexedDB and
    // the secret holds only its handle.
    function keepKey(name, pair) {
      var api = "store.keepKey()";
      var currentCall = env.current();
      if (env.overlaying()) return Promise.reject(inOverlay(api));
      if (!currentCall || currentCall.status !== "pending")
        return Promise.reject(new Error(api + " called outside an answer"));
      try {
        if (!pair || !isKey(pair.privateKey) || !isKey(pair.publicKey) || pair.privateKey.type !== "private")
          throw new TypeError(api + ": a CryptoKeyPair from generateKey or store.key");
        if (bytesDoor) {
          door(
            6,
            JSON.stringify({
              name: String(name),
              private: nativeKeys.get(pair.privateKey),
              public: nativeKeys.get(pair.publicKey),
            })
          );
          return Promise.resolve();
        }
        if (!keysDoor) throw new Error(api + ": this host keeps no keys");
        // The name holds the new handle only once IndexedDB has the pair, so
        // a refused write leaves the key it held. A later keepKey, set or
        // forget of the name while the pair is written supersedes it.
        var key = String(name),
          handle = "exact.key:" + platformUuid();
        keeping.set(key, handle);
        return hostWork(keysDoor.put(handle, { privateKey: pair.privateKey, publicKey: pair.publicKey })).then(
          function () {
            if (keeping.get(key) !== handle) return;
            keeping.delete(key);
            env.store().set(name, handle);
          },
          function (e) {
            if (keeping.get(key) === handle) keeping.delete(key);
            throw e;
          }
        );
      } catch (e) {
        return Promise.reject(e);
      }
    }
    function keptKey(name) {
      var api = "store.key()";
      var currentCall = env.current();
      if (env.overlaying()) return Promise.reject(inOverlay(api));
      if (!currentCall || currentCall.status !== "pending")
        return Promise.reject(new Error(api + " called outside an answer"));
      try {
        if (bytesDoor) {
          var json = door(7, String(name));
          return Promise.resolve(json ? nativePair(json, false, ["sign", "verify"]) : null);
        }
        var handle = env.store().get(name);
        if (handle === null) return Promise.resolve(null);
        if (!keysDoor) throw new Error(api + ": this host keeps no keys");
        return hostWork(keysDoor.get(handle));
      } catch (e) {
        return Promise.reject(e);
      }
    }
    Object.defineProperty(SubtleCrypto.prototype, Symbol.toStringTag, { value: "SubtleCrypto", configurable: true });
    Object.freeze(subtleObject);
    Object.defineProperty(Crypto.prototype, "subtle", {
      get: function () {
        if (this !== cryptoObject) throw new TypeError("Illegal invocation");
        return subtleObject;
      },
      enumerable: true,
      configurable: true,
    });
    Object.defineProperty(Crypto.prototype, Symbol.toStringTag, { value: "Crypto", configurable: true });
    Object.freeze(cryptoObject);
    // The web's own `CryptoKey` in a browser realm; natively, this one.
    if (bytesDoor || !PlatformCryptoKey)
      Object.defineProperty(global, "CryptoKey", { value: CryptoKey, writable: true, configurable: true });
    Object.defineProperty(global, "SubtleCrypto", { value: SubtleCrypto, writable: true, configurable: true });
    Object.defineProperty(global, "Crypto", { value: Crypto, writable: true, configurable: true });
    fixed(global, "crypto", cryptoObject);
    // A set or forget of the name supersedes a pair still being written.
    function supersede(name) {
      keeping.delete(String(name));
    }
    return { keepKey: keepKey, keptKey: keptKey, supersede: supersede };
  };
})(globalThis);
